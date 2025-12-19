//! Chart.yaml to Pack.yaml converter
//!
//! Converts Helm Chart.yaml metadata to Sherpack Pack.yaml format.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ChartError {
    #[error("Failed to parse Chart.yaml: {0}")]
    Parse(#[from] serde_yaml::Error),

    #[error("Missing required field: {0}")]
    MissingField(String),

    #[error("Invalid version: {0}")]
    InvalidVersion(String),
}

/// Helm Chart.yaml structure
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HelmChart {
    /// API version (v1 or v2)
    pub api_version: String,

    /// Chart name
    pub name: String,

    /// Chart version (SemVer)
    pub version: String,

    /// Kubernetes version constraint
    #[serde(default)]
    pub kube_version: Option<String>,

    /// Chart description
    #[serde(default)]
    pub description: Option<String>,

    /// Chart type (application or library)
    #[serde(default, rename = "type")]
    pub chart_type: Option<String>,

    /// Keywords for searching
    #[serde(default)]
    pub keywords: Vec<String>,

    /// Project home page
    #[serde(default)]
    pub home: Option<String>,

    /// Source code URLs
    #[serde(default)]
    pub sources: Vec<String>,

    /// Chart dependencies
    #[serde(default)]
    pub dependencies: Vec<HelmDependency>,

    /// Maintainers
    #[serde(default)]
    pub maintainers: Vec<HelmMaintainer>,

    /// Icon URL
    #[serde(default)]
    pub icon: Option<String>,

    /// App version
    #[serde(default)]
    pub app_version: Option<String>,

    /// Whether chart is deprecated
    #[serde(default)]
    pub deprecated: bool,

    /// Annotations
    #[serde(default)]
    pub annotations: BTreeMap<String, String>,
}

/// Helm dependency
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HelmDependency {
    /// Dependency name
    pub name: String,

    /// Version constraint
    pub version: String,

    /// Repository URL
    #[serde(default)]
    pub repository: Option<String>,

    /// Condition to enable
    #[serde(default)]
    pub condition: Option<String>,

    /// Tags for grouping
    #[serde(default)]
    pub tags: Vec<String>,

    /// Import values
    #[serde(default)]
    pub import_values: Vec<serde_yaml::Value>,

    /// Alias name
    #[serde(default)]
    pub alias: Option<String>,
}

/// Helm maintainer
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct HelmMaintainer {
    /// Maintainer name
    pub name: String,

    /// Email address
    #[serde(default)]
    pub email: Option<String>,

    /// URL
    #[serde(default)]
    pub url: Option<String>,
}

/// Sherpack Pack.yaml structure
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SherpackPack {
    /// API version
    pub api_version: String,

    /// Pack kind
    pub kind: String,

    /// Pack name
    pub name: String,

    /// Pack version
    pub version: String,

    /// Kubernetes version constraint
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kube_version: Option<String>,

    /// Description
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Keywords
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub keywords: Vec<String>,

    /// Home page
    #[serde(skip_serializing_if = "Option::is_none")]
    pub home: Option<String>,

    /// Source URLs
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub sources: Vec<String>,

    /// Dependencies
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub dependencies: Vec<SherpackDependency>,

    /// Maintainers
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub maintainers: Vec<HelmMaintainer>,

    /// Icon URL
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,

    /// App version
    #[serde(skip_serializing_if = "Option::is_none")]
    pub app_version: Option<String>,

    /// Annotations
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub annotations: BTreeMap<String, String>,
}

/// Sherpack dependency
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SherpackDependency {
    /// Dependency name
    pub name: String,

    /// Version constraint
    pub version: String,

    /// Repository URL
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repository: Option<String>,

    /// Condition to enable
    #[serde(skip_serializing_if = "Option::is_none")]
    pub condition: Option<String>,

    /// Tags for grouping
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,

    /// Alias name
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alias: Option<String>,
}

impl HelmChart {
    /// Parse a Chart.yaml string
    pub fn parse(content: &str) -> Result<Self, ChartError> {
        let chart: HelmChart = serde_yaml::from_str(content)?;

        // Validate required fields
        if chart.name.is_empty() {
            return Err(ChartError::MissingField("name".to_string()));
        }
        if chart.version.is_empty() {
            return Err(ChartError::MissingField("version".to_string()));
        }

        Ok(chart)
    }

    /// Convert to Sherpack Pack
    pub fn to_sherpack(&self) -> SherpackPack {
        let kind = match self.chart_type.as_deref() {
            Some("library") => "library".to_string(),
            _ => "application".to_string(),
        };

        let dependencies: Vec<SherpackDependency> = self
            .dependencies
            .iter()
            .map(|d| SherpackDependency {
                name: d.name.clone(),
                version: d.version.clone(),
                repository: d.repository.clone(),
                condition: d.condition.clone(),
                tags: d.tags.clone(),
                alias: d.alias.clone(),
            })
            .collect();

        // Handle deprecated annotation
        let mut annotations = self.annotations.clone();
        if self.deprecated {
            annotations.insert("sherpack.io/deprecated".to_string(), "true".to_string());
        }

        SherpackPack {
            api_version: "sherpack/v1".to_string(),
            kind,
            name: self.name.clone(),
            version: self.version.clone(),
            kube_version: self.kube_version.clone(),
            description: self.description.clone(),
            keywords: self.keywords.clone(),
            home: self.home.clone(),
            sources: self.sources.clone(),
            dependencies,
            maintainers: self.maintainers.clone(),
            icon: self.icon.clone(),
            app_version: self.app_version.clone(),
            annotations,
        }
    }
}

impl SherpackPack {
    /// Serialize to YAML string
    pub fn to_yaml(&self) -> Result<String, serde_yaml::Error> {
        // Create a custom serialization with proper ordering
        // Sherpack expects: apiVersion, kind, metadata: { name, version, ... }, dependencies, engine
        let mut yaml = String::new();

        yaml.push_str(&format!("apiVersion: {}\n", self.api_version));
        yaml.push_str(&format!("kind: {}\n", self.kind));
        yaml.push_str("\nmetadata:\n");
        yaml.push_str(&format!("  name: {}\n", self.name));
        yaml.push_str(&format!("  version: {}\n", self.version));

        if let Some(ref app_version) = self.app_version {
            yaml.push_str(&format!("  appVersion: \"{}\"\n", app_version));
        }

        if let Some(ref description) = self.description {
            yaml.push_str(&format!("  description: {}\n", description));
        }

        if let Some(ref kube_version) = self.kube_version {
            // Quote kubeVersion if it contains special characters
            if kube_version.starts_with('>') || kube_version.starts_with('<') || kube_version.starts_with('=') {
                yaml.push_str(&format!("  kubeVersion: \"{}\"\n", kube_version));
            } else {
                yaml.push_str(&format!("  kubeVersion: {}\n", kube_version));
            }
        }

        if let Some(ref home) = self.home {
            yaml.push_str(&format!("  home: {}\n", home));
        }

        if let Some(ref icon) = self.icon {
            yaml.push_str(&format!("  icon: {}\n", icon));
        }

        if !self.sources.is_empty() {
            yaml.push_str("  sources:\n");
            for source in &self.sources {
                yaml.push_str(&format!("    - {}\n", source));
            }
        }

        if !self.keywords.is_empty() {
            yaml.push_str("  keywords:\n");
            for keyword in &self.keywords {
                yaml.push_str(&format!("    - {}\n", keyword));
            }
        }

        if !self.maintainers.is_empty() {
            yaml.push_str("  maintainers:\n");
            for maintainer in &self.maintainers {
                yaml.push_str(&format!("    - name: {}\n", maintainer.name));
                if let Some(ref email) = maintainer.email {
                    yaml.push_str(&format!("      email: {}\n", email));
                }
                if let Some(ref url) = maintainer.url {
                    yaml.push_str(&format!("      url: {}\n", url));
                }
            }
        }

        if !self.annotations.is_empty() {
            yaml.push_str("  annotations:\n");
            for (key, value) in &self.annotations {
                // Handle multiline values with YAML block scalar
                if value.contains('\n') {
                    yaml.push_str(&format!("    {}: |\n", key));
                    for line in value.lines() {
                        yaml.push_str(&format!("      {}\n", line));
                    }
                } else {
                    // Escape the value properly
                    let escaped = value.replace('\\', "\\\\").replace('"', "\\\"");
                    yaml.push_str(&format!("    {}: \"{}\"\n", key, escaped));
                }
            }
        }

        if !self.dependencies.is_empty() {
            yaml.push('\n');
            yaml.push_str("dependencies:\n");
            for dep in &self.dependencies {
                yaml.push_str(&format!("  - name: {}\n", dep.name));
                yaml.push_str(&format!("    version: \"{}\"\n", dep.version));
                if let Some(ref repo) = dep.repository {
                    yaml.push_str(&format!("    repository: {}\n", repo));
                }
                if let Some(ref condition) = dep.condition {
                    yaml.push_str(&format!("    condition: {}\n", condition));
                }
                if let Some(ref alias) = dep.alias {
                    yaml.push_str(&format!("    alias: {}\n", alias));
                }
                if !dep.tags.is_empty() {
                    yaml.push_str("    tags:\n");
                    for tag in &dep.tags {
                        yaml.push_str(&format!("      - {}\n", tag));
                    }
                }
            }
        }

        Ok(yaml)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_chart() {
        let content = r#"
apiVersion: v2
name: my-app
version: 1.0.0
description: My application
"#;
        let chart = HelmChart::parse(content).unwrap();
        assert_eq!(chart.name, "my-app");
        assert_eq!(chart.version, "1.0.0");
        assert_eq!(chart.description, Some("My application".to_string()));
    }

    #[test]
    fn test_convert_to_sherpack() {
        let content = r#"
apiVersion: v2
name: my-app
version: 1.0.0
type: application
"#;
        let chart = HelmChart::parse(content).unwrap();
        let pack = chart.to_sherpack();

        assert_eq!(pack.api_version, "sherpack/v1");
        assert_eq!(pack.kind, "application");
        assert_eq!(pack.name, "my-app");
    }

    #[test]
    fn test_convert_library() {
        let content = r#"
apiVersion: v2
name: my-lib
version: 1.0.0
type: library
"#;
        let chart = HelmChart::parse(content).unwrap();
        let pack = chart.to_sherpack();

        assert_eq!(pack.kind, "library");
    }

    #[test]
    fn test_convert_with_dependencies() {
        let content = r#"
apiVersion: v2
name: my-app
version: 1.0.0
dependencies:
  - name: postgresql
    version: "12.x"
    repository: https://charts.bitnami.com/bitnami
    condition: postgresql.enabled
"#;
        let chart = HelmChart::parse(content).unwrap();
        let pack = chart.to_sherpack();

        assert_eq!(pack.dependencies.len(), 1);
        assert_eq!(pack.dependencies[0].name, "postgresql");
        assert_eq!(pack.dependencies[0].version, "12.x");
    }

    #[test]
    fn test_yaml_output() {
        let content = r#"
apiVersion: v2
name: my-app
version: 1.0.0
appVersion: "2.0.0"
description: My application
"#;
        let chart = HelmChart::parse(content).unwrap();
        let pack = chart.to_sherpack();
        let yaml = pack.to_yaml().unwrap();

        assert!(yaml.contains("apiVersion: sherpack/v1"));
        assert!(yaml.contains("kind: application"));
        assert!(yaml.contains("metadata:"));
        assert!(yaml.contains("  name: my-app"));
        assert!(yaml.contains("appVersion: \"2.0.0\""));
    }

    #[test]
    fn test_deprecated_annotation() {
        let content = r#"
apiVersion: v2
name: old-app
version: 1.0.0
deprecated: true
"#;
        let chart = HelmChart::parse(content).unwrap();
        let pack = chart.to_sherpack();

        assert!(pack.annotations.contains_key("sherpack.io/deprecated"));
    }
}
