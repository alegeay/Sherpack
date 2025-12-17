//! Schema validation for values
//!
//! This module provides schema validation with support for two formats:
//! - Standard JSON Schema (compatible with Helm's values.schema.json)
//! - Simplified Sherpack schema format (more intuitive for YAML users)

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use std::path::Path;

use crate::error::{CoreError, Result, ValidationErrorInfo};
use crate::values::Values;

/// Simplified Sherpack schema type
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum SherpType {
    String,
    Number,
    Integer,
    Boolean,
    Array,
    Object,
    Any,
}

/// Simplified Sherpack schema definition for a single property
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SherpProperty {
    /// Type of the property
    #[serde(rename = "type")]
    pub prop_type: SherpType,

    /// Description for documentation
    #[serde(default)]
    pub description: Option<String>,

    /// Default value
    #[serde(default)]
    pub default: Option<JsonValue>,

    /// Whether this property is required
    #[serde(default)]
    pub required: bool,

    /// Allowed values (enum constraint)
    #[serde(default)]
    pub enum_values: Option<Vec<JsonValue>>,

    /// Pattern for string validation (regex)
    #[serde(default)]
    pub pattern: Option<String>,

    /// Minimum value for numbers
    #[serde(default)]
    pub min: Option<f64>,

    /// Maximum value for numbers
    #[serde(default)]
    pub max: Option<f64>,

    /// Minimum length for strings
    #[serde(default)]
    pub min_length: Option<usize>,

    /// Maximum length for strings
    #[serde(default)]
    pub max_length: Option<usize>,

    /// Nested properties for objects
    #[serde(default)]
    pub properties: Option<HashMap<String, SherpProperty>>,

    /// Item schema for arrays
    #[serde(default)]
    pub items: Option<Box<SherpProperty>>,

    /// Minimum array items
    #[serde(default)]
    pub min_items: Option<usize>,

    /// Maximum array items
    #[serde(default)]
    pub max_items: Option<usize>,
}

/// Root schema definition in simplified format
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SherpSchema {
    /// Schema format identifier
    #[serde(default = "default_schema_version")]
    pub schema_version: String,

    /// Optional schema title
    #[serde(default)]
    pub title: Option<String>,

    /// Optional schema description
    #[serde(default)]
    pub description: Option<String>,

    /// Property definitions
    pub properties: HashMap<String, SherpProperty>,
}

fn default_schema_version() -> String {
    "sherpack/v1".to_string()
}

/// Schema format detection
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SchemaFormat {
    JsonSchema,
    SherpSchema,
}

/// Unified schema that handles both formats
#[derive(Debug, Clone)]
pub enum Schema {
    /// Standard JSON Schema
    JsonSchema(JsonValue),
    /// Simplified Sherpack schema
    SherpSchema(SherpSchema),
}

impl Schema {
    /// Load schema from a file, auto-detecting format
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref();
        let content = std::fs::read_to_string(path)?;

        // Detect format based on extension and content
        let format = detect_schema_format(path, &content)?;

        match format {
            SchemaFormat::JsonSchema => {
                let value: JsonValue = if path
                    .extension()
                    .map(|e| e == "json")
                    .unwrap_or(false)
                {
                    serde_json::from_str(&content)?
                } else {
                    serde_yaml::from_str(&content)?
                };
                Ok(Schema::JsonSchema(value))
            }
            SchemaFormat::SherpSchema => {
                let sherp: SherpSchema = serde_yaml::from_str(&content)?;
                Ok(Schema::SherpSchema(sherp))
            }
        }
    }

    /// Load from JSON Schema string
    pub fn from_json_schema(json: &str) -> Result<Self> {
        let value: JsonValue = serde_json::from_str(json)?;
        Ok(Schema::JsonSchema(value))
    }

    /// Load from simplified schema YAML string
    pub fn from_sherp_schema(yaml: &str) -> Result<Self> {
        let sherp: SherpSchema = serde_yaml::from_str(yaml)?;
        Ok(Schema::SherpSchema(sherp))
    }

    /// Convert to JSON Schema for validation
    pub fn to_json_schema(&self) -> JsonValue {
        match self {
            Schema::JsonSchema(v) => v.clone(),
            Schema::SherpSchema(s) => convert_sherp_to_json_schema(s),
        }
    }

    /// Extract defaults from the schema
    pub fn extract_defaults(&self) -> JsonValue {
        match self {
            Schema::JsonSchema(v) => extract_json_schema_defaults(v),
            Schema::SherpSchema(s) => extract_sherp_defaults(s),
        }
    }

    /// Get defaults as Values
    pub fn defaults_as_values(&self) -> Values {
        Values(self.extract_defaults())
    }
}

/// Detect schema format from file path and content
fn detect_schema_format(path: &Path, content: &str) -> Result<SchemaFormat> {
    // Check file extension first
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

    // values.schema.json -> JSON Schema
    if name == "values.schema.json" || ext == "json" {
        return Ok(SchemaFormat::JsonSchema);
    }

    // Check for JSON Schema markers in content
    let value: JsonValue = serde_yaml::from_str(content).map_err(|e| CoreError::InvalidSchema {
        message: format!("Failed to parse schema: {}", e),
    })?;

    if let Some(obj) = value.as_object() {
        // JSON Schema indicators
        if obj.contains_key("$schema") || obj.contains_key("$id") {
            return Ok(SchemaFormat::JsonSchema);
        }

        // Sherpack schema indicator
        if obj
            .get("schemaVersion")
            .and_then(|v| v.as_str())
            .map(|s| s.starts_with("sherpack/"))
            .unwrap_or(false)
        {
            return Ok(SchemaFormat::SherpSchema);
        }

        // Heuristic: if it has "type": "object" at root with nested structure,
        // treat as JSON Schema; otherwise Sherpack
        if obj.contains_key("type") && obj.get("type") == Some(&JsonValue::String("object".into()))
        {
            return Ok(SchemaFormat::JsonSchema);
        }
    }

    // Default to Sherpack schema for .yaml files
    Ok(SchemaFormat::SherpSchema)
}

/// Convert Sherpack schema to JSON Schema
fn convert_sherp_to_json_schema(sherp: &SherpSchema) -> JsonValue {
    let mut schema = serde_json::Map::new();

    schema.insert(
        "$schema".into(),
        JsonValue::String("http://json-schema.org/draft-07/schema#".into()),
    );
    schema.insert("type".into(), JsonValue::String("object".into()));

    if let Some(title) = &sherp.title {
        schema.insert("title".into(), JsonValue::String(title.clone()));
    }
    if let Some(desc) = &sherp.description {
        schema.insert("description".into(), JsonValue::String(desc.clone()));
    }

    let (properties, required) = convert_sherp_properties(&sherp.properties);
    schema.insert("properties".into(), properties);

    if !required.is_empty() {
        schema.insert(
            "required".into(),
            JsonValue::Array(required.into_iter().map(JsonValue::String).collect()),
        );
    }

    JsonValue::Object(schema)
}

fn convert_sherp_properties(props: &HashMap<String, SherpProperty>) -> (JsonValue, Vec<String>) {
    let mut json_props = serde_json::Map::new();
    let mut required = Vec::new();

    for (name, prop) in props {
        json_props.insert(name.clone(), convert_sherp_property(prop));
        if prop.required {
            required.push(name.clone());
        }
    }

    (JsonValue::Object(json_props), required)
}

fn convert_sherp_property(prop: &SherpProperty) -> JsonValue {
    let mut json = serde_json::Map::new();

    // Type conversion
    let type_str = match prop.prop_type {
        SherpType::String => "string",
        SherpType::Number => "number",
        SherpType::Integer => "integer",
        SherpType::Boolean => "boolean",
        SherpType::Array => "array",
        SherpType::Object => "object",
        SherpType::Any => {
            // JSON Schema doesn't have "any", omit type
            return JsonValue::Object(json);
        }
    };
    json.insert("type".into(), JsonValue::String(type_str.into()));

    // Optional fields
    if let Some(desc) = &prop.description {
        json.insert("description".into(), JsonValue::String(desc.clone()));
    }
    if let Some(default) = &prop.default {
        json.insert("default".into(), default.clone());
    }
    if let Some(enum_vals) = &prop.enum_values {
        json.insert("enum".into(), JsonValue::Array(enum_vals.clone()));
    }
    if let Some(pattern) = &prop.pattern {
        json.insert("pattern".into(), JsonValue::String(pattern.clone()));
    }

    // Numeric constraints
    if let Some(min) = prop.min {
        json.insert("minimum".into(), JsonValue::from(min));
    }
    if let Some(max) = prop.max {
        json.insert("maximum".into(), JsonValue::from(max));
    }

    // String constraints
    if let Some(min_len) = prop.min_length {
        json.insert("minLength".into(), JsonValue::from(min_len));
    }
    if let Some(max_len) = prop.max_length {
        json.insert("maxLength".into(), JsonValue::from(max_len));
    }

    // Nested objects
    if let Some(nested_props) = &prop.properties {
        let (nested_json, nested_required) = convert_sherp_properties(nested_props);
        json.insert("properties".into(), nested_json);
        if !nested_required.is_empty() {
            json.insert(
                "required".into(),
                JsonValue::Array(nested_required.into_iter().map(JsonValue::String).collect()),
            );
        }
    }

    // Array items
    if let Some(items) = &prop.items {
        json.insert("items".into(), convert_sherp_property(items));
    }
    if let Some(min_items) = prop.min_items {
        json.insert("minItems".into(), JsonValue::from(min_items));
    }
    if let Some(max_items) = prop.max_items {
        json.insert("maxItems".into(), JsonValue::from(max_items));
    }

    JsonValue::Object(json)
}

/// Extract defaults from JSON Schema
fn extract_json_schema_defaults(schema: &JsonValue) -> JsonValue {
    extract_defaults_recursive(schema)
}

fn extract_defaults_recursive(schema: &JsonValue) -> JsonValue {
    let obj = match schema.as_object() {
        Some(o) => o,
        None => return JsonValue::Null,
    };

    // If there's a direct default, return it
    if let Some(default) = obj.get("default") {
        return default.clone();
    }

    // For objects, recursively extract property defaults
    if obj.get("type") == Some(&JsonValue::String("object".into())) {
        if let Some(props) = obj.get("properties").and_then(|p| p.as_object()) {
            let mut defaults = serde_json::Map::new();

            for (key, prop_schema) in props {
                let prop_default = extract_defaults_recursive(prop_schema);
                if !prop_default.is_null() {
                    defaults.insert(key.clone(), prop_default);
                }
            }

            if !defaults.is_empty() {
                return JsonValue::Object(defaults);
            }
        }
    }

    JsonValue::Null
}

/// Extract defaults from Sherpack schema
fn extract_sherp_defaults(sherp: &SherpSchema) -> JsonValue {
    extract_sherp_property_defaults(&sherp.properties)
}

fn extract_sherp_property_defaults(props: &HashMap<String, SherpProperty>) -> JsonValue {
    let mut defaults = serde_json::Map::new();

    for (name, prop) in props {
        let value = if let Some(default) = &prop.default {
            default.clone()
        } else if let Some(nested) = &prop.properties {
            let nested_defaults = extract_sherp_property_defaults(nested);
            if nested_defaults.is_null()
                || nested_defaults
                    .as_object()
                    .map(|o| o.is_empty())
                    .unwrap_or(true)
            {
                continue;
            }
            nested_defaults
        } else {
            continue;
        };

        defaults.insert(name.clone(), value);
    }

    if defaults.is_empty() {
        JsonValue::Null
    } else {
        JsonValue::Object(defaults)
    }
}

/// Result of schema validation
#[derive(Debug)]
pub struct ValidationResult {
    /// Whether the values are valid
    pub is_valid: bool,
    /// Validation errors
    pub errors: Vec<ValidationErrorInfo>,
}

impl ValidationResult {
    /// Create a successful validation result
    pub fn success() -> Self {
        Self {
            is_valid: true,
            errors: vec![],
        }
    }

    /// Create a failed validation result with errors
    pub fn failure(errors: Vec<ValidationErrorInfo>) -> Self {
        Self {
            is_valid: false,
            errors,
        }
    }
}

/// Schema validator with cached compiled schema
pub struct SchemaValidator {
    /// The original schema
    schema: Schema,

    /// Compiled JSON Schema validator
    compiled: jsonschema::Validator,

    /// Extracted default values
    defaults: JsonValue,
}

impl SchemaValidator {
    /// Create a new validator from a schema
    pub fn new(schema: Schema) -> Result<Self> {
        let json_schema = schema.to_json_schema();
        let defaults = schema.extract_defaults();

        let compiled =
            jsonschema::validator_for(&json_schema).map_err(|e| CoreError::InvalidSchema {
                message: format!("Invalid schema: {}", e),
            })?;

        Ok(Self {
            schema,
            compiled,
            defaults,
        })
    }

    /// Validate values against the schema
    pub fn validate(&self, values: &JsonValue) -> ValidationResult {
        if self.compiled.is_valid(values) {
            return ValidationResult::success();
        }

        // Collect all validation errors
        let errors: Vec<ValidationErrorInfo> = self
            .compiled
            .iter_errors(values)
            .map(|e| {
                let path = e.instance_path.to_string();
                ValidationErrorInfo {
                    path: if path.is_empty() {
                        "(root)".to_string()
                    } else {
                        path
                    },
                    message: format_validation_error(&e),
                    expected: None,
                    actual: None,
                }
            })
            .collect();

        ValidationResult::failure(errors)
    }

    /// Get extracted default values
    pub fn defaults(&self) -> &JsonValue {
        &self.defaults
    }

    /// Get defaults as Values
    pub fn defaults_as_values(&self) -> Values {
        Values(self.defaults.clone())
    }

    /// Get the original schema
    pub fn schema(&self) -> &Schema {
        &self.schema
    }
}

/// Format a validation error into a user-friendly message
fn format_validation_error(error: &jsonschema::ValidationError) -> String {
    let msg = error.to_string();

    // Clean up common patterns for better readability
    msg.replace("\"", "'")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sherp_schema_parse() {
        let yaml = r#"
schemaVersion: sherpack/v1
title: Test Schema
properties:
  app:
    type: object
    properties:
      name:
        type: string
        required: true
      replicas:
        type: integer
        default: 1
        min: 0
        max: 100
"#;

        let schema = Schema::from_sherp_schema(yaml).unwrap();
        match schema {
            Schema::SherpSchema(s) => {
                assert_eq!(s.title, Some("Test Schema".to_string()));
                assert!(s.properties.contains_key("app"));
            }
            _ => panic!("Expected SherpSchema"),
        }
    }

    #[test]
    fn test_sherp_to_json_schema_conversion() {
        let yaml = r#"
schemaVersion: sherpack/v1
properties:
  name:
    type: string
    required: true
  replicas:
    type: integer
    default: 3
"#;

        let schema = Schema::from_sherp_schema(yaml).unwrap();
        let json_schema = schema.to_json_schema();

        let obj = json_schema.as_object().unwrap();
        assert_eq!(obj.get("type"), Some(&JsonValue::String("object".into())));
        assert!(obj.contains_key("properties"));

        let required = obj.get("required").unwrap().as_array().unwrap();
        assert!(required.contains(&JsonValue::String("name".into())));
    }

    #[test]
    fn test_extract_defaults() {
        let yaml = r#"
schemaVersion: sherpack/v1
properties:
  replicas:
    type: integer
    default: 3
  image:
    type: object
    properties:
      tag:
        type: string
        default: latest
      pullPolicy:
        type: string
        default: IfNotPresent
"#;

        let schema = Schema::from_sherp_schema(yaml).unwrap();
        let defaults = schema.extract_defaults();

        assert_eq!(defaults.get("replicas"), Some(&JsonValue::from(3)));

        let image = defaults.get("image").unwrap();
        assert_eq!(image.get("tag"), Some(&JsonValue::String("latest".into())));
        assert_eq!(
            image.get("pullPolicy"),
            Some(&JsonValue::String("IfNotPresent".into()))
        );
    }

    #[test]
    fn test_validation_success() {
        let yaml = r#"
schemaVersion: sherpack/v1
properties:
  replicas:
    type: integer
    min: 0
    max: 10
"#;

        let schema = Schema::from_sherp_schema(yaml).unwrap();
        let validator = SchemaValidator::new(schema).unwrap();

        let values = serde_json::json!({
            "replicas": 5
        });

        let result = validator.validate(&values);
        assert!(result.is_valid);
        assert!(result.errors.is_empty());
    }

    #[test]
    fn test_validation_failure() {
        let yaml = r#"
schemaVersion: sherpack/v1
properties:
  replicas:
    type: integer
    min: 0
    max: 10
"#;

        let schema = Schema::from_sherp_schema(yaml).unwrap();
        let validator = SchemaValidator::new(schema).unwrap();

        let values = serde_json::json!({
            "replicas": "not a number"
        });

        let result = validator.validate(&values);
        assert!(!result.is_valid);
        assert!(!result.errors.is_empty());
    }

    #[test]
    fn test_json_schema_detection() {
        let json_schema = r#"{
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "properties": {
                "name": { "type": "string" }
            }
        }"#;

        let schema = Schema::from_json_schema(json_schema).unwrap();
        match schema {
            Schema::JsonSchema(_) => {}
            _ => panic!("Expected JsonSchema"),
        }
    }
}
