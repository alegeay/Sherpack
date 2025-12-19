//! Pack definition and loading

use serde::{Deserialize, Serialize};
use semver::Version;
use std::path::{Path, PathBuf};
use std::time::Duration;

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

    /// CRD handling configuration
    #[serde(default)]
    pub crds: CrdConfig,
}

/// CRD handling configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CrdConfig {
    /// Install CRDs from crds/ directory (default: true)
    #[serde(default = "default_true")]
    pub install: bool,

    /// CRD upgrade behavior
    #[serde(default)]
    pub upgrade: CrdUpgradeConfig,

    /// CRD uninstall behavior
    #[serde(default)]
    pub uninstall: CrdUninstallConfig,

    /// Wait for CRDs to be Established before continuing (default: true)
    #[serde(default = "default_true")]
    pub wait_ready: bool,

    /// Timeout for CRD readiness (default: 60s)
    #[serde(default = "default_wait_timeout", with = "humantime_serde")]
    pub wait_timeout: Duration,
}

impl Default for CrdConfig {
    fn default() -> Self {
        Self {
            install: true,
            upgrade: CrdUpgradeConfig::default(),
            uninstall: CrdUninstallConfig::default(),
            wait_ready: true,
            wait_timeout: default_wait_timeout(),
        }
    }
}

fn default_wait_timeout() -> Duration {
    Duration::from_secs(60)
}

/// CRD upgrade strategy
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CrdUpgradeStrategy {
    /// Only allow safe, additive changes (default)
    #[default]
    Safe,
    /// Apply all changes (may break existing CRs)
    Force,
    /// Never update CRDs
    Skip,
}

/// CRD upgrade configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CrdUpgradeConfig {
    /// Allow CRD updates (default: true)
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// Upgrade strategy (default: safe)
    #[serde(default)]
    pub strategy: CrdUpgradeStrategy,
}

impl Default for CrdUpgradeConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            strategy: CrdUpgradeStrategy::Safe,
        }
    }
}

/// CRD uninstall configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CrdUninstallConfig {
    /// Keep CRDs on uninstall (default: true)
    /// If false, requires --confirm-crd-deletion flag
    #[serde(default = "default_true")]
    pub keep: bool,
}

impl Default for CrdUninstallConfig {
    fn default() -> Self {
        Self { keep: true }
    }
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

/// When to resolve a dependency
///
/// Controls whether a dependency is resolved/downloaded based on conditions.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ResolvePolicy {
    /// Always resolve, regardless of condition (useful for vendoring/caching)
    Always,

    /// Only resolve if condition evaluates to true (default)
    ///
    /// If no condition is set, behaves like `Always`.
    /// Evaluated against values.yaml at resolution time.
    #[default]
    WhenEnabled,

    /// Never resolve - dependency must already exist locally
    ///
    /// Useful for air-gapped environments where dependencies are pre-vendored.
    Never,
}

/// Pack dependency
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Dependency {
    /// Dependency name
    pub name: String,

    /// Version constraint (semver)
    pub version: String,

    /// Repository URL
    pub repository: String,

    /// Static enable/disable flag
    ///
    /// When `false`, this dependency is completely ignored during resolution.
    /// Unlike `condition`, this is evaluated at parse time, not against values.
    /// Defaults to `true`.
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// Runtime condition expression
    ///
    /// A dot-separated path evaluated against values.yaml.
    /// Example: `redis.enabled` checks `values.redis.enabled`.
    ///
    /// When combined with `resolve: when-enabled`, the condition is evaluated
    /// at resolution time to skip downloading disabled dependencies.
    #[serde(default)]
    pub condition: Option<String>,

    /// Resolution policy
    ///
    /// Controls when this dependency is resolved/downloaded.
    /// Defaults to `when-enabled`.
    #[serde(default)]
    pub resolve: ResolvePolicy,

    /// Tags for conditional inclusion
    #[serde(default)]
    pub tags: Vec<String>,

    /// Alias name (overrides dependency name in templates)
    #[serde(default)]
    pub alias: Option<String>,
}

impl Dependency {
    /// Get the effective name (alias if set, otherwise name)
    #[inline]
    pub fn effective_name(&self) -> &str {
        self.alias.as_deref().unwrap_or(&self.name)
    }

    /// Check if this dependency should be resolved given the current values
    ///
    /// Returns `false` if:
    /// - `enabled` is `false`
    /// - `resolve` is `Never`
    /// - `resolve` is `WhenEnabled` and condition evaluates to `false`
    pub fn should_resolve(&self, values: &serde_json::Value) -> bool {
        // Static disable always wins
        if !self.enabled {
            return false;
        }

        match self.resolve {
            ResolvePolicy::Always => true,
            ResolvePolicy::Never => false,
            ResolvePolicy::WhenEnabled => {
                // If no condition, treat as enabled
                let Some(condition) = &self.condition else {
                    return true;
                };

                evaluate_condition(condition, values)
            }
        }
    }
}

/// Evaluate a simple dot-path condition against values
///
/// Supports paths like `redis.enabled`, `features.cache.memory`.
/// Returns `true` if the path exists and is truthy.
fn evaluate_condition(condition: &str, values: &serde_json::Value) -> bool {
    let path: Vec<&str> = condition.split('.').collect();

    let mut current = values;
    for part in &path {
        match current.get(*part) {
            Some(v) => current = v,
            None => return false, // Path doesn't exist → falsy
        }
    }

    // Coerce to boolean
    match current {
        serde_json::Value::Bool(b) => *b,
        serde_json::Value::Null => false,
        serde_json::Value::String(s) => !s.is_empty() && s != "false" && s != "0",
        serde_json::Value::Number(n) => n.as_f64().map(|f| f != 0.0).unwrap_or(false),
        serde_json::Value::Array(a) => !a.is_empty(),
        serde_json::Value::Object(o) => !o.is_empty(),
    }
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

    /// CRDs directory (if present)
    pub crds_dir: Option<PathBuf>,

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

        // Detect crds/ directory
        let crds_dir = {
            let dir = root.join("crds");
            if dir.exists() && dir.is_dir() {
                Some(dir)
            } else {
                None
            }
        };

        Ok(Self {
            pack,
            root,
            templates_dir,
            crds_dir,
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

    /// Get list of CRD files from crds/ directory
    ///
    /// CRD files are not templated and are applied before regular templates.
    /// Files are sorted alphabetically for deterministic ordering.
    pub fn crd_files(&self) -> Result<Vec<PathBuf>> {
        let Some(crds_dir) = &self.crds_dir else {
            return Ok(Vec::new());
        };

        let mut files = Vec::new();

        for entry in walkdir::WalkDir::new(crds_dir)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path();
            if path.is_file() {
                // Only include YAML files (CRDs should be YAML)
                if let Some(ext) = path.extension() {
                    let ext = ext.to_string_lossy().to_lowercase();
                    if matches!(ext.as_str(), "yaml" | "yml") {
                        files.push(path.to_path_buf());
                    }
                }
            }
        }

        // Sort for deterministic ordering
        files.sort();
        Ok(files)
    }

    /// Check if this pack has CRDs
    pub fn has_crds(&self) -> bool {
        self.crds_dir.is_some()
    }

    /// Load all CRD manifests from crds/ directory
    ///
    /// CRD files may contain Jinja templating syntax. Files with templating
    /// are flagged with `is_templated: true` and should be rendered before use.
    pub fn load_crds(&self) -> Result<Vec<CrdManifest>> {
        let files = self.crd_files()?;
        let mut crds = Vec::new();

        for file_path in files {
            let content = std::fs::read_to_string(&file_path)?;
            let relative_path = file_path
                .strip_prefix(&self.root)
                .unwrap_or(&file_path)
                .to_path_buf();

            // Check if file contains Jinja syntax (applies to whole file)
            let file_is_templated = contains_jinja_syntax(&content);

            // Parse multi-document YAML
            for (idx, doc) in content.split("---").enumerate() {
                let doc = doc.trim();
                if doc.is_empty() || doc.lines().all(|l| l.trim().is_empty() || l.trim().starts_with('#')) {
                    continue;
                }

                // For templated files, we can't validate kind until after rendering
                // But we should still try to parse to catch obvious errors
                let is_templated = file_is_templated || contains_jinja_syntax(doc);

                if !is_templated {
                    // Validate it's a CRD (only for non-templated files)
                    let parsed: serde_yaml::Value = serde_yaml::from_str(doc)?;
                    let kind = parsed.get("kind").and_then(|k| k.as_str());

                    if kind != Some("CustomResourceDefinition") {
                        return Err(CoreError::InvalidPack {
                            message: format!(
                                "File {} contains non-CRD resource (kind: {}). Only CustomResourceDefinition is allowed in crds/ directory",
                                relative_path.display(),
                                kind.unwrap_or("unknown")
                            ),
                        });
                    }

                    let name = parsed
                        .get("metadata")
                        .and_then(|m| m.get("name"))
                        .and_then(|n| n.as_str())
                        .unwrap_or("unknown")
                        .to_string();

                    crds.push(CrdManifest {
                        name,
                        source_file: relative_path.clone(),
                        document_index: idx,
                        content: doc.to_string(),
                        is_templated: false,
                    });
                } else {
                    // For templated CRDs, use a placeholder name
                    // The real name will be extracted after rendering
                    crds.push(CrdManifest {
                        name: format!("templated-{}-{}", relative_path.display(), idx),
                        source_file: relative_path.clone(),
                        document_index: idx,
                        content: doc.to_string(),
                        is_templated: true,
                    });
                }
            }
        }

        Ok(crds)
    }

    /// Get only static (non-templated) CRDs
    pub fn static_crds(&self) -> Result<Vec<CrdManifest>> {
        Ok(self.load_crds()?.into_iter().filter(|c| !c.is_templated).collect())
    }

    /// Get only templated CRDs (need rendering before use)
    pub fn templated_crds(&self) -> Result<Vec<CrdManifest>> {
        Ok(self.load_crds()?.into_iter().filter(|c| c.is_templated).collect())
    }

    /// Check if this pack has templated CRDs
    pub fn has_templated_crds(&self) -> Result<bool> {
        Ok(self.load_crds()?.iter().any(|c| c.is_templated))
    }
}

/// A CRD manifest loaded from crds/ directory
#[derive(Debug, Clone)]
pub struct CrdManifest {
    /// CRD name (metadata.name)
    pub name: String,
    /// Source file path (relative to pack root)
    pub source_file: PathBuf,
    /// Document index within the file (for multi-document YAML)
    pub document_index: usize,
    /// Raw YAML content
    pub content: String,
    /// Whether this CRD contains Jinja templating syntax
    pub is_templated: bool,
}

/// Check if content contains Jinja templating syntax
fn contains_jinja_syntax(content: &str) -> bool {
    content.contains("{{") || content.contains("{%") || content.contains("{#")
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
    use serde_json::json;

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

    #[test]
    fn test_dependency_defaults() {
        let yaml = r#"
name: redis
version: "^7.0"
repository: https://repo.example.com
"#;
        let dep: Dependency = serde_yaml::from_str(yaml).unwrap();

        assert_eq!(dep.name, "redis");
        assert!(dep.enabled); // default: true
        assert_eq!(dep.resolve, ResolvePolicy::WhenEnabled); // default
        assert!(dep.condition.is_none());
        assert!(dep.alias.is_none());
    }

    #[test]
    fn test_dependency_with_all_fields() {
        let yaml = r#"
name: postgresql
version: "^12.0"
repository: https://charts.bitnami.com
enabled: false
condition: database.postgresql.enabled
resolve: always
alias: db
tags:
  - database
  - backend
"#;
        let dep: Dependency = serde_yaml::from_str(yaml).unwrap();

        assert_eq!(dep.name, "postgresql");
        assert!(!dep.enabled);
        assert_eq!(dep.resolve, ResolvePolicy::Always);
        assert_eq!(dep.condition.as_deref(), Some("database.postgresql.enabled"));
        assert_eq!(dep.alias.as_deref(), Some("db"));
        assert_eq!(dep.effective_name(), "db");
        assert_eq!(dep.tags, vec!["database", "backend"]);
    }

    #[test]
    fn test_resolve_policy_serialization() {
        assert_eq!(
            serde_yaml::to_string(&ResolvePolicy::Always).unwrap().trim(),
            "always"
        );
        assert_eq!(
            serde_yaml::to_string(&ResolvePolicy::WhenEnabled).unwrap().trim(),
            "when-enabled"
        );
        assert_eq!(
            serde_yaml::to_string(&ResolvePolicy::Never).unwrap().trim(),
            "never"
        );
    }

    #[test]
    fn test_evaluate_condition_simple_bool() {
        let values = json!({
            "redis": {
                "enabled": true
            },
            "postgresql": {
                "enabled": false
            }
        });

        assert!(evaluate_condition("redis.enabled", &values));
        assert!(!evaluate_condition("postgresql.enabled", &values));
    }

    #[test]
    fn test_evaluate_condition_nested_path() {
        let values = json!({
            "features": {
                "cache": {
                    "redis": {
                        "enabled": true
                    }
                }
            }
        });

        assert!(evaluate_condition("features.cache.redis.enabled", &values));
        assert!(!evaluate_condition("features.cache.memcached.enabled", &values));
    }

    #[test]
    fn test_evaluate_condition_missing_path() {
        let values = json!({
            "redis": {}
        });

        assert!(!evaluate_condition("redis.enabled", &values));
        assert!(!evaluate_condition("nonexistent.path", &values));
    }

    #[test]
    fn test_evaluate_condition_truthy_values() {
        let values = json!({
            "string_true": "yes",
            "string_false": "false",
            "string_zero": "0",
            "string_empty": "",
            "number_one": 1,
            "number_zero": 0,
            "array_empty": [],
            "array_full": [1, 2],
            "object_empty": {},
            "object_full": {"key": "value"},
            "null_val": null
        });

        assert!(evaluate_condition("string_true", &values));
        assert!(!evaluate_condition("string_false", &values));
        assert!(!evaluate_condition("string_zero", &values));
        assert!(!evaluate_condition("string_empty", &values));
        assert!(evaluate_condition("number_one", &values));
        assert!(!evaluate_condition("number_zero", &values));
        assert!(!evaluate_condition("array_empty", &values));
        assert!(evaluate_condition("array_full", &values));
        assert!(!evaluate_condition("object_empty", &values));
        assert!(evaluate_condition("object_full", &values));
        assert!(!evaluate_condition("null_val", &values));
    }

    #[test]
    fn test_should_resolve_disabled() {
        let dep = Dependency {
            name: "redis".to_string(),
            version: "^7.0".to_string(),
            repository: "https://repo.example.com".to_string(),
            enabled: false,
            condition: None,
            resolve: ResolvePolicy::Always,
            tags: vec![],
            alias: None,
        };

        // enabled: false always wins, even with resolve: always
        assert!(!dep.should_resolve(&json!({})));
    }

    #[test]
    fn test_should_resolve_never() {
        let dep = Dependency {
            name: "redis".to_string(),
            version: "^7.0".to_string(),
            repository: "https://repo.example.com".to_string(),
            enabled: true,
            condition: None,
            resolve: ResolvePolicy::Never,
            tags: vec![],
            alias: None,
        };

        assert!(!dep.should_resolve(&json!({})));
    }

    #[test]
    fn test_should_resolve_always() {
        let dep = Dependency {
            name: "redis".to_string(),
            version: "^7.0".to_string(),
            repository: "https://repo.example.com".to_string(),
            enabled: true,
            condition: Some("redis.enabled".to_string()),
            resolve: ResolvePolicy::Always,
            tags: vec![],
            alias: None,
        };

        // resolve: always ignores condition
        assert!(dep.should_resolve(&json!({"redis": {"enabled": false}})));
    }

    #[test]
    fn test_should_resolve_when_enabled_no_condition() {
        let dep = Dependency {
            name: "redis".to_string(),
            version: "^7.0".to_string(),
            repository: "https://repo.example.com".to_string(),
            enabled: true,
            condition: None,
            resolve: ResolvePolicy::WhenEnabled,
            tags: vec![],
            alias: None,
        };

        // No condition = always resolve
        assert!(dep.should_resolve(&json!({})));
    }

    #[test]
    fn test_should_resolve_when_enabled_with_condition() {
        let dep = Dependency {
            name: "redis".to_string(),
            version: "^7.0".to_string(),
            repository: "https://repo.example.com".to_string(),
            enabled: true,
            condition: Some("redis.enabled".to_string()),
            resolve: ResolvePolicy::WhenEnabled,
            tags: vec![],
            alias: None,
        };

        assert!(dep.should_resolve(&json!({"redis": {"enabled": true}})));
        assert!(!dep.should_resolve(&json!({"redis": {"enabled": false}})));
        assert!(!dep.should_resolve(&json!({}))); // Missing = false
    }

    // ==========================================
    // CRD Configuration Tests
    // ==========================================

    #[test]
    fn test_crd_config_defaults() {
        let config = CrdConfig::default();

        assert!(config.install);
        assert!(config.upgrade.enabled);
        assert_eq!(config.upgrade.strategy, CrdUpgradeStrategy::Safe);
        assert!(config.uninstall.keep);
        assert!(config.wait_ready);
        assert_eq!(config.wait_timeout, Duration::from_secs(60));
    }

    #[test]
    fn test_crd_config_deserialize_defaults() {
        let yaml = r#"
apiVersion: sherpack/v1
kind: application
metadata:
  name: test
  version: 1.0.0
"#;
        let pack: Pack = serde_yaml::from_str(yaml).unwrap();

        // All CRD config should use defaults
        assert!(pack.crds.install);
        assert!(pack.crds.wait_ready);
    }

    #[test]
    fn test_crd_config_deserialize_custom() {
        let yaml = r#"
apiVersion: sherpack/v1
kind: application
metadata:
  name: test
  version: 1.0.0
crds:
  install: false
  upgrade:
    enabled: true
    strategy: force
  uninstall:
    keep: false
  waitReady: true
  waitTimeout: 120s
"#;
        let pack: Pack = serde_yaml::from_str(yaml).unwrap();

        assert!(!pack.crds.install);
        assert!(pack.crds.upgrade.enabled);
        assert_eq!(pack.crds.upgrade.strategy, CrdUpgradeStrategy::Force);
        assert!(!pack.crds.uninstall.keep);
        assert!(pack.crds.wait_ready);
        assert_eq!(pack.crds.wait_timeout, Duration::from_secs(120));
    }

    #[test]
    fn test_crd_upgrade_strategy_serialization() {
        assert_eq!(
            serde_yaml::to_string(&CrdUpgradeStrategy::Safe).unwrap().trim(),
            "safe"
        );
        assert_eq!(
            serde_yaml::to_string(&CrdUpgradeStrategy::Force).unwrap().trim(),
            "force"
        );
        assert_eq!(
            serde_yaml::to_string(&CrdUpgradeStrategy::Skip).unwrap().trim(),
            "skip"
        );
    }

    #[test]
    fn test_crd_manifest() {
        let manifest = CrdManifest {
            name: "myresources.example.com".to_string(),
            source_file: PathBuf::from("crds/myresource.yaml"),
            document_index: 0,
            content: "apiVersion: apiextensions.k8s.io/v1\nkind: CustomResourceDefinition".to_string(),
            is_templated: false,
        };

        assert_eq!(manifest.name, "myresources.example.com");
        assert_eq!(manifest.source_file, PathBuf::from("crds/myresource.yaml"));
        assert!(!manifest.is_templated);
    }

    #[test]
    fn test_contains_jinja_syntax() {
        assert!(contains_jinja_syntax("{{ values.name }}"));
        assert!(contains_jinja_syntax("{% if condition %}"));
        assert!(contains_jinja_syntax("{# comment #}"));
        assert!(!contains_jinja_syntax("plain: yaml"));
        assert!(!contains_jinja_syntax("name: test"));
    }

    #[test]
    fn test_crd_manifest_templated() {
        let manifest = CrdManifest {
            name: "templated-crd-0".to_string(),
            source_file: PathBuf::from("crds/dynamic-crd.yaml"),
            document_index: 0,
            content: "name: {{ values.crdName }}".to_string(),
            is_templated: true,
        };

        assert!(manifest.is_templated);
    }
}
