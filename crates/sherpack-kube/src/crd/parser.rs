//! CRD YAML parser
//!
//! Parses CustomResourceDefinition YAML manifests into structured `CrdSchema`
//! for semantic comparison and change analysis.

use serde_json::Value;

use super::schema::{
    AdditionalProperties, CrdNames, CrdSchema, CrdScope, CrdVersionSchema, OpenApiSchema,
    PrinterColumn, PropertyType, ScaleSubresource, SchemaProperty, Subresources,
};
use crate::error::{KubeError, Result};

/// Parser for CRD YAML manifests
pub struct CrdParser;

impl CrdParser {
    /// Parse a CRD YAML manifest into a structured schema
    pub fn parse(yaml: &str) -> Result<CrdSchema> {
        let value: Value = serde_yaml::from_str(yaml)
            .map_err(|e| KubeError::Serialization(format!("Invalid CRD YAML: {}", e)))?;

        Self::parse_value(&value)
    }

    /// Parse from a serde_json::Value (useful for dynamic objects)
    pub fn parse_value(value: &Value) -> Result<CrdSchema> {
        // Validate it's a CRD
        let kind = value
            .get("kind")
            .and_then(Value::as_str)
            .ok_or_else(|| KubeError::InvalidConfig("Missing 'kind' field".to_string()))?;

        if kind != "CustomResourceDefinition" {
            return Err(KubeError::InvalidConfig(format!(
                "Expected CustomResourceDefinition, got {}",
                kind
            )));
        }

        // Extract metadata
        let metadata = value
            .get("metadata")
            .ok_or_else(|| KubeError::InvalidConfig("Missing 'metadata' field".to_string()))?;

        let name = metadata
            .get("name")
            .and_then(Value::as_str)
            .ok_or_else(|| KubeError::InvalidConfig("Missing 'metadata.name' field".to_string()))?
            .to_string();

        // Extract spec
        let spec = value
            .get("spec")
            .ok_or_else(|| KubeError::InvalidConfig("Missing 'spec' field".to_string()))?;

        // Parse group
        let group = spec
            .get("group")
            .and_then(Value::as_str)
            .ok_or_else(|| KubeError::InvalidConfig("Missing 'spec.group' field".to_string()))?
            .to_string();

        // Parse scope
        let scope = spec
            .get("scope")
            .and_then(Value::as_str)
            .map(|s| match s {
                "Cluster" => CrdScope::Cluster,
                _ => CrdScope::Namespaced,
            })
            .unwrap_or(CrdScope::Namespaced);

        // Parse names
        let names = Self::parse_names(spec.get("names"))?;

        // Parse versions
        let versions = Self::parse_versions(spec.get("versions"))?;

        Ok(CrdSchema {
            name,
            group,
            scope,
            names,
            versions,
        })
    }

    /// Parse CRD names section
    fn parse_names(names_value: Option<&Value>) -> Result<CrdNames> {
        let names = names_value
            .ok_or_else(|| KubeError::InvalidConfig("Missing 'spec.names' field".to_string()))?;

        Ok(CrdNames {
            kind: names
                .get("kind")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            plural: names
                .get("plural")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            singular: names
                .get("singular")
                .and_then(Value::as_str)
                .map(String::from),
            short_names: names
                .get("shortNames")
                .and_then(Value::as_array)
                .map(|arr| {
                    arr.iter()
                        .filter_map(Value::as_str)
                        .map(String::from)
                        .collect()
                })
                .unwrap_or_default(),
            list_kind: names
                .get("listKind")
                .and_then(Value::as_str)
                .map(String::from),
            categories: names
                .get("categories")
                .and_then(Value::as_array)
                .map(|arr| {
                    arr.iter()
                        .filter_map(Value::as_str)
                        .map(String::from)
                        .collect()
                })
                .unwrap_or_default(),
        })
    }

    /// Parse CRD versions array
    fn parse_versions(versions_value: Option<&Value>) -> Result<Vec<CrdVersionSchema>> {
        let versions = versions_value
            .and_then(Value::as_array)
            .ok_or_else(|| KubeError::InvalidConfig("Missing 'spec.versions' array".to_string()))?;

        versions.iter().map(Self::parse_version).collect()
    }

    /// Parse a single CRD version
    fn parse_version(version: &Value) -> Result<CrdVersionSchema> {
        let name = version
            .get("name")
            .and_then(Value::as_str)
            .ok_or_else(|| KubeError::InvalidConfig("Version missing 'name' field".to_string()))?
            .to_string();

        let served = version
            .get("served")
            .and_then(Value::as_bool)
            .unwrap_or(true);

        let storage = version
            .get("storage")
            .and_then(Value::as_bool)
            .unwrap_or(false);

        let deprecated = version
            .get("deprecated")
            .and_then(Value::as_bool)
            .unwrap_or(false);

        let deprecation_warning = version
            .get("deprecationWarning")
            .and_then(Value::as_str)
            .map(String::from);

        // Parse schema
        let schema = version
            .get("schema")
            .and_then(|s| s.get("openAPIV3Schema"))
            .map(Self::parse_openapi_schema)
            .transpose()?;

        // Parse printer columns
        let printer_columns = version
            .get("additionalPrinterColumns")
            .and_then(Value::as_array)
            .map(|arr| arr.iter().filter_map(Self::parse_printer_column).collect())
            .unwrap_or_default();

        // Parse subresources
        let subresources = Self::parse_subresources(version.get("subresources"));

        Ok(CrdVersionSchema {
            name,
            served,
            storage,
            deprecated,
            deprecation_warning,
            schema,
            printer_columns,
            subresources,
        })
    }

    /// Parse OpenAPI v3 schema
    fn parse_openapi_schema(schema: &Value) -> Result<OpenApiSchema> {
        let properties = schema
            .get("properties")
            .and_then(Value::as_object)
            .map(|obj| {
                obj.iter()
                    .map(|(k, v)| (k.clone(), Self::parse_schema_property(v)))
                    .collect()
            })
            .unwrap_or_default();

        let required = schema
            .get("required")
            .and_then(Value::as_array)
            .map(|arr| {
                arr.iter()
                    .filter_map(Value::as_str)
                    .map(String::from)
                    .collect()
            })
            .unwrap_or_default();

        let x_preserve_unknown = schema
            .get("x-kubernetes-preserve-unknown-fields")
            .and_then(Value::as_bool)
            .unwrap_or(false);

        Ok(OpenApiSchema {
            properties,
            required,
            x_preserve_unknown,
        })
    }

    /// Parse a single schema property (recursive)
    fn parse_schema_property(prop: &Value) -> SchemaProperty {
        let type_ = prop
            .get("type")
            .and_then(Value::as_str)
            .map(PropertyType::parse)
            .unwrap_or_default();

        let description = prop
            .get("description")
            .and_then(Value::as_str)
            .map(String::from);

        let default = prop.get("default").cloned();

        let format = prop.get("format").and_then(Value::as_str).map(String::from);

        let pattern = prop
            .get("pattern")
            .and_then(Value::as_str)
            .map(String::from);

        let enum_values = prop.get("enum").and_then(Value::as_array).cloned();

        let minimum = prop.get("minimum").and_then(Value::as_f64);
        let maximum = prop.get("maximum").and_then(Value::as_f64);
        let exclusive_minimum = prop.get("exclusiveMinimum").and_then(Value::as_f64);
        let exclusive_maximum = prop.get("exclusiveMaximum").and_then(Value::as_f64);
        let multiple_of = prop.get("multipleOf").and_then(Value::as_f64);

        let min_length = prop.get("minLength").and_then(Value::as_u64);
        let max_length = prop.get("maxLength").and_then(Value::as_u64);

        let min_items = prop.get("minItems").and_then(Value::as_u64);
        let max_items = prop.get("maxItems").and_then(Value::as_u64);
        let unique_items = prop
            .get("uniqueItems")
            .and_then(Value::as_bool)
            .unwrap_or(false);

        let min_properties = prop.get("minProperties").and_then(Value::as_u64);
        let max_properties = prop.get("maxProperties").and_then(Value::as_u64);

        let nullable = prop
            .get("nullable")
            .and_then(Value::as_bool)
            .unwrap_or(false);

        // Recursively parse nested properties
        let properties = prop
            .get("properties")
            .and_then(Value::as_object)
            .map(|obj| {
                obj.iter()
                    .map(|(k, v)| (k.clone(), Self::parse_schema_property(v)))
                    .collect()
            });

        let required = prop.get("required").and_then(Value::as_array).map(|arr| {
            arr.iter()
                .filter_map(Value::as_str)
                .map(String::from)
                .collect()
        });

        // Parse array items
        let items = prop
            .get("items")
            .map(|v| Box::new(Self::parse_schema_property(v)));

        // Parse additional properties
        let additional_properties = prop.get("additionalProperties").map(|v| {
            if v.is_boolean() {
                if v.as_bool().unwrap_or(true) {
                    AdditionalProperties::Allowed
                } else {
                    AdditionalProperties::Denied
                }
            } else {
                AdditionalProperties::Schema(Box::new(Self::parse_schema_property(v)))
            }
        });

        let x_preserve_unknown = prop
            .get("x-kubernetes-preserve-unknown-fields")
            .and_then(Value::as_bool)
            .unwrap_or(false);

        let x_embedded_resource = prop
            .get("x-kubernetes-embedded-resource")
            .and_then(Value::as_bool)
            .unwrap_or(false);

        let x_int_or_string = prop
            .get("x-kubernetes-int-or-string")
            .and_then(Value::as_bool)
            .unwrap_or(false);

        SchemaProperty {
            type_,
            description,
            default,
            format,
            pattern,
            enum_values,
            minimum,
            maximum,
            exclusive_minimum,
            exclusive_maximum,
            multiple_of,
            min_length,
            max_length,
            min_items,
            max_items,
            unique_items,
            min_properties,
            max_properties,
            nullable,
            properties,
            required,
            items,
            additional_properties,
            x_preserve_unknown,
            x_embedded_resource,
            x_int_or_string,
        }
    }

    /// Parse printer column
    fn parse_printer_column(col: &Value) -> Option<PrinterColumn> {
        Some(PrinterColumn {
            name: col.get("name").and_then(Value::as_str)?.to_string(),
            type_: col
                .get("type")
                .and_then(Value::as_str)
                .unwrap_or("string")
                .to_string(),
            json_path: col.get("jsonPath").and_then(Value::as_str)?.to_string(),
            description: col
                .get("description")
                .and_then(Value::as_str)
                .map(String::from),
            priority: col.get("priority").and_then(Value::as_i64).unwrap_or(0) as i32,
            format: col.get("format").and_then(Value::as_str).map(String::from),
        })
    }

    /// Parse subresources section
    fn parse_subresources(subresources: Option<&Value>) -> Subresources {
        let Some(sub) = subresources else {
            return Subresources::default();
        };

        let status = sub.get("status").is_some();

        let scale = sub.get("scale").map(|s| ScaleSubresource {
            spec_replicas_path: s
                .get("specReplicasPath")
                .and_then(Value::as_str)
                .unwrap_or(".spec.replicas")
                .to_string(),
            status_replicas_path: s
                .get("statusReplicasPath")
                .and_then(Value::as_str)
                .unwrap_or(".status.replicas")
                .to_string(),
            label_selector_path: s
                .get("labelSelectorPath")
                .and_then(Value::as_str)
                .map(String::from),
        });

        Subresources { status, scale }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_CRD: &str = r#"
apiVersion: apiextensions.k8s.io/v1
kind: CustomResourceDefinition
metadata:
  name: certificates.cert-manager.io
spec:
  group: cert-manager.io
  scope: Namespaced
  names:
    kind: Certificate
    plural: certificates
    singular: certificate
    shortNames:
      - cert
      - certs
    categories:
      - cert-manager
  versions:
    - name: v1
      served: true
      storage: true
      subresources:
        status: {}
      additionalPrinterColumns:
        - name: Ready
          type: string
          jsonPath: .status.conditions[?(@.type=="Ready")].status
        - name: Secret
          type: string
          jsonPath: .spec.secretName
        - name: Age
          type: date
          jsonPath: .metadata.creationTimestamp
      schema:
        openAPIV3Schema:
          type: object
          required:
            - spec
          properties:
            spec:
              type: object
              required:
                - secretName
                - issuerRef
              properties:
                secretName:
                  type: string
                  maxLength: 253
                  description: The name of the secret to store the certificate
                issuerRef:
                  type: object
                  required:
                    - name
                  properties:
                    name:
                      type: string
                    kind:
                      type: string
                      enum:
                        - Issuer
                        - ClusterIssuer
                duration:
                  type: string
                  pattern: "^[0-9]+(h|m|s)$"
                renewBefore:
                  type: string
                dnsNames:
                  type: array
                  items:
                    type: string
            status:
              type: object
              properties:
                conditions:
                  type: array
                  items:
                    type: object
    - name: v1beta1
      served: true
      storage: false
      deprecated: true
      deprecationWarning: "cert-manager.io/v1beta1 is deprecated, use v1"
      schema:
        openAPIV3Schema:
          type: object
"#;

    #[test]
    fn test_parse_crd() {
        let schema = CrdParser::parse(SAMPLE_CRD).unwrap();

        assert_eq!(schema.name, "certificates.cert-manager.io");
        assert_eq!(schema.group, "cert-manager.io");
        assert_eq!(schema.scope, CrdScope::Namespaced);
        assert_eq!(schema.names.kind, "Certificate");
        assert_eq!(schema.names.plural, "certificates");
        assert_eq!(schema.names.short_names, vec!["cert", "certs"]);
        assert_eq!(schema.versions.len(), 2);
    }

    #[test]
    fn test_parse_versions() {
        let schema = CrdParser::parse(SAMPLE_CRD).unwrap();

        let v1 = &schema.versions[0];
        assert_eq!(v1.name, "v1");
        assert!(v1.served);
        assert!(v1.storage);
        assert!(!v1.deprecated);
        assert!(v1.subresources.status);

        let v1beta1 = &schema.versions[1];
        assert_eq!(v1beta1.name, "v1beta1");
        assert!(v1beta1.served);
        assert!(!v1beta1.storage);
        assert!(v1beta1.deprecated);
        assert_eq!(
            v1beta1.deprecation_warning,
            Some("cert-manager.io/v1beta1 is deprecated, use v1".to_string())
        );
    }

    #[test]
    fn test_parse_printer_columns() {
        let schema = CrdParser::parse(SAMPLE_CRD).unwrap();
        let v1 = &schema.versions[0];

        assert_eq!(v1.printer_columns.len(), 3);
        assert_eq!(v1.printer_columns[0].name, "Ready");
        assert_eq!(v1.printer_columns[0].type_, "string");
        assert_eq!(v1.printer_columns[1].name, "Secret");
        assert_eq!(v1.printer_columns[2].name, "Age");
        assert_eq!(v1.printer_columns[2].type_, "date");
    }

    #[test]
    fn test_parse_schema_properties() {
        let schema = CrdParser::parse(SAMPLE_CRD).unwrap();
        let v1 = &schema.versions[0];
        let openapi = v1.schema.as_ref().unwrap();

        // Check root properties
        assert!(openapi.properties.contains_key("spec"));
        assert!(openapi.properties.contains_key("status"));
        assert!(openapi.required.contains(&"spec".to_string()));

        // Check spec properties
        let spec = &openapi.properties["spec"];
        let spec_props = spec.properties.as_ref().unwrap();

        assert!(spec_props.contains_key("secretName"));
        assert!(spec_props.contains_key("issuerRef"));
        assert!(spec_props.contains_key("dnsNames"));

        // Check secretName constraints
        let secret_name = &spec_props["secretName"];
        assert_eq!(secret_name.type_, PropertyType::String);
        assert_eq!(secret_name.max_length, Some(253));

        // Check duration pattern
        let duration = &spec_props["duration"];
        assert_eq!(duration.pattern, Some("^[0-9]+(h|m|s)$".to_string()));

        // Check issuerRef.kind enum
        let issuer_ref = &spec_props["issuerRef"];
        let issuer_props = issuer_ref.properties.as_ref().unwrap();
        let kind = &issuer_props["kind"];
        assert!(kind.enum_values.is_some());
        let enums = kind.enum_values.as_ref().unwrap();
        assert_eq!(enums.len(), 2);

        // Check dnsNames array
        let dns_names = &spec_props["dnsNames"];
        assert_eq!(dns_names.type_, PropertyType::Array);
        assert!(dns_names.items.is_some());
        assert_eq!(
            dns_names.items.as_ref().unwrap().type_,
            PropertyType::String
        );
    }

    #[test]
    fn test_parse_invalid_kind() {
        let yaml = r#"
apiVersion: v1
kind: ConfigMap
metadata:
  name: test
"#;
        let result = CrdParser::parse(yaml);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Expected CustomResourceDefinition")
        );
    }

    #[test]
    fn test_parse_cluster_scope() {
        let yaml = r#"
apiVersion: apiextensions.k8s.io/v1
kind: CustomResourceDefinition
metadata:
  name: clusterissuers.cert-manager.io
spec:
  group: cert-manager.io
  scope: Cluster
  names:
    kind: ClusterIssuer
    plural: clusterissuers
  versions:
    - name: v1
      served: true
      storage: true
      schema:
        openAPIV3Schema:
          type: object
"#;
        let schema = CrdParser::parse(yaml).unwrap();
        assert_eq!(schema.scope, CrdScope::Cluster);
    }

    #[test]
    fn test_parse_with_scale_subresource() {
        let yaml = r#"
apiVersion: apiextensions.k8s.io/v1
kind: CustomResourceDefinition
metadata:
  name: scalables.example.com
spec:
  group: example.com
  scope: Namespaced
  names:
    kind: Scalable
    plural: scalables
  versions:
    - name: v1
      served: true
      storage: true
      subresources:
        status: {}
        scale:
          specReplicasPath: .spec.replicas
          statusReplicasPath: .status.replicas
          labelSelectorPath: .status.selector
      schema:
        openAPIV3Schema:
          type: object
"#;
        let schema = CrdParser::parse(yaml).unwrap();
        let v1 = &schema.versions[0];

        assert!(v1.subresources.status);
        let scale = v1.subresources.scale.as_ref().unwrap();
        assert_eq!(scale.spec_replicas_path, ".spec.replicas");
        assert_eq!(scale.status_replicas_path, ".status.replicas");
        assert_eq!(
            scale.label_selector_path,
            Some(".status.selector".to_string())
        );
    }
}
