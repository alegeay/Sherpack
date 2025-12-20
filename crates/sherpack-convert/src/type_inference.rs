#![allow(clippy::collapsible_if)]
#![allow(clippy::collapsible_match)]
#![allow(clippy::or_fun_call)]

//! Type inference from values.yaml
//!
//! Analyzes the structure of values.yaml to infer types for template variables.
//! This enables smarter conversion of Go templates to Jinja2, particularly for
//! distinguishing between list and dictionary iteration.
//!
//! # Example
//!
//! ```rust
//! use sherpack_convert::{TypeContext, InferredType};
//!
//! let yaml = r#"
//! controller:
//!   containerPort:
//!     http: 80
//!     https: 443
//!   replicas: 3
//!   labels:
//!     - app
//!     - version
//! "#;
//!
//! let ctx = TypeContext::from_yaml(yaml).unwrap();
//! assert_eq!(ctx.get_type("controller.containerPort"), InferredType::Dict);
//! assert_eq!(ctx.get_type("controller.replicas"), InferredType::Scalar);
//! assert_eq!(ctx.get_type("controller.labels"), InferredType::List);
//! ```

use serde_yaml::Value;
use std::collections::HashMap;

// =============================================================================
// INFERRED TYPES
// =============================================================================

/// Types that can be inferred from values.yaml structure
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InferredType {
    /// Scalar value: string, number, boolean, or null
    Scalar,
    /// Sequence/Array: `[]`
    List,
    /// Mapping/Dictionary: `{}`
    Dict,
    /// Type could not be determined (path not found or ambiguous)
    Unknown,
}

impl InferredType {
    /// Returns true if this type represents a collection (List or Dict)
    #[inline]
    pub fn is_collection(&self) -> bool {
        matches!(self, Self::List | Self::Dict)
    }

    /// Returns true if this is a dictionary type
    #[inline]
    pub fn is_dict(&self) -> bool {
        matches!(self, Self::Dict)
    }

    /// Returns true if this is a list type
    #[inline]
    pub fn is_list(&self) -> bool {
        matches!(self, Self::List)
    }
}

impl std::fmt::Display for InferredType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Scalar => write!(f, "scalar"),
            Self::List => write!(f, "list"),
            Self::Dict => write!(f, "dict"),
            Self::Unknown => write!(f, "unknown"),
        }
    }
}

// =============================================================================
// TYPE CONTEXT
// =============================================================================

/// Context holding inferred types for all paths in values.yaml
///
/// Built by traversing the values.yaml structure and recording the type
/// of each path. Used by the transformer to make smarter conversion decisions.
#[derive(Debug, Default, Clone)]
pub struct TypeContext {
    /// Map of dot-separated paths to their inferred types
    /// e.g., "controller.containerPort" -> Dict
    types: HashMap<String, InferredType>,
}

impl TypeContext {
    /// Creates an empty type context
    pub fn new() -> Self {
        Self::default()
    }

    /// Builds a type context from a YAML string
    ///
    /// # Errors
    ///
    /// Returns an error if the YAML cannot be parsed
    pub fn from_yaml(yaml: &str) -> Result<Self, serde_yaml::Error> {
        let value: Value = serde_yaml::from_str(yaml)?;
        Ok(Self::from_value(&value))
    }

    /// Builds a type context from a parsed YAML value
    pub fn from_value(value: &Value) -> Self {
        let mut ctx = Self::new();
        ctx.collect_types_recursive("", value);
        ctx
    }

    /// Recursively collects types from the YAML structure
    fn collect_types_recursive(&mut self, prefix: &str, value: &Value) {
        match value {
            Value::Mapping(map) => {
                // This node is a dictionary
                if !prefix.is_empty() {
                    self.types.insert(prefix.to_string(), InferredType::Dict);
                }

                // Recurse into children
                for (key, child) in map {
                    if let Some(key_str) = key.as_str() {
                        let child_path = if prefix.is_empty() {
                            key_str.to_string()
                        } else {
                            format!("{}.{}", prefix, key_str)
                        };
                        self.collect_types_recursive(&child_path, child);
                    }
                }
            }
            Value::Sequence(seq) => {
                // This node is a list
                if !prefix.is_empty() {
                    self.types.insert(prefix.to_string(), InferredType::List);
                }

                // Optionally analyze list item structure for nested types
                // For now, we don't recurse into list items as they may be heterogeneous
                if let Some(first) = seq.first() {
                    if let Value::Mapping(_) = first {
                        // List of objects - could extract common structure
                        // but for now just mark as List
                    }
                }
            }
            _ => {
                // Scalar value (string, number, bool, null)
                if !prefix.is_empty() {
                    self.types.insert(prefix.to_string(), InferredType::Scalar);
                }
            }
        }
    }

    /// Gets the inferred type for a path
    ///
    /// The path can be in various formats:
    /// - `"controller.containerPort"` (plain)
    /// - `"values.controller.containerPort"` (with values prefix)
    /// - `".Values.controller.containerPort"` (Go template style)
    ///
    /// All formats are normalized before lookup.
    pub fn get_type(&self, path: &str) -> InferredType {
        let normalized = Self::normalize_path(path);
        self.types
            .get(&normalized)
            .copied()
            .unwrap_or(InferredType::Unknown)
    }

    /// Checks if a path exists in the context
    pub fn contains(&self, path: &str) -> bool {
        let normalized = Self::normalize_path(path);
        self.types.contains_key(&normalized)
    }

    /// Returns all known paths and their types
    pub fn all_types(&self) -> impl Iterator<Item = (&str, InferredType)> {
        self.types.iter().map(|(k, v)| (k.as_str(), *v))
    }

    /// Returns the number of paths in the context
    pub fn len(&self) -> usize {
        self.types.len()
    }

    /// Returns true if no types have been collected
    pub fn is_empty(&self) -> bool {
        self.types.is_empty()
    }

    /// Normalizes a path by removing common prefixes
    ///
    /// - `.Values.x.y` -> `x.y`
    /// - `values.x.y` -> `x.y`
    /// - `x.y` -> `x.y`
    fn normalize_path(path: &str) -> String {
        let path = path.trim();

        // Remove leading dot
        let path = path.strip_prefix('.').unwrap_or(path);

        // Remove "Values." or "values." prefix
        let path = path
            .strip_prefix("Values.")
            .or_else(|| path.strip_prefix("values."))
            .unwrap_or(path);

        path.to_string()
    }
}

// =============================================================================
// HEURISTICS FOR UNKNOWN TYPES
// =============================================================================

/// Heuristics for guessing types when not found in values.yaml
///
/// These are patterns commonly seen in Helm charts that strongly suggest
/// a particular type.
pub struct TypeHeuristics;

impl TypeHeuristics {
    /// Common suffixes that indicate a dictionary type
    const DICT_SUFFIXES: &'static [&'static str] = &[
        "annotations",
        "labels",
        "selector",
        "matchLabels",
        "nodeSelector",
        "config",
        "configMap",
        "data",
        "stringData",
        "env",
        "ports",
        "containerPort",
        "hostPort",
        "resources",
        "limits",
        "requests",
        "securityContext",
        "podSecurityContext",
        "affinity",
        "tolerations",
        "headers",
        "proxyHeaders",
        "extraArgs",
    ];

    /// Common suffixes that indicate a list type
    const LIST_SUFFIXES: &'static [&'static str] = &[
        "items",
        "containers",
        "initContainers",
        "volumes",
        "volumeMounts",
        "envFrom",
        "imagePullSecrets",
        "hosts",
        "rules",
        "paths",
        "tls",
        "extraVolumes",
        "extraVolumeMounts",
        "extraContainers",
        "extraInitContainers",
        "extraEnvs",
    ];

    /// Guesses the type based on the path name using heuristics
    ///
    /// Returns `None` if no heuristic matches.
    pub fn guess_type(path: &str) -> Option<InferredType> {
        let last_segment = path.rsplit('.').next().unwrap_or(path);
        let lower = last_segment.to_ascii_lowercase();

        // Check dict patterns (exact match or ends with)
        for suffix in Self::DICT_SUFFIXES {
            let suffix_lower = suffix.to_ascii_lowercase();
            if lower == suffix_lower || lower.ends_with(&suffix_lower) {
                return Some(InferredType::Dict);
            }
        }

        // Check list patterns (exact match or ends with)
        for suffix in Self::LIST_SUFFIXES {
            let suffix_lower = suffix.to_ascii_lowercase();
            if lower == suffix_lower || lower.ends_with(&suffix_lower) {
                return Some(InferredType::List);
            }
        }

        None
    }
}

// =============================================================================
// TESTS
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_types() {
        let yaml = r#"
controller:
  replicas: 3
  enabled: true
  name: nginx
"#;
        let ctx = TypeContext::from_yaml(yaml).unwrap();

        assert_eq!(ctx.get_type("controller"), InferredType::Dict);
        assert_eq!(ctx.get_type("controller.replicas"), InferredType::Scalar);
        assert_eq!(ctx.get_type("controller.enabled"), InferredType::Scalar);
        assert_eq!(ctx.get_type("controller.name"), InferredType::Scalar);
    }

    #[test]
    fn test_nested_dict() {
        let yaml = r#"
controller:
  containerPort:
    http: 80
    https: 443
  image:
    repository: nginx
    tag: latest
"#;
        let ctx = TypeContext::from_yaml(yaml).unwrap();

        assert_eq!(ctx.get_type("controller"), InferredType::Dict);
        assert_eq!(ctx.get_type("controller.containerPort"), InferredType::Dict);
        assert_eq!(
            ctx.get_type("controller.containerPort.http"),
            InferredType::Scalar
        );
        assert_eq!(ctx.get_type("controller.image"), InferredType::Dict);
        assert_eq!(
            ctx.get_type("controller.image.repository"),
            InferredType::Scalar
        );
    }

    #[test]
    fn test_list_types() {
        let yaml = r#"
controller:
  extraEnvs:
    - name: FOO
      value: bar
    - name: BAZ
      value: qux
  labels:
    - app
    - version
"#;
        let ctx = TypeContext::from_yaml(yaml).unwrap();

        assert_eq!(ctx.get_type("controller.extraEnvs"), InferredType::List);
        assert_eq!(ctx.get_type("controller.labels"), InferredType::List);
    }

    #[test]
    fn test_path_normalization() {
        let yaml = r#"
controller:
  replicas: 3
"#;
        let ctx = TypeContext::from_yaml(yaml).unwrap();

        // All these should resolve to the same path
        assert_eq!(ctx.get_type("controller.replicas"), InferredType::Scalar);
        assert_eq!(
            ctx.get_type("values.controller.replicas"),
            InferredType::Scalar
        );
        assert_eq!(
            ctx.get_type(".Values.controller.replicas"),
            InferredType::Scalar
        );
        assert_eq!(
            ctx.get_type("Values.controller.replicas"),
            InferredType::Scalar
        );
    }

    #[test]
    fn test_unknown_path() {
        let yaml = r#"
controller:
  replicas: 3
"#;
        let ctx = TypeContext::from_yaml(yaml).unwrap();

        assert_eq!(ctx.get_type("nonexistent"), InferredType::Unknown);
        assert_eq!(ctx.get_type("controller.unknown"), InferredType::Unknown);
    }

    #[test]
    fn test_heuristics_dict() {
        assert_eq!(
            TypeHeuristics::guess_type("controller.annotations"),
            Some(InferredType::Dict)
        );
        assert_eq!(
            TypeHeuristics::guess_type("controller.labels"),
            Some(InferredType::Dict)
        );
        assert_eq!(
            TypeHeuristics::guess_type("pod.nodeSelector"),
            Some(InferredType::Dict)
        );
        assert_eq!(
            TypeHeuristics::guess_type("controller.containerPort"),
            Some(InferredType::Dict)
        );
    }

    #[test]
    fn test_heuristics_list() {
        assert_eq!(
            TypeHeuristics::guess_type("spec.containers"),
            Some(InferredType::List)
        );
        assert_eq!(
            TypeHeuristics::guess_type("controller.extraVolumes"),
            Some(InferredType::List)
        );
        assert_eq!(
            TypeHeuristics::guess_type("pod.imagePullSecrets"),
            Some(InferredType::List)
        );
    }

    #[test]
    fn test_heuristics_unknown() {
        assert_eq!(TypeHeuristics::guess_type("controller.replicas"), None);
        assert_eq!(TypeHeuristics::guess_type("custom.field"), None);
    }

    #[test]
    fn test_complex_structure() {
        let yaml = r#"
global:
  image:
    registry: docker.io
controller:
  kind: Deployment
  hostNetwork: false
  containerPort:
    http: 80
    https: 443
  admissionWebhooks:
    enabled: true
    patch:
      image:
        registry: registry.k8s.io
        image: ingress-nginx/kube-webhook-certgen
        tag: v1.4.1
tcp: {}
udp: {}
"#;
        let ctx = TypeContext::from_yaml(yaml).unwrap();

        // Top-level
        assert_eq!(ctx.get_type("global"), InferredType::Dict);
        assert_eq!(ctx.get_type("controller"), InferredType::Dict);
        assert_eq!(ctx.get_type("tcp"), InferredType::Dict);
        assert_eq!(ctx.get_type("udp"), InferredType::Dict);

        // Nested
        assert_eq!(ctx.get_type("global.image"), InferredType::Dict);
        assert_eq!(ctx.get_type("controller.containerPort"), InferredType::Dict);
        assert_eq!(
            ctx.get_type("controller.admissionWebhooks.patch.image"),
            InferredType::Dict
        );

        // Scalars
        assert_eq!(ctx.get_type("controller.kind"), InferredType::Scalar);
        assert_eq!(ctx.get_type("controller.hostNetwork"), InferredType::Scalar);
        assert_eq!(
            ctx.get_type("controller.admissionWebhooks.enabled"),
            InferredType::Scalar
        );
    }

    #[test]
    fn test_is_methods() {
        assert!(InferredType::Dict.is_dict());
        assert!(InferredType::Dict.is_collection());
        assert!(!InferredType::Dict.is_list());

        assert!(InferredType::List.is_list());
        assert!(InferredType::List.is_collection());
        assert!(!InferredType::List.is_dict());

        assert!(!InferredType::Scalar.is_collection());
        assert!(!InferredType::Unknown.is_collection());
    }

    #[test]
    fn test_display() {
        assert_eq!(format!("{}", InferredType::Scalar), "scalar");
        assert_eq!(format!("{}", InferredType::List), "list");
        assert_eq!(format!("{}", InferredType::Dict), "dict");
        assert_eq!(format!("{}", InferredType::Unknown), "unknown");
    }

    #[test]
    fn test_len_and_is_empty() {
        let empty = TypeContext::new();
        assert!(empty.is_empty());
        assert_eq!(empty.len(), 0);

        let ctx = TypeContext::from_yaml("foo: bar").unwrap();
        assert!(!ctx.is_empty());
        assert!(ctx.len() > 0);
    }
}
