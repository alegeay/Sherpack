//! Diff engine for comparing releases and detecting cluster drift
//!
//! ## Key Features
//!
//! - **Release comparison**: Compare two release manifests (like `helm diff`)
//! - **Drift detection**: Compare release manifest with actual cluster state
//! - **Three-way merge**: Compare desired vs last-applied vs live state
//! - **Server-managed field filtering**: Ignore K8s-managed fields like `resourceVersion`
//! - **Structured output**: Color-coded diffs with context
//!
//! ## Addressing Helm Frustrations
//!
//! This implementation fixes several known issues with Helm's diff:
//!
//! 1. **helm diff only compares revisions** - We support live cluster comparison
//! 2. **managedFields noise** - We filter server-managed fields automatically
//! 3. **No three-way merge** - We support comparing desired/last-applied/live
//! 4. **Poor output** - We provide grouped, color-coded, contextual diffs
//!
//! ## Example
//!
//! ```ignore
//! let engine = DiffEngine::new();
//!
//! // Compare two releases
//! let diff = engine.diff_releases(&old_release, &new_release);
//!
//! // Detect drift from cluster state
//! let drift = engine.detect_drift(&release, &client).await?;
//!
//! // Three-way comparison
//! let three_way = engine.three_way_diff(&desired, &last_applied, &live);
//! ```

use kube::api::{Api, DynamicObject, GroupVersionKind, ListParams};
use kube::discovery::{ApiResource, Scope};
use kube::{Client, Discovery};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use similar::{ChangeTag, TextDiff};
use std::collections::{BTreeMap, HashMap, HashSet};

use crate::error::{KubeError, Result};
use crate::release::StoredRelease;

/// Fields to strip when normalizing resources for comparison
/// These are server-managed and not part of the desired state
const SERVER_MANAGED_FIELDS: &[&str] = &[
    "metadata.managedFields",
    "metadata.resourceVersion",
    "metadata.uid",
    "metadata.generation",
    "metadata.creationTimestamp",
    "metadata.selfLink",
    "metadata.deletionTimestamp",
    "metadata.deletionGracePeriodSeconds",
    "metadata.ownerReferences", // Often set by controllers
];

/// Annotations to strip when comparing (set by kubectl/helm)
const IGNORED_ANNOTATIONS: &[&str] = &[
    "kubectl.kubernetes.io/last-applied-configuration",
    "deployment.kubernetes.io/revision",
    "meta.helm.sh/release-name",
    "meta.helm.sh/release-namespace",
];

/// Labels to optionally ignore when comparing
const OPTIONALLY_IGNORED_LABELS: &[&str] = &["app.kubernetes.io/managed-by", "helm.sh/chart"];

/// Diff engine for release comparison and drift detection
pub struct DiffEngine {
    /// Show context lines around changes
    pub context_lines: usize,
    /// Ignore status fields entirely
    pub ignore_status: bool,
    /// Additional JSON paths to ignore
    pub ignore_paths: HashSet<String>,
    /// Ignore label differences for managed-by labels
    pub ignore_management_labels: bool,
}

impl DiffEngine {
    /// Create a new diff engine with default settings
    pub fn new() -> Self {
        Self {
            context_lines: 3,
            ignore_status: true,
            ignore_paths: HashSet::new(),
            ignore_management_labels: true,
        }
    }

    /// Set the number of context lines
    pub fn with_context(mut self, lines: usize) -> Self {
        self.context_lines = lines;
        self
    }

    /// Include status fields in comparison
    pub fn include_status(mut self) -> Self {
        self.ignore_status = false;
        self
    }

    /// Add a JSON path to ignore
    pub fn ignore_path(mut self, path: &str) -> Self {
        self.ignore_paths.insert(path.to_string());
        self
    }

    /// Compare two releases
    pub fn diff_releases(&self, old: &StoredRelease, new: &StoredRelease) -> DiffResult {
        let old_resources = parse_manifest_resources(&old.manifest);
        let new_resources = parse_manifest_resources(&new.manifest);

        let mut changes = Vec::new();

        // Find added and modified resources
        for (key, new_content) in &new_resources {
            match old_resources.get(key) {
                Some(old_content) => {
                    // Normalize both for comparison
                    let old_normalized = self.normalize_resource(old_content);
                    let new_normalized = self.normalize_resource(new_content);

                    if old_normalized != new_normalized {
                        changes.push(ResourceChange {
                            kind: key.kind.clone(),
                            api_version: key.api_version.clone(),
                            name: key.name.clone(),
                            namespace: key.namespace.clone(),
                            change_type: ChangeType::Modified,
                            diff: Some(self.compute_text_diff(&old_normalized, &new_normalized)),
                            is_drift: false,
                            source: DiffSource::ReleaseComparison,
                        });
                    }
                }
                None => {
                    changes.push(ResourceChange {
                        kind: key.kind.clone(),
                        api_version: key.api_version.clone(),
                        name: key.name.clone(),
                        namespace: key.namespace.clone(),
                        change_type: ChangeType::Added,
                        diff: Some(DiffContent::new_addition(new_content)),
                        is_drift: false,
                        source: DiffSource::ReleaseComparison,
                    });
                }
            }
        }

        // Find removed resources
        for (key, old_content) in &old_resources {
            if !new_resources.contains_key(key) {
                changes.push(ResourceChange {
                    kind: key.kind.clone(),
                    api_version: key.api_version.clone(),
                    name: key.name.clone(),
                    namespace: key.namespace.clone(),
                    change_type: ChangeType::Removed,
                    diff: Some(DiffContent::new_removal(old_content)),
                    is_drift: false,
                    source: DiffSource::ReleaseComparison,
                });
            }
        }

        // Sort changes for consistent output
        changes.sort_by(|a, b| {
            a.kind
                .cmp(&b.kind)
                .then_with(|| a.namespace.cmp(&b.namespace))
                .then_with(|| a.name.cmp(&b.name))
        });

        DiffResult {
            old_version: old.version,
            new_version: new.version,
            changes,
            has_drift: false,
        }
    }

    /// Compare a release manifest with actual cluster state
    ///
    /// This addresses the Helm frustration where `helm diff` only compares
    /// revisions, not actual cluster state. Manual changes (drift) are detected.
    pub async fn detect_drift(
        &self,
        release: &StoredRelease,
        client: &Client,
    ) -> Result<DiffResult> {
        let manifest_resources = parse_manifest_resources(&release.manifest);
        let mut changes = Vec::new();

        // Discover available APIs
        let discovery = Discovery::new(client.clone())
            .run()
            .await
            .map_err(|e| KubeError::Storage(format!("API discovery failed: {}", e)))?;

        for (key, manifest_content) in &manifest_resources {
            match self
                .fetch_live_resource(client, &discovery, key, &release.namespace)
                .await
            {
                Ok(Some(live_yaml)) => {
                    // Normalize both for comparison
                    let manifest_normalized = self.normalize_resource(manifest_content);
                    let live_normalized = self.normalize_resource(&live_yaml);

                    if manifest_normalized != live_normalized {
                        changes.push(ResourceChange {
                            kind: key.kind.clone(),
                            api_version: key.api_version.clone(),
                            name: key.name.clone(),
                            namespace: key.namespace.clone(),
                            change_type: ChangeType::Modified,
                            diff: Some(
                                self.compute_text_diff(&manifest_normalized, &live_normalized),
                            ),
                            is_drift: true, // This is drift - cluster differs from release
                            source: DiffSource::ClusterDrift,
                        });
                    }
                }
                Ok(None) => {
                    // Resource exists in manifest but not in cluster
                    changes.push(ResourceChange {
                        kind: key.kind.clone(),
                        api_version: key.api_version.clone(),
                        name: key.name.clone(),
                        namespace: key.namespace.clone(),
                        change_type: ChangeType::Missing,
                        diff: Some(DiffContent::new_removal(manifest_content)),
                        is_drift: true,
                        source: DiffSource::ClusterDrift,
                    });
                }
                Err(e) => {
                    // API error - might be a deprecated API or permissions issue
                    changes.push(ResourceChange {
                        kind: key.kind.clone(),
                        api_version: key.api_version.clone(),
                        name: key.name.clone(),
                        namespace: key.namespace.clone(),
                        change_type: ChangeType::Unknown,
                        diff: None,
                        is_drift: false,
                        source: DiffSource::Error(e.to_string()),
                    });
                }
            }
        }

        // Check for extra resources in cluster that aren't in manifest
        // (resources that might have been added manually)
        let extra_resources = self
            .find_extra_cluster_resources(client, &discovery, release, &manifest_resources)
            .await?;

        for (key, live_yaml) in extra_resources {
            changes.push(ResourceChange {
                kind: key.kind.clone(),
                api_version: key.api_version.clone(),
                name: key.name.clone(),
                namespace: key.namespace.clone(),
                change_type: ChangeType::Extra,
                diff: Some(DiffContent::new_addition(&live_yaml)),
                is_drift: true,
                source: DiffSource::ClusterDrift,
            });
        }

        // Sort changes
        changes.sort_by(|a, b| {
            a.kind
                .cmp(&b.kind)
                .then_with(|| a.namespace.cmp(&b.namespace))
                .then_with(|| a.name.cmp(&b.name))
        });

        let has_drift = changes.iter().any(|c| c.is_drift);

        Ok(DiffResult {
            old_version: release.version,
            new_version: release.version,
            changes,
            has_drift,
        })
    }

    /// Three-way diff: desired vs last-applied vs live
    ///
    /// This provides the most complete picture:
    /// - What you want to apply (desired)
    /// - What was last applied (stored release)
    /// - What's actually in the cluster (live)
    ///
    /// Returns both "what will change" and "what has drifted"
    pub async fn three_way_diff(
        &self,
        desired_manifest: &str,
        last_applied: &StoredRelease,
        client: &Client,
    ) -> Result<ThreeWayDiffResult> {
        let desired_resources = parse_manifest_resources(desired_manifest);
        let last_applied_resources = parse_manifest_resources(&last_applied.manifest);

        let discovery = Discovery::new(client.clone())
            .run()
            .await
            .map_err(|e| KubeError::Storage(format!("API discovery failed: {}", e)))?;

        let mut changes = Vec::new();

        // Collect all unique resource keys
        let mut all_keys: HashSet<&ResourceKey> = HashSet::new();
        all_keys.extend(desired_resources.keys());
        all_keys.extend(last_applied_resources.keys());

        for key in all_keys {
            let desired = desired_resources.get(key);
            let last = last_applied_resources.get(key);
            let live = self
                .fetch_live_resource(client, &discovery, key, &last_applied.namespace)
                .await
                .ok()
                .flatten();

            let change = self.compute_three_way_change(key, desired, last, live.as_deref());
            if let Some(c) = change {
                changes.push(c);
            }
        }

        // Sort changes
        changes.sort_by(|a, b| {
            a.kind
                .cmp(&b.kind)
                .then_with(|| a.namespace.cmp(&b.namespace))
                .then_with(|| a.name.cmp(&b.name))
        });

        let has_pending_changes = changes.iter().any(|c| {
            matches!(
                c.change_type,
                ChangeType::Added | ChangeType::Modified | ChangeType::Removed
            )
        });
        let has_drift = changes.iter().any(|c| c.is_drift);

        Ok(ThreeWayDiffResult {
            changes,
            has_pending_changes,
            has_drift,
        })
    }

    /// Compute a three-way change for a single resource
    fn compute_three_way_change(
        &self,
        key: &ResourceKey,
        desired: Option<&String>,
        last_applied: Option<&String>,
        live: Option<&str>,
    ) -> Option<ResourceChange> {
        match (desired, last_applied, live) {
            // New resource (in desired, not in last or live)
            (Some(d), None, None) => Some(ResourceChange {
                kind: key.kind.clone(),
                api_version: key.api_version.clone(),
                name: key.name.clone(),
                namespace: key.namespace.clone(),
                change_type: ChangeType::Added,
                diff: Some(DiffContent::new_addition(d)),
                is_drift: false,
                source: DiffSource::ThreeWay,
            }),

            // Resource to be removed (not in desired, was in last)
            (None, Some(l), _) => Some(ResourceChange {
                kind: key.kind.clone(),
                api_version: key.api_version.clone(),
                name: key.name.clone(),
                namespace: key.namespace.clone(),
                change_type: ChangeType::Removed,
                diff: Some(DiffContent::new_removal(l)),
                is_drift: false,
                source: DiffSource::ThreeWay,
            }),

            // Resource modified (in both desired and last)
            (Some(d), Some(l), live_opt) => {
                let d_norm = self.normalize_resource(d);
                let l_norm = self.normalize_resource(l);

                let will_change = d_norm != l_norm;
                let has_drift = live_opt
                    .map(|live| self.normalize_resource(live) != l_norm)
                    .unwrap_or(false);

                if will_change || has_drift {
                    let diff_target = live_opt.unwrap_or(l.as_str());
                    Some(ResourceChange {
                        kind: key.kind.clone(),
                        api_version: key.api_version.clone(),
                        name: key.name.clone(),
                        namespace: key.namespace.clone(),
                        change_type: if will_change {
                            ChangeType::Modified
                        } else {
                            ChangeType::Unchanged
                        },
                        diff: if will_change {
                            Some(
                                self.compute_text_diff(
                                    &self.normalize_resource(diff_target),
                                    &d_norm,
                                ),
                            )
                        } else {
                            None
                        },
                        is_drift: has_drift,
                        source: DiffSource::ThreeWay,
                    })
                } else {
                    None // No changes
                }
            }

            // Extra resource in cluster (not in desired or last, but exists)
            (None, None, Some(live)) => Some(ResourceChange {
                kind: key.kind.clone(),
                api_version: key.api_version.clone(),
                name: key.name.clone(),
                namespace: key.namespace.clone(),
                change_type: ChangeType::Extra,
                diff: Some(DiffContent::new_addition(live)),
                is_drift: true,
                source: DiffSource::ThreeWay,
            }),

            // Nothing anywhere
            (None, None, None) => None,

            // Resource only in desired (new)
            (Some(d), None, Some(_)) => Some(ResourceChange {
                kind: key.kind.clone(),
                api_version: key.api_version.clone(),
                name: key.name.clone(),
                namespace: key.namespace.clone(),
                change_type: ChangeType::Added,
                diff: Some(DiffContent::new_addition(d)),
                is_drift: false, // Adopting existing resource
                source: DiffSource::ThreeWay,
            }),
        }
    }

    /// Fetch a live resource from the cluster
    async fn fetch_live_resource(
        &self,
        client: &Client,
        discovery: &Discovery,
        key: &ResourceKey,
        default_namespace: &str,
    ) -> Result<Option<String>> {
        // Parse apiVersion to get group and version
        let (group, version) = parse_api_version(&key.api_version);

        // Find the API resource in discovery
        let gvk = GroupVersionKind::gvk(&group, &version, &key.kind);

        let (ar, caps) = match discovery.resolve_gvk(&gvk) {
            Some(r) => r,
            None => {
                // API not found - might be deprecated or CRD not installed
                return Err(KubeError::Storage(format!(
                    "API {}/{} {} not found in cluster",
                    group, version, key.kind
                )));
            }
        };

        let namespace = key.namespace.as_deref().unwrap_or(default_namespace);

        // Create dynamic API
        let api: Api<DynamicObject> = if caps.scope == Scope::Cluster {
            Api::all_with(client.clone(), &ar)
        } else {
            Api::namespaced_with(client.clone(), namespace, &ar)
        };

        // Fetch the resource
        match api.get(&key.name).await {
            Ok(obj) => {
                // Serialize back to YAML
                let yaml = serde_yaml::to_string(&obj)
                    .map_err(|e| KubeError::Serialization(e.to_string()))?;
                Ok(Some(yaml))
            }
            Err(kube::Error::Api(e)) if e.code == 404 => Ok(None),
            Err(e) => Err(KubeError::Storage(format!(
                "Failed to fetch {}/{}: {}",
                key.kind, key.name, e
            ))),
        }
    }

    /// Find extra resources in cluster managed by this release but not in manifest
    async fn find_extra_cluster_resources(
        &self,
        client: &Client,
        _discovery: &Discovery,
        release: &StoredRelease,
        manifest_resources: &HashMap<ResourceKey, String>,
    ) -> Result<Vec<(ResourceKey, String)>> {
        let mut extra = Vec::new();

        // Query for resources with Sherpack labels matching this release
        let label_selector = format!(
            "app.kubernetes.io/managed-by=sherpack,sherpack.io/release-name={}",
            release.name
        );

        // Check common resource types for extras
        // This is a simplified approach - a full implementation would check all types
        let resource_types = [
            ("v1", "ConfigMap"),
            ("v1", "Secret"),
            ("v1", "Service"),
            ("apps/v1", "Deployment"),
            ("apps/v1", "StatefulSet"),
            ("apps/v1", "DaemonSet"),
            ("batch/v1", "Job"),
            ("batch/v1", "CronJob"),
        ];

        for (api_version, kind) in resource_types {
            let ar = match kind {
                "ConfigMap" => ApiResource::erase::<k8s_openapi::api::core::v1::ConfigMap>(&()),
                "Secret" => ApiResource::erase::<k8s_openapi::api::core::v1::Secret>(&()),
                "Service" => ApiResource::erase::<k8s_openapi::api::core::v1::Service>(&()),
                "Deployment" => ApiResource::erase::<k8s_openapi::api::apps::v1::Deployment>(&()),
                "StatefulSet" => ApiResource::erase::<k8s_openapi::api::apps::v1::StatefulSet>(&()),
                "DaemonSet" => ApiResource::erase::<k8s_openapi::api::apps::v1::DaemonSet>(&()),
                "Job" => ApiResource::erase::<k8s_openapi::api::batch::v1::Job>(&()),
                "CronJob" => ApiResource::erase::<k8s_openapi::api::batch::v1::CronJob>(&()),
                _ => continue,
            };

            let api: Api<DynamicObject> =
                Api::namespaced_with(client.clone(), &release.namespace, &ar);

            let lp = ListParams::default().labels(&label_selector);

            match api.list(&lp).await {
                Ok(list) => {
                    for obj in list.items {
                        let name = obj.metadata.name.clone().unwrap_or_default();
                        let namespace = obj.metadata.namespace.clone();

                        let key = ResourceKey {
                            api_version: api_version.to_string(),
                            kind: kind.to_string(),
                            name: name.clone(),
                            namespace,
                        };

                        // Check if this resource is in the manifest
                        if !manifest_resources.contains_key(&key) {
                            let yaml = serde_yaml::to_string(&obj).unwrap_or_default();
                            extra.push((key, yaml));
                        }
                    }
                }
                Err(_) => continue, // Skip on error (permissions, etc.)
            }
        }

        Ok(extra)
    }

    /// Normalize a resource for comparison by stripping server-managed fields
    fn normalize_resource(&self, content: &str) -> String {
        // Parse as JSON for easier manipulation
        let mut value: JsonValue = match serde_yaml::from_str(content) {
            Ok(v) => v,
            Err(_) => return content.to_string(),
        };

        // Remove server-managed fields
        self.strip_server_managed_fields(&mut value);

        // Remove ignored annotations
        self.strip_ignored_annotations(&mut value);

        // Optionally remove management labels
        if self.ignore_management_labels {
            self.strip_management_labels(&mut value);
        }

        // Remove status if configured
        if self.ignore_status
            && let Some(obj) = value.as_object_mut()
        {
            obj.remove("status");
        }

        // Remove custom ignored paths
        for path in &self.ignore_paths {
            self.remove_json_path(&mut value, path);
        }

        // Re-serialize to YAML with sorted keys for consistent comparison
        serde_yaml::to_string(&value).unwrap_or_else(|_| content.to_string())
    }

    /// Strip server-managed fields from a resource
    fn strip_server_managed_fields(&self, value: &mut JsonValue) {
        for path in SERVER_MANAGED_FIELDS {
            self.remove_json_path(value, path);
        }
    }

    /// Strip ignored annotations
    fn strip_ignored_annotations(&self, value: &mut JsonValue) {
        if let Some(metadata) = value.get_mut("metadata").and_then(|m| m.as_object_mut())
            && let Some(annotations) = metadata
                .get_mut("annotations")
                .and_then(|a| a.as_object_mut())
        {
            for annotation in IGNORED_ANNOTATIONS {
                annotations.remove(*annotation);
            }
            // Remove annotations entirely if empty
            if annotations.is_empty() {
                metadata.remove("annotations");
            }
        }
    }

    /// Strip management labels if configured
    fn strip_management_labels(&self, value: &mut JsonValue) {
        if let Some(metadata) = value.get_mut("metadata").and_then(|m| m.as_object_mut())
            && let Some(labels) = metadata.get_mut("labels").and_then(|l| l.as_object_mut())
        {
            for label in OPTIONALLY_IGNORED_LABELS {
                labels.remove(*label);
            }
        }
    }

    /// Remove a JSON path from a value
    fn remove_json_path(&self, value: &mut JsonValue, path: &str) {
        let parts: Vec<&str> = path.split('.').collect();
        Self::remove_path_recursive(value, &parts);
    }

    fn remove_path_recursive(value: &mut JsonValue, path: &[&str]) {
        if path.is_empty() {
            return;
        }

        if path.len() == 1 {
            if let Some(obj) = value.as_object_mut() {
                obj.remove(path[0]);
            }
            return;
        }

        if let Some(obj) = value.as_object_mut()
            && let Some(child) = obj.get_mut(path[0])
        {
            Self::remove_path_recursive(child, &path[1..]);
        }
    }

    /// Compute a text diff between two strings
    fn compute_text_diff(&self, old: &str, new: &str) -> DiffContent {
        let diff = TextDiff::from_lines(old, new);
        let mut lines = Vec::new();

        for change in diff.iter_all_changes() {
            let line_type = match change.tag() {
                ChangeTag::Delete => LineType::Removed,
                ChangeTag::Insert => LineType::Added,
                ChangeTag::Equal => LineType::Context,
            };

            lines.push(DiffLine {
                line_type,
                content: change.value().trim_end().to_string(),
                old_line_no: change.old_index(),
                new_line_no: change.new_index(),
            });
        }

        DiffContent { lines }
    }

    /// Generate a human-readable summary
    pub fn summary(&self, result: &DiffResult) -> String {
        let added = result
            .changes
            .iter()
            .filter(|c| c.change_type == ChangeType::Added)
            .count();
        let modified = result
            .changes
            .iter()
            .filter(|c| c.change_type == ChangeType::Modified)
            .count();
        let removed = result
            .changes
            .iter()
            .filter(|c| c.change_type == ChangeType::Removed)
            .count();
        let drift = result.changes.iter().filter(|c| c.is_drift).count();
        let missing = result
            .changes
            .iter()
            .filter(|c| c.change_type == ChangeType::Missing)
            .count();
        let extra = result
            .changes
            .iter()
            .filter(|c| c.change_type == ChangeType::Extra)
            .count();

        let mut parts = Vec::new();

        if added > 0 {
            parts.push(format!("{} added", added));
        }
        if modified > 0 {
            parts.push(format!("{} modified", modified));
        }
        if removed > 0 {
            parts.push(format!("{} removed", removed));
        }
        if missing > 0 {
            parts.push(format!("{} missing", missing));
        }
        if extra > 0 {
            parts.push(format!("{} extra", extra));
        }
        if drift > 0 {
            parts.push(format!("{} drifted", drift));
        }

        if parts.is_empty() {
            "No changes".to_string()
        } else {
            parts.join(", ")
        }
    }

    /// Format diff result for terminal output with colors
    pub fn format_colored(&self, result: &DiffResult) -> String {
        use console::{Style, style};

        let mut output = String::new();

        // Header
        if result.old_version != result.new_version {
            output.push_str(&format!(
                "{}\n\n",
                style(format!(
                    "Comparing release v{} → v{}",
                    result.old_version, result.new_version
                ))
                .bold()
            ));
        } else if result.has_drift {
            output.push_str(&format!(
                "{}\n\n",
                style(format!(
                    "Drift detected in release v{} vs cluster state",
                    result.old_version
                ))
                .bold()
                .yellow()
            ));
        }

        // Summary
        output.push_str(&format!("{}\n\n", self.summary(result)));

        // Group changes by type
        let mut by_type: BTreeMap<ChangeType, Vec<&ResourceChange>> = BTreeMap::new();
        for change in &result.changes {
            by_type.entry(change.change_type).or_default().push(change);
        }

        // Output each group
        for (change_type, changes) in by_type {
            let (header_style, symbol) = match change_type {
                ChangeType::Added => (Style::new().green().bold(), "+"),
                ChangeType::Modified => (Style::new().yellow().bold(), "~"),
                ChangeType::Removed => (Style::new().red().bold(), "-"),
                ChangeType::Missing => (Style::new().red().bold(), "?"),
                ChangeType::Extra => (Style::new().cyan().bold(), "!"),
                ChangeType::Unchanged => (Style::new().dim(), "="),
                ChangeType::Unknown => (Style::new().dim(), "?"),
            };

            output.push_str(&format!(
                "{}\n",
                header_style.apply_to(format!("=== {} ({}) ===", change_type, changes.len()))
            ));

            for change in changes {
                let drift_marker = if change.is_drift {
                    style(" [DRIFT]").yellow().to_string()
                } else {
                    String::new()
                };

                output.push_str(&format!(
                    "  {} {}{}\n",
                    style(symbol).bold(),
                    change.display_name(),
                    drift_marker
                ));

                // Show diff if available and not too large
                if let Some(diff) = &change.diff {
                    if diff.lines.len() <= 50 {
                        for line in &diff.lines {
                            let (prefix, line_style) = match line.line_type {
                                LineType::Added => ("+", Style::new().green()),
                                LineType::Removed => ("-", Style::new().red()),
                                LineType::Context => (" ", Style::new().dim()),
                            };
                            output.push_str(&format!(
                                "    {}{}\n",
                                prefix,
                                line_style.apply_to(&line.content)
                            ));
                        }
                    } else {
                        output.push_str(&format!(
                            "    {} ({} lines, use --verbose for full diff)\n",
                            style("...").dim(),
                            diff.lines.len()
                        ));
                    }
                }
            }
            output.push('\n');
        }

        output
    }
}

impl Default for DiffEngine {
    fn default() -> Self {
        Self::new()
    }
}

/// Parse apiVersion into (group, version)
fn parse_api_version(api_version: &str) -> (String, String) {
    if let Some((group, version)) = api_version.split_once('/') {
        (group.to_string(), version.to_string())
    } else {
        // Core API (e.g., "v1")
        (String::new(), api_version.to_string())
    }
}

/// Result of comparing releases or detecting drift
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffResult {
    /// Old release version (or current for drift detection)
    pub old_version: u32,

    /// New release version (or current for drift detection)
    pub new_version: u32,

    /// List of resource changes
    pub changes: Vec<ResourceChange>,

    /// Whether any changes are drift (manual cluster modifications)
    pub has_drift: bool,
}

impl DiffResult {
    /// Check if there are any changes
    pub fn has_changes(&self) -> bool {
        !self.changes.is_empty()
    }

    /// Get changes by type
    pub fn changes_by_type(&self, change_type: ChangeType) -> Vec<&ResourceChange> {
        self.changes
            .iter()
            .filter(|c| c.change_type == change_type)
            .collect()
    }

    /// Get drift changes only
    pub fn drift_changes(&self) -> Vec<&ResourceChange> {
        self.changes.iter().filter(|c| c.is_drift).collect()
    }
}

/// Result of three-way diff
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThreeWayDiffResult {
    /// List of resource changes
    pub changes: Vec<ResourceChange>,

    /// Whether there are pending changes to apply
    pub has_pending_changes: bool,

    /// Whether any drift was detected
    pub has_drift: bool,
}

/// A change to a single Kubernetes resource
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceChange {
    /// Resource kind (Deployment, Service, etc.)
    pub kind: String,

    /// API version (apps/v1, v1, etc.)
    pub api_version: String,

    /// Resource name
    pub name: String,

    /// Resource namespace (empty for cluster-scoped)
    pub namespace: Option<String>,

    /// Type of change
    pub change_type: ChangeType,

    /// Detailed diff (if available)
    pub diff: Option<DiffContent>,

    /// Whether this change is drift (manual cluster modification)
    pub is_drift: bool,

    /// Source of the diff
    pub source: DiffSource,
}

impl ResourceChange {
    /// Get a display name for the resource
    pub fn display_name(&self) -> String {
        match &self.namespace {
            Some(ns) => format!("{}/{}/{}", ns, self.kind, self.name),
            None => format!("{}/{}", self.kind, self.name),
        }
    }
}

/// Source of the diff comparison
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DiffSource {
    /// Comparing two releases
    ReleaseComparison,
    /// Detecting cluster drift
    ClusterDrift,
    /// Three-way comparison
    ThreeWay,
    /// Error during fetch
    Error(String),
}

/// Type of resource change
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChangeType {
    /// Resource was added
    Added,

    /// Resource was modified
    Modified,

    /// Resource was removed (scheduled for deletion)
    Removed,

    /// Resource is missing from cluster (should exist)
    Missing,

    /// Resource exists in cluster but not in manifest (drift)
    Extra,

    /// Resource is unchanged
    Unchanged,

    /// Status unknown (API error)
    Unknown,
}

impl std::fmt::Display for ChangeType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ChangeType::Added => write!(f, "Added"),
            ChangeType::Modified => write!(f, "Modified"),
            ChangeType::Removed => write!(f, "Removed"),
            ChangeType::Missing => write!(f, "Missing"),
            ChangeType::Extra => write!(f, "Extra"),
            ChangeType::Unchanged => write!(f, "Unchanged"),
            ChangeType::Unknown => write!(f, "Unknown"),
        }
    }
}

/// Detailed diff content
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffContent {
    /// Lines of the diff
    pub lines: Vec<DiffLine>,
}

impl DiffContent {
    /// Create a diff showing all lines as additions
    pub fn new_addition(content: &str) -> Self {
        let lines = content
            .lines()
            .enumerate()
            .map(|(i, line)| DiffLine {
                line_type: LineType::Added,
                content: line.to_string(),
                old_line_no: None,
                new_line_no: Some(i),
            })
            .collect();

        Self { lines }
    }

    /// Create a diff showing all lines as removals
    pub fn new_removal(content: &str) -> Self {
        let lines = content
            .lines()
            .enumerate()
            .map(|(i, line)| DiffLine {
                line_type: LineType::Removed,
                content: line.to_string(),
                old_line_no: Some(i),
                new_line_no: None,
            })
            .collect();

        Self { lines }
    }

    /// Count added lines
    pub fn added_count(&self) -> usize {
        self.lines
            .iter()
            .filter(|l| l.line_type == LineType::Added)
            .count()
    }

    /// Count removed lines
    pub fn removed_count(&self) -> usize {
        self.lines
            .iter()
            .filter(|l| l.line_type == LineType::Removed)
            .count()
    }

    /// Generate a unified diff string
    pub fn to_unified_diff(&self) -> String {
        let mut output = String::new();

        for line in &self.lines {
            let prefix = match line.line_type {
                LineType::Added => "+",
                LineType::Removed => "-",
                LineType::Context => " ",
            };
            output.push_str(prefix);
            output.push_str(&line.content);
            output.push('\n');
        }

        output
    }
}

/// A single line in a diff
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffLine {
    /// Type of line
    pub line_type: LineType,

    /// Content of the line
    pub content: String,

    /// Line number in old version
    pub old_line_no: Option<usize>,

    /// Line number in new version
    pub new_line_no: Option<usize>,
}

/// Type of diff line
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LineType {
    /// Line was added
    Added,

    /// Line was removed
    Removed,

    /// Unchanged context line
    Context,
}

/// Key for identifying a Kubernetes resource
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ResourceKey {
    /// API version (e.g., "apps/v1", "v1")
    pub api_version: String,
    /// Resource kind
    pub kind: String,
    /// Resource name
    pub name: String,
    /// Resource namespace
    pub namespace: Option<String>,
}

/// Parse a manifest into individual resources
pub fn parse_manifest_resources(manifest: &str) -> HashMap<ResourceKey, String> {
    let mut resources = HashMap::new();

    for doc in manifest.split("---") {
        let doc = doc.trim();
        if doc.is_empty() {
            continue;
        }

        // Parse as YAML to extract metadata
        let yaml: serde_yaml::Value = match serde_yaml::from_str(doc) {
            Ok(v) => v,
            Err(_) => continue,
        };

        let api_version = yaml
            .get("apiVersion")
            .and_then(|v| v.as_str())
            .unwrap_or("v1")
            .to_string();

        let kind = yaml
            .get("kind")
            .and_then(|v| v.as_str())
            .unwrap_or("Unknown")
            .to_string();

        let name = yaml
            .get("metadata")
            .and_then(|m| m.get("name"))
            .and_then(|n| n.as_str())
            .unwrap_or("unnamed")
            .to_string();

        let namespace = yaml
            .get("metadata")
            .and_then(|m| m.get("namespace"))
            .and_then(|n| n.as_str())
            .map(String::from);

        let key = ResourceKey {
            api_version,
            kind,
            name,
            namespace,
        };

        resources.insert(key, doc.to_string());
    }

    resources
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_release(manifest: &str) -> StoredRelease {
        StoredRelease {
            name: "test".to_string(),
            namespace: "default".to_string(),
            version: 1,
            state: crate::release::ReleaseState::Deployed,
            pack: sherpack_core::PackMetadata {
                name: "test".to_string(),
                version: semver::Version::new(1, 0, 0),
                description: None,
                app_version: None,
                kube_version: None,
                home: None,
                icon: None,
                sources: vec![],
                keywords: vec![],
                maintainers: vec![],
                annotations: Default::default(),
            },
            values: sherpack_core::Values::new(),
            values_provenance: Default::default(),
            manifest: manifest.to_string(),
            hooks: vec![],
            labels: Default::default(),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            notes: None,
        }
    }

    #[test]
    fn test_parse_manifest_resources() {
        let manifest = r#"
apiVersion: v1
kind: ConfigMap
metadata:
  name: my-config
  namespace: default
data:
  key: value
---
apiVersion: apps/v1
kind: Deployment
metadata:
  name: my-app
  namespace: default
spec:
  replicas: 1
"#;

        let resources = parse_manifest_resources(manifest);
        assert_eq!(resources.len(), 2);

        let cm_key = ResourceKey {
            api_version: "v1".to_string(),
            kind: "ConfigMap".to_string(),
            name: "my-config".to_string(),
            namespace: Some("default".to_string()),
        };
        assert!(resources.contains_key(&cm_key));

        let deploy_key = ResourceKey {
            api_version: "apps/v1".to_string(),
            kind: "Deployment".to_string(),
            name: "my-app".to_string(),
            namespace: Some("default".to_string()),
        };
        assert!(resources.contains_key(&deploy_key));
    }

    #[test]
    fn test_diff_releases_addition() {
        let engine = DiffEngine::new();

        let old = test_release("apiVersion: v1\nkind: ConfigMap\nmetadata:\n  name: cm1");
        let mut new = old.clone();
        new.version = 2;
        new.manifest = format!(
            "{}\n---\napiVersion: v1\nkind: ConfigMap\nmetadata:\n  name: cm2",
            old.manifest
        );

        let diff = engine.diff_releases(&old, &new);

        assert_eq!(diff.changes.len(), 1);
        assert_eq!(diff.changes[0].change_type, ChangeType::Added);
        assert_eq!(diff.changes[0].name, "cm2");
    }

    #[test]
    fn test_diff_releases_removal() {
        let engine = DiffEngine::new();

        let old = test_release(
            "apiVersion: v1\nkind: ConfigMap\nmetadata:\n  name: cm1\n---\napiVersion: v1\nkind: ConfigMap\nmetadata:\n  name: cm2",
        );
        let mut new = old.clone();
        new.version = 2;
        new.manifest = "apiVersion: v1\nkind: ConfigMap\nmetadata:\n  name: cm1".to_string();

        let diff = engine.diff_releases(&old, &new);

        assert_eq!(diff.changes.len(), 1);
        assert_eq!(diff.changes[0].change_type, ChangeType::Removed);
        assert_eq!(diff.changes[0].name, "cm2");
    }

    #[test]
    fn test_diff_releases_modification() {
        let engine = DiffEngine::new();

        let old = test_release(
            "apiVersion: v1\nkind: ConfigMap\nmetadata:\n  name: cm1\ndata:\n  key: old-value",
        );
        let mut new = old.clone();
        new.version = 2;
        new.manifest =
            "apiVersion: v1\nkind: ConfigMap\nmetadata:\n  name: cm1\ndata:\n  key: new-value"
                .to_string();

        let diff = engine.diff_releases(&old, &new);

        assert_eq!(diff.changes.len(), 1);
        assert_eq!(diff.changes[0].change_type, ChangeType::Modified);
        assert!(diff.changes[0].diff.is_some());
    }

    #[test]
    fn test_normalize_strips_managed_fields() {
        let engine = DiffEngine::new();

        let resource_with_managed = r#"
apiVersion: v1
kind: ConfigMap
metadata:
  name: test
  resourceVersion: "12345"
  uid: "abc-123"
  generation: 1
  creationTimestamp: "2024-01-01T00:00:00Z"
  managedFields:
    - manager: kubectl
      operation: Apply
data:
  key: value
"#;

        let normalized = engine.normalize_resource(resource_with_managed);

        // Check that managed fields are removed
        assert!(!normalized.contains("resourceVersion"));
        assert!(!normalized.contains("uid"));
        assert!(!normalized.contains("generation"));
        assert!(!normalized.contains("creationTimestamp"));
        assert!(!normalized.contains("managedFields"));

        // Check that actual data is preserved
        assert!(normalized.contains("ConfigMap"));
        assert!(normalized.contains("key: value"));
    }

    #[test]
    fn test_normalize_strips_ignored_annotations() {
        let engine = DiffEngine::new();

        let resource_with_annotations = r#"
apiVersion: v1
kind: ConfigMap
metadata:
  name: test
  annotations:
    kubectl.kubernetes.io/last-applied-configuration: "{}"
    my-custom-annotation: "keep-this"
data:
  key: value
"#;

        let normalized = engine.normalize_resource(resource_with_annotations);

        assert!(!normalized.contains("last-applied-configuration"));
        assert!(normalized.contains("my-custom-annotation"));
    }

    #[test]
    fn test_normalize_strips_status() {
        let engine = DiffEngine::new();

        let resource_with_status = r#"
apiVersion: apps/v1
kind: Deployment
metadata:
  name: test
spec:
  replicas: 1
status:
  availableReplicas: 1
  readyReplicas: 1
"#;

        let normalized = engine.normalize_resource(resource_with_status);

        assert!(!normalized.contains("status:"));
        assert!(!normalized.contains("availableReplicas"));
        assert!(normalized.contains("replicas: 1"));
    }

    #[test]
    fn test_diff_summary() {
        let engine = DiffEngine::new();
        let result = DiffResult {
            old_version: 1,
            new_version: 2,
            changes: vec![
                ResourceChange {
                    kind: "ConfigMap".to_string(),
                    api_version: "v1".to_string(),
                    name: "cm1".to_string(),
                    namespace: Some("default".to_string()),
                    change_type: ChangeType::Added,
                    diff: None,
                    is_drift: false,
                    source: DiffSource::ReleaseComparison,
                },
                ResourceChange {
                    kind: "Deployment".to_string(),
                    api_version: "apps/v1".to_string(),
                    name: "app".to_string(),
                    namespace: Some("default".to_string()),
                    change_type: ChangeType::Modified,
                    diff: None,
                    is_drift: true,
                    source: DiffSource::ClusterDrift,
                },
            ],
            has_drift: true,
        };

        let summary = engine.summary(&result);
        assert!(summary.contains("1 added"));
        assert!(summary.contains("1 modified"));
        assert!(summary.contains("1 drifted"));
    }

    #[test]
    fn test_parse_api_version() {
        assert_eq!(
            parse_api_version("apps/v1"),
            ("apps".to_string(), "v1".to_string())
        );
        assert_eq!(parse_api_version("v1"), (String::new(), "v1".to_string()));
        assert_eq!(
            parse_api_version("networking.k8s.io/v1"),
            ("networking.k8s.io".to_string(), "v1".to_string())
        );
    }

    #[test]
    fn test_diff_content_counts() {
        let content = DiffContent {
            lines: vec![
                DiffLine {
                    line_type: LineType::Removed,
                    content: "old".to_string(),
                    old_line_no: Some(0),
                    new_line_no: None,
                },
                DiffLine {
                    line_type: LineType::Added,
                    content: "new1".to_string(),
                    old_line_no: None,
                    new_line_no: Some(0),
                },
                DiffLine {
                    line_type: LineType::Added,
                    content: "new2".to_string(),
                    old_line_no: None,
                    new_line_no: Some(1),
                },
            ],
        };

        assert_eq!(content.added_count(), 2);
        assert_eq!(content.removed_count(), 1);
    }

    #[test]
    fn test_unified_diff_output() {
        let content = DiffContent {
            lines: vec![
                DiffLine {
                    line_type: LineType::Context,
                    content: "unchanged".to_string(),
                    old_line_no: Some(0),
                    new_line_no: Some(0),
                },
                DiffLine {
                    line_type: LineType::Removed,
                    content: "old".to_string(),
                    old_line_no: Some(1),
                    new_line_no: None,
                },
                DiffLine {
                    line_type: LineType::Added,
                    content: "new".to_string(),
                    old_line_no: None,
                    new_line_no: Some(1),
                },
            ],
        };

        let unified = content.to_unified_diff();
        assert!(unified.contains(" unchanged\n"));
        assert!(unified.contains("-old\n"));
        assert!(unified.contains("+new\n"));
    }

    #[test]
    fn test_resource_display_name() {
        let namespaced = ResourceChange {
            kind: "Deployment".to_string(),
            api_version: "apps/v1".to_string(),
            name: "my-app".to_string(),
            namespace: Some("production".to_string()),
            change_type: ChangeType::Modified,
            diff: None,
            is_drift: false,
            source: DiffSource::ReleaseComparison,
        };
        assert_eq!(namespaced.display_name(), "production/Deployment/my-app");

        let cluster_scoped = ResourceChange {
            kind: "ClusterRole".to_string(),
            api_version: "rbac.authorization.k8s.io/v1".to_string(),
            name: "admin".to_string(),
            namespace: None,
            change_type: ChangeType::Added,
            diff: None,
            is_drift: false,
            source: DiffSource::ReleaseComparison,
        };
        assert_eq!(cluster_scoped.display_name(), "ClusterRole/admin");
    }

    #[test]
    fn test_changes_by_type() {
        let result = DiffResult {
            old_version: 1,
            new_version: 2,
            changes: vec![
                ResourceChange {
                    kind: "ConfigMap".to_string(),
                    api_version: "v1".to_string(),
                    name: "cm1".to_string(),
                    namespace: None,
                    change_type: ChangeType::Added,
                    diff: None,
                    is_drift: false,
                    source: DiffSource::ReleaseComparison,
                },
                ResourceChange {
                    kind: "ConfigMap".to_string(),
                    api_version: "v1".to_string(),
                    name: "cm2".to_string(),
                    namespace: None,
                    change_type: ChangeType::Added,
                    diff: None,
                    is_drift: false,
                    source: DiffSource::ReleaseComparison,
                },
                ResourceChange {
                    kind: "Secret".to_string(),
                    api_version: "v1".to_string(),
                    name: "sec1".to_string(),
                    namespace: None,
                    change_type: ChangeType::Removed,
                    diff: None,
                    is_drift: false,
                    source: DiffSource::ReleaseComparison,
                },
            ],
            has_drift: false,
        };

        assert_eq!(result.changes_by_type(ChangeType::Added).len(), 2);
        assert_eq!(result.changes_by_type(ChangeType::Removed).len(), 1);
        assert_eq!(result.changes_by_type(ChangeType::Modified).len(), 0);
    }

    #[test]
    fn test_ignore_custom_path() {
        let engine = DiffEngine::new().ignore_path("spec.replicas");

        let resource = r#"
apiVersion: apps/v1
kind: Deployment
metadata:
  name: test
spec:
  replicas: 3
  template:
    spec:
      containers: []
"#;

        let normalized = engine.normalize_resource(resource);

        // replicas should be removed
        assert!(!normalized.contains("replicas"));
        // But template should remain
        assert!(normalized.contains("template"));
    }
}
