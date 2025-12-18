//! Repository configuration management
//!
//! Stores repository configuration in `~/.config/sherpack/repositories.yaml`

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::error::{RepoError, Result};

/// Repository configuration file
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RepositoryConfig {
    /// API version
    #[serde(default = "default_api_version")]
    pub api_version: String,

    /// Configured repositories
    #[serde(default)]
    pub repositories: Vec<Repository>,
}

fn default_api_version() -> String {
    "sherpack.io/v1".to_string()
}

impl Default for RepositoryConfig {
    fn default() -> Self {
        Self {
            api_version: default_api_version(),
            repositories: Vec::new(),
        }
    }
}

impl RepositoryConfig {
    /// Load configuration from default location
    pub fn load() -> Result<Self> {
        let path = Self::default_path()?;
        if path.exists() {
            Self::load_from(&path)
        } else {
            Ok(Self::default())
        }
    }

    /// Load configuration from a specific path
    pub fn load_from(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let config: Self = serde_yaml::from_str(&content)?;
        Ok(config)
    }

    /// Save configuration to default location
    pub fn save(&self) -> Result<()> {
        let path = Self::default_path()?;
        self.save_to(&path)
    }

    /// Save configuration to a specific path
    pub fn save_to(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = serde_yaml::to_string(self)?;
        std::fs::write(path, content)?;
        Ok(())
    }

    /// Get default configuration path
    pub fn default_path() -> Result<PathBuf> {
        let config_dir = dirs::config_dir().ok_or_else(|| RepoError::InvalidConfig {
            message: "Could not determine config directory".to_string(),
        })?;
        Ok(config_dir.join("sherpack").join("repositories.yaml"))
    }

    /// Get a repository by name
    pub fn get(&self, name: &str) -> Option<&Repository> {
        self.repositories.iter().find(|r| r.name == name)
    }

    /// Get a mutable repository by name
    pub fn get_mut(&mut self, name: &str) -> Option<&mut Repository> {
        self.repositories.iter_mut().find(|r| r.name == name)
    }

    /// Add a repository
    pub fn add(&mut self, repo: Repository) -> Result<()> {
        if self.get(&repo.name).is_some() {
            return Err(RepoError::RepositoryAlreadyExists {
                name: repo.name.clone(),
            });
        }
        self.repositories.push(repo);
        Ok(())
    }

    /// Remove a repository by name
    pub fn remove(&mut self, name: &str) -> Result<Repository> {
        let idx = self
            .repositories
            .iter()
            .position(|r| r.name == name)
            .ok_or_else(|| RepoError::RepositoryNotFound {
                name: name.to_string(),
            })?;
        Ok(self.repositories.remove(idx))
    }

    /// List all repository names
    pub fn names(&self) -> Vec<&str> {
        self.repositories.iter().map(|r| r.name.as_str()).collect()
    }
}

/// Repository definition
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Repository {
    /// Unique name for this repository
    pub name: String,

    /// Repository URL (HTTP(S) or OCI)
    pub url: String,

    /// Repository type (auto-detected if not specified)
    #[serde(default)]
    pub repo_type: RepositoryType,

    /// CA bundle for TLS verification (optional)
    #[serde(default)]
    pub ca_bundle: Option<PathBuf>,

    /// Skip TLS verification (insecure, not recommended)
    #[serde(default)]
    pub insecure_skip_tls: bool,

    /// Credential reference name (stored separately)
    #[serde(default)]
    pub credential_ref: Option<String>,

    /// ETag for caching (internal)
    #[serde(default)]
    pub etag: Option<String>,

    /// Last update timestamp (internal)
    #[serde(default)]
    pub last_updated: Option<chrono::DateTime<chrono::Utc>>,

    /// Additional metadata
    #[serde(default)]
    pub metadata: HashMap<String, String>,
}

impl Repository {
    /// Create a new repository from URL
    pub fn new(name: impl Into<String>, url: impl Into<String>) -> Result<Self> {
        let name = name.into();
        let url = url.into();
        let repo_type = RepositoryType::detect(&url)?;

        Ok(Self {
            name,
            url,
            repo_type,
            ca_bundle: None,
            insecure_skip_tls: false,
            credential_ref: None,
            etag: None,
            last_updated: None,
            metadata: HashMap::new(),
        })
    }

    /// Get the index URL for HTTP repositories
    pub fn index_url(&self) -> String {
        match &self.repo_type {
            RepositoryType::Http => {
                let base = self.url.trim_end_matches('/');
                format!("{}/index.yaml", base)
            }
            RepositoryType::Oci => self.url.clone(),
            RepositoryType::File => self.url.clone(),
        }
    }

    /// Check if this is an OCI repository
    pub fn is_oci(&self) -> bool {
        matches!(self.repo_type, RepositoryType::Oci)
    }

    /// Check if this is an HTTP repository
    pub fn is_http(&self) -> bool {
        matches!(self.repo_type, RepositoryType::Http)
    }
}

/// Repository type
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RepositoryType {
    /// Traditional HTTP repository with index.yaml
    #[default]
    Http,

    /// OCI-compliant registry
    Oci,

    /// Local filesystem
    File,
}

impl RepositoryType {
    /// Auto-detect repository type from URL
    pub fn detect(url: &str) -> Result<Self> {
        if url.starts_with("oci://") {
            Ok(RepositoryType::Oci)
        } else if url.starts_with("file://") || url.starts_with('/') {
            Ok(RepositoryType::File)
        } else if url.starts_with("http://") || url.starts_with("https://") {
            Ok(RepositoryType::Http)
        } else {
            Err(RepoError::InvalidRepositoryUrl {
                url: url.to_string(),
                reason: "URL must start with http://, https://, oci://, file://, or /".to_string(),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_repository_type_detection() {
        assert_eq!(
            RepositoryType::detect("https://charts.bitnami.com/bitnami").unwrap(),
            RepositoryType::Http
        );
        assert_eq!(
            RepositoryType::detect("oci://ghcr.io/myorg/charts").unwrap(),
            RepositoryType::Oci
        );
        assert_eq!(
            RepositoryType::detect("file:///path/to/repo").unwrap(),
            RepositoryType::File
        );
        assert_eq!(
            RepositoryType::detect("/absolute/path").unwrap(),
            RepositoryType::File
        );

        assert!(RepositoryType::detect("invalid").is_err());
    }

    #[test]
    fn test_repository_new() {
        let repo = Repository::new("bitnami", "https://charts.bitnami.com/bitnami").unwrap();
        assert_eq!(repo.name, "bitnami");
        assert_eq!(repo.repo_type, RepositoryType::Http);
        assert_eq!(
            repo.index_url(),
            "https://charts.bitnami.com/bitnami/index.yaml"
        );
    }

    #[test]
    fn test_config_add_remove() {
        let mut config = RepositoryConfig::default();

        let repo = Repository::new("test", "https://example.com").unwrap();
        config.add(repo).unwrap();

        assert!(config.get("test").is_some());
        assert!(config.add(Repository::new("test", "https://other.com").unwrap()).is_err());

        let removed = config.remove("test").unwrap();
        assert_eq!(removed.name, "test");
        assert!(config.get("test").is_none());
    }

    #[test]
    fn test_config_serialization() {
        let mut config = RepositoryConfig::default();
        config
            .add(Repository::new("bitnami", "https://charts.bitnami.com/bitnami").unwrap())
            .unwrap();

        let yaml = serde_yaml::to_string(&config).unwrap();
        assert!(yaml.contains("bitnami"));

        let parsed: RepositoryConfig = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(parsed.repositories.len(), 1);
    }
}
