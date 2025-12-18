//! Diff engine for comparing releases and detecting cluster drift
//!
//! Key features:
//! - Compare release manifests (like helm diff)
//! - Detect cluster drift (manual changes to resources)
//! - Show structured diffs with context

use serde::{Deserialize, Serialize};
use similar::{ChangeTag, TextDiff};
use std::collections::HashMap;

use crate::error::Result;
use crate::release::StoredRelease;

/// Diff engine for release comparison
pub struct DiffEngine {
    /// Show context lines around changes
    pub context_lines: usize,
}

impl DiffEngine {
    /// Create a new diff engine
    pub fn new() -> Self {
        Self { context_lines: 3 }
    }

    /// Set the number of context lines
    pub fn with_context(mut self, lines: usize) -> Self {
        self.context_lines = lines;
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
                Some(old_content) if old_content != new_content => {
                    changes.push(ResourceChange {
                        kind: key.kind.clone(),
                        name: key.name.clone(),
                        namespace: key.namespace.clone(),
                        change_type: ChangeType::Modified,
                        diff: Some(self.compute_text_diff(old_content, new_content)),
                        is_drift: false,
                    });
                }
                None => {
                    changes.push(ResourceChange {
                        kind: key.kind.clone(),
                        name: key.name.clone(),
                        namespace: key.namespace.clone(),
                        change_type: ChangeType::Added,
                        diff: Some(DiffContent::new_addition(new_content)),
                        is_drift: false,
                    });
                }
                _ => {} // Unchanged
            }
        }

        // Find removed resources
        for (key, old_content) in &old_resources {
            if !new_resources.contains_key(key) {
                changes.push(ResourceChange {
                    kind: key.kind.clone(),
                    name: key.name.clone(),
                    namespace: key.namespace.clone(),
                    change_type: ChangeType::Removed,
                    diff: Some(DiffContent::new_removal(old_content)),
                    is_drift: false,
                });
            }
        }

        DiffResult {
            old_version: old.version,
            new_version: new.version,
            changes,
            has_drift: false,
        }
    }

    /// Compare a release manifest with actual cluster state
    pub async fn detect_drift(
        &self,
        release: &StoredRelease,
        _client: &kube::Client,
    ) -> Result<DiffResult> {
        // TODO: Implement actual cluster state comparison
        // For now, return no drift
        Ok(DiffResult {
            old_version: release.version,
            new_version: release.version,
            changes: Vec::new(),
            has_drift: false,
        })
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
        if drift > 0 {
            parts.push(format!("{} drifted", drift));
        }

        if parts.is_empty() {
            "No changes".to_string()
        } else {
            parts.join(", ")
        }
    }
}

impl Default for DiffEngine {
    fn default() -> Self {
        Self::new()
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

/// A change to a single Kubernetes resource
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceChange {
    /// Resource kind (Deployment, Service, etc.)
    pub kind: String,

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

/// Type of resource change
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChangeType {
    /// Resource was added
    Added,

    /// Resource was modified
    Modified,

    /// Resource was removed
    Removed,
}

impl std::fmt::Display for ChangeType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ChangeType::Added => write!(f, "added"),
            ChangeType::Modified => write!(f, "modified"),
            ChangeType::Removed => write!(f, "removed"),
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
    fn new_addition(content: &str) -> Self {
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
    fn new_removal(content: &str) -> Self {
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
struct ResourceKey {
    kind: String,
    name: String,
    namespace: Option<String>,
}

/// Parse a manifest into individual resources
fn parse_manifest_resources(manifest: &str) -> HashMap<ResourceKey, String> {
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
            kind: "ConfigMap".to_string(),
            name: "my-config".to_string(),
            namespace: Some("default".to_string()),
        };
        assert!(resources.contains_key(&cm_key));
    }

    #[test]
    fn test_diff_releases_addition() {
        let engine = DiffEngine::new();

        let old = StoredRelease {
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
            manifest: "apiVersion: v1\nkind: ConfigMap\nmetadata:\n  name: cm1".to_string(),
            hooks: vec![],
            labels: Default::default(),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            notes: None,
        };

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
    fn test_diff_summary() {
        let engine = DiffEngine::new();
        let result = DiffResult {
            old_version: 1,
            new_version: 2,
            changes: vec![
                ResourceChange {
                    kind: "ConfigMap".to_string(),
                    name: "cm1".to_string(),
                    namespace: Some("default".to_string()),
                    change_type: ChangeType::Added,
                    diff: None,
                    is_drift: false,
                },
                ResourceChange {
                    kind: "Deployment".to_string(),
                    name: "app".to_string(),
                    namespace: Some("default".to_string()),
                    change_type: ChangeType::Modified,
                    diff: None,
                    is_drift: false,
                },
            ],
            has_drift: false,
        };

        let summary = engine.summary(&result);
        assert!(summary.contains("1 added"));
        assert!(summary.contains("1 modified"));
    }
}
