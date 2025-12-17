//! Pack definition and loading

use serde::{Deserialize, Serialize};
use semver::Version;
use std::path::{Path, PathBuf};

use crate::error::{CoreError, Result};

/// A Sherpack Pack - equivalent to a Helm Chart
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Pack {
    /// API version (sherpack/v1)
    pub api_version: String,

    /// Pack type
    #[serde(default)]
    pub kind: PackKind,

    /// Pack metadata
    pub metadata: PackMetadata,

    /// Dependencies
    #[serde(default)]
    pub dependencies: Vec<Dependency>,

    /// Engine configuration
    #[serde(default)]
    pub engine: EngineConfig,
}

/// Pack type
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum PackKind {
    #[default]
    Application,
    Library,
}

/// Pack metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PackMetadata {
    /// Pack name (required)
    pub name: String,

    /// Pack version (required, SemVer)
    #[serde(with = "version_serde")]
    pub version: Version,

    /// Description
    #[serde(default)]
    pub description: Option<String>,

    /// Application version
    #[serde(default)]
    pub app_version: Option<String>,

    /// Kubernetes version constraint
    #[serde(default)]
    pub kube_version: Option<String>,

    /// Home URL
    #[serde(default)]
    pub home: Option<String>,

    /// Icon URL
    #[serde(default)]
    pub icon: Option<String>,

    /// Source URLs
    #[serde(default)]
    pub sources: Vec<String>,

    /// Keywords
    #[serde(default)]
    pub keywords: Vec<String>,

    /// Maintainers
    #[serde(default)]
    pub maintainers: Vec<Maintainer>,

    /// Annotations
    #[serde(default)]
    pub annotations: std::collections::HashMap<String, String>,
}

/// Maintainer information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Maintainer {
    pub name: String,
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
}

/// Pack dependency
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Dependency {
    /// Dependency name
    pub name: String,

    /// Version constraint
    pub version: String,

    /// Repository URL
    pub repository: String,

    /// Condition to enable
    #[serde(default)]
    pub condition: Option<String>,

    /// Tags
    #[serde(default)]
    pub tags: Vec<String>,

    /// Alias name
    #[serde(default)]
    pub alias: Option<String>,
}

/// Engine configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineConfig {
    /// Fail on undefined variables
    #[serde(default = "default_true")]
    pub strict: bool,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self { strict: true }
    }
}

fn default_true() -> bool {
    true
}

/// Loaded pack with resolved paths
#[derive(Debug, Clone)]
pub struct LoadedPack {
    /// Pack definition
    pub pack: Pack,

    /// Root directory of the pack
    pub root: PathBuf,

    /// Templates directory
    pub templates_dir: PathBuf,

    /// Values file path
    pub values_path: PathBuf,

    /// Schema file path (if present)
    pub schema_path: Option<PathBuf>,
}

impl LoadedPack {
    /// Load a pack from a directory
    pub fn load<P: AsRef<Path>>(path: P) -> Result<Self> {
        let root = path.as_ref().to_path_buf();

        if !root.exists() {
            return Err(CoreError::PackNotFound {
                path: root.display().to_string(),
            });
        }

        // Load Pack.yaml
        let pack_file = root.join("Pack.yaml");
        if !pack_file.exists() {
            return Err(CoreError::InvalidPack {
                message: format!("Pack.yaml not found in {}", root.display()),
            });
        }

        let pack_content = std::fs::read_to_string(&pack_file)?;
        let pack: Pack = serde_yaml::from_str(&pack_content)?;

        // Validate
        if pack.api_version != "sherpack/v1" {
            return Err(CoreError::InvalidPack {
                message: format!(
                    "Unsupported API version: {}. Expected: sherpack/v1",
                    pack.api_version
                ),
            });
        }

        let templates_dir = root.join("templates");
        let values_path = root.join("values.yaml");
        let schema_path = Self::find_schema_file(&root);

        Ok(Self {
            pack,
            root,
            templates_dir,
            values_path,
            schema_path,
        })
    }

    /// Find schema file, checking multiple standard locations
    fn find_schema_file(root: &Path) -> Option<PathBuf> {
        let candidates = [
            "values.schema.yaml", // Sherpack default
            "values.schema.json", // JSON Schema (Helm compatible)
            "schema.yaml",
            "schema.json",
        ];

        for candidate in candidates {
            let path = root.join(candidate);
            if path.exists() {
                return Some(path);
            }
        }
        None
    }

    /// Load the schema if present
    pub fn load_schema(&self) -> Result<Option<crate::schema::Schema>> {
        match &self.schema_path {
            Some(path) => Ok(Some(crate::schema::Schema::from_file(path)?)),
            None => Ok(None),
        }
    }

    /// Get list of template files
    pub fn template_files(&self) -> Result<Vec<PathBuf>> {
        let mut files = Vec::new();

        if !self.templates_dir.exists() {
            return Ok(files);
        }

        for entry in walkdir::WalkDir::new(&self.templates_dir)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path();
            if path.is_file() {
                // Include .yaml, .yml, .j2, .jinja2, .txt files
                if let Some(ext) = path.extension() {
                    let ext = ext.to_string_lossy().to_lowercase();
                    if matches!(ext.as_str(), "yaml" | "yml" | "j2" | "jinja2" | "txt" | "json") {
                        files.push(path.to_path_buf());
                    }
                }
            }
        }

        files.sort();
        Ok(files)
    }
}

/// Custom serde for semver::Version
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
    fn test_pack_deserialize() {
        let yaml = r#"
apiVersion: sherpack/v1
kind: application
metadata:
  name: myapp
  version: 1.0.0
  description: My application
"#;
        let pack: Pack = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(pack.metadata.name, "myapp");
        assert_eq!(pack.metadata.version.to_string(), "1.0.0");
        assert_eq!(pack.kind, PackKind::Application);
    }
}
