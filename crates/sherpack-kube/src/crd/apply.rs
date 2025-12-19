//! CRD application operations
//!
//! This module provides the CrdManager for applying and managing CRDs
//! in a Kubernetes cluster.

use std::time::Duration;

use kube::{
    Client,
    api::{Api, DynamicObject, Patch, PatchParams},
    core::GroupVersionKind,
    discovery::{ApiResource, Discovery},
};
use serde::{Deserialize, Serialize};

use crate::error::{KubeError, Result};

/// Field manager for CRD operations
const CRD_FIELD_MANAGER: &str = "sherpack-crd";

/// Resource category for ordering during installation
///
/// Resources are installed in order from lowest to highest category value.
/// This ensures dependencies (CRDs, RBAC) exist before workloads.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ResourceCategory {
    /// CustomResourceDefinition - installed first
    Crd = 0,
    /// Namespace - created before namespace-scoped resources
    Namespace = 1,
    /// ResourceQuota, LimitRange, PodSecurityPolicy
    NamespaceConfig = 2,
    /// ClusterRole, ClusterRoleBinding
    ClusterRbac = 10,
    /// Role, RoleBinding, ServiceAccount
    NamespacedRbac = 11,
    /// ConfigMap, Secret
    Config = 20,
    /// PersistentVolume, PersistentVolumeClaim, StorageClass
    Storage = 21,
    /// NetworkPolicy, Service, Endpoints, Ingress
    Network = 30,
    /// Deployment, StatefulSet, DaemonSet, ReplicaSet, Pod
    Workload = 40,
    /// Job, CronJob
    Batch = 50,
    /// HorizontalPodAutoscaler, VerticalPodAutoscaler, PodDisruptionBudget
    Autoscaling = 60,
    /// Custom Resources (using CRDs)
    CustomResource = 70,
    /// Everything else
    Other = 100,
}

impl ResourceCategory {
    /// Categorize a resource by its kind and apiVersion
    pub fn from_resource(kind: &str, api_version: &str) -> Self {
        match kind {
            "CustomResourceDefinition" => Self::Crd,
            "Namespace" => Self::Namespace,
            "ResourceQuota" | "LimitRange" | "PodSecurityPolicy" => Self::NamespaceConfig,
            "ClusterRole" | "ClusterRoleBinding" => Self::ClusterRbac,
            "Role" | "RoleBinding" | "ServiceAccount" => Self::NamespacedRbac,
            "ConfigMap" | "Secret" => Self::Config,
            "PersistentVolume" | "PersistentVolumeClaim" | "StorageClass" => Self::Storage,
            "NetworkPolicy" | "Service" | "Endpoints" | "Ingress" | "IngressClass" => Self::Network,
            "Deployment" | "StatefulSet" | "DaemonSet" | "ReplicaSet" | "Pod" => Self::Workload,
            "Job" | "CronJob" => Self::Batch,
            "HorizontalPodAutoscaler" | "VerticalPodAutoscaler" | "PodDisruptionBudget" => {
                Self::Autoscaling
            }
            _ => {
                // Check if it's a custom resource (non-core API)
                if Self::is_custom_api_version(api_version) {
                    Self::CustomResource
                } else {
                    Self::Other
                }
            }
        }
    }

    /// Check if an apiVersion indicates a custom resource
    pub fn is_custom_api_version(api_version: &str) -> bool {
        // Core APIs: v1, apps/v1, batch/v1, etc.
        // Custom APIs: mygroup.example.com/v1, stable.example.com/v1beta1
        let core_groups = [
            "v1",
            "apps",
            "batch",
            "autoscaling",
            "policy",
            "networking.k8s.io",
            "rbac.authorization.k8s.io",
            "storage.k8s.io",
            "admissionregistration.k8s.io",
            "apiextensions.k8s.io",
            "certificates.k8s.io",
            "coordination.k8s.io",
            "discovery.k8s.io",
            "events.k8s.io",
            "flowcontrol.apiserver.k8s.io",
            "node.k8s.io",
            "scheduling.k8s.io",
        ];

        // Extract group from apiVersion (e.g., "apps/v1" -> "apps")
        let group = api_version
            .rsplit('/')
            .next_back()
            .map(|_| api_version.rsplit('/').nth(1).unwrap_or(api_version));

        match group {
            Some(g) => !core_groups.contains(&g),
            None => !core_groups.contains(&api_version), // v1 case
        }
    }
}

/// CRD condition for waiting
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CrdCondition {
    #[serde(rename = "type")]
    condition_type: String,
    status: String,
    #[serde(default)]
    reason: Option<String>,
    #[serde(default)]
    message: Option<String>,
}

/// CRD status for checking readiness
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CrdStatus {
    #[serde(default)]
    conditions: Vec<CrdCondition>,
    #[serde(default)]
    accepted_names: Option<CrdAcceptedNames>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CrdAcceptedNames {
    kind: String,
    plural: String,
}

/// Manager for CRD operations
pub struct CrdManager {
    client: Client,
}

impl CrdManager {
    /// Create a new CrdManager
    pub fn new(client: Client) -> Self {
        Self { client }
    }

    /// Get the underlying Kubernetes client
    pub fn client(&self) -> &Client {
        &self.client
    }

    /// Apply a CRD using Server-Side Apply
    ///
    /// Uses Server-Side Apply which is the modern, recommended approach.
    /// This handles conflicts properly and provides field ownership tracking.
    pub async fn apply_crd(&self, manifest: &str, dry_run: bool) -> Result<CrdApplyResult> {
        let obj: DynamicObject = serde_yaml::from_str(manifest)
            .map_err(|e| KubeError::Serialization(format!("Invalid CRD YAML: {}", e)))?;

        let name = obj
            .metadata
            .name
            .as_deref()
            .ok_or_else(|| KubeError::InvalidConfig("CRD missing metadata.name".to_string()))?;

        // CRDs are cluster-scoped
        let api: Api<DynamicObject> = Api::all_with(
            self.client.clone(),
            &ApiResource::erase::<k8s_openapi::apiextensions_apiserver::pkg::apis::apiextensions::v1::CustomResourceDefinition>(&()),
        );

        // Check if CRD exists
        let exists = api.get_opt(name).await.map_err(KubeError::Api)?.is_some();

        // Use Server-Side Apply
        let patch_params = PatchParams {
            field_manager: Some(CRD_FIELD_MANAGER.to_string()),
            dry_run,
            force: true, // Take ownership of all fields
            ..Default::default()
        };

        // Apply using Server-Side Apply
        api.patch(name, &patch_params, &Patch::Apply(&obj))
            .await
            .map_err(|e| {
                KubeError::InvalidConfig(format!("Failed to apply CRD {}: {}", name, e))
            })?;

        Ok(CrdApplyResult {
            name: name.to_string(),
            created: !exists,
        })
    }

    /// Get a CRD from the cluster
    pub async fn get_crd(&self, name: &str) -> Result<Option<DynamicObject>> {
        let api: Api<DynamicObject> = Api::all_with(
            self.client.clone(),
            &ApiResource::erase::<k8s_openapi::apiextensions_apiserver::pkg::apis::apiextensions::v1::CustomResourceDefinition>(&()),
        );

        api.get_opt(name).await.map_err(KubeError::Api)
    }

    /// Wait for a CRD to be established (ready)
    ///
    /// A CRD is ready when it has the "Established" condition set to "True".
    pub async fn wait_for_crd(&self, name: &str, timeout: Duration) -> Result<()> {
        use tokio::time::{Instant, sleep};

        let api: Api<DynamicObject> = Api::all_with(
            self.client.clone(),
            &ApiResource::erase::<k8s_openapi::apiextensions_apiserver::pkg::apis::apiextensions::v1::CustomResourceDefinition>(&()),
        );

        let start = Instant::now();
        let poll_interval = Duration::from_millis(500);

        loop {
            if start.elapsed() > timeout {
                return Err(KubeError::Timeout(format!(
                    "CRD {} not established after {:?}",
                    name, timeout
                )));
            }

            match api.get(name).await {
                Ok(crd) => {
                    if Self::is_crd_established(&crd) {
                        return Ok(());
                    }
                }
                Err(kube::Error::Api(resp)) if resp.code == 404 => {
                    // CRD doesn't exist yet, keep waiting
                }
                Err(e) => return Err(KubeError::Api(e)),
            }

            sleep(poll_interval).await;
        }
    }

    /// Check if a CRD is established
    fn is_crd_established(crd: &DynamicObject) -> bool {
        let status = crd.data.get("status");
        let conditions = status
            .and_then(|s| s.get("conditions"))
            .and_then(|c| c.as_array());

        conditions
            .map(|conds| {
                conds.iter().any(|c| {
                    c.get("type").and_then(|t| t.as_str()) == Some("Established")
                        && c.get("status").and_then(|s| s.as_str()) == Some("True")
                })
            })
            .unwrap_or(false)
    }

    /// Apply multiple CRDs and wait for them to be ready
    pub async fn apply_crds(
        &self,
        manifests: &[String],
        timeout: Duration,
        dry_run: bool,
    ) -> Result<Vec<CrdApplyResult>> {
        let mut results = Vec::with_capacity(manifests.len());
        let mut crd_names = Vec::with_capacity(manifests.len());

        // First, apply all CRDs
        for manifest in manifests {
            let result = self.apply_crd(manifest, dry_run).await?;
            crd_names.push(result.name.clone());
            results.push(result);
        }

        // Then wait for all to be established (unless dry-run)
        if !dry_run {
            for name in &crd_names {
                self.wait_for_crd(name, timeout).await?;
            }
        }

        Ok(results)
    }

    /// Count CustomResources of a CRD type
    ///
    /// Used before CRD deletion to warn about data loss.
    pub async fn count_custom_resources(
        &self,
        discovery: &Discovery,
        crd_name: &str,
    ) -> Result<usize> {
        // Parse CRD name: "myresources.example.com" -> plural=myresources, group=example.com
        let parts: Vec<&str> = crd_name.splitn(2, '.').collect();
        if parts.len() != 2 {
            return Ok(0); // Invalid CRD name format
        }

        let plural = parts[0];
        let group = parts[1];

        // Try to find the API resource
        let gvk = GroupVersionKind {
            group: group.to_string(),
            version: "v1".to_string(), // Try v1 first
            kind: plural.to_string(),
        };

        if let Some((ar, _caps)) = discovery.resolve_gvk(&gvk) {
            // Use all_with for both namespaced and cluster-scoped resources
            // to search across all namespaces
            let api: Api<DynamicObject> = Api::all_with(self.client.clone(), &ar);

            match api.list(&Default::default()).await {
                Ok(list) => Ok(list.items.len()),
                Err(_) => Ok(0),
            }
        } else {
            Ok(0)
        }
    }

    /// Delete a CRD from the cluster
    pub async fn delete_crd(&self, name: &str) -> Result<()> {
        use kube::api::DeleteParams;

        let api: Api<DynamicObject> = Api::all_with(
            self.client.clone(),
            &ApiResource::erase::<k8s_openapi::apiextensions_apiserver::pkg::apis::apiextensions::v1::CustomResourceDefinition>(&()),
        );

        api.delete(name, &DeleteParams::default())
            .await
            .map_err(|e| {
                KubeError::InvalidConfig(format!("Failed to delete CRD {}: {}", name, e))
            })?;

        Ok(())
    }
}

/// Result of applying a CRD
#[derive(Debug, Clone)]
pub struct CrdApplyResult {
    /// CRD name
    pub name: String,
    /// Whether it was created (true) or updated (false)
    pub created: bool,
}

impl CrdApplyResult {
    /// Get a display message for this result
    pub fn message(&self) -> String {
        if self.created {
            format!("created CRD {}", self.name)
        } else {
            format!("updated CRD {}", self.name)
        }
    }
}

/// Result of a CRD upgrade operation
#[derive(Debug, Default)]
pub struct CrdUpgradeResult {
    /// CRDs that were successfully applied
    pub applied: Vec<CrdApplyResult>,
    /// CRDs that were skipped (with reasons)
    pub skipped: Vec<(String, String)>,
    /// CRDs that were rejected (with reasons)
    pub rejected: Vec<(String, String)>,
    /// Warning messages
    pub warnings: Vec<String>,
}

impl CrdUpgradeResult {
    /// Create an empty result
    pub fn empty() -> Self {
        Self::default()
    }

    /// Check if all operations succeeded
    pub fn is_success(&self) -> bool {
        self.rejected.is_empty()
    }

    /// Get total count of CRDs processed
    pub fn total(&self) -> usize {
        self.applied.len() + self.skipped.len() + self.rejected.len()
    }

    /// Add an applied CRD
    pub fn add_applied(&mut self, result: CrdApplyResult) {
        self.applied.push(result);
    }

    /// Add a skipped CRD
    pub fn add_skipped(&mut self, name: String, reason: String) {
        self.skipped.push((name, reason));
    }

    /// Add a rejected CRD
    pub fn add_rejected(&mut self, name: String, reason: String) {
        self.rejected.push((name, reason));
    }

    /// Add a warning
    pub fn add_warning(&mut self, warning: String) {
        self.warnings.push(warning);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resource_category_crd() {
        assert_eq!(
            ResourceCategory::from_resource("CustomResourceDefinition", "apiextensions.k8s.io/v1"),
            ResourceCategory::Crd
        );
    }

    #[test]
    fn test_resource_category_namespace() {
        assert_eq!(
            ResourceCategory::from_resource("Namespace", "v1"),
            ResourceCategory::Namespace
        );
    }

    #[test]
    fn test_resource_category_rbac() {
        assert_eq!(
            ResourceCategory::from_resource("ClusterRole", "rbac.authorization.k8s.io/v1"),
            ResourceCategory::ClusterRbac
        );
        assert_eq!(
            ResourceCategory::from_resource("ServiceAccount", "v1"),
            ResourceCategory::NamespacedRbac
        );
    }

    #[test]
    fn test_resource_category_workload() {
        assert_eq!(
            ResourceCategory::from_resource("Deployment", "apps/v1"),
            ResourceCategory::Workload
        );
        assert_eq!(
            ResourceCategory::from_resource("StatefulSet", "apps/v1"),
            ResourceCategory::Workload
        );
    }

    #[test]
    fn test_resource_category_custom_resource() {
        assert_eq!(
            ResourceCategory::from_resource("Certificate", "cert-manager.io/v1"),
            ResourceCategory::CustomResource
        );
        assert_eq!(
            ResourceCategory::from_resource("VirtualService", "networking.istio.io/v1beta1"),
            ResourceCategory::CustomResource
        );
    }

    #[test]
    fn test_resource_category_ordering() {
        assert!(ResourceCategory::Crd < ResourceCategory::Namespace);
        assert!(ResourceCategory::Namespace < ResourceCategory::ClusterRbac);
        assert!(ResourceCategory::ClusterRbac < ResourceCategory::Config);
        assert!(ResourceCategory::Config < ResourceCategory::Workload);
        assert!(ResourceCategory::Workload < ResourceCategory::CustomResource);
        assert!(ResourceCategory::CustomResource < ResourceCategory::Other);
    }

    #[test]
    fn test_is_custom_api_version() {
        // Core APIs
        assert!(!ResourceCategory::is_custom_api_version("v1"));
        assert!(!ResourceCategory::is_custom_api_version("apps/v1"));
        assert!(!ResourceCategory::is_custom_api_version("batch/v1"));

        // Custom APIs
        assert!(ResourceCategory::is_custom_api_version(
            "cert-manager.io/v1"
        ));
        assert!(ResourceCategory::is_custom_api_version(
            "example.com/v1alpha1"
        ));
    }

    #[test]
    fn test_crd_apply_result_message() {
        let created = CrdApplyResult {
            name: "tests.example.com".to_string(),
            created: true,
        };
        assert!(created.message().contains("created"));

        let updated = CrdApplyResult {
            name: "tests.example.com".to_string(),
            created: false,
        };
        assert!(updated.message().contains("updated"));
    }

    #[test]
    fn test_crd_upgrade_result() {
        let mut result = CrdUpgradeResult::default();

        result.add_applied(CrdApplyResult {
            name: "test1.example.com".to_string(),
            created: true,
        });
        result.add_skipped("test2.example.com".to_string(), "skipped".to_string());
        result.add_rejected("test3.example.com".to_string(), "dangerous".to_string());

        assert_eq!(result.total(), 3);
        assert!(!result.is_success());
    }
}
