//! Lock file format for reproducible builds
//!
//! Key features:
//! - Exact versions (not ranges)
//! - SHA256 integrity verification
//! - Pack.yaml hash to detect changes

use chrono::{DateTime, Utc};
use semver::Version;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::Path;

use crate::error::{RepoError, Result};

/// Lock file format (Pack.lock.yaml)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LockFile {
    /// Lock file format version
    #[serde(default = "default_version")]
    pub version: u32,

    /// When this lock file was generated
    pub generated: DateTime<Utc>,

    /// SHA256 hash of Pack.yaml to detect changes
    pub pack_yaml_digest: String,

    /// Lock policy (how strict to be)
    #[serde(default)]
    pub policy: LockPolicy,

    /// Locked dependencies
    #[serde(default)]
    pub dependencies: Vec<LockedDependency>,
}

fn default_version() -> u32 {
    1
}

/// Lock policy - how strict should verification be?
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LockPolicy {
    /// Exact version + SHA must match (highest security)
    Strict,

    /// Exact version only, ignore SHA (allows republishing)
    #[default]
    Version,

    /// Allow patch updates within semver (1.2.3 -> 1.2.4)
    SemverPatch,

    /// Allow minor updates within semver (1.2.3 -> 1.3.0)
    SemverMinor,
}

/// A locked dependency with exact version and integrity hash
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LockedDependency {
    /// Dependency name
    pub name: String,

    /// Exact resolved version (NOT a range)
    #[serde(with = "version_serde")]
    pub version: Version,

    /// Repository URL where this was resolved from
    pub repository: String,

    /// SHA256 digest of the pack archive
    pub digest: String,

    /// Original version constraint from Pack.yaml
    pub constraint: String,

    /// Alias if specified
    #[serde(default)]
    pub alias: Option<String>,

    /// Transitive dependencies (by name)
    #[serde(default)]
    pub dependencies: Vec<String>,
}

impl LockedDependency {
    /// Effective name (alias or original name)
    pub fn effective_name(&self) -> &str {
        self.alias.as_deref().unwrap_or(&self.name)
    }
}

impl LockFile {
    /// Create a new lock file
    pub fn new(pack_yaml_content: &str) -> Self {
        Self {
            version: 1,
            generated: Utc::now(),
            pack_yaml_digest: compute_sha256(pack_yaml_content.as_bytes()),
            policy: LockPolicy::default(),
            dependencies: Vec::new(),
        }
    }

    /// Load lock file from path
    pub fn load(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Err(RepoError::LockFileNotFound {
                path: path.display().to_string(),
            });
        }
        let content = std::fs::read_to_string(path)?;
        let lock: Self = serde_yaml::from_str(&content)?;
        Ok(lock)
    }

    /// Save lock file to path
    pub fn save(&self, path: &Path) -> Result<()> {
        let content = serde_yaml::to_string(self)?;
        std::fs::write(path, content)?;
        Ok(())
    }

    /// Check if lock file is outdated (Pack.yaml changed)
    pub fn is_outdated(&self, pack_yaml_content: &str) -> bool {
        let current_digest = compute_sha256(pack_yaml_content.as_bytes());
        self.pack_yaml_digest != current_digest
    }

    /// Add a locked dependency
    pub fn add(&mut self, dep: LockedDependency) {
        // Remove existing entry with same name
        self.dependencies
            .retain(|d| d.effective_name() != dep.effective_name());
        self.dependencies.push(dep);
    }

    /// Get a locked dependency by name
    pub fn get(&self, name: &str) -> Option<&LockedDependency> {
        self.dependencies.iter().find(|d| d.effective_name() == name)
    }

    /// Build a map of name -> locked dependency
    pub fn as_map(&self) -> HashMap<&str, &LockedDependency> {
        self.dependencies
            .iter()
            .map(|d| (d.effective_name(), d))
            .collect()
    }

    /// Verify integrity of a downloaded archive
    pub fn verify(&self, name: &str, data: &[u8]) -> Result<VerifyResult> {
        let locked = self.get(name).ok_or_else(|| RepoError::LockFileNotFound {
            path: format!("dependency '{}' not in lock file", name),
        })?;

        let actual_digest = compute_sha256(data);

        match self.policy {
            LockPolicy::Strict => {
                if locked.digest != actual_digest {
                    Err(RepoError::IntegrityCheckFailed {
                        name: name.to_string(),
                        expected: locked.digest.clone(),
                        actual: actual_digest,
                    })
                } else {
                    Ok(VerifyResult::Match)
                }
            }
            LockPolicy::Version => {
                // Version matches (we downloaded what was requested)
                // SHA might differ (republished), that's OK
                if locked.digest != actual_digest {
                    Ok(VerifyResult::DigestChanged {
                        expected: locked.digest.clone(),
                        actual: actual_digest,
                    })
                } else {
                    Ok(VerifyResult::Match)
                }
            }
            LockPolicy::SemverPatch | LockPolicy::SemverMinor => {
                // More permissive - just check that we got something
                Ok(VerifyResult::Match)
            }
        }
    }
}

/// Result of integrity verification
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VerifyResult {
    /// Perfect match
    Match,
    /// Version matches but digest changed (republished)
    DigestChanged { expected: String, actual: String },
}

/// Compute SHA256 digest of data
fn compute_sha256(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    let result = hasher.finalize();
    format!("sha256:{}", hex::encode(result))
}

/// Serde helper for semver::Version
mod version_serde {
    use semver::Version;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(version: &Version, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&version.to_string())
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Version, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Version::parse(&s).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lock_file_creation() {
        let pack_yaml = "apiVersion: sherpack/v1\nmetadata:\n  name: test\n  version: 1.0.0\n";
        let lock = LockFile::new(pack_yaml);

        assert_eq!(lock.version, 1);
        assert!(!lock.pack_yaml_digest.is_empty());
        assert!(lock.dependencies.is_empty());
    }

    #[test]
    fn test_lock_file_outdated() {
        let pack_yaml_v1 = "version: 1.0.0";
        let pack_yaml_v2 = "version: 1.0.1";

        let lock = LockFile::new(pack_yaml_v1);

        assert!(!lock.is_outdated(pack_yaml_v1));
        assert!(lock.is_outdated(pack_yaml_v2));
    }

    #[test]
    fn test_add_dependency() {
        let mut lock = LockFile::new("test");

        lock.add(LockedDependency {
            name: "nginx".to_string(),
            version: Version::new(15, 0, 0),
            repository: "https://charts.bitnami.com/bitnami".to_string(),
            digest: "sha256:abc123".to_string(),
            constraint: "^15.0.0".to_string(),
            alias: None,
            dependencies: vec![],
        });

        assert_eq!(lock.dependencies.len(), 1);
        assert!(lock.get("nginx").is_some());
    }

    #[test]
    fn test_verify_strict() {
        let mut lock = LockFile::new("test");
        lock.policy = LockPolicy::Strict;
        lock.add(LockedDependency {
            name: "test".to_string(),
            version: Version::new(1, 0, 0),
            repository: "https://example.com".to_string(),
            digest: compute_sha256(b"test data"),
            constraint: "1.0.0".to_string(),
            alias: None,
            dependencies: vec![],
        });

        // Matching data should pass
        let result = lock.verify("test", b"test data").unwrap();
        assert_eq!(result, VerifyResult::Match);

        // Different data should fail
        let result = lock.verify("test", b"different data");
        assert!(result.is_err());
    }

    #[test]
    fn test_verify_version_policy() {
        let mut lock = LockFile::new("test");
        lock.policy = LockPolicy::Version;
        lock.add(LockedDependency {
            name: "test".to_string(),
            version: Version::new(1, 0, 0),
            repository: "https://example.com".to_string(),
            digest: compute_sha256(b"original data"),
            constraint: "1.0.0".to_string(),
            alias: None,
            dependencies: vec![],
        });

        // Different data should just warn (DigestChanged), not error
        let result = lock.verify("test", b"republished data").unwrap();
        assert!(matches!(result, VerifyResult::DigestChanged { .. }));
    }

    #[test]
    fn test_serialization() {
        let mut lock = LockFile::new("test");
        lock.add(LockedDependency {
            name: "nginx".to_string(),
            version: Version::new(15, 0, 0),
            repository: "https://charts.bitnami.com/bitnami".to_string(),
            digest: "sha256:abc123".to_string(),
            constraint: "^15.0.0".to_string(),
            alias: Some("web".to_string()),
            dependencies: vec!["common".to_string()],
        });

        let yaml = serde_yaml::to_string(&lock).unwrap();
        assert!(yaml.contains("nginx"));
        assert!(yaml.contains("15.0.0"));
        assert!(yaml.contains("web"));

        let parsed: LockFile = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(parsed.dependencies.len(), 1);
        assert_eq!(parsed.get("web").unwrap().name, "nginx");
    }

    #[test]
    fn test_effective_name() {
        let dep_no_alias = LockedDependency {
            name: "nginx".to_string(),
            version: Version::new(1, 0, 0),
            repository: String::new(),
            digest: String::new(),
            constraint: String::new(),
            alias: None,
            dependencies: vec![],
        };
        assert_eq!(dep_no_alias.effective_name(), "nginx");

        let dep_with_alias = LockedDependency {
            name: "nginx".to_string(),
            version: Version::new(1, 0, 0),
            repository: String::new(),
            digest: String::new(),
            constraint: String::new(),
            alias: Some("web".to_string()),
            dependencies: vec![],
        };
        assert_eq!(dep_with_alias.effective_name(), "web");
    }
}
