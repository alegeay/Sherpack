//! CRD Schema representation for comparison and analysis
//!
//! This module provides structured types for representing CRD schemas,
//! enabling semantic comparison between versions rather than raw YAML diffing.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// A parsed CustomResourceDefinition ready for comparison
///
/// This is a simplified representation focused on the fields that matter
/// for upgrade safety analysis.
#[derive(Debug, Clone, PartialEq)]
pub struct CrdSchema {
    /// Full CRD name (e.g., "certificates.cert-manager.io")
    pub name: String,
    /// API group (e.g., "cert-manager.io")
    pub group: String,
    /// Resource scope
    pub scope: CrdScope,
    /// Resource names (kind, plural, singular, shortNames)
    pub names: CrdNames,
    /// API versions with their schemas
    pub versions: Vec<CrdVersionSchema>,
}

impl CrdSchema {
    /// Get the storage version
    pub fn storage_version(&self) -> Option<&CrdVersionSchema> {
        self.versions.iter().find(|v| v.storage)
    }

    /// Get all served versions
    pub fn served_versions(&self) -> impl Iterator<Item = &CrdVersionSchema> {
        self.versions.iter().filter(|v| v.served)
    }

    /// Check if a specific version exists
    pub fn has_version(&self, name: &str) -> bool {
        self.versions.iter().any(|v| v.name == name)
    }
}

/// CRD scope - whether resources are namespaced or cluster-wide
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum CrdScope {
    #[default]
    Namespaced,
    Cluster,
}

impl std::fmt::Display for CrdScope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Namespaced => write!(f, "Namespaced"),
            Self::Cluster => write!(f, "Cluster"),
        }
    }
}

/// CRD naming information
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CrdNames {
    /// Kind (e.g., "Certificate")
    pub kind: String,
    /// Plural name (e.g., "certificates")
    pub plural: String,
    /// Singular name (e.g., "certificate")
    pub singular: Option<String>,
    /// Short names for kubectl (e.g., ["cert", "certs"])
    pub short_names: Vec<String>,
    /// List kind (e.g., "CertificateList")
    pub list_kind: Option<String>,
    /// Categories for grouping in kubectl (e.g., ["all"])
    pub categories: Vec<String>,
}

/// A single API version of a CRD
#[derive(Debug, Clone, PartialEq)]
pub struct CrdVersionSchema {
    /// Version name (e.g., "v1", "v1beta1", "v1alpha1")
    pub name: String,
    /// Whether this version is served by the API server
    pub served: bool,
    /// Whether this is the storage version
    pub storage: bool,
    /// Whether this version is deprecated
    pub deprecated: bool,
    /// Deprecation warning message
    pub deprecation_warning: Option<String>,
    /// OpenAPI v3 schema for validation
    pub schema: Option<OpenApiSchema>,
    /// Additional printer columns for kubectl
    pub printer_columns: Vec<PrinterColumn>,
    /// Subresources configuration
    pub subresources: Subresources,
}

impl CrdVersionSchema {
    /// Check if this version has a schema
    pub fn has_schema(&self) -> bool {
        self.schema.is_some()
    }

    /// Get the root spec schema if present
    pub fn spec_schema(&self) -> Option<&SchemaProperty> {
        self.schema.as_ref().and_then(|s| s.properties.get("spec"))
    }

    /// Get the root status schema if present
    pub fn status_schema(&self) -> Option<&SchemaProperty> {
        self.schema
            .as_ref()
            .and_then(|s| s.properties.get("status"))
    }
}

/// OpenAPI v3 schema for CRD validation
///
/// This is a simplified representation focusing on validation-relevant fields.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct OpenApiSchema {
    /// Root properties (typically: apiVersion, kind, metadata, spec, status)
    pub properties: BTreeMap<String, SchemaProperty>,
    /// Required field names at root level
    pub required: Vec<String>,
    /// Whether to preserve unknown fields
    pub x_preserve_unknown: bool,
}

impl OpenApiSchema {
    /// Check if a property exists at root level
    pub fn has_property(&self, name: &str) -> bool {
        self.properties.contains_key(name)
    }

    /// Check if a property is required
    pub fn is_required(&self, name: &str) -> bool {
        self.required.contains(&name.to_string())
    }
}

/// Schema for a single property
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SchemaProperty {
    /// Property type
    pub type_: PropertyType,
    /// Human-readable description
    pub description: Option<String>,
    /// Default value
    pub default: Option<serde_json::Value>,
    /// Format hint (e.g., "date-time", "email", "uri")
    pub format: Option<String>,
    /// Regex pattern for strings
    pub pattern: Option<String>,
    /// Allowed values (enum)
    pub enum_values: Option<Vec<serde_json::Value>>,
    /// Minimum value for numbers
    pub minimum: Option<f64>,
    /// Maximum value for numbers
    pub maximum: Option<f64>,
    /// Exclusive minimum
    pub exclusive_minimum: Option<f64>,
    /// Exclusive maximum
    pub exclusive_maximum: Option<f64>,
    /// Multiple of (for integers)
    pub multiple_of: Option<f64>,
    /// Minimum string length
    pub min_length: Option<u64>,
    /// Maximum string length
    pub max_length: Option<u64>,
    /// Minimum array items
    pub min_items: Option<u64>,
    /// Maximum array items
    pub max_items: Option<u64>,
    /// Whether array items must be unique
    pub unique_items: bool,
    /// Minimum object properties
    pub min_properties: Option<u64>,
    /// Maximum object properties
    pub max_properties: Option<u64>,
    /// Whether null is allowed
    pub nullable: bool,
    /// Nested object properties
    pub properties: Option<BTreeMap<String, SchemaProperty>>,
    /// Required nested properties
    pub required: Option<Vec<String>>,
    /// Array item schema
    pub items: Option<Box<SchemaProperty>>,
    /// Additional properties for objects
    pub additional_properties: Option<AdditionalProperties>,
    /// Preserve unknown fields
    pub x_preserve_unknown: bool,
    /// Kubernetes embedded resource
    pub x_embedded_resource: bool,
    /// Integer or string (for ports, etc.)
    pub x_int_or_string: bool,
}

impl SchemaProperty {
    /// Create a simple string property
    pub fn string() -> Self {
        Self {
            type_: PropertyType::String,
            ..Default::default()
        }
    }

    /// Create a simple integer property
    pub fn integer() -> Self {
        Self {
            type_: PropertyType::Integer,
            ..Default::default()
        }
    }

    /// Create a simple boolean property
    pub fn boolean() -> Self {
        Self {
            type_: PropertyType::Boolean,
            ..Default::default()
        }
    }

    /// Create an object property with nested properties
    pub fn object(properties: BTreeMap<String, SchemaProperty>) -> Self {
        Self {
            type_: PropertyType::Object,
            properties: Some(properties),
            ..Default::default()
        }
    }

    /// Create an array property with item schema
    pub fn array(items: SchemaProperty) -> Self {
        Self {
            type_: PropertyType::Array,
            items: Some(Box::new(items)),
            ..Default::default()
        }
    }

    /// Check if this property has nested properties
    pub fn has_nested_properties(&self) -> bool {
        self.properties.as_ref().is_some_and(|p| !p.is_empty())
    }

    /// Get a nested property by path (dot-separated)
    pub fn get_nested(&self, path: &str) -> Option<&SchemaProperty> {
        let mut current = self;
        for part in path.split('.') {
            current = current.properties.as_ref()?.get(part)?;
        }
        Some(current)
    }

    /// Check if a nested property is required
    pub fn is_required(&self, name: &str) -> bool {
        self.required
            .as_ref()
            .is_some_and(|r| r.contains(&name.to_string()))
    }
}

/// Property type in OpenAPI schema
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum PropertyType {
    String,
    Integer,
    Number,
    Boolean,
    Array,
    #[default]
    Object,
    /// Unknown or unspecified type
    Unknown(String),
}

impl PropertyType {
    /// Parse from string representation
    pub fn parse(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "string" => Self::String,
            "integer" => Self::Integer,
            "number" => Self::Number,
            "boolean" => Self::Boolean,
            "array" => Self::Array,
            "object" => Self::Object,
            other => Self::Unknown(other.to_string()),
        }
    }

    /// Check if types are compatible (for upgrade safety)
    pub fn is_compatible_with(&self, other: &Self) -> bool {
        match (self, other) {
            // Same types are always compatible
            (a, b) if a == b => true,
            // Integer is compatible with Number (widening)
            (Self::Integer, Self::Number) => true,
            // Unknown types - be conservative
            (Self::Unknown(_), _) | (_, Self::Unknown(_)) => false,
            // Everything else is incompatible
            _ => false,
        }
    }
}

impl std::fmt::Display for PropertyType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::String => write!(f, "string"),
            Self::Integer => write!(f, "integer"),
            Self::Number => write!(f, "number"),
            Self::Boolean => write!(f, "boolean"),
            Self::Array => write!(f, "array"),
            Self::Object => write!(f, "object"),
            Self::Unknown(s) => write!(f, "{}", s),
        }
    }
}

/// Additional properties configuration for objects
#[derive(Debug, Clone, PartialEq, Default)]
pub enum AdditionalProperties {
    /// Additional properties are allowed (any type)
    #[default]
    Allowed,
    /// Additional properties are not allowed
    Denied,
    /// Additional properties must match a schema
    Schema(Box<SchemaProperty>),
}

/// Printer column for kubectl output
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrinterColumn {
    /// Column name shown in kubectl output
    pub name: String,
    /// Column type (string, integer, date, etc.)
    pub type_: String,
    /// JSON path to extract value
    pub json_path: String,
    /// Column description
    pub description: Option<String>,
    /// Priority (0 = always shown, higher = hidden by default)
    pub priority: i32,
    /// Output format
    pub format: Option<String>,
}

/// Subresources configuration
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Subresources {
    /// Whether status subresource is enabled
    pub status: bool,
    /// Scale subresource configuration
    pub scale: Option<ScaleSubresource>,
}

impl Subresources {
    /// Check if any subresources are enabled
    pub fn any_enabled(&self) -> bool {
        self.status || self.scale.is_some()
    }
}

/// Scale subresource configuration for HPA integration
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScaleSubresource {
    /// JSON path to spec.replicas
    pub spec_replicas_path: String,
    /// JSON path to status.replicas
    pub status_replicas_path: String,
    /// Optional JSON path to label selector
    pub label_selector_path: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_property_type_compatibility() {
        assert!(PropertyType::String.is_compatible_with(&PropertyType::String));
        assert!(PropertyType::Integer.is_compatible_with(&PropertyType::Number));
        assert!(!PropertyType::String.is_compatible_with(&PropertyType::Integer));
        assert!(!PropertyType::Number.is_compatible_with(&PropertyType::Integer));
    }

    #[test]
    fn test_schema_property_nested() {
        let mut nested = BTreeMap::new();
        nested.insert("replicas".to_string(), SchemaProperty::integer());
        nested.insert("image".to_string(), SchemaProperty::string());

        let spec = SchemaProperty {
            type_: PropertyType::Object,
            properties: Some(nested),
            required: Some(vec!["replicas".to_string()]),
            ..Default::default()
        };

        assert!(spec.has_nested_properties());
        assert!(spec.is_required("replicas"));
        assert!(!spec.is_required("image"));
        assert!(spec.get_nested("replicas").is_some());
        assert!(spec.get_nested("nonexistent").is_none());
    }

    #[test]
    fn test_crd_scope_display() {
        assert_eq!(CrdScope::Namespaced.to_string(), "Namespaced");
        assert_eq!(CrdScope::Cluster.to_string(), "Cluster");
    }

    #[test]
    fn test_crd_schema_versions() {
        let schema = CrdSchema {
            name: "tests.example.com".to_string(),
            group: "example.com".to_string(),
            scope: CrdScope::Namespaced,
            names: CrdNames {
                kind: "Test".to_string(),
                plural: "tests".to_string(),
                ..Default::default()
            },
            versions: vec![
                CrdVersionSchema {
                    name: "v1".to_string(),
                    served: true,
                    storage: true,
                    deprecated: false,
                    deprecation_warning: None,
                    schema: None,
                    printer_columns: vec![],
                    subresources: Subresources::default(),
                },
                CrdVersionSchema {
                    name: "v1beta1".to_string(),
                    served: true,
                    storage: false,
                    deprecated: true,
                    deprecation_warning: Some("Use v1 instead".to_string()),
                    schema: None,
                    printer_columns: vec![],
                    subresources: Subresources::default(),
                },
            ],
        };

        assert!(schema.has_version("v1"));
        assert!(schema.has_version("v1beta1"));
        assert!(!schema.has_version("v2"));

        assert_eq!(schema.storage_version().unwrap().name, "v1");
        assert_eq!(schema.served_versions().count(), 2);
    }
}
