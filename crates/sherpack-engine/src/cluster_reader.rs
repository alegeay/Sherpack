//! Cluster reader trait for the `lookup()` template function
//!
//! This module decouples the engine from a specific Kubernetes client.
//! `sherpack-engine` defines the trait; concrete implementations live in
//! crates that have a cluster client (e.g. `sherpack-kube::lookup`).
//!
//! # Helm-compatible semantics
//!
//! `lookup` is *non-fatal*: errors (404, 403, network, unknown kind) all
//! resolve to an empty result rather than failing the render. Errors are
//! surfaced through `LookupState::take_warnings()` so the caller can log
//! them after the render.
//!
//! # Determinism caveat
//!
//! Templates that use `lookup` are non-deterministic by construction:
//! the same Pack rendered against different clusters produces different
//! manifests. This is the same trade-off Helm makes; document it loudly.
//!
//! # Wiring
//!
//! ```ignore
//! use sherpack_engine::{Engine, cluster_reader::ClusterReader};
//! use std::sync::Arc;
//!
//! let reader: Arc<dyn ClusterReader> = Arc::new(MyReader::new());
//! let engine = Engine::builder()
//!     .strict(true)
//!     .with_cluster_reader(reader)
//!     .build();
//! ```

use minijinja::{Environment, Value};
use serde_json::Value as JsonValue;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

/// Reads existing cluster resources at template-render time.
///
/// Implementations MUST be non-fatal: any error (network, RBAC, missing
/// resource, unknown kind) MUST resolve to `None` / empty list.
///
/// Implementations are responsible for any sync/async bridging (e.g.
/// `tokio::task::block_in_place`) — the trait is intentionally sync to
/// match MiniJinja's function signature requirements.
pub trait ClusterReader: Send + Sync {
    /// Look up a single resource by name. Returns `None` if not found
    /// or any error occurred.
    ///
    /// `namespace == ""` means cluster-scoped or any namespace, depending
    /// on the kind's scope.
    fn lookup_one(
        &self,
        api_version: &str,
        kind: &str,
        namespace: &str,
        name: &str,
    ) -> Option<JsonValue>;

    /// List all resources of a kind in a namespace.
    /// `namespace == ""` lists across all namespaces (or cluster-wide
    /// for cluster-scoped resources). Returns an empty Vec on error.
    fn lookup_list(&self, api_version: &str, kind: &str, namespace: &str) -> Vec<JsonValue>;
}

/// Cache key for `lookup` calls (one per (apiVersion, kind, namespace, name))
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct LookupKey {
    api_version: String,
    kind: String,
    namespace: String,
    /// Empty string means "list" mode
    name: String,
}

/// Per-render state for the `lookup()` function.
///
/// Holds:
/// - the cluster reader (Arc-shared, used by the closure registered on the env)
/// - a per-render cache (so duplicate lookups in the same render hit the cluster once)
/// - aggregated warnings (deduped by kind+name) for the caller to surface
///
/// Cloning is cheap (all internals are `Arc`), and clones share the same
/// cache + warnings — exactly what we want when the closure captures it.
#[derive(Clone)]
pub struct LookupState {
    reader: Arc<dyn ClusterReader>,
    cache: Arc<Mutex<HashMap<LookupKey, JsonValue>>>,
    warnings: Arc<Mutex<Vec<String>>>,
    /// Deduplication set — we only record one warning per (kind, name) per render
    warned_keys: Arc<Mutex<HashSet<(String, String)>>>,
}

impl LookupState {
    /// Build a new state from a reader.
    pub fn new(reader: Arc<dyn ClusterReader>) -> Self {
        Self {
            reader,
            cache: Arc::new(Mutex::new(HashMap::new())),
            warnings: Arc::new(Mutex::new(Vec::new())),
            warned_keys: Arc::new(Mutex::new(HashSet::new())),
        }
    }

    /// Register the `lookup` function on the given environment.
    ///
    /// This *replaces* any pre-existing `lookup` registration (the no-op
    /// stub from `functions::lookup`).
    pub fn register(&self, env: &mut Environment<'static>) {
        let state = self.clone();
        env.add_function(
            "lookup",
            move |api_version: String,
                  kind: String,
                  namespace: String,
                  name: String|
                  -> Result<Value, minijinja::Error> {
                Ok(state.do_lookup(&api_version, &kind, &namespace, &name))
            },
        );
    }

    /// Take the accumulated warnings, leaving the state empty.
    /// Caller is responsible for surfacing these (e.g. via tracing or
    /// the render report).
    pub fn take_warnings(&self) -> Vec<String> {
        let mut w = self.warnings.lock().unwrap();
        std::mem::take(&mut *w)
    }

    fn do_lookup(
        &self,
        api_version: &str,
        kind: &str,
        namespace: &str,
        name: &str,
    ) -> Value {
        let key = LookupKey {
            api_version: api_version.to_string(),
            kind: kind.to_string(),
            namespace: namespace.to_string(),
            name: name.to_string(),
        };

        // Cache hit: serve from previous fetch in this render
        if let Some(cached) = self.cache.lock().unwrap().get(&key) {
            return Value::from_serialize(cached);
        }

        // Miss: ask the reader
        let result: JsonValue = if name.is_empty() {
            let items = self.reader.lookup_list(api_version, kind, namespace);
            // Match Helm's list shape: {items: [...]}
            JsonValue::Object(
                serde_json::Map::from_iter([(
                    "items".to_string(),
                    JsonValue::Array(items),
                )])
                .into_iter()
                .collect(),
            )
        } else {
            self.reader
                .lookup_one(api_version, kind, namespace, name)
                .unwrap_or_else(|| JsonValue::Object(serde_json::Map::new()))
        };

        // Warn once per (kind, name) when a non-empty lookup result is used.
        // Helps users spot non-deterministic templates without spamming.
        if !is_empty_lookup_result(&result, name.is_empty())
            && self
                .warned_keys
                .lock()
                .unwrap()
                .insert((kind.to_string(), name.to_string()))
        {
            self.warnings.lock().unwrap().push(format!(
                "lookup() returned cluster state for {}/{}{} — render is non-deterministic",
                kind,
                if namespace.is_empty() { "<all-ns>" } else { namespace },
                if name.is_empty() {
                    String::new()
                } else {
                    format!("/{}", name)
                }
            ));
        }

        // Cache for the rest of this render
        self.cache.lock().unwrap().insert(key, result.clone());
        Value::from_serialize(result)
    }
}

fn is_empty_lookup_result(v: &JsonValue, list_mode: bool) -> bool {
    match v {
        JsonValue::Object(m) if list_mode => m
            .get("items")
            .and_then(|i| i.as_array())
            .is_none_or(|a| a.is_empty()),
        JsonValue::Object(m) => m.is_empty(),
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test reader that returns a fixed map of resources.
    struct MockReader {
        objects: HashMap<(String, String, String, String), JsonValue>,
        call_count: Arc<Mutex<usize>>,
    }

    impl MockReader {
        fn new() -> Self {
            Self {
                objects: HashMap::new(),
                call_count: Arc::new(Mutex::new(0)),
            }
        }

        fn with(mut self, av: &str, kind: &str, ns: &str, name: &str, val: JsonValue) -> Self {
            self.objects.insert(
                (av.into(), kind.into(), ns.into(), name.into()),
                val,
            );
            self
        }
    }

    impl ClusterReader for MockReader {
        fn lookup_one(&self, av: &str, k: &str, ns: &str, n: &str) -> Option<JsonValue> {
            *self.call_count.lock().unwrap() += 1;
            self.objects
                .get(&(av.into(), k.into(), ns.into(), n.into()))
                .cloned()
        }

        fn lookup_list(&self, _av: &str, _k: &str, _ns: &str) -> Vec<JsonValue> {
            *self.call_count.lock().unwrap() += 1;
            Vec::new()
        }
    }

    #[test]
    fn test_lookup_returns_empty_when_not_found() {
        let reader = Arc::new(MockReader::new());
        let state = LookupState::new(reader);
        let v = state.do_lookup("v1", "Secret", "default", "missing");
        // Empty result should serialize to {}
        assert_eq!(v.len().unwrap_or(0), 0);
    }

    #[test]
    fn test_lookup_returns_existing_resource() {
        let reader = Arc::new(
            MockReader::new().with(
                "v1",
                "Secret",
                "default",
                "tls-cert",
                serde_json::json!({"data": {"tls.crt": "abc"}}),
            ),
        );
        let state = LookupState::new(reader);
        let v = state.do_lookup("v1", "Secret", "default", "tls-cert");
        let data = v.get_attr("data").unwrap();
        let crt = data.get_attr("tls.crt").unwrap();
        assert_eq!(crt.to_string(), "abc");
    }

    #[test]
    fn test_cache_dedups_repeated_calls() {
        let reader = MockReader::new().with(
            "v1",
            "Secret",
            "default",
            "x",
            serde_json::json!({"data": {}}),
        );
        let counter = reader.call_count.clone();
        let state = LookupState::new(Arc::new(reader));

        for _ in 0..5 {
            let _ = state.do_lookup("v1", "Secret", "default", "x");
        }

        assert_eq!(*counter.lock().unwrap(), 1, "should only hit reader once");
    }

    #[test]
    fn test_cache_distinguishes_keys() {
        let reader = MockReader::new()
            .with("v1", "Secret", "default", "a", serde_json::json!({}))
            .with("v1", "Secret", "default", "b", serde_json::json!({}));
        let counter = reader.call_count.clone();
        let state = LookupState::new(Arc::new(reader));

        state.do_lookup("v1", "Secret", "default", "a");
        state.do_lookup("v1", "Secret", "default", "b");
        state.do_lookup("v1", "Secret", "default", "a");

        assert_eq!(*counter.lock().unwrap(), 2);
    }

    #[test]
    fn test_warning_emitted_only_for_nonempty_results() {
        let reader = Arc::new(
            MockReader::new().with(
                "v1",
                "Secret",
                "default",
                "real",
                serde_json::json!({"data": {"x": "y"}}),
            ),
        );
        let state = LookupState::new(reader);

        state.do_lookup("v1", "Secret", "default", "missing"); // empty → no warn
        state.do_lookup("v1", "Secret", "default", "real"); // non-empty → warn

        let w = state.take_warnings();
        assert_eq!(w.len(), 1);
        assert!(w[0].contains("Secret"));
        assert!(w[0].contains("real"));
    }

    #[test]
    fn test_warning_deduped_by_kind_and_name() {
        let reader = Arc::new(
            MockReader::new().with(
                "v1",
                "Secret",
                "default",
                "real",
                serde_json::json!({"data": {"x": "y"}}),
            ),
        );
        let state = LookupState::new(reader);

        // 10 calls — only 1 warning
        for _ in 0..10 {
            state.do_lookup("v1", "Secret", "default", "real");
        }

        assert_eq!(state.take_warnings().len(), 1);
    }

    #[test]
    fn test_take_warnings_clears() {
        let reader = Arc::new(
            MockReader::new().with(
                "v1",
                "ConfigMap",
                "default",
                "x",
                serde_json::json!({"data": {"k": "v"}}),
            ),
        );
        let state = LookupState::new(reader);
        state.do_lookup("v1", "ConfigMap", "default", "x");

        assert_eq!(state.take_warnings().len(), 1);
        assert_eq!(state.take_warnings().len(), 0);
    }

    #[test]
    fn test_list_mode_returns_items_wrapper() {
        let reader = Arc::new(MockReader::new());
        let state = LookupState::new(reader);
        let v = state.do_lookup("v1", "Secret", "default", "");
        let items = v.get_attr("items").expect("list mode returns {items: []}");
        assert!(items.try_iter().is_ok());
    }
}
