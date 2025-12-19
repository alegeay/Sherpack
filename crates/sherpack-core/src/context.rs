//! Template rendering context

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

use crate::pack::PackMetadata;
use crate::release::ReleaseInfo;
use crate::values::Values;

/// Context available to all templates
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateContext {
    /// User values (merged)
    pub values: JsonValue,

    /// Release information
    pub release: ReleaseInfo,

    /// Pack metadata
    pub pack: PackInfo,

    /// Cluster capabilities
    pub capabilities: Capabilities,

    /// Current template info
    pub template: TemplateInfo,
}

/// Pack information for templates
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PackInfo {
    /// Pack name
    pub name: String,

    /// Pack version
    pub version: String,

    /// App version
    pub app_version: Option<String>,
}

impl From<&PackMetadata> for PackInfo {
    fn from(meta: &PackMetadata) -> Self {
        Self {
            name: meta.name.clone(),
            version: meta.version.to_string(),
            app_version: meta.app_version.clone(),
        }
    }
}

/// Cluster capabilities
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Capabilities {
    /// Kubernetes version
    pub kube_version: KubeVersion,

    /// Available API versions
    pub api_versions: Vec<String>,
}

/// Kubernetes version info
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KubeVersion {
    pub version: String,
    pub major: String,
    pub minor: String,
}

impl Default for KubeVersion {
    fn default() -> Self {
        // Default to a recent stable Kubernetes version for lint/template modes
        Self {
            version: "v1.28.0".to_string(),
            major: "1".to_string(),
            minor: "28".to_string(),
        }
    }
}

impl KubeVersion {
    pub fn new(version: &str) -> Self {
        let version = version.trim_start_matches('v');
        let parts: Vec<&str> = version.split('.').collect();

        Self {
            version: format!("v{}", version),
            major: parts.first().unwrap_or(&"1").to_string(),
            minor: parts.get(1).unwrap_or(&"28").to_string(),
        }
    }
}

/// Current template information
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TemplateInfo {
    /// Template name (filename)
    pub name: String,

    /// Base path
    pub base_path: String,
}

impl TemplateContext {
    /// Create a new template context
    pub fn new(values: Values, release: ReleaseInfo, pack: &PackMetadata) -> Self {
        Self {
            values: values.into_inner(),
            release,
            pack: PackInfo::from(pack),
            capabilities: Capabilities::default(),
            template: TemplateInfo::default(),
        }
    }

    /// Set the current template info
    pub fn with_template(mut self, name: &str, base_path: &str) -> Self {
        self.template = TemplateInfo {
            name: name.to_string(),
            base_path: base_path.to_string(),
        };
        self
    }

    /// Set capabilities
    pub fn with_capabilities(mut self, capabilities: Capabilities) -> Self {
        self.capabilities = capabilities;
        self
    }

    /// Convert to minijinja-compatible context
    pub fn to_json(&self) -> JsonValue {
        serde_json::to_value(self).unwrap_or(JsonValue::Null)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use semver::Version;

    #[test]
    fn test_template_context() {
        let values = Values::from_yaml("replicas: 3").unwrap();
        let release = ReleaseInfo::for_install("myapp", "default");
        let pack = PackMetadata {
            name: "mypack".to_string(),
            version: Version::new(1, 0, 0),
            description: None,
            app_version: Some("2.0.0".to_string()),
            kube_version: None,
            home: None,
            icon: None,
            sources: vec![],
            keywords: vec![],
            maintainers: vec![],
            annotations: Default::default(),
        };

        let ctx = TemplateContext::new(values, release, &pack);

        assert_eq!(ctx.pack.name, "mypack");
        assert_eq!(ctx.release.name, "myapp");
        assert!(ctx.release.is_install);
    }
}
