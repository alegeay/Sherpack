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
}
