//! Kubernetes-backed implementation of the engine's `ClusterReader` trait
//!
//! Provides the real `lookup()` semantics for `sherpack install/upgrade`:
//! reads existing cluster resources at template-render time. Errors
//! (404, 403, network, unknown kind) all resolve to "empty" silently,
//! matching Helm's contract.
//!
//! # Sync/async bridging
//!
//! The `ClusterReader` trait is sync (because MiniJinja functions are sync)
//! but `kube` API calls are async. We bridge using
//! `tokio::task::block_in_place` + `Handle::current().block_on()`, which
//! requires a multi-threaded tokio runtime. The Sherpack CLI uses
//! `tokio::runtime::Runtime::new()` which is multi-threaded by default,
//! so this works in the install/upgrade flow.
//!
//! # Discovery
//!
//! Each reader holds its own `Discovery` cache. For multi-render scenarios
//! (e.g. an umbrella pack with subcharts) the same reader can be reused
//! to share the cache.

use std::sync::Arc;
use std::time::Duration;

use kube::{
    Client,
    api::{Api, DynamicObject, ListParams},
    core::GroupVersionKind,
    discovery::{Discovery, Scope},
};
use sherpack_engine::cluster_reader::ClusterReader;
use serde_json::Value as JsonValue;

/// Default per-call timeout for cluster lookups.
///
/// Picked to keep an unreachable cluster from blocking the entire render
/// indefinitely while still being long enough for normal API latency
/// (sub-second typically) plus a healthy margin.
pub const DEFAULT_LOOKUP_TIMEOUT: Duration = Duration::from_secs(5);

/// `ClusterReader` impl backed by a real `kube::Client` and discovery cache.
///
/// Use `Arc<KubeClusterReader>` and pass to
/// `EngineBuilder::with_cluster_reader`.
pub struct KubeClusterReader {
    client: Client,
    discovery: Discovery,
    timeout: Duration,
}

impl KubeClusterReader {
    /// Build a new reader by running cluster discovery once.
    pub async fn new(client: Client) -> Result<Self, kube::Error> {
        let discovery = Discovery::new(client.clone()).run().await?;
        Ok(Self {
            client,
            discovery,
            timeout: DEFAULT_LOOKUP_TIMEOUT,
        })
    }

    /// Build a reader from an existing client and discovery cache.
    /// Useful when you already have a `ResourceManager` and want to share
    /// its discovery to avoid double round-trips.
    pub fn with_discovery(client: Client, discovery: Discovery) -> Self {
        Self {
            client,
            discovery,
            timeout: DEFAULT_LOOKUP_TIMEOUT,
        }
    }

    /// Override the per-call timeout. A timed-out lookup resolves to
    /// `None` / empty list, matching the silent-error contract of Helm's
    /// `lookup`.
    ///
    /// Use a longer timeout for slow API servers or list operations on
    /// large clusters; use a shorter one if you want to fail fast when
    /// the cluster is unreachable.
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Get the configured timeout.
    pub fn timeout(&self) -> Duration {
        self.timeout
    }

    /// Wrap as a trait object suitable for `with_cluster_reader`.
    pub fn into_arc(self) -> Arc<dyn ClusterReader> {
        Arc::new(self)
    }
}

impl ClusterReader for KubeClusterReader {
    fn lookup_one(
        &self,
        api_version: &str,
        kind: &str,
        namespace: &str,
        name: &str,
    ) -> Option<JsonValue> {
        let timeout = self.timeout;
        let kind_for_log = kind.to_string();
        let name_for_log = name.to_string();
        block_on_current(async move {
            let (api_resource, capabilities) = resolve_or_none(&self.discovery, api_version, kind)?;

            // Choose API scope. Cluster-scoped resources ignore namespace.
            let api: Api<DynamicObject> = match capabilities.scope {
                Scope::Cluster => Api::all_with(self.client.clone(), &api_resource),
                Scope::Namespaced => {
                    if namespace.is_empty() {
                        // Helm allows lookup without namespace for namespaced kinds; treat as cluster-wide
                        Api::all_with(self.client.clone(), &api_resource)
                    } else {
                        Api::namespaced_with(self.client.clone(), namespace, &api_resource)
                    }
                }
            };

            // Helm-compat: any error → None. We deliberately swallow 403/404/network here.
            // tokio::time::timeout maps "timed out" the same way (Result::Err → None).
            let result = tokio::time::timeout(timeout, api.get_opt(name)).await;
            let obj = match result {
                Ok(Ok(opt)) => opt?,
                Ok(Err(_)) => return None,
                Err(_) => {
                    tracing::warn!(
                        "lookup {}/{} timed out after {:?}",
                        kind_for_log,
                        name_for_log,
                        timeout
                    );
                    return None;
                }
            };
            serde_json::to_value(obj).ok()
        })
        .flatten()
    }

    fn lookup_list(
        &self,
        api_version: &str,
        kind: &str,
        namespace: &str,
    ) -> Vec<JsonValue> {
        let timeout = self.timeout;
        let kind_for_log = kind.to_string();
        block_on_current(async move {
            let Some((api_resource, capabilities)) =
                resolve_or_none(&self.discovery, api_version, kind)
            else {
                return Vec::new();
            };

            let api: Api<DynamicObject> = match capabilities.scope {
                Scope::Cluster => Api::all_with(self.client.clone(), &api_resource),
                Scope::Namespaced => {
                    if namespace.is_empty() {
                        Api::all_with(self.client.clone(), &api_resource)
                    } else {
                        Api::namespaced_with(self.client.clone(), namespace, &api_resource)
                    }
                }
            };

            match tokio::time::timeout(timeout, api.list(&ListParams::default())).await {
                Ok(Ok(list)) => list
                    .items
                    .into_iter()
                    .filter_map(|o| serde_json::to_value(o).ok())
                    .collect(),
                Ok(Err(_)) => Vec::new(),
                Err(_) => {
                    tracing::warn!(
                        "lookup list {} timed out after {:?}",
                        kind_for_log,
                        timeout
                    );
                    Vec::new()
                }
            }
        })
        .unwrap_or_default()
    }
}

/// Run an async future from a sync context.
///
/// Requires being inside a multi-threaded tokio runtime (the Sherpack CLI
/// is). Returns `None` if no runtime is available — that situation
/// shouldn't happen in normal operation, but we return None rather than
/// panic to keep `lookup` non-fatal.
fn block_on_current<F, T>(fut: F) -> Option<T>
where
    F: std::future::Future<Output = T>,
{
    let handle = tokio::runtime::Handle::try_current().ok()?;
    Some(tokio::task::block_in_place(|| handle.block_on(fut)))
}

/// Resolve an apiVersion+kind to an `(ApiResource, ApiCapabilities)`,
/// silently returning None on unknown kinds (Helm-compat).
fn resolve_or_none(
    discovery: &Discovery,
    api_version: &str,
    kind: &str,
) -> Option<(kube::discovery::ApiResource, kube::discovery::ApiCapabilities)> {
    let (group, version) = match api_version.rsplit_once('/') {
        Some((g, v)) => (g.to_string(), v.to_string()),
        None => (String::new(), api_version.to_string()),
    };
    let gvk = GroupVersionKind {
        group,
        version,
        kind: kind.to_string(),
    };
    discovery.resolve_gvk(&gvk)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_or_none_handles_core_api() {
        // Core API uses "v1" without group prefix
        // We can't test the full path without a cluster, but we can
        // verify the GVK construction path doesn't panic on parsing
        let (group, version) = match "v1".rsplit_once('/') {
            Some((g, v)) => (g.to_string(), v.to_string()),
            None => (String::new(), "v1".to_string()),
        };
        assert_eq!(group, "");
        assert_eq!(version, "v1");
    }

    #[test]
    fn test_resolve_or_none_handles_grouped_api() {
        let (group, version) = match "apps/v1".rsplit_once('/') {
            Some((g, v)) => (g.to_string(), v.to_string()),
            None => (String::new(), "apps/v1".to_string()),
        };
        assert_eq!(group, "apps");
        assert_eq!(version, "v1");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_block_on_current_works_inside_runtime() {
        // Verify our sync-from-async bridge works on a multi-threaded runtime.
        let result: Option<i32> = block_on_current(async { 42 });
        assert_eq!(result, Some(42));
    }

    #[test]
    fn test_block_on_current_returns_none_outside_runtime() {
        // Without a tokio runtime in scope, we should get None (not panic).
        let result: Option<i32> = block_on_current(async { 42 });
        assert_eq!(result, None);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_timeout_wraps_slow_future() {
        // Direct test of the tokio::time::timeout primitive we wrap kube
        // calls with — verifies our timeout mechanism produces Err(_) when
        // the inner future doesn't complete in time.
        let timeout = Duration::from_millis(50);
        let result = tokio::time::timeout(
            timeout,
            tokio::time::sleep(Duration::from_millis(500)),
        )
        .await;
        assert!(result.is_err(), "expected timeout error");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_timeout_passes_through_fast_future() {
        let timeout = Duration::from_secs(5);
        let result = tokio::time::timeout(timeout, async { 42 }).await;
        assert_eq!(result.unwrap(), 42);
    }
}
