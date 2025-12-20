//! Repository index types
//!
//! Helm-compatible repository index format with extensions for Sherpack

use chrono::{DateTime, Utc};
use semver::Version;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::error::{RepoError, Result};

/// Repository index (Helm-compatible)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RepositoryIndex {
    /// API version
    #[serde(default = "default_api_version")]
    pub api_version: String,

    /// When this index was generated
    #[serde(default = "Utc::now")]
    pub generated: DateTime<Utc>,

    /// Packs indexed by name
    #[serde(default)]
    pub entries: HashMap<String, Vec<PackEntry>>,
}

fn default_api_version() -> String {
    "v1".to_string()
}

impl Default for RepositoryIndex {
    fn default() -> Self {
        Self {
            api_version: default_api_version(),
            generated: Utc::now(),
            entries: HashMap::new(),
        }
    }
}

impl RepositoryIndex {
    /// Parse index from YAML string
    pub fn from_yaml(yaml: &str) -> Result<Self> {
        serde_yaml::from_str(yaml).map_err(|e| RepoError::IndexParseError {
            message: e.to_string(),
        })
    }

    /// Parse index from bytes
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        let yaml = std::str::from_utf8(bytes).map_err(|e| RepoError::IndexParseError {
            message: format!("Invalid UTF-8: {}", e),
        })?;
        Self::from_yaml(yaml)
    }

    /// Get all versions of a pack
    pub fn get(&self, name: &str) -> Option<&Vec<PackEntry>> {
        self.entries.get(name)
    }

    /// Get the latest (highest semver) version of a pack
    pub fn get_latest(&self, name: &str) -> Option<&PackEntry> {
        self.entries.get(name).and_then(|versions| {
            versions.iter().max_by(|a, b| {
                let va = Version::parse(&a.version).ok();
                let vb = Version::parse(&b.version).ok();
                match (va, vb) {
                    (Some(va), Some(vb)) => va.cmp(&vb),
                    (Some(_), None) => std::cmp::Ordering::Greater,
                    (None, Some(_)) => std::cmp::Ordering::Less,
                    (None, None) => a.version.cmp(&b.version),
                }
            })
        })
    }

    /// Get a specific version of a pack
    pub fn get_version(&self, name: &str, version: &str) -> Option<&PackEntry> {
        self.entries
            .get(name)?
            .iter()
            .find(|e| e.version == version)
    }

    /// Find versions matching a semver constraint
    pub fn find_matching(&self, name: &str, constraint: &str) -> Result<Vec<&PackEntry>> {
        let entries = self
            .entries
            .get(name)
            .ok_or_else(|| RepoError::PackNotFound {
                name: name.to_string(),
                repo: "unknown".to_string(),
            })?;

        // Parse version constraint
        let req =
            semver::VersionReq::parse(constraint).map_err(|e| RepoError::ResolutionFailed {
                message: format!("Invalid version constraint '{}': {}", constraint, e),
            })?;

        let matching: Vec<_> = entries
            .iter()
            .filter(|e| {
                Version::parse(&e.version)
                    .map(|v| req.matches(&v))
                    .unwrap_or(false)
            })
            .collect();

        Ok(matching)
    }

    /// Find the highest version matching a constraint
    pub fn find_best_match(&self, name: &str, constraint: &str) -> Result<&PackEntry> {
        let matching = self.find_matching(name, constraint)?;

        matching
            .into_iter()
            .max_by(|a, b| {
                let va = Version::parse(&a.version).ok();
                let vb = Version::parse(&b.version).ok();
                match (va, vb) {
                    (Some(va), Some(vb)) => va.cmp(&vb),
                    (Some(_), None) => std::cmp::Ordering::Greater,
                    (None, Some(_)) => std::cmp::Ordering::Less,
                    (None, None) => a.version.cmp(&b.version),
                }
            })
            .ok_or_else(|| RepoError::UnsatisfiableConstraint {
                name: name.to_string(),
                constraint: constraint.to_string(),
                available: self
                    .entries
                    .get(name)
                    .map(|v| {
                        v.iter()
                            .map(|e| e.version.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    })
                    .unwrap_or_else(|| "none".to_string()),
            })
    }

    /// Search packs by query
    pub fn search(&self, query: &str) -> Vec<&PackEntry> {
        let query_lower = query.to_lowercase();

        let mut results: Vec<_> = self
            .entries
            .iter()
            .filter_map(|(name, versions)| {
                // Check if name or description matches
                let name_matches = name.to_lowercase().contains(&query_lower);
                let latest = versions.iter().max_by(|a, b| {
                    Version::parse(&a.version)
                        .ok()
                        .cmp(&Version::parse(&b.version).ok())
                })?;

                let desc_matches = latest
                    .description
                    .as_ref()
                    .map(|d| d.to_lowercase().contains(&query_lower))
                    .unwrap_or(false);

                let keyword_matches = latest
                    .keywords
                    .iter()
                    .any(|k| k.to_lowercase().contains(&query_lower));

                if name_matches || desc_matches || keyword_matches {
                    Some((name_matches, latest))
                } else {
                    None
                }
            })
            .collect();

        // Sort: exact name matches first, then by name
        results.sort_by(|(a_name_match, a), (b_name_match, b)| {
            match (a_name_match, b_name_match) {
                (true, false) => std::cmp::Ordering::Less,
                (false, true) => std::cmp::Ordering::Greater,
                _ => a.name.cmp(&b.name),
            }
        });

        results.into_iter().map(|(_, e)| e).collect()
    }

    /// List all pack names
    pub fn names(&self) -> Vec<&str> {
        self.entries.keys().map(|s| s.as_str()).collect()
    }

    /// Add an entry to the index
    pub fn add_entry(&mut self, entry: PackEntry) {
        self.entries
            .entry(entry.name.clone())
            .or_default()
            .push(entry);
    }

    /// Merge another index into this one
    pub fn merge(&mut self, other: RepositoryIndex) {
        for (name, entries) in other.entries {
            self.entries.entry(name).or_default().extend(entries);
        }
        self.generated = Utc::now();
    }
}

/// Pack entry in the index
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PackEntry {
    /// Pack name
    pub name: String,

    /// Pack version (semver)
    pub version: String,

    /// Application version
    #[serde(default)]
    pub app_version: Option<String>,

    /// Description
    #[serde(default)]
    pub description: Option<String>,

    /// Home URL
    #[serde(default)]
    pub home: Option<String>,

    /// Icon URL
    #[serde(default)]
    pub icon: Option<String>,

    /// Source URLs
    #[serde(default)]
    pub sources: Vec<String>,

    /// Keywords for search
    #[serde(default)]
    pub keywords: Vec<String>,

    /// Maintainers
    #[serde(default)]
    pub maintainers: Vec<Maintainer>,

    /// URLs to download the pack archive
    #[serde(default)]
    pub urls: Vec<String>,

    /// SHA256 digest of the archive
    #[serde(default)]
    pub digest: Option<String>,

    /// Creation timestamp
    #[serde(default)]
    pub created: Option<DateTime<Utc>>,

    /// Deprecated flag
    #[serde(default)]
    pub deprecated: bool,

    /// Pack dependencies
    #[serde(default)]
    pub dependencies: Vec<IndexDependency>,

    /// Annotations
    #[serde(default)]
    pub annotations: HashMap<String, String>,

    /// API version
    #[serde(default)]
    pub api_version: Option<String>,

    /// Pack type (application or library)
    #[serde(default)]
    pub r#type: Option<String>,
}

impl PackEntry {
    /// Get the primary download URL
    pub fn download_url(&self) -> Option<&str> {
        self.urls.first().map(|s| s.as_str())
    }

    /// Parse version as semver
    pub fn parsed_version(&self) -> Option<Version> {
        Version::parse(&self.version).ok()
    }
}

/// Maintainer in index
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Maintainer {
    pub name: String,
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
}

/// Dependency in index entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexDependency {
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub repository: Option<String>,
    #[serde(default)]
    pub condition: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub alias: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_index() -> RepositoryIndex {
        let yaml = r#"
apiVersion: v1
generated: "2024-01-01T00:00:00Z"
entries:
  nginx:
    - name: nginx
      version: "15.0.0"
      appVersion: "1.25.0"
      description: NGINX Open Source
      keywords:
        - webserver
        - http
      urls:
        - https://example.com/charts/nginx-15.0.0.tgz
      digest: "sha256:abc123"
    - name: nginx
      version: "14.0.0"
      appVersion: "1.24.0"
      description: NGINX Open Source
      urls:
        - https://example.com/charts/nginx-14.0.0.tgz
  redis:
    - name: redis
      version: "17.0.0"
      description: Redis database
      keywords:
        - cache
        - database
      urls:
        - https://example.com/charts/redis-17.0.0.tgz
"#;
        RepositoryIndex::from_yaml(yaml).unwrap()
    }

    #[test]
    fn test_parse_index() {
        let index = sample_index();
        assert_eq!(index.entries.len(), 2);
        assert!(index.entries.contains_key("nginx"));
        assert!(index.entries.contains_key("redis"));
    }

    #[test]
    fn test_get_latest() {
        let index = sample_index();
        let latest = index.get_latest("nginx").unwrap();
        assert_eq!(latest.version, "15.0.0");
    }

    #[test]
    fn test_get_version() {
        let index = sample_index();
        let v14 = index.get_version("nginx", "14.0.0").unwrap();
        assert_eq!(v14.app_version, Some("1.24.0".to_string()));
    }

    #[test]
    fn test_find_matching() {
        let index = sample_index();

        // Exact match
        let matching = index.find_matching("nginx", "=15.0.0").unwrap();
        assert_eq!(matching.len(), 1);
        assert_eq!(matching[0].version, "15.0.0");

        // Range match
        let matching = index.find_matching("nginx", ">=14.0.0").unwrap();
        assert_eq!(matching.len(), 2);

        // Caret match
        let matching = index.find_matching("nginx", "^14.0.0").unwrap();
        assert_eq!(matching.len(), 1); // Only 14.x
    }

    #[test]
    fn test_find_best_match() {
        let index = sample_index();
        let best = index.find_best_match("nginx", ">=14.0.0").unwrap();
        assert_eq!(best.version, "15.0.0");
    }

    #[test]
    fn test_search() {
        let index = sample_index();

        // Search by name
        let results = index.search("nginx");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "nginx");

        // Search by keyword
        let results = index.search("cache");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "redis");

        // Search by description
        let results = index.search("database");
        assert_eq!(results.len(), 1);

        // No matches
        let results = index.search("postgresql");
        assert!(results.is_empty());
    }

    #[test]
    fn test_add_entry() {
        let mut index = RepositoryIndex::default();
        index.add_entry(PackEntry {
            name: "test".to_string(),
            version: "1.0.0".to_string(),
            app_version: None,
            description: Some("Test pack".to_string()),
            home: None,
            icon: None,
            sources: vec![],
            keywords: vec![],
            maintainers: vec![],
            urls: vec!["https://example.com/test-1.0.0.tgz".to_string()],
            digest: None,
            created: None,
            deprecated: false,
            dependencies: vec![],
            annotations: HashMap::new(),
            api_version: None,
            r#type: None,
        });

        assert!(index.get("test").is_some());
        assert_eq!(index.get("test").unwrap().len(), 1);
    }

    #[test]
    fn test_merge() {
        let mut index1 = sample_index();
        let mut index2 = RepositoryIndex::default();
        index2.add_entry(PackEntry {
            name: "postgresql".to_string(),
            version: "12.0.0".to_string(),
            app_version: None,
            description: Some("PostgreSQL".to_string()),
            home: None,
            icon: None,
            sources: vec![],
            keywords: vec![],
            maintainers: vec![],
            urls: vec![],
            digest: None,
            created: None,
            deprecated: false,
            dependencies: vec![],
            annotations: HashMap::new(),
            api_version: None,
            r#type: None,
        });

        index1.merge(index2);
        assert!(index1.get("postgresql").is_some());
    }
}
