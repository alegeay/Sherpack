//! Kubernetes resource operations for applying and deleting manifests
//!
//! This module provides functions to apply and delete Kubernetes resources
//! from YAML manifests, similar to `kubectl apply -f` and `kubectl delete -f`.
//!
//! Key features:
//! - Server-Side Apply for idempotent resource management
//! - Dynamic resource handling without compile-time type knowledge
//! - Proper ordering for creation (dependencies first) and deletion (reverse)
//! - Support for resource policy annotations (keep on delete)
//! - Retry logic for transient conflicts

use kube::{
    Client,
    api::{Api, DeleteParams, DynamicObject, Patch, PatchParams},
    core::{GroupVersionKind, TypeMeta},
    discovery::{ApiCapabilities, ApiResource, Discovery, Scope},
};

use crate::crd::ResourceCategory;
use crate::error::{KubeError, Result};

/// Field manager name for Server-Side Apply
const FIELD_MANAGER: &str = "sherpack";

/// Annotation to keep resource on uninstall (Helm-compatible)
const RESOURCE_POLICY_ANNOTATION: &str = "helm.sh/resource-policy";
const RESOURCE_POLICY_KEEP: &str = "keep";

/// Sherpack-specific annotation
const SHERPACK_RESOURCE_POLICY: &str = "sherpack.io/resource-policy";

/// Result of applying a single resource
#[derive(Debug, Clone)]
pub struct ApplyResult {
    /// Resource kind
    pub kind: String,
    /// Resource name
    pub name: String,
    /// Resource namespace (None for cluster-scoped)
    pub namespace: Option<String>,
    /// Whether it was created (true) or updated (false)
    pub created: bool,
}

/// Result of deleting a single resource
#[derive(Debug, Clone)]
pub struct DeleteResult {
    /// Resource kind
    pub kind: String,
    /// Resource name
    pub name: String,
    /// Resource namespace (None for cluster-scoped)
    pub namespace: Option<String>,
    /// Whether it was actually deleted (false if skipped due to policy)
    pub deleted: bool,
    /// Reason if not deleted
    pub skip_reason: Option<String>,
}

/// Summary of apply/delete operations
#[derive(Debug, Clone, Default)]
pub struct OperationSummary {
    /// Successfully processed resources
    pub succeeded: Vec<String>,
    /// Failed resources with errors
    pub failed: Vec<(String, String)>,
    /// Skipped resources (e.g., due to policy)
    pub skipped: Vec<(String, String)>,
}

impl OperationSummary {
    /// Check if all operations succeeded
    pub fn is_success(&self) -> bool {
        self.failed.is_empty()
    }

    /// Get total count
    pub fn total(&self) -> usize {
        self.succeeded.len() + self.failed.len() + self.skipped.len()
    }

    /// Format as human-readable summary
    pub fn summary(&self) -> String {
        let mut parts = Vec::with_capacity(3); // At most 3 parts
        if !self.succeeded.is_empty() {
            parts.push(format!("{} succeeded", self.succeeded.len()));
        }
        if !self.failed.is_empty() {
            parts.push(format!("{} failed", self.failed.len()));
        }
        if !self.skipped.is_empty() {
            parts.push(format!("{} skipped", self.skipped.len()));
        }
        if parts.is_empty() {
            "No resources processed".to_string()
        } else {
            parts.join(", ")
        }
    }
}

/// Parsed resource ready for Kubernetes operations
#[derive(Debug, Clone)]
struct ParsedResource {
    /// The dynamic object
    obj: DynamicObject,
    /// Group-Version-Kind
    gvk: GroupVersionKind,
    /// API resource metadata
    api_resource: ApiResource,
    /// API capabilities (namespaced vs cluster-scoped)
    capabilities: ApiCapabilities,
}

impl ParsedResource {
    /// Get display name for logging
    fn display_name(&self) -> String {
        let name = self.obj.metadata.name.as_deref().unwrap_or("unnamed");
        match &self.obj.metadata.namespace {
            Some(ns) => format!("{}/{}/{}", ns, self.gvk.kind, name),
            None => format!("{}/{}", self.gvk.kind, name),
        }
    }

    /// Check if resource has keep policy annotation
    fn has_keep_policy(&self) -> bool {
        self.obj
            .metadata
            .annotations
            .as_ref()
            .map(|annotations| {
                // Check both Helm and Sherpack annotations (no allocation)
                annotations
                    .get(RESOURCE_POLICY_ANNOTATION)
                    .map(String::as_str)
                    == Some(RESOURCE_POLICY_KEEP)
                    || annotations
                        .get(SHERPACK_RESOURCE_POLICY)
                        .map(String::as_str)
                        == Some(RESOURCE_POLICY_KEEP)
            })
            .unwrap_or(false)
    }
}

/// Resource manager for applying and deleting Kubernetes resources
pub struct ResourceManager {
    /// Kubernetes client
    client: Client,
    /// Cached discovery information
    discovery: Discovery,
}

impl ResourceManager {
    /// Create a new ResourceManager
    pub async fn new(client: Client) -> Result<Self> {
        let discovery = Discovery::new(client.clone())
            .run()
            .await
            .map_err(KubeError::Api)?;

        Ok(Self { client, discovery })
    }

    /// Create from existing client and discovery (for reuse)
    pub fn with_discovery(client: Client, discovery: Discovery) -> Self {
        Self { client, discovery }
    }

    /// Refresh discovery cache (call after CRD changes)
    pub async fn refresh_discovery(&mut self) -> Result<()> {
        self.discovery = Discovery::new(self.client.clone())
            .run()
            .await
            .map_err(KubeError::Api)?;
        Ok(())
    }

    /// Apply a manifest to the cluster using Server-Side Apply
    ///
    /// # Arguments
    /// * `namespace` - Default namespace for namespaced resources without explicit namespace
    /// * `manifest` - YAML manifest (can contain multiple documents separated by ---)
    /// * `dry_run` - If true, validate without applying
    ///
    /// # Returns
    /// Summary of applied resources
    pub async fn apply_manifest(
        &self,
        namespace: &str,
        manifest: &str,
        dry_run: bool,
    ) -> Result<OperationSummary> {
        let resources = self.parse_manifest(manifest, namespace)?;
        self.apply_resources(&resources, dry_run).await
    }

    /// Delete resources from a manifest
    ///
    /// # Arguments
    /// * `namespace` - Default namespace for namespaced resources
    /// * `manifest` - YAML manifest (can contain multiple documents)
    /// * `dry_run` - If true, validate without deleting
    ///
    /// # Returns
    /// Summary of deleted resources
    pub async fn delete_manifest(
        &self,
        namespace: &str,
        manifest: &str,
        dry_run: bool,
    ) -> Result<OperationSummary> {
        let resources = self.parse_manifest(manifest, namespace)?;
        self.delete_resources(&resources, dry_run).await
    }

    /// Parse a YAML manifest into ParsedResource list
    fn parse_manifest(
        &self,
        manifest: &str,
        default_namespace: &str,
    ) -> Result<Vec<ParsedResource>> {
        let mut resources = Vec::new();

        for (index, doc) in manifest.split("---").enumerate() {
            let doc = doc.trim();
            if doc.is_empty() {
                continue;
            }

            // Skip YAML comments-only documents
            if doc
                .lines()
                .all(|l| l.trim().is_empty() || l.trim().starts_with('#'))
            {
                continue;
            }

            match self.parse_single_document(doc, default_namespace) {
                Ok(resource) => resources.push(resource),
                Err(e) => {
                    // Include document index in error for debugging
                    return Err(KubeError::InvalidConfig(format!(
                        "Failed to parse document {}: {}",
                        index, e
                    )));
                }
            }
        }

        Ok(resources)
    }

    /// Parse a single YAML document into ParsedResource
    fn parse_single_document(&self, doc: &str, default_namespace: &str) -> Result<ParsedResource> {
        // Parse YAML into DynamicObject
        let mut obj: DynamicObject = serde_yaml::from_str(doc)
            .map_err(|e| KubeError::Serialization(format!("YAML parse error: {}", e)))?;

        // Extract TypeMeta (apiVersion + kind)
        let type_meta = obj.types.as_ref().ok_or_else(|| {
            KubeError::InvalidConfig("Resource missing apiVersion or kind".to_string())
        })?;

        // Convert to GroupVersionKind
        let gvk = gvk_from_type_meta(type_meta);

        // Resolve GVK to ApiResource using discovery
        let (api_resource, capabilities) = self.discovery.resolve_gvk(&gvk).ok_or_else(|| {
            KubeError::InvalidConfig(format!(
                "Unknown resource type: {}/{}",
                type_meta.api_version, type_meta.kind
            ))
        })?;

        // Apply default namespace for namespaced resources
        if capabilities.scope == Scope::Namespaced && obj.metadata.namespace.is_none() {
            obj.metadata.namespace = Some(default_namespace.to_string());
        }

        Ok(ParsedResource {
            obj,
            gvk,
            api_resource,
            capabilities,
        })
    }

    /// Apply parsed resources to the cluster
    async fn apply_resources(
        &self,
        resources: &[ParsedResource],
        dry_run: bool,
    ) -> Result<OperationSummary> {
        let mut summary = OperationSummary::default();

        // Sort resources by creation order (namespaces first, then CRDs, then others)
        let sorted = self.sort_for_apply(resources);

        for resource in sorted {
            let name = resource.display_name();

            match self.apply_single_resource(resource, dry_run).await {
                Ok(result) => {
                    let action = if result.created {
                        "created"
                    } else {
                        "configured"
                    };
                    summary.succeeded.push(format!("{} ({})", name, action));
                }
                Err(e) => {
                    summary.failed.push((name, e.to_string()));
                }
            }
        }

        Ok(summary)
    }

    /// Apply a single resource using Server-Side Apply
    async fn apply_single_resource(
        &self,
        resource: &ParsedResource,
        dry_run: bool,
    ) -> Result<ApplyResult> {
        let name = resource.obj.metadata.name.as_deref().ok_or_else(|| {
            KubeError::InvalidConfig("Resource missing metadata.name".to_string())
        })?;

        let api = self.api_for_resource(resource);

        // Check if resource exists (to determine created vs updated)
        let exists = api.get_opt(name).await.map_err(KubeError::Api)?.is_some();

        // Build patch params for Server-Side Apply
        let mut params = PatchParams::apply(FIELD_MANAGER);
        params.force = true; // Take ownership of fields

        if dry_run {
            params.dry_run = true;
        }

        // Perform Server-Side Apply
        let _result = api
            .patch(name, &params, &Patch::Apply(&resource.obj))
            .await
            .map_err(|e| {
                KubeError::InvalidConfig(format!(
                    "Failed to apply {}: {}",
                    resource.display_name(),
                    e
                ))
            })?;

        Ok(ApplyResult {
            kind: resource.gvk.kind.clone(),
            name: name.to_string(),
            namespace: resource.obj.metadata.namespace.clone(),
            created: !exists,
        })
    }

    /// Delete parsed resources from the cluster
    async fn delete_resources(
        &self,
        resources: &[ParsedResource],
        dry_run: bool,
    ) -> Result<OperationSummary> {
        let mut summary = OperationSummary::default();

        // Sort resources in reverse order for deletion
        let sorted = self.sort_for_delete(resources);

        for resource in sorted {
            let name = resource.display_name();

            // Check for keep policy
            if resource.has_keep_policy() {
                summary
                    .skipped
                    .push((name, "resource-policy: keep".to_string()));
                continue;
            }

            match self.delete_single_resource(resource, dry_run).await {
                Ok(result) => {
                    if result.deleted {
                        summary.succeeded.push(format!("{} (deleted)", name));
                    } else if let Some(reason) = result.skip_reason {
                        summary.skipped.push((name, reason));
                    }
                }
                Err(e) => {
                    // NotFound is not an error for deletion
                    if e.is_not_found() {
                        summary.skipped.push((name, "not found".to_string()));
                    } else {
                        summary.failed.push((name, e.to_string()));
                    }
                }
            }
        }

        Ok(summary)
    }

    /// Delete a single resource
    async fn delete_single_resource(
        &self,
        resource: &ParsedResource,
        dry_run: bool,
    ) -> Result<DeleteResult> {
        let name = resource.obj.metadata.name.as_deref().ok_or_else(|| {
            KubeError::InvalidConfig("Resource missing metadata.name".to_string())
        })?;

        let api = self.api_for_resource(resource);

        // Build delete params
        let params = DeleteParams {
            propagation_policy: Some(kube::api::PropagationPolicy::Background),
            dry_run,
            ..Default::default()
        };

        // Delete the resource
        match api.delete(name, &params).await {
            Ok(_) => Ok(DeleteResult {
                kind: resource.gvk.kind.clone(),
                name: name.to_string(),
                namespace: resource.obj.metadata.namespace.clone(),
                deleted: true,
                skip_reason: None,
            }),
            Err(kube::Error::Api(resp)) if resp.code == 404 => Ok(DeleteResult {
                kind: resource.gvk.kind.clone(),
                name: name.to_string(),
                namespace: resource.obj.metadata.namespace.clone(),
                deleted: false,
                skip_reason: Some("not found".to_string()),
            }),
            Err(e) => Err(KubeError::Api(e)),
        }
    }

    /// Sort resources for creation (dependencies first)
    ///
    /// Uses ResourceCategory for proper ordering:
    /// CRDs → Namespaces → RBAC → Config → Network → Workloads → Custom Resources
    fn sort_for_apply<'a>(&self, resources: &'a [ParsedResource]) -> Vec<&'a ParsedResource> {
        let mut sorted: Vec<&ParsedResource> = resources.iter().collect();
        sorted.sort_by(|a, b| {
            let cat_a = ResourceCategory::from_resource(
                &a.gvk.kind,
                a.obj
                    .types
                    .as_ref()
                    .map(|t| t.api_version.as_str())
                    .unwrap_or("v1"),
            );
            let cat_b = ResourceCategory::from_resource(
                &b.gvk.kind,
                b.obj
                    .types
                    .as_ref()
                    .map(|t| t.api_version.as_str())
                    .unwrap_or("v1"),
            );
            cat_a.cmp(&cat_b)
        });
        sorted
    }

    /// Sort resources for deletion (reverse of creation order)
    fn sort_for_delete<'a>(&self, resources: &'a [ParsedResource]) -> Vec<&'a ParsedResource> {
        let mut sorted: Vec<&ParsedResource> = resources.iter().collect();
        sorted.sort_by(|a, b| {
            let cat_a = ResourceCategory::from_resource(
                &a.gvk.kind,
                a.obj
                    .types
                    .as_ref()
                    .map(|t| t.api_version.as_str())
                    .unwrap_or("v1"),
            );
            let cat_b = ResourceCategory::from_resource(
                &b.gvk.kind,
                b.obj
                    .types
                    .as_ref()
                    .map(|t| t.api_version.as_str())
                    .unwrap_or("v1"),
            );
            cat_b.cmp(&cat_a) // Reverse order
        });
        sorted
    }

    /// Check if a resource is a CRD
    pub fn is_crd(kind: &str) -> bool {
        kind == "CustomResourceDefinition"
    }

    /// Filter resources into CRDs and non-CRDs
    #[allow(dead_code)]
    fn partition_crds<'a>(
        &self,
        resources: &'a [ParsedResource],
    ) -> (Vec<&'a ParsedResource>, Vec<&'a ParsedResource>) {
        resources.iter().partition(|r| Self::is_crd(&r.gvk.kind))
    }

    /// Create an Api client for a parsed resource
    fn api_for_resource(&self, resource: &ParsedResource) -> Api<DynamicObject> {
        if resource.capabilities.scope == Scope::Namespaced {
            let ns = resource
                .obj
                .metadata
                .namespace
                .as_deref()
                .unwrap_or("default");
            Api::namespaced_with(self.client.clone(), ns, &resource.api_resource)
        } else {
            Api::all_with(self.client.clone(), &resource.api_resource)
        }
    }
}

/// Convert TypeMeta to GroupVersionKind
///
/// This function parses the apiVersion field to extract group and version:
/// - "apps/v1" -> group="apps", version="v1"
/// - "v1" -> group="", version="v1" (core API)
fn gvk_from_type_meta(tm: &TypeMeta) -> GroupVersionKind {
    let (group, version) = match tm.api_version.rsplit_once('/') {
        Some((g, v)) => (g.to_string(), v.to_string()),
        None => (String::new(), tm.api_version.clone()),
    };

    GroupVersionKind {
        group,
        version,
        kind: tm.kind.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resource_category_ordering() {
        // CRDs should come before everything
        assert!(ResourceCategory::Crd < ResourceCategory::Namespace);
        assert!(ResourceCategory::Namespace < ResourceCategory::ClusterRbac);
        assert!(ResourceCategory::ClusterRbac < ResourceCategory::Config);
        assert!(ResourceCategory::Config < ResourceCategory::Workload);
        assert!(ResourceCategory::Workload < ResourceCategory::CustomResource);
    }

    #[test]
    fn test_resource_manager_is_crd() {
        assert!(ResourceManager::is_crd("CustomResourceDefinition"));
        assert!(!ResourceManager::is_crd("Deployment"));
        assert!(!ResourceManager::is_crd("ConfigMap"));
    }

    #[test]
    fn test_gvk_from_type_meta() {
        // Test with group (apps/v1)
        let tm = TypeMeta {
            api_version: "apps/v1".to_string(),
            kind: "Deployment".to_string(),
        };
        let gvk = gvk_from_type_meta(&tm);
        assert_eq!(gvk.group, "apps");
        assert_eq!(gvk.version, "v1");
        assert_eq!(gvk.kind, "Deployment");

        // Test core API (v1 without group)
        let tm_core = TypeMeta {
            api_version: "v1".to_string(),
            kind: "ConfigMap".to_string(),
        };
        let gvk_core = gvk_from_type_meta(&tm_core);
        assert_eq!(gvk_core.group, "");
        assert_eq!(gvk_core.version, "v1");
        assert_eq!(gvk_core.kind, "ConfigMap");
    }

    #[test]
    fn test_gvk_from_type_meta_various_api_groups() {
        // networking.k8s.io/v1
        let tm = TypeMeta {
            api_version: "networking.k8s.io/v1".to_string(),
            kind: "Ingress".to_string(),
        };
        let gvk = gvk_from_type_meta(&tm);
        assert_eq!(gvk.group, "networking.k8s.io");
        assert_eq!(gvk.version, "v1");

        // batch/v1
        let tm_batch = TypeMeta {
            api_version: "batch/v1".to_string(),
            kind: "Job".to_string(),
        };
        let gvk_batch = gvk_from_type_meta(&tm_batch);
        assert_eq!(gvk_batch.group, "batch");
        assert_eq!(gvk_batch.version, "v1");

        // autoscaling/v2
        let tm_hpa = TypeMeta {
            api_version: "autoscaling/v2".to_string(),
            kind: "HorizontalPodAutoscaler".to_string(),
        };
        let gvk_hpa = gvk_from_type_meta(&tm_hpa);
        assert_eq!(gvk_hpa.group, "autoscaling");
        assert_eq!(gvk_hpa.version, "v2");
    }

    #[test]
    fn test_operation_summary() {
        let mut summary = OperationSummary::default();
        summary.succeeded.push("deployment/nginx".to_string());
        summary.succeeded.push("service/nginx".to_string());
        summary.skipped.push((
            "secret/keep-me".to_string(),
            "resource-policy: keep".to_string(),
        ));

        assert!(summary.is_success());
        assert_eq!(summary.total(), 3);
        assert!(summary.summary().contains("2 succeeded"));
        assert!(summary.summary().contains("1 skipped"));
    }

    #[test]
    fn test_operation_summary_empty() {
        let summary = OperationSummary::default();

        assert!(summary.is_success());
        assert_eq!(summary.total(), 0);
        assert_eq!(summary.summary(), "No resources processed");
    }

    #[test]
    fn test_operation_summary_with_failures() {
        let mut summary = OperationSummary::default();
        summary.succeeded.push("deployment/app".to_string());
        summary.failed.push((
            "service/broken".to_string(),
            "Connection refused".to_string(),
        ));

        assert!(!summary.is_success());
        assert_eq!(summary.total(), 2);
        assert!(summary.summary().contains("1 succeeded"));
        assert!(summary.summary().contains("1 failed"));
    }

    #[test]
    fn test_operation_summary_only_skipped() {
        let mut summary = OperationSummary::default();
        summary
            .skipped
            .push(("secret/keep".to_string(), "keep policy".to_string()));
        summary
            .skipped
            .push(("configmap/keep".to_string(), "keep policy".to_string()));

        assert!(summary.is_success());
        assert_eq!(summary.total(), 2);
        assert!(summary.summary().contains("2 skipped"));
    }

    #[test]
    fn test_apply_result_created() {
        let result = ApplyResult {
            kind: "Deployment".to_string(),
            name: "my-app".to_string(),
            namespace: Some("default".to_string()),
            created: true,
        };

        assert!(result.created);
        assert_eq!(result.kind, "Deployment");
        assert_eq!(result.namespace, Some("default".to_string()));
    }

    #[test]
    fn test_apply_result_updated() {
        let result = ApplyResult {
            kind: "ConfigMap".to_string(),
            name: "config".to_string(),
            namespace: Some("default".to_string()),
            created: false,
        };

        assert!(!result.created);
    }

    #[test]
    fn test_delete_result_deleted() {
        let result = DeleteResult {
            kind: "Pod".to_string(),
            name: "old-pod".to_string(),
            namespace: Some("default".to_string()),
            deleted: true,
            skip_reason: None,
        };

        assert!(result.deleted);
        assert!(result.skip_reason.is_none());
    }

    #[test]
    fn test_delete_result_skipped() {
        let result = DeleteResult {
            kind: "Secret".to_string(),
            name: "important-secret".to_string(),
            namespace: Some("default".to_string()),
            deleted: false,
            skip_reason: Some("resource-policy: keep".to_string()),
        };

        assert!(!result.deleted);
        assert_eq!(
            result.skip_reason,
            Some("resource-policy: keep".to_string())
        );
    }

    #[test]
    fn test_delete_result_cluster_scoped() {
        let result = DeleteResult {
            kind: "ClusterRole".to_string(),
            name: "admin-role".to_string(),
            namespace: None,
            deleted: true,
            skip_reason: None,
        };

        assert!(result.namespace.is_none());
    }

    #[test]
    fn test_resource_policy_annotation_names() {
        // Verify our annotation constants
        assert_eq!(RESOURCE_POLICY_ANNOTATION, "helm.sh/resource-policy");
        assert_eq!(SHERPACK_RESOURCE_POLICY, "sherpack.io/resource-policy");
        assert_eq!(RESOURCE_POLICY_KEEP, "keep");
    }

    #[test]
    fn test_field_manager_constant() {
        assert_eq!(FIELD_MANAGER, "sherpack");
    }
}
