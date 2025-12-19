//! Sync wave execution system
//!
//! Sync waves provide fine-grained control over resource deployment order,
//! similar to ArgoCD's sync waves but integrated with Sherpack's health checking.
//!
//! Resources are grouped by wave number and applied in order. Each wave waits
//! for the previous wave to be healthy before proceeding.
//!
//! # Example
//!
//! ```yaml
//! apiVersion: apps/v1
//! kind: Deployment
//! metadata:
//!   name: postgres
//!   annotations:
//!     sherpack.io/sync-wave: "0"  # Applied first
//! ---
//! apiVersion: batch/v1
//! kind: Job
//! metadata:
//!   name: db-migrate
//!   annotations:
//!     sherpack.io/sync-wave: "1"  # Waits for wave 0
//!     sherpack.io/wait-for: "Deployment/postgres"  # Explicit dependency
//! ---
//! apiVersion: apps/v1
//! kind: Deployment
//! metadata:
//!   name: app
//!   annotations:
//!     sherpack.io/sync-wave: "2"  # Waits for wave 1
//! ```

use std::collections::{BTreeMap, HashMap, HashSet};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_yaml::Value;

use crate::annotations::{self, ResourceRef};
use crate::error::{KubeError, Result};

/// A parsed Kubernetes resource with its metadata
#[derive(Debug, Clone)]
pub struct Resource {
    /// The full YAML content
    pub yaml: String,
    /// Parsed YAML value
    pub value: Value,
    /// Resource kind
    pub kind: String,
    /// Resource name
    pub name: String,
    /// Resource namespace (if specified)
    pub namespace: Option<String>,
    /// Sync wave number
    pub wave: i32,
    /// Explicit dependencies (wait-for)
    pub dependencies: Vec<ResourceRef>,
    /// Whether this is a hook
    pub is_hook: bool,
    /// Hook phases (if it's a hook)
    pub hook_phases: Vec<String>,
    /// Whether to skip waiting for this resource
    pub skip_wait: bool,
}

impl Resource {
    /// Parse a resource from YAML
    pub fn from_yaml(yaml: &str) -> Result<Self> {
        let value: Value = serde_yaml::from_str(yaml)
            .map_err(|e| KubeError::InvalidManifest(format!("Failed to parse YAML: {}", e)))?;

        let kind = value
            .get("kind")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let metadata = value.get("metadata");
        let name = metadata
            .and_then(|m| m.get("name"))
            .and_then(|n| n.as_str())
            .unwrap_or("")
            .to_string();

        let namespace = metadata
            .and_then(|m| m.get("namespace"))
            .and_then(|n| n.as_str())
            .map(String::from);

        // Parse annotations
        let annotations: BTreeMap<String, String> = metadata
            .and_then(|m| m.get("annotations"))
            .and_then(|a| serde_yaml::from_value(a.clone()).ok())
            .unwrap_or_default();

        let wave = annotations::parse_sync_wave(&annotations);
        let dependencies = annotations::parse_wait_for(&annotations);
        let skip_wait = annotations::should_skip_wait(&annotations);

        // Check if it's a hook
        let hook_value = annotations::get_annotation(
            &annotations,
            annotations::sherpack::HOOK,
            annotations::helm::HOOK,
        );
        let is_hook = hook_value.is_some();
        let hook_phases = hook_value
            .map(annotations::parse_hook_phases)
            .unwrap_or_default();

        Ok(Self {
            yaml: yaml.to_string(),
            value,
            kind,
            name,
            namespace,
            wave,
            dependencies,
            is_hook,
            hook_phases,
            skip_wait,
        })
    }

    /// Get a unique key for this resource
    pub fn key(&self) -> String {
        format!("{}/{}", self.kind, self.name)
    }

    /// Get the resource reference
    pub fn as_ref(&self) -> ResourceRef {
        ResourceRef::new(&self.kind, &self.name)
    }
}

/// A wave of resources to apply together
#[derive(Debug, Clone)]
pub struct Wave {
    /// Wave number
    pub number: i32,
    /// Resources in this wave (non-hooks)
    pub resources: Vec<Resource>,
}

impl Wave {
    /// Check if this wave is empty
    pub fn is_empty(&self) -> bool {
        self.resources.is_empty()
    }

    /// Get all resource keys in this wave
    pub fn resource_keys(&self) -> Vec<String> {
        self.resources.iter().map(|r| r.key()).collect()
    }
}

/// Execution plan for a release
#[derive(Debug, Clone)]
pub struct ExecutionPlan {
    /// Resources grouped by wave number
    pub waves: Vec<Wave>,
    /// Hooks organized by phase
    pub hooks: HashMap<String, Vec<Resource>>,
    /// Explicit dependencies (wait-for annotations)
    pub dependencies: HashMap<String, Vec<ResourceRef>>,
    /// All resources by key for quick lookup
    resource_index: HashMap<String, Resource>,
}

impl ExecutionPlan {
    /// Build an execution plan from a rendered manifest
    pub fn from_manifest(manifest: &str) -> Result<Self> {
        let mut waves_map: BTreeMap<i32, Vec<Resource>> = BTreeMap::new();
        let mut hooks: HashMap<String, Vec<Resource>> = HashMap::new();
        let mut dependencies: HashMap<String, Vec<ResourceRef>> = HashMap::new();
        let mut resource_index: HashMap<String, Resource> = HashMap::new();

        // Parse each document in the manifest
        for doc in manifest.split("---") {
            let doc = doc.trim();
            if doc.is_empty() {
                continue;
            }

            let resource = match Resource::from_yaml(doc) {
                Ok(r) => r,
                Err(_) => continue, // Skip unparseable documents
            };

            if resource.name.is_empty() {
                continue; // Skip resources without names
            }

            let key = resource.key();

            // Store dependencies
            if !resource.dependencies.is_empty() {
                dependencies.insert(key.clone(), resource.dependencies.clone());
            }

            // Store in index
            resource_index.insert(key.clone(), resource.clone());

            if resource.is_hook {
                // Group hooks by phase
                for phase in &resource.hook_phases {
                    hooks
                        .entry(phase.clone())
                        .or_default()
                        .push(resource.clone());
                }
            } else {
                // Group non-hooks by wave
                waves_map.entry(resource.wave).or_default().push(resource);
            }
        }

        // Sort hooks by weight within each phase
        for hooks_list in hooks.values_mut() {
            hooks_list.sort_by_key(|h| {
                let annotations: BTreeMap<String, String> = h
                    .value
                    .get("metadata")
                    .and_then(|m| m.get("annotations"))
                    .and_then(|a| serde_yaml::from_value(a.clone()).ok())
                    .unwrap_or_default();
                annotations::parse_hook_weight(&annotations)
            });
        }

        // Convert to Wave structs
        let waves: Vec<Wave> = waves_map
            .into_iter()
            .map(|(number, resources)| Wave { number, resources })
            .collect();

        Ok(Self {
            waves,
            hooks,
            dependencies,
            resource_index,
        })
    }

    /// Get all resources (non-hooks) in order
    pub fn all_resources(&self) -> Vec<&Resource> {
        self.waves.iter().flat_map(|w| w.resources.iter()).collect()
    }

    /// Get hooks for a specific phase
    pub fn hooks_for_phase(&self, phase: &str) -> Vec<&Resource> {
        self.hooks
            .get(phase)
            .map(|h| h.iter().collect())
            .unwrap_or_default()
    }

    /// Get a resource by key
    pub fn get_resource(&self, key: &str) -> Option<&Resource> {
        self.resource_index.get(key)
    }

    /// Check if all dependencies for a resource are satisfied
    pub fn dependencies_satisfied(&self, key: &str, ready_resources: &HashSet<String>) -> bool {
        match self.dependencies.get(key) {
            Some(deps) => deps
                .iter()
                .all(|dep| ready_resources.contains(&dep.to_string())),
            None => true,
        }
    }

    /// Get wave count
    pub fn wave_count(&self) -> usize {
        self.waves.len()
    }

    /// Get total resource count (excluding hooks)
    pub fn resource_count(&self) -> usize {
        self.waves.iter().map(|w| w.resources.len()).sum()
    }

    /// Get hook count for a phase
    pub fn hook_count(&self, phase: &str) -> usize {
        self.hooks.get(phase).map(|h| h.len()).unwrap_or(0)
    }

    /// Generate a summary of the execution plan
    pub fn summary(&self) -> ExecutionPlanSummary {
        let wave_summaries: Vec<WaveSummary> = self
            .waves
            .iter()
            .map(|w| WaveSummary {
                number: w.number,
                resource_count: w.resources.len(),
                resources: w.resources.iter().map(|r| r.key()).collect(),
            })
            .collect();

        let hook_counts: HashMap<String, usize> = self
            .hooks
            .iter()
            .map(|(phase, hooks)| (phase.clone(), hooks.len()))
            .collect();

        ExecutionPlanSummary {
            waves: wave_summaries,
            hooks: hook_counts,
            total_resources: self.resource_count(),
            total_waves: self.wave_count(),
        }
    }
}

/// Summary of an execution plan for display
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionPlanSummary {
    pub waves: Vec<WaveSummary>,
    pub hooks: HashMap<String, usize>,
    pub total_resources: usize,
    pub total_waves: usize,
}

impl ExecutionPlanSummary {
    /// Format as a human-readable string
    pub fn display(&self) -> String {
        let mut lines = vec![format!(
            "Execution Plan: {} resources in {} waves",
            self.total_resources, self.total_waves
        )];

        for wave in &self.waves {
            lines.push(format!(
                "  Wave {}: {} resources",
                wave.number, wave.resource_count
            ));
            for resource in &wave.resources {
                lines.push(format!("    - {}", resource));
            }
        }

        if !self.hooks.is_empty() {
            lines.push("  Hooks:".to_string());
            for (phase, count) in &self.hooks {
                lines.push(format!("    - {}: {} hooks", phase, count));
            }
        }

        lines.join("\n")
    }
}

/// Summary of a single wave
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WaveSummary {
    pub number: i32,
    pub resource_count: usize,
    pub resources: Vec<String>,
}

/// Configuration for wave execution
#[derive(Debug, Clone)]
pub struct WaveExecutionConfig {
    /// Whether to wait for resources to be ready
    pub wait: bool,
    /// Timeout for waiting
    pub timeout: Duration,
    /// Whether to use atomic mode (rollback on failure)
    pub atomic: bool,
    /// Whether to show progress
    pub show_progress: bool,
}

impl Default for WaveExecutionConfig {
    fn default() -> Self {
        Self {
            wait: true,
            timeout: Duration::from_secs(300),
            atomic: false,
            show_progress: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_resource() {
        let yaml = r#"
apiVersion: apps/v1
kind: Deployment
metadata:
  name: my-app
  namespace: production
  annotations:
    sherpack.io/sync-wave: "1"
spec:
  replicas: 3
"#;

        let resource = Resource::from_yaml(yaml).unwrap();
        assert_eq!(resource.kind, "Deployment");
        assert_eq!(resource.name, "my-app");
        assert_eq!(resource.namespace, Some("production".to_string()));
        assert_eq!(resource.wave, 1);
        assert!(!resource.is_hook);
    }

    #[test]
    fn test_parse_hook() {
        let yaml = r#"
apiVersion: batch/v1
kind: Job
metadata:
  name: migrate
  annotations:
    helm.sh/hook: pre-install,pre-upgrade
    helm.sh/hook-weight: "5"
spec:
  template:
    spec:
      containers:
        - name: migrate
          image: migrate:latest
"#;

        let resource = Resource::from_yaml(yaml).unwrap();
        assert_eq!(resource.kind, "Job");
        assert_eq!(resource.name, "migrate");
        assert!(resource.is_hook);
        assert_eq!(resource.hook_phases, vec!["pre-install", "pre-upgrade"]);
    }

    #[test]
    fn test_parse_wait_for() {
        let yaml = r#"
apiVersion: apps/v1
kind: Deployment
metadata:
  name: app
  annotations:
    sherpack.io/sync-wave: "2"
    sherpack.io/wait-for: "Deployment/postgres,Service/redis"
spec:
  replicas: 1
"#;

        let resource = Resource::from_yaml(yaml).unwrap();
        assert_eq!(resource.wave, 2);
        assert_eq!(resource.dependencies.len(), 2);
        assert_eq!(resource.dependencies[0].kind, "Deployment");
        assert_eq!(resource.dependencies[0].name, "postgres");
    }

    #[test]
    fn test_execution_plan() {
        let manifest = r#"
apiVersion: apps/v1
kind: Deployment
metadata:
  name: postgres
  annotations:
    sherpack.io/sync-wave: "0"
---
apiVersion: batch/v1
kind: Job
metadata:
  name: migrate
  annotations:
    sherpack.io/hook: post-install
    sherpack.io/sync-wave: "1"
---
apiVersion: apps/v1
kind: Deployment
metadata:
  name: app
  annotations:
    sherpack.io/sync-wave: "2"
"#;

        let plan = ExecutionPlan::from_manifest(manifest).unwrap();

        // Should have 2 waves (wave 0 and wave 2, since wave 1 is a hook)
        assert_eq!(plan.wave_count(), 2);
        assert_eq!(plan.resource_count(), 2);
        assert_eq!(plan.hook_count("post-install"), 1);
    }

    #[test]
    fn test_wave_ordering() {
        let manifest = r#"
apiVersion: apps/v1
kind: Deployment
metadata:
  name: third
  annotations:
    sherpack.io/sync-wave: "10"
---
apiVersion: apps/v1
kind: Deployment
metadata:
  name: first
  annotations:
    sherpack.io/sync-wave: "-5"
---
apiVersion: apps/v1
kind: Deployment
metadata:
  name: second
  annotations:
    sherpack.io/sync-wave: "0"
"#;

        let plan = ExecutionPlan::from_manifest(manifest).unwrap();

        assert_eq!(plan.waves.len(), 3);
        assert_eq!(plan.waves[0].number, -5);
        assert_eq!(plan.waves[0].resources[0].name, "first");
        assert_eq!(plan.waves[1].number, 0);
        assert_eq!(plan.waves[1].resources[0].name, "second");
        assert_eq!(plan.waves[2].number, 10);
        assert_eq!(plan.waves[2].resources[0].name, "third");
    }

    #[test]
    fn test_summary_display() {
        let manifest = r#"
apiVersion: apps/v1
kind: Deployment
metadata:
  name: db
  annotations:
    sherpack.io/sync-wave: "0"
---
apiVersion: apps/v1
kind: Deployment
metadata:
  name: app
  annotations:
    sherpack.io/sync-wave: "1"
"#;

        let plan = ExecutionPlan::from_manifest(manifest).unwrap();
        let summary = plan.summary();
        let display = summary.display();

        assert!(display.contains("2 resources"));
        assert!(display.contains("2 waves"));
        assert!(display.contains("Deployment/db"));
        assert!(display.contains("Deployment/app"));
    }

    #[test]
    fn test_default_wave() {
        let yaml = r#"
apiVersion: apps/v1
kind: Deployment
metadata:
  name: no-wave
spec:
  replicas: 1
"#;

        let resource = Resource::from_yaml(yaml).unwrap();
        assert_eq!(resource.wave, 0); // Default wave
    }

    #[test]
    fn test_skip_wait() {
        let yaml = r#"
apiVersion: v1
kind: ConfigMap
metadata:
  name: config
  annotations:
    sherpack.io/skip-wait: "true"
data:
  key: value
"#;

        let resource = Resource::from_yaml(yaml).unwrap();
        assert!(resource.skip_wait);
    }
}
