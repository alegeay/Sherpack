//! Values handling with deep merge support

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::path::Path;

use crate::error::{CoreError, Result};

/// Values container with deep merge capability
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Values(pub JsonValue);

impl Values {
    /// Create empty values
    pub fn new() -> Self {
        Self(JsonValue::Object(serde_json::Map::new()))
    }

    /// Load values from a YAML file
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let content = std::fs::read_to_string(path.as_ref())?;
        Self::from_yaml(&content)
    }

    /// Parse values from YAML string
    pub fn from_yaml(yaml: &str) -> Result<Self> {
        let value: JsonValue = serde_yaml::from_str(yaml)?;
        Ok(Self(value))
    }

    /// Parse values from JSON string
    pub fn from_json(json: &str) -> Result<Self> {
        let value: JsonValue = serde_json::from_str(json)?;
        Ok(Self(value))
    }

    /// Deep merge another Values into this one
    ///
    /// Rules:
    /// - Scalars: overlay replaces base
    /// - Objects: recursive merge
    /// - Arrays: overlay replaces base (not appended)
    pub fn merge(&mut self, overlay: &Values) {
        deep_merge(&mut self.0, &overlay.0);
    }

    /// Merge multiple values in order
    pub fn merge_all(values: Vec<Values>) -> Self {
        let mut result = Values::new();
        for v in values {
            result.merge(&v);
        }
        result
    }

    /// Set a value by dotted path (e.g., "image.tag")
    pub fn set(&mut self, path: &str, value: JsonValue) -> Result<()> {
        let parts: Vec<&str> = path.split('.').collect();
        set_nested(&mut self.0, &parts, value)
    }

    /// Get a value by dotted path
    pub fn get(&self, path: &str) -> Option<&JsonValue> {
        let parts: Vec<&str> = path.split('.').collect();
        get_nested(&self.0, &parts)
    }

    /// Get the inner JSON value
    pub fn inner(&self) -> &JsonValue {
        &self.0
    }

    /// Convert to JSON value
    pub fn into_inner(self) -> JsonValue {
        self.0
    }

    /// Check if values are empty
    pub fn is_empty(&self) -> bool {
        match &self.0 {
            JsonValue::Object(map) => map.is_empty(),
            JsonValue::Null => true,
            _ => false,
        }
    }

    /// Merge with schema defaults applied first
    ///
    /// The merge order is: schema defaults (lowest priority) -> base values (higher priority)
    /// This ensures that schema defaults are only used when values are not explicitly set.
    pub fn with_schema_defaults(schema_defaults: Values, base: Values) -> Self {
        // Start with schema defaults, then merge base on top
        let mut result = schema_defaults;
        result.merge(&base);
        result
    }

    // =========================================================================
    // Subchart Value Scoping
    // =========================================================================

    /// Scope values for a subchart
    ///
    /// When rendering a subchart, it should only see:
    /// 1. Values under `<subchart_name>.*` in the parent, as its root values
    /// 2. Global values under `global.*` preserved as-is
    ///
    /// The subchart's own `values.yaml` defaults are merged separately before this.
    ///
    /// # Example
    ///
    /// Parent values:
    /// ```yaml
    /// global:
    ///   imageRegistry: docker.io
    /// redis:
    ///   enabled: true
    ///   replicas: 3
    /// postgresql:
    ///   enabled: false
    /// ```
    ///
    /// Calling `scope_for_subchart("redis")` produces:
    /// ```yaml
    /// global:
    ///   imageRegistry: docker.io
    /// enabled: true
    /// replicas: 3
    /// ```
    pub fn scope_for_subchart(&self, subchart_name: &str) -> Values {
        let mut scoped = serde_json::Map::new();

        if let JsonValue::Object(parent_obj) = &self.0 {
            // 1. Copy global values if present
            if let Some(global) = parent_obj.get("global") {
                scoped.insert("global".to_string(), global.clone());
            }

            // 2. Extract subchart-specific values as root values
            if let Some(JsonValue::Object(subchart_obj)) = parent_obj.get(subchart_name) {
                for (k, v) in subchart_obj {
                    scoped.insert(k.clone(), v.clone());
                }
            }
        }

        Values(JsonValue::Object(scoped))
    }

    /// Merge subchart defaults with scoped parent values
    ///
    /// This is the complete subchart value resolution:
    /// 1. Start with subchart's own `values.yaml` defaults
    /// 2. Merge in the scoped values from parent
    ///
    /// # Arguments
    /// * `subchart_defaults` - Values from the subchart's `values.yaml`
    /// * `parent_values` - Parent's merged values
    /// * `subchart_name` - Name of the subchart (for scoping)
    pub fn for_subchart(
        subchart_defaults: Values,
        parent_values: &Values,
        subchart_name: &str,
    ) -> Values {
        let mut result = subchart_defaults;
        let scoped = parent_values.scope_for_subchart(subchart_name);
        result.merge(&scoped);
        result
    }

    /// Export subchart values back to parent namespace
    ///
    /// This is the inverse of `scope_for_subchart` - takes subchart-scoped values
    /// and wraps them under the subchart's namespace for parent access.
    ///
    /// # Example
    ///
    /// Subchart values:
    /// ```yaml
    /// global:
    ///   imageRegistry: docker.io
    /// enabled: true
    /// replicas: 3
    /// ```
    ///
    /// Calling `export_to_parent("redis")` produces:
    /// ```yaml
    /// global:
    ///   imageRegistry: docker.io
    /// redis:
    ///   enabled: true
    ///   replicas: 3
    /// ```
    pub fn export_to_parent(&self, subchart_name: &str) -> Values {
        let mut parent = serde_json::Map::new();
        let mut subchart_values = serde_json::Map::new();

        if let JsonValue::Object(obj) = &self.0 {
            for (k, v) in obj {
                if k == "global" {
                    // Global stays at parent root level
                    parent.insert(k.clone(), v.clone());
                } else {
                    // Everything else goes under subchart namespace
                    subchart_values.insert(k.clone(), v.clone());
                }
            }
        }

        if !subchart_values.is_empty() {
            parent.insert(
                subchart_name.to_string(),
                JsonValue::Object(subchart_values),
            );
        }

        Values(JsonValue::Object(parent))
    }

    // =========================================================================
    // JsonValue-based methods (for use with TemplateContext which stores JsonValue)
    // =========================================================================

    /// Scope values for a subchart from raw JsonValue
    ///
    /// Same as `scope_for_subchart` but works with `&JsonValue` directly,
    /// useful when values are already in JsonValue form (e.g., from TemplateContext).
    pub fn scope_json_for_subchart(parent_json: &JsonValue, subchart_name: &str) -> Values {
        let mut scoped = serde_json::Map::new();

        if let JsonValue::Object(parent_obj) = parent_json {
            // 1. Copy global values if present
            if let Some(global) = parent_obj.get("global") {
                scoped.insert("global".to_string(), global.clone());
            }

            // 2. Extract subchart-specific values as root values
            if let Some(JsonValue::Object(subchart_obj)) = parent_obj.get(subchart_name) {
                for (k, v) in subchart_obj {
                    scoped.insert(k.clone(), v.clone());
                }
            }
        }

        Values(JsonValue::Object(scoped))
    }

    /// Merge subchart defaults with scoped parent values from JsonValue
    ///
    /// Same as `for_subchart` but accepts `&JsonValue` for parent values.
    pub fn for_subchart_json(
        subchart_defaults: Values,
        parent_json: &JsonValue,
        subchart_name: &str,
    ) -> Values {
        let mut result = subchart_defaults;
        let scoped = Self::scope_json_for_subchart(parent_json, subchart_name);
        result.merge(&scoped);
        result
    }
}

/// Deep merge two JSON values
fn deep_merge(base: &mut JsonValue, overlay: &JsonValue) {
    match (base, overlay) {
        (JsonValue::Object(base_map), JsonValue::Object(overlay_map)) => {
            for (key, overlay_value) in overlay_map {
                match base_map.get_mut(key) {
                    Some(base_value) => deep_merge(base_value, overlay_value),
                    None => {
                        base_map.insert(key.clone(), overlay_value.clone());
                    }
                }
            }
        }
        (base, overlay) => {
            *base = overlay.clone();
        }
    }
}

/// Set a nested value by path
fn set_nested(value: &mut JsonValue, path: &[&str], new_value: JsonValue) -> Result<()> {
    if path.is_empty() {
        *value = new_value;
        return Ok(());
    }

    let key = path[0];
    let remaining = &path[1..];

    // Ensure we have an object
    if !value.is_object() {
        *value = JsonValue::Object(serde_json::Map::new());
    }

    // SAFETY: We just ensured it's an object above
    let map = value
        .as_object_mut()
        .expect("value should be an object after initialization");

    if remaining.is_empty() {
        map.insert(key.to_string(), new_value);
    } else {
        let entry = map
            .entry(key.to_string())
            .or_insert_with(|| JsonValue::Object(serde_json::Map::new()));
        set_nested(entry, remaining, new_value)?;
    }

    Ok(())
}

/// Get a nested value by path
fn get_nested<'a>(value: &'a JsonValue, path: &[&str]) -> Option<&'a JsonValue> {
    if path.is_empty() {
        return Some(value);
    }

    let key = path[0];
    let remaining = &path[1..];

    match value {
        JsonValue::Object(map) => {
            map.get(key).and_then(|v| get_nested(v, remaining))
        }
        _ => None,
    }
}

/// Parse --set arguments (key=value format)
pub fn parse_set_values(set_args: &[String]) -> Result<Values> {
    let mut values = Values::new();

    for arg in set_args {
        let (key, val) = arg.split_once('=').ok_or_else(|| CoreError::ValuesMerge {
            message: format!("Invalid --set format: '{}'. Expected key=value", arg),
        })?;

        // Try to parse as JSON, fallback to string
        let json_value = if val == "true" {
            JsonValue::Bool(true)
        } else if val == "false" {
            JsonValue::Bool(false)
        } else if val == "null" {
            JsonValue::Null
        } else if let Ok(num) = val.parse::<i64>() {
            JsonValue::Number(num.into())
        } else if let Ok(num) = val.parse::<f64>() {
            JsonValue::Number(serde_json::Number::from_f64(num).unwrap_or(0.into()))
        } else if val.starts_with('[') || val.starts_with('{') {
            serde_json::from_str(val).unwrap_or(JsonValue::String(val.to_string()))
        } else {
            JsonValue::String(val.to_string())
        };

        values.set(key, json_value)?;
    }

    Ok(values)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deep_merge() {
        let mut base = Values::from_yaml(r#"
image:
  repository: nginx
  tag: "1.0"
replicas: 1
"#).unwrap();

        let overlay = Values::from_yaml(r#"
image:
  tag: "2.0"
  pullPolicy: Always
replicas: 3
"#).unwrap();

        base.merge(&overlay);

        assert_eq!(base.get("image.repository").unwrap(), "nginx");
        assert_eq!(base.get("image.tag").unwrap(), "2.0");
        assert_eq!(base.get("image.pullPolicy").unwrap(), "Always");
        assert_eq!(base.get("replicas").unwrap(), 3);
    }

    #[test]
    fn test_set_nested() {
        let mut values = Values::new();
        values.set("image.tag", JsonValue::String("v1".into())).unwrap();
        values.set("replicas", JsonValue::Number(3.into())).unwrap();

        assert_eq!(values.get("image.tag").unwrap(), "v1");
        assert_eq!(values.get("replicas").unwrap(), 3);
    }

    #[test]
    fn test_parse_set_values() {
        let args = vec![
            "image.tag=v2".to_string(),
            "replicas=5".to_string(),
            "debug=true".to_string(),
        ];

        let values = parse_set_values(&args).unwrap();

        assert_eq!(values.get("image.tag").unwrap(), "v2");
        assert_eq!(values.get("replicas").unwrap(), 5);
        assert_eq!(values.get("debug").unwrap(), true);
    }

    #[test]
    fn test_scope_for_subchart_basic() {
        let parent = Values::from_yaml(r#"
global:
  imageRegistry: docker.io
redis:
  enabled: true
  replicas: 3
postgresql:
  enabled: false
"#).unwrap();

        let scoped = parent.scope_for_subchart("redis");

        // Global should be preserved
        assert_eq!(scoped.get("global.imageRegistry").unwrap(), "docker.io");

        // Redis values should be at root level
        assert_eq!(scoped.get("enabled").unwrap(), true);
        assert_eq!(scoped.get("replicas").unwrap(), 3);

        // PostgreSQL should NOT be present
        assert!(scoped.get("postgresql").is_none());
        assert!(scoped.get("redis").is_none());
    }

    #[test]
    fn test_scope_for_subchart_no_global() {
        let parent = Values::from_yaml(r#"
redis:
  host: localhost
  port: 6379
"#).unwrap();

        let scoped = parent.scope_for_subchart("redis");

        assert_eq!(scoped.get("host").unwrap(), "localhost");
        assert_eq!(scoped.get("port").unwrap(), 6379);
        assert!(scoped.get("global").is_none());
    }

    #[test]
    fn test_scope_for_subchart_missing_subchart() {
        let parent = Values::from_yaml(r#"
global:
  debug: true
redis:
  enabled: true
"#).unwrap();

        let scoped = parent.scope_for_subchart("postgresql");

        // Only global should be present
        assert_eq!(scoped.get("global.debug").unwrap(), true);
        assert!(scoped.get("enabled").is_none());
    }

    #[test]
    fn test_for_subchart_with_defaults() {
        let subchart_defaults = Values::from_yaml(r#"
enabled: false
replicas: 1
image:
  repository: redis
  tag: "7.0"
"#).unwrap();

        let parent = Values::from_yaml(r#"
global:
  pullPolicy: Always
redis:
  enabled: true
  replicas: 3
"#).unwrap();

        let result = Values::for_subchart(subchart_defaults, &parent, "redis");

        // Global from parent
        assert_eq!(result.get("global.pullPolicy").unwrap(), "Always");

        // Overridden by parent's redis.*
        assert_eq!(result.get("enabled").unwrap(), true);
        assert_eq!(result.get("replicas").unwrap(), 3);

        // Default values not overridden
        assert_eq!(result.get("image.repository").unwrap(), "redis");
        assert_eq!(result.get("image.tag").unwrap(), "7.0");
    }

    #[test]
    fn test_export_to_parent() {
        let subchart = Values::from_yaml(r#"
global:
  imageRegistry: docker.io
enabled: true
replicas: 3
image:
  tag: "7.0"
"#).unwrap();

        let exported = subchart.export_to_parent("redis");

        // Global at root
        assert_eq!(exported.get("global.imageRegistry").unwrap(), "docker.io");

        // Other values under redis namespace
        assert_eq!(exported.get("redis.enabled").unwrap(), true);
        assert_eq!(exported.get("redis.replicas").unwrap(), 3);
        assert_eq!(exported.get("redis.image.tag").unwrap(), "7.0");
    }

    #[test]
    fn test_scope_and_export_roundtrip() {
        let original_parent = Values::from_yaml(r#"
global:
  env: production
redis:
  enabled: true
  maxMemory: 256mb
"#).unwrap();

        // Scope for subchart
        let scoped = original_parent.scope_for_subchart("redis");

        // Export back to parent namespace
        let exported = scoped.export_to_parent("redis");

        // Should match original structure (for redis values)
        assert_eq!(exported.get("global.env").unwrap(), "production");
        assert_eq!(exported.get("redis.enabled").unwrap(), true);
        assert_eq!(exported.get("redis.maxMemory").unwrap(), "256mb");
    }

    #[test]
    fn test_scope_json_for_subchart() {
        let parent_json = serde_json::json!({
            "global": {
                "imageRegistry": "docker.io"
            },
            "redis": {
                "enabled": true,
                "replicas": 3
            },
            "postgresql": {
                "enabled": false
            }
        });

        let scoped = Values::scope_json_for_subchart(&parent_json, "redis");

        // Global should be preserved
        assert_eq!(scoped.get("global.imageRegistry").unwrap(), "docker.io");

        // Redis values should be at root level
        assert_eq!(scoped.get("enabled").unwrap(), true);
        assert_eq!(scoped.get("replicas").unwrap(), 3);

        // PostgreSQL should NOT be present
        assert!(scoped.get("postgresql").is_none());
        assert!(scoped.get("redis").is_none());
    }

    #[test]
    fn test_for_subchart_json() {
        let subchart_defaults = Values::from_yaml(r#"
enabled: false
replicas: 1
image:
  repository: redis
  tag: "7.0"
"#).unwrap();

        let parent_json = serde_json::json!({
            "global": {
                "pullPolicy": "Always"
            },
            "redis": {
                "enabled": true,
                "replicas": 3
            }
        });

        let result = Values::for_subchart_json(subchart_defaults, &parent_json, "redis");

        // Global from parent
        assert_eq!(result.get("global.pullPolicy").unwrap(), "Always");

        // Overridden by parent's redis.*
        assert_eq!(result.get("enabled").unwrap(), true);
        assert_eq!(result.get("replicas").unwrap(), 3);

        // Default values not overridden
        assert_eq!(result.get("image.repository").unwrap(), "redis");
        assert_eq!(result.get("image.tag").unwrap(), "7.0");
    }
}
