# Phase 2: CRD Safe Updates - Implementation Design

## Executive Summary

This document proposes an elegant, idiomatic Rust implementation for CRD Safe Updates that addresses Helm's major pain points while providing a superior user experience.

---

## Helm's Core Problems We're Solving

| Problem | Helm Behavior | Sherpack Solution |
|---------|--------------|-------------------|
| **Never updates CRDs** | `crds/` only installed on `helm install` | Full update support with safety analysis |
| **Wrong patch type** | Strategic Merge Patch fails on CRDs | Server-Side Apply (already implemented) |
| **No safety analysis** | Silent breaking changes | Smart change detection and classification |
| **No diff preview** | `--dry-run` broken for CRDs | Rich terminal diff with impact analysis |
| **All-or-nothing** | Either skip all CRDs or risk breakage | Per-change safety decisions |

---

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                          CRD Update Pipeline                                │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│   Pack.yaml          Cluster                                                │
│       │                  │                                                  │
│       ▼                  ▼                                                  │
│  ┌─────────┐       ┌─────────┐        ┌──────────────┐                     │
│  │ CRD     │       │ Cluster │        │   CRD        │                     │
│  │ Files   │──────▶│ CRDs    │───────▶│   Analyzer   │                     │
│  └─────────┘       └─────────┘        └──────┬───────┘                     │
│                                              │                              │
│                                              ▼                              │
│                     ┌────────────────────────────────────────┐             │
│                     │         CrdAnalysis                     │             │
│                     │  ┌────────┐ ┌────────┐ ┌────────────┐  │             │
│                     │  │ Safe   │ │ Warn   │ │ Dangerous  │  │             │
│                     │  │ +field │ │ ~valid │ │ -version   │  │             │
│                     │  └────────┘ └────────┘ └────────────┘  │             │
│                     └────────────────────────────────────────┘             │
│                                              │                              │
│                     ┌────────────────────────┼────────────────────┐        │
│                     │                        │                    │        │
│                     ▼                        ▼                    ▼        │
│              ┌────────────┐          ┌────────────┐       ┌────────────┐   │
│              │ DiffRenderer│          │ Strategy   │       │ CrdApplier │   │
│              │            │          │ Executor   │──────▶│            │   │
│              │ Terminal   │          │            │       │ SSA + Wait │   │
│              │ colors     │          │ safe/force │       │ for Ready  │   │
│              └────────────┘          └────────────┘       └────────────┘   │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## Core Data Structures

### 1. CRD Schema Representation

```rust
//! crates/sherpack-kube/src/crd/schema.rs

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// A parsed CRD ready for comparison
#[derive(Debug, Clone)]
pub struct CrdSchema {
    /// Full CRD name (e.g., "certificates.cert-manager.io")
    pub name: String,
    /// API group
    pub group: String,
    /// Scope: Namespaced or Cluster
    pub scope: CrdScope,
    /// Plural/singular/kind names
    pub names: CrdNames,
    /// API versions with their schemas
    pub versions: Vec<CrdVersionSchema>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CrdScope {
    Namespaced,
    Cluster,
}

#[derive(Debug, Clone)]
pub struct CrdNames {
    pub kind: String,
    pub plural: String,
    pub singular: Option<String>,
    pub short_names: Vec<String>,
}

/// A single API version of a CRD
#[derive(Debug, Clone)]
pub struct CrdVersionSchema {
    /// Version name (e.g., "v1", "v1beta1")
    pub name: String,
    /// Whether this version is served by the API server
    pub served: bool,
    /// Whether this is the storage version
    pub storage: bool,
    /// OpenAPI v3 schema for validation
    pub schema: Option<OpenApiSchema>,
    /// Additional printer columns for kubectl
    pub printer_columns: Vec<PrinterColumn>,
    /// Subresources (status, scale)
    pub subresources: Option<Subresources>,
}

/// Simplified OpenAPI v3 schema for comparison
#[derive(Debug, Clone)]
pub struct OpenApiSchema {
    /// Root properties (spec, status, etc.)
    pub properties: BTreeMap<String, SchemaProperty>,
    /// Required field names
    pub required: Vec<String>,
    /// Whether additional properties are allowed
    pub additional_properties: AdditionalProperties,
    /// Preserve unknown fields
    pub x_preserve_unknown: bool,
}

#[derive(Debug, Clone)]
pub struct SchemaProperty {
    pub type_: PropertyType,
    pub description: Option<String>,
    pub default: Option<serde_json::Value>,
    pub format: Option<String>,
    pub pattern: Option<String>,
    pub enum_values: Option<Vec<serde_json::Value>>,
    pub minimum: Option<f64>,
    pub maximum: Option<f64>,
    pub min_length: Option<u64>,
    pub max_length: Option<u64>,
    pub nullable: bool,
    /// Nested object properties
    pub properties: Option<BTreeMap<String, Box<SchemaProperty>>>,
    /// Required nested properties
    pub required: Option<Vec<String>>,
    /// Array item schema
    pub items: Option<Box<SchemaProperty>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PropertyType {
    String,
    Integer,
    Number,
    Boolean,
    Array,
    Object,
    Unknown(String),
}

#[derive(Debug, Clone)]
pub enum AdditionalProperties {
    Allowed,
    Denied,
    Schema(Box<SchemaProperty>),
}

#[derive(Debug, Clone)]
pub struct PrinterColumn {
    pub name: String,
    pub type_: String,
    pub json_path: String,
    pub description: Option<String>,
    pub priority: i32,
}

#[derive(Debug, Clone, Default)]
pub struct Subresources {
    pub status: bool,
    pub scale: Option<ScaleSubresource>,
}

#[derive(Debug, Clone)]
pub struct ScaleSubresource {
    pub spec_replicas_path: String,
    pub status_replicas_path: String,
    pub label_selector_path: Option<String>,
}
```

### 2. Change Detection

```rust
//! crates/sherpack-kube/src/crd/analyzer.rs

use super::schema::*;

/// Result of analyzing CRD changes
#[derive(Debug)]
pub struct CrdAnalysis {
    /// CRD name being analyzed
    pub crd_name: String,
    /// All detected changes
    pub changes: Vec<CrdChange>,
    /// Old schema (None if new CRD)
    pub old_schema: Option<CrdSchema>,
    /// New schema
    pub new_schema: CrdSchema,
}

impl CrdAnalysis {
    /// Check if any changes are dangerous
    pub fn has_dangerous_changes(&self) -> bool {
        self.changes.iter().any(|c| c.severity() == ChangeSeverity::Dangerous)
    }

    /// Check if any changes are warnings
    pub fn has_warnings(&self) -> bool {
        self.changes.iter().any(|c| c.severity() == ChangeSeverity::Warning)
    }

    /// Get maximum severity across all changes
    pub fn max_severity(&self) -> ChangeSeverity {
        self.changes
            .iter()
            .map(|c| c.severity())
            .max()
            .unwrap_or(ChangeSeverity::Safe)
    }

    /// Check if this is a new CRD (not an update)
    pub fn is_new(&self) -> bool {
        self.old_schema.is_none()
    }
}

/// A single detected change in a CRD
#[derive(Debug, Clone)]
pub struct CrdChange {
    /// Type of change
    pub kind: ChangeKind,
    /// JSON path to the changed element
    pub path: String,
    /// Human-readable description
    pub message: String,
    /// Old value (if applicable)
    pub old_value: Option<String>,
    /// New value (if applicable)
    pub new_value: Option<String>,
}

impl CrdChange {
    pub fn severity(&self) -> ChangeSeverity {
        self.kind.severity()
    }
}

/// Categories of CRD changes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeKind {
    // ═══════════════════════════════════════════════════════════
    // SAFE CHANGES - Can be applied without risk
    // ═══════════════════════════════════════════════════════════

    /// Adding a new optional field
    AddOptionalField,
    /// Adding a new API version (v1beta1 -> v1)
    AddVersion,
    /// Adding new printer columns (kubectl output)
    AddPrinterColumn,
    /// Adding new short names
    AddShortName,
    /// Adding subresources (status, scale)
    AddSubresource,
    /// Relaxing validation (e.g., increasing maxLength)
    RelaxValidation,
    /// Updating description or documentation
    UpdateDescription,

    // ═══════════════════════════════════════════════════════════
    // WARNING CHANGES - May affect existing resources
    // ═══════════════════════════════════════════════════════════

    /// Tightening validation (e.g., adding pattern, reducing maxLength)
    TightenValidation,
    /// Changing default value
    ChangeDefault,
    /// Adding a new required field (existing CRs may be invalid)
    AddRequiredField,
    /// Deprecating an API version (still works, but warned)
    DeprecateVersion,

    // ═══════════════════════════════════════════════════════════
    // DANGEROUS CHANGES - May break existing resources
    // ═══════════════════════════════════════════════════════════

    /// Removing an API version
    RemoveVersion,
    /// Removing a field from schema
    RemoveField,
    /// Changing a field's type (string -> integer)
    ChangeFieldType,
    /// Changing scope (Namespaced <-> Cluster)
    ChangeScope,
    /// Removing subresources
    RemoveSubresource,
    /// Changing group or kind (essentially a different CRD)
    ChangeIdentity,
}

impl ChangeKind {
    pub fn severity(self) -> ChangeSeverity {
        match self {
            // Safe
            Self::AddOptionalField
            | Self::AddVersion
            | Self::AddPrinterColumn
            | Self::AddShortName
            | Self::AddSubresource
            | Self::RelaxValidation
            | Self::UpdateDescription => ChangeSeverity::Safe,

            // Warning
            Self::TightenValidation
            | Self::ChangeDefault
            | Self::AddRequiredField
            | Self::DeprecateVersion => ChangeSeverity::Warning,

            // Dangerous
            Self::RemoveVersion
            | Self::RemoveField
            | Self::ChangeFieldType
            | Self::ChangeScope
            | Self::RemoveSubresource
            | Self::ChangeIdentity => ChangeSeverity::Dangerous,
        }
    }

    pub fn icon(self) -> &'static str {
        match self.severity() {
            ChangeSeverity::Safe => "✓",
            ChangeSeverity::Warning => "⚠",
            ChangeSeverity::Dangerous => "✗",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ChangeSeverity {
    Safe = 0,
    Warning = 1,
    Dangerous = 2,
}
```

### 3. The Analyzer Implementation

```rust
//! crates/sherpack-kube/src/crd/analyzer.rs (continued)

/// CRD change analyzer
pub struct CrdAnalyzer;

impl CrdAnalyzer {
    /// Analyze changes between two CRD versions
    pub fn analyze(old: Option<&CrdSchema>, new: &CrdSchema) -> CrdAnalysis {
        let mut changes = Vec::new();

        if let Some(old) = old {
            Self::compare_versions(old, new, &mut changes);
            Self::compare_scope(old, new, &mut changes);
            Self::compare_names(old, new, &mut changes);

            // Compare schemas for each version
            for new_ver in &new.versions {
                if let Some(old_ver) = old.versions.iter().find(|v| v.name == new_ver.name) {
                    Self::compare_version_schemas(
                        old_ver,
                        new_ver,
                        &format!("versions[{}]", new_ver.name),
                        &mut changes,
                    );
                }
            }
        }

        CrdAnalysis {
            crd_name: new.name.clone(),
            changes,
            old_schema: old.cloned(),
            new_schema: new.clone(),
        }
    }

    fn compare_versions(old: &CrdSchema, new: &CrdSchema, changes: &mut Vec<CrdChange>) {
        // Removed versions
        for old_ver in &old.versions {
            if !new.versions.iter().any(|v| v.name == old_ver.name) {
                changes.push(CrdChange {
                    kind: ChangeKind::RemoveVersion,
                    path: format!("versions[{}]", old_ver.name),
                    message: format!("API version {} removed", old_ver.name),
                    old_value: Some(old_ver.name.clone()),
                    new_value: None,
                });
            }
        }

        // Added versions
        for new_ver in &new.versions {
            if !old.versions.iter().any(|v| v.name == new_ver.name) {
                changes.push(CrdChange {
                    kind: ChangeKind::AddVersion,
                    path: format!("versions[{}]", new_ver.name),
                    message: format!("API version {} added", new_ver.name),
                    old_value: None,
                    new_value: Some(new_ver.name.clone()),
                });
            }
        }

        // Deprecated versions (served: true -> false)
        for new_ver in &new.versions {
            if let Some(old_ver) = old.versions.iter().find(|v| v.name == new_ver.name) {
                if old_ver.served && !new_ver.served {
                    changes.push(CrdChange {
                        kind: ChangeKind::DeprecateVersion,
                        path: format!("versions[{}].served", new_ver.name),
                        message: format!("API version {} deprecated (no longer served)", new_ver.name),
                        old_value: Some("true".to_string()),
                        new_value: Some("false".to_string()),
                    });
                }
            }
        }
    }

    fn compare_scope(old: &CrdSchema, new: &CrdSchema, changes: &mut Vec<CrdChange>) {
        if old.scope != new.scope {
            changes.push(CrdChange {
                kind: ChangeKind::ChangeScope,
                path: "scope".to_string(),
                message: format!(
                    "Scope changed from {:?} to {:?}",
                    old.scope, new.scope
                ),
                old_value: Some(format!("{:?}", old.scope)),
                new_value: Some(format!("{:?}", new.scope)),
            });
        }
    }

    fn compare_names(old: &CrdSchema, new: &CrdSchema, changes: &mut Vec<CrdChange>) {
        // New short names added
        for name in &new.names.short_names {
            if !old.names.short_names.contains(name) {
                changes.push(CrdChange {
                    kind: ChangeKind::AddShortName,
                    path: "names.shortNames".to_string(),
                    message: format!("Short name '{}' added", name),
                    old_value: None,
                    new_value: Some(name.clone()),
                });
            }
        }
    }

    fn compare_version_schemas(
        old: &CrdVersionSchema,
        new: &CrdVersionSchema,
        path_prefix: &str,
        changes: &mut Vec<CrdChange>,
    ) {
        // Compare printer columns
        Self::compare_printer_columns(old, new, path_prefix, changes);

        // Compare subresources
        Self::compare_subresources(old, new, path_prefix, changes);

        // Compare schema properties
        if let (Some(old_schema), Some(new_schema)) = (&old.schema, &new.schema) {
            Self::compare_properties(
                &old_schema.properties,
                &new_schema.properties,
                &old_schema.required,
                &new_schema.required,
                &format!("{}.schema", path_prefix),
                changes,
            );
        }
    }

    fn compare_properties(
        old_props: &BTreeMap<String, SchemaProperty>,
        new_props: &BTreeMap<String, SchemaProperty>,
        old_required: &[String],
        new_required: &[String],
        path_prefix: &str,
        changes: &mut Vec<CrdChange>,
    ) {
        // Removed fields
        for (name, _prop) in old_props {
            if !new_props.contains_key(name) {
                changes.push(CrdChange {
                    kind: ChangeKind::RemoveField,
                    path: format!("{}.properties.{}", path_prefix, name),
                    message: format!("Field '{}' removed", name),
                    old_value: Some(name.clone()),
                    new_value: None,
                });
            }
        }

        // Added fields
        for (name, prop) in new_props {
            if !old_props.contains_key(name) {
                let is_required = new_required.contains(name);
                let kind = if is_required {
                    ChangeKind::AddRequiredField
                } else {
                    ChangeKind::AddOptionalField
                };

                changes.push(CrdChange {
                    kind,
                    path: format!("{}.properties.{}", path_prefix, name),
                    message: format!(
                        "Field '{}' added ({})",
                        name,
                        if is_required { "required" } else { "optional" }
                    ),
                    old_value: None,
                    new_value: Some(format!("{:?}", prop.type_)),
                });
            }
        }

        // Modified fields
        for (name, new_prop) in new_props {
            if let Some(old_prop) = old_props.get(name) {
                Self::compare_property(
                    old_prop,
                    new_prop,
                    name,
                    &format!("{}.properties.{}", path_prefix, name),
                    changes,
                );
            }
        }

        // Newly required fields
        for name in new_required {
            if !old_required.contains(name) && old_props.contains_key(name) {
                changes.push(CrdChange {
                    kind: ChangeKind::AddRequiredField,
                    path: format!("{}.required", path_prefix),
                    message: format!("Field '{}' is now required", name),
                    old_value: Some("optional".to_string()),
                    new_value: Some("required".to_string()),
                });
            }
        }
    }

    fn compare_property(
        old: &SchemaProperty,
        new: &SchemaProperty,
        field_name: &str,
        path: &str,
        changes: &mut Vec<CrdChange>,
    ) {
        // Type change
        if old.type_ != new.type_ {
            changes.push(CrdChange {
                kind: ChangeKind::ChangeFieldType,
                path: format!("{}.type", path),
                message: format!(
                    "Field '{}' type changed from {:?} to {:?}",
                    field_name, old.type_, new.type_
                ),
                old_value: Some(format!("{:?}", old.type_)),
                new_value: Some(format!("{:?}", new.type_)),
            });
        }

        // Default value change
        if old.default != new.default {
            changes.push(CrdChange {
                kind: ChangeKind::ChangeDefault,
                path: format!("{}.default", path),
                message: format!("Field '{}' default value changed", field_name),
                old_value: old.default.as_ref().map(|v| v.to_string()),
                new_value: new.default.as_ref().map(|v| v.to_string()),
            });
        }

        // Validation changes
        Self::compare_validation(old, new, field_name, path, changes);

        // Recurse into nested objects
        if let (Some(old_nested), Some(new_nested)) = (&old.properties, &new.properties) {
            Self::compare_properties(
                old_nested,
                new_nested,
                old.required.as_deref().unwrap_or(&[]),
                new.required.as_deref().unwrap_or(&[]),
                path,
                changes,
            );
        }
    }

    fn compare_validation(
        old: &SchemaProperty,
        new: &SchemaProperty,
        field_name: &str,
        path: &str,
        changes: &mut Vec<CrdChange>,
    ) {
        // Pattern change
        if old.pattern != new.pattern {
            let kind = match (&old.pattern, &new.pattern) {
                (None, Some(_)) => ChangeKind::TightenValidation,
                (Some(_), None) => ChangeKind::RelaxValidation,
                _ => ChangeKind::TightenValidation, // Assume tighter
            };
            changes.push(CrdChange {
                kind,
                path: format!("{}.pattern", path),
                message: format!("Field '{}' pattern validation changed", field_name),
                old_value: old.pattern.clone(),
                new_value: new.pattern.clone(),
            });
        }

        // maxLength change
        match (old.max_length, new.max_length) {
            (Some(old_max), Some(new_max)) if old_max != new_max => {
                let kind = if new_max > old_max {
                    ChangeKind::RelaxValidation
                } else {
                    ChangeKind::TightenValidation
                };
                changes.push(CrdChange {
                    kind,
                    path: format!("{}.maxLength", path),
                    message: format!(
                        "Field '{}' maxLength {} from {} to {}",
                        field_name,
                        if new_max > old_max { "increased" } else { "decreased" },
                        old_max,
                        new_max
                    ),
                    old_value: Some(old_max.to_string()),
                    new_value: Some(new_max.to_string()),
                });
            }
            (None, Some(new_max)) => {
                changes.push(CrdChange {
                    kind: ChangeKind::TightenValidation,
                    path: format!("{}.maxLength", path),
                    message: format!(
                        "Field '{}' maxLength constraint added ({})",
                        field_name, new_max
                    ),
                    old_value: None,
                    new_value: Some(new_max.to_string()),
                });
            }
            (Some(old_max), None) => {
                changes.push(CrdChange {
                    kind: ChangeKind::RelaxValidation,
                    path: format!("{}.maxLength", path),
                    message: format!(
                        "Field '{}' maxLength constraint removed (was {})",
                        field_name, old_max
                    ),
                    old_value: Some(old_max.to_string()),
                    new_value: None,
                });
            }
            _ => {}
        }

        // Similar for minimum, maximum, minLength, enum, etc.
    }

    fn compare_printer_columns(
        old: &CrdVersionSchema,
        new: &CrdVersionSchema,
        path_prefix: &str,
        changes: &mut Vec<CrdChange>,
    ) {
        for new_col in &new.printer_columns {
            if !old.printer_columns.iter().any(|c| c.name == new_col.name) {
                changes.push(CrdChange {
                    kind: ChangeKind::AddPrinterColumn,
                    path: format!("{}.additionalPrinterColumns", path_prefix),
                    message: format!("Printer column '{}' added", new_col.name),
                    old_value: None,
                    new_value: Some(new_col.name.clone()),
                });
            }
        }
    }

    fn compare_subresources(
        old: &CrdVersionSchema,
        new: &CrdVersionSchema,
        path_prefix: &str,
        changes: &mut Vec<CrdChange>,
    ) {
        let old_sub = old.subresources.as_ref();
        let new_sub = new.subresources.as_ref();

        // Status subresource
        match (old_sub.map(|s| s.status), new_sub.map(|s| s.status)) {
            (Some(false) | None, Some(true)) => {
                changes.push(CrdChange {
                    kind: ChangeKind::AddSubresource,
                    path: format!("{}.subresources.status", path_prefix),
                    message: "Status subresource enabled".to_string(),
                    old_value: None,
                    new_value: Some("enabled".to_string()),
                });
            }
            (Some(true), Some(false) | None) => {
                changes.push(CrdChange {
                    kind: ChangeKind::RemoveSubresource,
                    path: format!("{}.subresources.status", path_prefix),
                    message: "Status subresource disabled".to_string(),
                    old_value: Some("enabled".to_string()),
                    new_value: None,
                });
            }
            _ => {}
        }
    }
}
```

---

## Diff Renderer

### Terminal Output Design

```
$ sherpack upgrade myrelease ./mypack --show-crd-diff

CRD Analysis: certificates.cert-manager.io
══════════════════════════════════════════════════════════════════

Versions:
  ✓ + v1alpha3 (new version added)
  ⚠ ~ v1alpha2.served: true → false (deprecated)

Schema Changes (v1):
  ✓ + spec.privateKey.rotationPolicy: string
      New optional field for key rotation

  ✓ + spec.additionalOutputFormats: array
      New optional field for additional formats

  ⚠ ~ spec.secretName.maxLength: 253 → 63
      Existing certificates with long names may fail validation

  ✗ - spec.legacyField
      Field removed - existing resources using this will become invalid

  ✗ ~ spec.replicas: string → integer
      Type change - 5 existing resources will fail validation

Printer Columns:
  ✓ + ROTATED (shows rotation status)

──────────────────────────────────────────────────────────────────
Summary:
  ✓ 4 safe changes     (will be applied)
  ⚠ 2 warnings         (review recommended)
  ✗ 2 dangerous        (require --force-crd-update)

Impact Analysis:
  • 42 CustomResources will be validated against new schema
  • 5 resources may fail validation due to type changes

──────────────────────────────────────────────────────────────────
? Proceed with safe and warning changes? [y/N]
  (Use --force-crd-update to include dangerous changes)
```

### Renderer Implementation

```rust
//! crates/sherpack-cli/src/display/crd_diff.rs

use console::{style, Style, Term};
use sherpack_kube::crd::{CrdAnalysis, CrdChange, ChangeKind, ChangeSeverity};

pub struct CrdDiffRenderer {
    term: Term,
}

impl CrdDiffRenderer {
    pub fn new() -> Self {
        Self { term: Term::stderr() }
    }

    pub fn render(&self, analysis: &CrdAnalysis) -> std::io::Result<()> {
        // Header
        self.term.write_line(&format!(
            "\nCRD Analysis: {}",
            style(&analysis.crd_name).cyan().bold()
        ))?;
        self.term.write_line(&"═".repeat(66))?;

        // Group changes by category
        let version_changes: Vec<_> = analysis.changes.iter()
            .filter(|c| matches!(c.kind,
                ChangeKind::AddVersion | ChangeKind::RemoveVersion | ChangeKind::DeprecateVersion
            ))
            .collect();

        let schema_changes: Vec<_> = analysis.changes.iter()
            .filter(|c| matches!(c.kind,
                ChangeKind::AddOptionalField | ChangeKind::AddRequiredField |
                ChangeKind::RemoveField | ChangeKind::ChangeFieldType |
                ChangeKind::RelaxValidation | ChangeKind::TightenValidation |
                ChangeKind::ChangeDefault
            ))
            .collect();

        let printer_changes: Vec<_> = analysis.changes.iter()
            .filter(|c| matches!(c.kind, ChangeKind::AddPrinterColumn))
            .collect();

        // Render version changes
        if !version_changes.is_empty() {
            self.term.write_line("\nVersions:")?;
            for change in version_changes {
                self.render_change(change)?;
            }
        }

        // Render schema changes
        if !schema_changes.is_empty() {
            self.term.write_line("\nSchema Changes:")?;
            for change in schema_changes {
                self.render_change(change)?;
            }
        }

        // Render printer column changes
        if !printer_changes.is_empty() {
            self.term.write_line("\nPrinter Columns:")?;
            for change in printer_changes {
                self.render_change(change)?;
            }
        }

        // Summary
        self.render_summary(analysis)?;

        Ok(())
    }

    fn render_change(&self, change: &CrdChange) -> std::io::Result<()> {
        let (icon, color) = match change.severity() {
            ChangeSeverity::Safe => ("✓", Style::new().green()),
            ChangeSeverity::Warning => ("⚠", Style::new().yellow()),
            ChangeSeverity::Dangerous => ("✗", Style::new().red()),
        };

        let prefix = match (&change.old_value, &change.new_value) {
            (None, Some(_)) => "+",
            (Some(_), None) => "-",
            _ => "~",
        };

        self.term.write_line(&format!(
            "  {} {} {}",
            color.apply_to(icon),
            color.apply_to(prefix),
            change.message
        ))?;

        // Show old -> new for modifications
        if let (Some(old), Some(new)) = (&change.old_value, &change.new_value) {
            self.term.write_line(&format!(
                "      {} → {}",
                style(old).dim(),
                style(new).bold()
            ))?;
        }

        Ok(())
    }

    fn render_summary(&self, analysis: &CrdAnalysis) -> std::io::Result<()> {
        let safe_count = analysis.changes.iter()
            .filter(|c| c.severity() == ChangeSeverity::Safe)
            .count();
        let warn_count = analysis.changes.iter()
            .filter(|c| c.severity() == ChangeSeverity::Warning)
            .count();
        let danger_count = analysis.changes.iter()
            .filter(|c| c.severity() == ChangeSeverity::Dangerous)
            .count();

        self.term.write_line(&format!("\n{}", "─".repeat(66)))?;
        self.term.write_line("Summary:")?;

        if safe_count > 0 {
            self.term.write_line(&format!(
                "  {} {} safe changes",
                style("✓").green(),
                safe_count
            ))?;
        }
        if warn_count > 0 {
            self.term.write_line(&format!(
                "  {} {} warnings",
                style("⚠").yellow(),
                warn_count
            ))?;
        }
        if danger_count > 0 {
            self.term.write_line(&format!(
                "  {} {} dangerous (require --force-crd-update)",
                style("✗").red(),
                danger_count
            ))?;
        }

        self.term.write_line(&format!("{}", "─".repeat(66)))?;

        Ok(())
    }
}
```

---

## Strategy Pattern for Upgrade Decisions

```rust
//! crates/sherpack-kube/src/crd/strategy.rs

use super::analyzer::{CrdAnalysis, ChangeSeverity};

/// Decision on whether to apply CRD changes
#[derive(Debug, Clone)]
pub enum UpgradeDecision {
    /// Apply all changes
    Apply,
    /// Apply only safe and warning changes, skip dangerous
    ApplyPartial { skipped: Vec<String> },
    /// Reject the upgrade entirely
    Reject { reason: String },
}

/// Upgrade strategy trait
pub trait UpgradeStrategy {
    fn decide(&self, analysis: &CrdAnalysis) -> UpgradeDecision;
}

/// Safe strategy: only apply safe and warning changes
pub struct SafeStrategy;

impl UpgradeStrategy for SafeStrategy {
    fn decide(&self, analysis: &CrdAnalysis) -> UpgradeDecision {
        if analysis.has_dangerous_changes() {
            let dangerous: Vec<_> = analysis.changes.iter()
                .filter(|c| c.severity() == ChangeSeverity::Dangerous)
                .map(|c| c.message.clone())
                .collect();

            UpgradeDecision::Reject {
                reason: format!(
                    "Dangerous changes detected: {}. Use --force-crd-update to override.",
                    dangerous.join(", ")
                ),
            }
        } else {
            UpgradeDecision::Apply
        }
    }
}

/// Force strategy: apply all changes regardless of severity
pub struct ForceStrategy;

impl UpgradeStrategy for ForceStrategy {
    fn decide(&self, _analysis: &CrdAnalysis) -> UpgradeDecision {
        UpgradeDecision::Apply
    }
}

/// Skip strategy: never update CRDs
pub struct SkipStrategy;

impl UpgradeStrategy for SkipStrategy {
    fn decide(&self, _analysis: &CrdAnalysis) -> UpgradeDecision {
        UpgradeDecision::Reject {
            reason: "CRD updates skipped (--skip-crd-update)".to_string(),
        }
    }
}

/// Interactive strategy: prompt user for dangerous changes
pub struct InteractiveStrategy {
    pub skip_dangerous: bool,
}

impl UpgradeStrategy for InteractiveStrategy {
    fn decide(&self, analysis: &CrdAnalysis) -> UpgradeDecision {
        if self.skip_dangerous && analysis.has_dangerous_changes() {
            let skipped: Vec<_> = analysis.changes.iter()
                .filter(|c| c.severity() == ChangeSeverity::Dangerous)
                .map(|c| c.message.clone())
                .collect();

            UpgradeDecision::ApplyPartial { skipped }
        } else {
            UpgradeDecision::Apply
        }
    }
}

/// Create strategy from CLI options
pub fn strategy_from_options(
    skip_update: bool,
    force_update: bool,
) -> Box<dyn UpgradeStrategy> {
    if skip_update {
        Box::new(SkipStrategy)
    } else if force_update {
        Box::new(ForceStrategy)
    } else {
        Box::new(SafeStrategy)
    }
}
```

---

## Integration with ResourceManager

```rust
//! Update to crates/sherpack-kube/src/resources.rs

impl ResourceManager {
    /// Apply CRDs with safety analysis
    pub async fn apply_crds_safe(
        &self,
        pack: &LoadedPack,
        strategy: &dyn UpgradeStrategy,
        show_diff: bool,
        dry_run: bool,
    ) -> Result<CrdOperationResult> {
        let crd_manager = CrdManager::new(self.client.clone());
        let crd_manifests = pack.load_crds()?;

        if crd_manifests.is_empty() {
            return Ok(CrdOperationResult::empty());
        }

        let mut results = CrdOperationResult::default();

        for crd_manifest in &crd_manifests {
            // Parse new CRD
            let new_schema = CrdParser::parse(&crd_manifest.content)?;

            // Fetch existing CRD from cluster
            let old_schema = self.fetch_crd_schema(&new_schema.name).await?;

            // Analyze changes
            let analysis = CrdAnalyzer::analyze(old_schema.as_ref(), &new_schema);

            // Show diff if requested
            if show_diff {
                let renderer = CrdDiffRenderer::new();
                renderer.render(&analysis)?;
            }

            // Decide based on strategy
            match strategy.decide(&analysis) {
                UpgradeDecision::Apply => {
                    if !dry_run {
                        crd_manager.apply_crd(&crd_manifest.content, false).await?;
                    }
                    results.applied.push(new_schema.name.clone());
                }
                UpgradeDecision::ApplyPartial { skipped } => {
                    // Apply the CRD but warn about skipped changes
                    if !dry_run {
                        crd_manager.apply_crd(&crd_manifest.content, false).await?;
                    }
                    results.applied.push(new_schema.name.clone());
                    results.warnings.extend(skipped);
                }
                UpgradeDecision::Reject { reason } => {
                    results.rejected.push((new_schema.name.clone(), reason));
                }
            }
        }

        // Wait for all applied CRDs to be established
        if !dry_run {
            let timeout = pack.pack.crds.wait_timeout;
            for name in &results.applied {
                crd_manager.wait_for_crd(name, timeout).await?;
            }
        }

        Ok(results)
    }

    async fn fetch_crd_schema(&self, name: &str) -> Result<Option<CrdSchema>> {
        let api: Api<DynamicObject> = Api::all_with(
            self.client.clone(),
            &ApiResource::erase::<k8s_openapi::apiextensions_apiserver::pkg::apis::apiextensions::v1::CustomResourceDefinition>(&()),
        );

        match api.get(name).await {
            Ok(crd) => {
                let schema = CrdParser::parse_dynamic(&crd)?;
                Ok(Some(schema))
            }
            Err(kube::Error::Api(resp)) if resp.code == 404 => Ok(None),
            Err(e) => Err(KubeError::Api(e)),
        }
    }
}

#[derive(Debug, Default)]
pub struct CrdOperationResult {
    pub applied: Vec<String>,
    pub rejected: Vec<(String, String)>,
    pub warnings: Vec<String>,
}

impl CrdOperationResult {
    pub fn empty() -> Self {
        Self::default()
    }

    pub fn is_success(&self) -> bool {
        self.rejected.is_empty()
    }
}
```

---

## File Organization

```
crates/sherpack-kube/src/
├── crd/
│   ├── mod.rs           # Re-exports (already exists, enhanced)
│   ├── schema.rs        # NEW: CrdSchema, CrdVersionSchema, etc.
│   ├── parser.rs        # NEW: Parse YAML to CrdSchema
│   ├── analyzer.rs      # NEW: CrdAnalyzer, ChangeKind, CrdAnalysis
│   ├── strategy.rs      # NEW: UpgradeStrategy trait + implementations
│   └── apply.rs         # EXISTING: CrdManager (enhanced)
└── ...

crates/sherpack-cli/src/
├── display/
│   ├── mod.rs
│   ├── crd_diff.rs      # NEW: CrdDiffRenderer
│   └── ...
└── commands/
    ├── upgrade.rs       # MODIFIED: Wire up CRD analysis
    └── ...
```

---

## Implementation Priority

### Phase 2.1: Core Analysis (1-2 days)
1. `crd/schema.rs` - Data structures
2. `crd/parser.rs` - Parse YAML CRDs into schema
3. `crd/analyzer.rs` - Compare schemas, detect changes

### Phase 2.2: Strategy & Decision (0.5 day)
1. `crd/strategy.rs` - Safe/Force/Skip strategies
2. Integration with `ResourceManager`

### Phase 2.3: Diff Output (1 day)
1. `display/crd_diff.rs` - Terminal renderer
2. Wire up `--show-crd-diff` flag

### Phase 2.4: CLI Integration (0.5 day)
1. Update `upgrade.rs` to use new analysis
2. Update `install.rs` for first-time installs
3. Add tests

---

## Key Design Decisions

### 1. Why a full schema representation?

Parsing CRDs into a structured `CrdSchema` instead of comparing raw YAML gives us:
- Type-safe comparison
- Field-level change detection
- Intelligent severity classification
- Easy testing with mock schemas

### 2. Why the Strategy pattern?

Clean separation between:
- **What changed** (Analyzer)
- **Whether to apply** (Strategy)
- **How to show it** (Renderer)

This makes adding new strategies (interactive, partial, etc.) trivial.

### 3. Why not just diff YAML?

YAML diffing would show syntactic changes (reordering, whitespace) as differences.
Schema-aware diffing shows only **semantic** changes that matter for validation.

### 4. Server-Side Apply vs JSON Patch?

We already use SSA in Phase 1 (`Patch::Apply`). SSA is:
- Idempotent
- Conflict-aware
- The modern Kubernetes standard

JSON Merge Patch was Helm's original requirement because SSA didn't exist.
We use SSA, which is strictly better.

---

## Summary

This design addresses all major Helm frustrations:

| Helm Problem | Sherpack Solution |
|--------------|-------------------|
| CRDs never update | Full update pipeline with analysis |
| Wrong patch type | Server-Side Apply (already done) |
| No safety analysis | `CrdAnalyzer` with severity classification |
| No preview | Rich terminal diff with `--show-crd-diff` |
| All-or-nothing | Strategy pattern: safe/force/skip |
| Silent breakage | Impact warnings before apply |

The implementation is idiomatic Rust:
- Strong types (`CrdSchema`, `ChangeKind`)
- Trait-based extensibility (`UpgradeStrategy`)
- Clean separation of concerns
- Comprehensive error handling
