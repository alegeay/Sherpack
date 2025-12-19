//! CRD change analyzer
//!
//! Analyzes changes between two CRD versions and classifies them by severity.
//! This enables safe upgrades by identifying breaking changes before they're applied.

use std::collections::BTreeSet;

use super::schema::{
    CrdNames, CrdSchema, CrdVersionSchema, OpenApiSchema, PrinterColumn, SchemaProperty,
    Subresources,
};

/// Result of analyzing CRD changes
#[derive(Debug)]
pub struct CrdAnalysis {
    /// CRD name being analyzed
    pub crd_name: String,
    /// All detected changes
    pub changes: Vec<CrdChange>,
    /// Whether this is a new CRD (not an update)
    pub is_new: bool,
}

impl CrdAnalysis {
    /// Create analysis for a new CRD
    pub fn new_crd(name: String) -> Self {
        Self {
            crd_name: name,
            changes: vec![],
            is_new: true,
        }
    }

    /// Check if any changes are dangerous
    pub fn has_dangerous_changes(&self) -> bool {
        self.changes
            .iter()
            .any(|c| c.severity() == ChangeSeverity::Dangerous)
    }

    /// Check if any changes are warnings
    pub fn has_warnings(&self) -> bool {
        self.changes
            .iter()
            .any(|c| c.severity() == ChangeSeverity::Warning)
    }

    /// Get maximum severity across all changes
    pub fn max_severity(&self) -> ChangeSeverity {
        self.changes
            .iter()
            .map(|c| c.severity())
            .max()
            .unwrap_or(ChangeSeverity::Safe)
    }

    /// Count changes by severity
    pub fn count_by_severity(&self) -> (usize, usize, usize) {
        let safe = self
            .changes
            .iter()
            .filter(|c| c.severity() == ChangeSeverity::Safe)
            .count();
        let warn = self
            .changes
            .iter()
            .filter(|c| c.severity() == ChangeSeverity::Warning)
            .count();
        let danger = self
            .changes
            .iter()
            .filter(|c| c.severity() == ChangeSeverity::Dangerous)
            .count();
        (safe, warn, danger)
    }

    /// Get all dangerous changes
    pub fn dangerous_changes(&self) -> impl Iterator<Item = &CrdChange> {
        self.changes
            .iter()
            .filter(|c| c.severity() == ChangeSeverity::Dangerous)
    }
}

/// A single detected change in a CRD
#[derive(Debug, Clone)]
pub struct CrdChange {
    /// Type of change
    pub kind: ChangeKind,
    /// JSON-like path to the changed element (e.g., `versions[v1].schema.spec.replicas`)
    pub path: String,
    /// Human-readable description
    pub message: String,
    /// Old value (if applicable)
    pub old_value: Option<String>,
    /// New value (if applicable)
    pub new_value: Option<String>,
}

impl CrdChange {
    /// Get the severity of this change
    pub fn severity(&self) -> ChangeSeverity {
        self.kind.severity()
    }

    /// Get the icon for this change type
    pub fn icon(&self) -> &'static str {
        match self.severity() {
            ChangeSeverity::Safe => "✓",
            ChangeSeverity::Warning => "⚠",
            ChangeSeverity::Dangerous => "✗",
        }
    }

    /// Get the prefix character (+, -, ~)
    pub fn prefix(&self) -> &'static str {
        match (&self.old_value, &self.new_value) {
            (None, Some(_)) => "+",
            (Some(_), None) => "-",
            _ => "~",
        }
    }
}

/// Categories of CRD changes
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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
    /// Adding new categories
    AddCategory,
    /// Adding subresources (status, scale)
    AddSubresource,
    /// Relaxing validation (e.g., increasing maxLength, removing pattern)
    RelaxValidation,
    /// Updating description or documentation
    UpdateDescription,
    /// Making a nullable field non-nullable with a default
    AddDefault,

    // ═══════════════════════════════════════════════════════════
    // WARNING CHANGES - May affect existing resources
    // ═══════════════════════════════════════════════════════════
    /// Tightening validation (e.g., adding pattern, reducing maxLength)
    TightenValidation,
    /// Changing default value
    ChangeDefault,
    /// Adding a new required field (existing CRs may be invalid without migration)
    AddRequiredField,
    /// Making an optional field required
    MakeRequired,
    /// Deprecating an API version (still works, but warned)
    DeprecateVersion,
    /// Changing enum values (adding is safe, removing is dangerous)
    ChangeEnumValues,

    // ═══════════════════════════════════════════════════════════
    // DANGEROUS CHANGES - May break existing resources
    // ═══════════════════════════════════════════════════════════
    /// Removing an API version
    RemoveVersion,
    /// Removing a field from schema
    RemoveField,
    /// Removing a required field (may indicate schema redesign)
    RemoveRequiredField,
    /// Changing a field's type (string -> integer)
    ChangeFieldType,
    /// Changing scope (Namespaced <-> Cluster)
    ChangeScope,
    /// Removing subresources
    RemoveSubresource,
    /// Changing group (essentially a different CRD)
    ChangeGroup,
    /// Changing kind name
    ChangeKindName,
    /// Removing enum values
    RemoveEnumValue,
    /// Changing storage version
    ChangeStorageVersion,
}

impl ChangeKind {
    /// Get the severity of this change type
    pub fn severity(self) -> ChangeSeverity {
        match self {
            // Safe
            Self::AddOptionalField
            | Self::AddVersion
            | Self::AddPrinterColumn
            | Self::AddShortName
            | Self::AddCategory
            | Self::AddSubresource
            | Self::RelaxValidation
            | Self::UpdateDescription
            | Self::AddDefault => ChangeSeverity::Safe,

            // Warning
            Self::TightenValidation
            | Self::ChangeDefault
            | Self::AddRequiredField
            | Self::MakeRequired
            | Self::DeprecateVersion
            | Self::ChangeEnumValues => ChangeSeverity::Warning,

            // Dangerous
            Self::RemoveVersion
            | Self::RemoveField
            | Self::RemoveRequiredField
            | Self::ChangeFieldType
            | Self::ChangeScope
            | Self::RemoveSubresource
            | Self::ChangeGroup
            | Self::ChangeKindName
            | Self::RemoveEnumValue
            | Self::ChangeStorageVersion => ChangeSeverity::Dangerous,
        }
    }

    /// Get a short description of this change type
    pub fn description(self) -> &'static str {
        match self {
            Self::AddOptionalField => "optional field added",
            Self::AddVersion => "API version added",
            Self::AddPrinterColumn => "printer column added",
            Self::AddShortName => "short name added",
            Self::AddCategory => "category added",
            Self::AddSubresource => "subresource enabled",
            Self::RelaxValidation => "validation relaxed",
            Self::UpdateDescription => "description updated",
            Self::AddDefault => "default value added",
            Self::TightenValidation => "validation tightened",
            Self::ChangeDefault => "default value changed",
            Self::AddRequiredField => "required field added",
            Self::MakeRequired => "field now required",
            Self::DeprecateVersion => "version deprecated",
            Self::ChangeEnumValues => "enum values changed",
            Self::RemoveVersion => "API version removed",
            Self::RemoveField => "field removed",
            Self::RemoveRequiredField => "required field removed",
            Self::ChangeFieldType => "field type changed",
            Self::ChangeScope => "scope changed",
            Self::RemoveSubresource => "subresource disabled",
            Self::ChangeGroup => "group changed",
            Self::ChangeKindName => "kind name changed",
            Self::RemoveEnumValue => "enum value removed",
            Self::ChangeStorageVersion => "storage version changed",
        }
    }
}

/// Severity of a CRD change
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ChangeSeverity {
    /// Safe change, can be applied without risk
    Safe = 0,
    /// Warning: may affect existing resources, review recommended
    Warning = 1,
    /// Dangerous: may break existing resources or cause data loss
    Dangerous = 2,
}

impl ChangeSeverity {
    /// Get display color name for this severity
    pub fn color(self) -> &'static str {
        match self {
            Self::Safe => "green",
            Self::Warning => "yellow",
            Self::Dangerous => "red",
        }
    }
}

/// CRD change analyzer
pub struct CrdAnalyzer;

impl CrdAnalyzer {
    /// Analyze changes between two CRD versions
    ///
    /// If `old` is None, this is a new CRD installation.
    pub fn analyze(old: Option<&CrdSchema>, new: &CrdSchema) -> CrdAnalysis {
        let Some(old) = old else {
            return CrdAnalysis::new_crd(new.name.clone());
        };

        let mut changes = Vec::new();

        // Compare top-level identity
        Self::compare_identity(old, new, &mut changes);

        // Compare scope
        Self::compare_scope(old, new, &mut changes);

        // Compare names
        Self::compare_names(&old.names, &new.names, &mut changes);

        // Compare versions
        Self::compare_versions(old, new, &mut changes);

        CrdAnalysis {
            crd_name: new.name.clone(),
            changes,
            is_new: false,
        }
    }

    /// Compare CRD identity (group)
    fn compare_identity(old: &CrdSchema, new: &CrdSchema, changes: &mut Vec<CrdChange>) {
        if old.group != new.group {
            changes.push(CrdChange {
                kind: ChangeKind::ChangeGroup,
                path: "spec.group".to_string(),
                message: format!("API group changed from '{}' to '{}'", old.group, new.group),
                old_value: Some(old.group.clone()),
                new_value: Some(new.group.clone()),
            });
        }
    }

    /// Compare CRD scope
    fn compare_scope(old: &CrdSchema, new: &CrdSchema, changes: &mut Vec<CrdChange>) {
        if old.scope != new.scope {
            changes.push(CrdChange {
                kind: ChangeKind::ChangeScope,
                path: "spec.scope".to_string(),
                message: format!("Scope changed from {} to {}", old.scope, new.scope),
                old_value: Some(old.scope.to_string()),
                new_value: Some(new.scope.to_string()),
            });
        }
    }

    /// Compare CRD names
    fn compare_names(old: &CrdNames, new: &CrdNames, changes: &mut Vec<CrdChange>) {
        // Kind name change is dangerous
        if old.kind != new.kind {
            changes.push(CrdChange {
                kind: ChangeKind::ChangeKindName,
                path: "spec.names.kind".to_string(),
                message: format!("Kind name changed from '{}' to '{}'", old.kind, new.kind),
                old_value: Some(old.kind.clone()),
                new_value: Some(new.kind.clone()),
            });
        }

        // New short names (safe)
        let old_shorts: BTreeSet<_> = old.short_names.iter().collect();
        let new_shorts: BTreeSet<_> = new.short_names.iter().collect();

        for name in new_shorts.difference(&old_shorts) {
            changes.push(CrdChange {
                kind: ChangeKind::AddShortName,
                path: "spec.names.shortNames".to_string(),
                message: format!("Short name '{}' added", name),
                old_value: None,
                new_value: Some((*name).clone()),
            });
        }

        // New categories (safe)
        let old_cats: BTreeSet<_> = old.categories.iter().collect();
        let new_cats: BTreeSet<_> = new.categories.iter().collect();

        for cat in new_cats.difference(&old_cats) {
            changes.push(CrdChange {
                kind: ChangeKind::AddCategory,
                path: "spec.names.categories".to_string(),
                message: format!("Category '{}' added", cat),
                old_value: None,
                new_value: Some((*cat).clone()),
            });
        }
    }

    /// Compare API versions
    fn compare_versions(old: &CrdSchema, new: &CrdSchema, changes: &mut Vec<CrdChange>) {
        let old_version_names: BTreeSet<_> = old.versions.iter().map(|v| &v.name).collect();
        let new_version_names: BTreeSet<_> = new.versions.iter().map(|v| &v.name).collect();

        // Removed versions (dangerous)
        for name in old_version_names.difference(&new_version_names) {
            changes.push(CrdChange {
                kind: ChangeKind::RemoveVersion,
                path: format!("spec.versions[{}]", name),
                message: format!("API version '{}' removed", name),
                old_value: Some((*name).clone()),
                new_value: None,
            });
        }

        // Added versions (safe)
        for name in new_version_names.difference(&old_version_names) {
            changes.push(CrdChange {
                kind: ChangeKind::AddVersion,
                path: format!("spec.versions[{}]", name),
                message: format!("API version '{}' added", name),
                old_value: None,
                new_value: Some((*name).clone()),
            });
        }

        // Compare each version that exists in both
        for new_ver in &new.versions {
            if let Some(old_ver) = old.versions.iter().find(|v| v.name == new_ver.name) {
                Self::compare_version(old_ver, new_ver, changes);
            }
        }

        // Check for storage version change
        let old_storage = old.versions.iter().find(|v| v.storage).map(|v| &v.name);
        let new_storage = new.versions.iter().find(|v| v.storage).map(|v| &v.name);

        if old_storage != new_storage {
            changes.push(CrdChange {
                kind: ChangeKind::ChangeStorageVersion,
                path: "spec.versions[].storage".to_string(),
                message: format!(
                    "Storage version changed from '{}' to '{}'",
                    old_storage.map(String::as_str).unwrap_or("none"),
                    new_storage.map(String::as_str).unwrap_or("none")
                ),
                old_value: old_storage.cloned(),
                new_value: new_storage.cloned(),
            });
        }
    }

    /// Compare a single version
    fn compare_version(
        old: &CrdVersionSchema,
        new: &CrdVersionSchema,
        changes: &mut Vec<CrdChange>,
    ) {
        let path_prefix = format!("spec.versions[{}]", new.name);

        // Deprecated status
        if !old.deprecated && new.deprecated {
            changes.push(CrdChange {
                kind: ChangeKind::DeprecateVersion,
                path: format!("{}.deprecated", path_prefix),
                message: format!(
                    "Version '{}' is now deprecated{}",
                    new.name,
                    new.deprecation_warning
                        .as_ref()
                        .map(|w| format!(": {}", w))
                        .unwrap_or_default()
                ),
                old_value: Some("false".to_string()),
                new_value: Some("true".to_string()),
            });
        }

        // Compare printer columns
        Self::compare_printer_columns(
            &old.printer_columns,
            &new.printer_columns,
            &path_prefix,
            changes,
        );

        // Compare subresources
        Self::compare_subresources(&old.subresources, &new.subresources, &path_prefix, changes);

        // Compare schemas
        if let (Some(old_schema), Some(new_schema)) = (&old.schema, &new.schema) {
            Self::compare_schemas(
                old_schema,
                new_schema,
                &format!("{}.schema", path_prefix),
                changes,
            );
        }
    }

    /// Compare printer columns
    fn compare_printer_columns(
        old: &[PrinterColumn],
        new: &[PrinterColumn],
        path_prefix: &str,
        changes: &mut Vec<CrdChange>,
    ) {
        let old_names: BTreeSet<_> = old.iter().map(|c| &c.name).collect();
        let new_names: BTreeSet<_> = new.iter().map(|c| &c.name).collect();

        for name in new_names.difference(&old_names) {
            changes.push(CrdChange {
                kind: ChangeKind::AddPrinterColumn,
                path: format!("{}.additionalPrinterColumns", path_prefix),
                message: format!("Printer column '{}' added", name),
                old_value: None,
                new_value: Some((*name).clone()),
            });
        }
    }

    /// Compare subresources
    fn compare_subresources(
        old: &Subresources,
        new: &Subresources,
        path_prefix: &str,
        changes: &mut Vec<CrdChange>,
    ) {
        // Status subresource
        if !old.status && new.status {
            changes.push(CrdChange {
                kind: ChangeKind::AddSubresource,
                path: format!("{}.subresources.status", path_prefix),
                message: "Status subresource enabled".to_string(),
                old_value: None,
                new_value: Some("enabled".to_string()),
            });
        } else if old.status && !new.status {
            changes.push(CrdChange {
                kind: ChangeKind::RemoveSubresource,
                path: format!("{}.subresources.status", path_prefix),
                message: "Status subresource disabled".to_string(),
                old_value: Some("enabled".to_string()),
                new_value: None,
            });
        }

        // Scale subresource
        match (&old.scale, &new.scale) {
            (None, Some(_)) => {
                changes.push(CrdChange {
                    kind: ChangeKind::AddSubresource,
                    path: format!("{}.subresources.scale", path_prefix),
                    message: "Scale subresource enabled".to_string(),
                    old_value: None,
                    new_value: Some("enabled".to_string()),
                });
            }
            (Some(_), None) => {
                changes.push(CrdChange {
                    kind: ChangeKind::RemoveSubresource,
                    path: format!("{}.subresources.scale", path_prefix),
                    message: "Scale subresource disabled".to_string(),
                    old_value: Some("enabled".to_string()),
                    new_value: None,
                });
            }
            _ => {}
        }
    }

    /// Compare OpenAPI schemas
    fn compare_schemas(
        old: &OpenApiSchema,
        new: &OpenApiSchema,
        path_prefix: &str,
        changes: &mut Vec<CrdChange>,
    ) {
        Self::compare_properties(
            &old.properties,
            &new.properties,
            &old.required,
            &new.required,
            path_prefix,
            changes,
        );
    }

    /// Compare property maps (recursive)
    fn compare_properties(
        old_props: &std::collections::BTreeMap<String, SchemaProperty>,
        new_props: &std::collections::BTreeMap<String, SchemaProperty>,
        old_required: &[String],
        new_required: &[String],
        path_prefix: &str,
        changes: &mut Vec<CrdChange>,
    ) {
        let old_names: BTreeSet<_> = old_props.keys().collect();
        let new_names: BTreeSet<_> = new_props.keys().collect();

        // Removed fields
        for name in old_names.difference(&new_names) {
            let was_required = old_required.contains(*name);
            let kind = if was_required {
                ChangeKind::RemoveRequiredField
            } else {
                ChangeKind::RemoveField
            };

            changes.push(CrdChange {
                kind,
                path: format!("{}.properties.{}", path_prefix, name),
                message: format!(
                    "Field '{}' removed{}",
                    name,
                    if was_required { " (was required)" } else { "" }
                ),
                old_value: Some((*name).to_string()),
                new_value: None,
            });
        }

        // Added fields
        for name in new_names.difference(&old_names) {
            let is_required = new_required.contains(*name);
            let kind = if is_required {
                ChangeKind::AddRequiredField
            } else {
                ChangeKind::AddOptionalField
            };

            let prop = &new_props[*name];
            changes.push(CrdChange {
                kind,
                path: format!("{}.properties.{}", path_prefix, name),
                message: format!(
                    "Field '{}' added ({}, {})",
                    name,
                    prop.type_,
                    if is_required { "required" } else { "optional" }
                ),
                old_value: None,
                new_value: Some(format!("{}", prop.type_)),
            });
        }

        // Modified fields
        for name in old_names.intersection(&new_names) {
            let old_prop = &old_props[*name];
            let new_prop = &new_props[*name];
            let prop_path = format!("{}.properties.{}", path_prefix, name);

            Self::compare_property(old_prop, new_prop, name, &prop_path, changes);
        }

        // Required status changes (for existing fields)
        for name in new_required {
            if old_props.contains_key(name) && !old_required.contains(name) {
                changes.push(CrdChange {
                    kind: ChangeKind::MakeRequired,
                    path: format!("{}.required", path_prefix),
                    message: format!("Field '{}' is now required", name),
                    old_value: Some("optional".to_string()),
                    new_value: Some("required".to_string()),
                });
            }
        }
    }

    /// Compare a single property
    fn compare_property(
        old: &SchemaProperty,
        new: &SchemaProperty,
        name: &str,
        path: &str,
        changes: &mut Vec<CrdChange>,
    ) {
        // Type change (dangerous)
        if old.type_ != new.type_ && !old.type_.is_compatible_with(&new.type_) {
            changes.push(CrdChange {
                kind: ChangeKind::ChangeFieldType,
                path: format!("{}.type", path),
                message: format!(
                    "Field '{}' type changed from {} to {}",
                    name, old.type_, new.type_
                ),
                old_value: Some(format!("{}", old.type_)),
                new_value: Some(format!("{}", new.type_)),
            });
        }

        // Default value changes
        match (&old.default, &new.default) {
            (None, Some(v)) => {
                changes.push(CrdChange {
                    kind: ChangeKind::AddDefault,
                    path: format!("{}.default", path),
                    message: format!("Field '{}' default value added: {}", name, v),
                    old_value: None,
                    new_value: Some(v.to_string()),
                });
            }
            (Some(old_v), Some(new_v)) if old_v != new_v => {
                changes.push(CrdChange {
                    kind: ChangeKind::ChangeDefault,
                    path: format!("{}.default", path),
                    message: format!("Field '{}' default changed", name),
                    old_value: Some(old_v.to_string()),
                    new_value: Some(new_v.to_string()),
                });
            }
            _ => {}
        }

        // Description update (safe)
        if old.description != new.description && new.description.is_some() {
            changes.push(CrdChange {
                kind: ChangeKind::UpdateDescription,
                path: format!("{}.description", path),
                message: format!("Field '{}' description updated", name),
                old_value: old.description.clone(),
                new_value: new.description.clone(),
            });
        }

        // Validation changes
        Self::compare_validation(old, new, name, path, changes);

        // Enum changes
        Self::compare_enums(old, new, name, path, changes);

        // Recurse into nested properties
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

        // Array items
        if let (Some(old_items), Some(new_items)) = (&old.items, &new.items) {
            Self::compare_property(
                old_items,
                new_items,
                &format!("{}[]", name),
                &format!("{}.items", path),
                changes,
            );
        }
    }

    /// Compare validation constraints
    fn compare_validation(
        old: &SchemaProperty,
        new: &SchemaProperty,
        name: &str,
        path: &str,
        changes: &mut Vec<CrdChange>,
    ) {
        // Pattern
        match (&old.pattern, &new.pattern) {
            (None, Some(p)) => {
                changes.push(CrdChange {
                    kind: ChangeKind::TightenValidation,
                    path: format!("{}.pattern", path),
                    message: format!("Field '{}' pattern constraint added: {}", name, p),
                    old_value: None,
                    new_value: Some(p.clone()),
                });
            }
            (Some(p), None) => {
                changes.push(CrdChange {
                    kind: ChangeKind::RelaxValidation,
                    path: format!("{}.pattern", path),
                    message: format!("Field '{}' pattern constraint removed", name),
                    old_value: Some(p.clone()),
                    new_value: None,
                });
            }
            (Some(old_p), Some(new_p)) if old_p != new_p => {
                changes.push(CrdChange {
                    kind: ChangeKind::TightenValidation,
                    path: format!("{}.pattern", path),
                    message: format!("Field '{}' pattern changed", name),
                    old_value: Some(old_p.clone()),
                    new_value: Some(new_p.clone()),
                });
            }
            _ => {}
        }

        // maxLength
        match (old.max_length, new.max_length) {
            (None, Some(v)) => {
                changes.push(CrdChange {
                    kind: ChangeKind::TightenValidation,
                    path: format!("{}.maxLength", path),
                    message: format!("Field '{}' maxLength constraint added ({})", name, v),
                    old_value: None,
                    new_value: Some(v.to_string()),
                });
            }
            (Some(v), None) => {
                changes.push(CrdChange {
                    kind: ChangeKind::RelaxValidation,
                    path: format!("{}.maxLength", path),
                    message: format!("Field '{}' maxLength constraint removed (was {})", name, v),
                    old_value: Some(v.to_string()),
                    new_value: None,
                });
            }
            (Some(old_v), Some(new_v)) if old_v != new_v => {
                let kind = if new_v > old_v {
                    ChangeKind::RelaxValidation
                } else {
                    ChangeKind::TightenValidation
                };
                changes.push(CrdChange {
                    kind,
                    path: format!("{}.maxLength", path),
                    message: format!(
                        "Field '{}' maxLength {} ({} → {})",
                        name,
                        if new_v > old_v {
                            "increased"
                        } else {
                            "decreased"
                        },
                        old_v,
                        new_v
                    ),
                    old_value: Some(old_v.to_string()),
                    new_value: Some(new_v.to_string()),
                });
            }
            _ => {}
        }

        // minLength
        match (old.min_length, new.min_length) {
            (None, Some(v)) if v > 0 => {
                changes.push(CrdChange {
                    kind: ChangeKind::TightenValidation,
                    path: format!("{}.minLength", path),
                    message: format!("Field '{}' minLength constraint added ({})", name, v),
                    old_value: None,
                    new_value: Some(v.to_string()),
                });
            }
            (Some(v), None) => {
                changes.push(CrdChange {
                    kind: ChangeKind::RelaxValidation,
                    path: format!("{}.minLength", path),
                    message: format!("Field '{}' minLength constraint removed (was {})", name, v),
                    old_value: Some(v.to_string()),
                    new_value: None,
                });
            }
            (Some(old_v), Some(new_v)) if old_v != new_v => {
                let kind = if new_v < old_v {
                    ChangeKind::RelaxValidation
                } else {
                    ChangeKind::TightenValidation
                };
                changes.push(CrdChange {
                    kind,
                    path: format!("{}.minLength", path),
                    message: format!(
                        "Field '{}' minLength {} ({} → {})",
                        name,
                        if new_v < old_v {
                            "decreased"
                        } else {
                            "increased"
                        },
                        old_v,
                        new_v
                    ),
                    old_value: Some(old_v.to_string()),
                    new_value: Some(new_v.to_string()),
                });
            }
            _ => {}
        }

        // minimum/maximum for numbers (similar logic)
        Self::compare_numeric_constraints(old, new, name, path, changes);
    }

    /// Compare numeric constraints
    fn compare_numeric_constraints(
        old: &SchemaProperty,
        new: &SchemaProperty,
        name: &str,
        path: &str,
        changes: &mut Vec<CrdChange>,
    ) {
        // minimum
        match (old.minimum, new.minimum) {
            (None, Some(v)) => {
                changes.push(CrdChange {
                    kind: ChangeKind::TightenValidation,
                    path: format!("{}.minimum", path),
                    message: format!("Field '{}' minimum constraint added ({})", name, v),
                    old_value: None,
                    new_value: Some(v.to_string()),
                });
            }
            (Some(v), None) => {
                changes.push(CrdChange {
                    kind: ChangeKind::RelaxValidation,
                    path: format!("{}.minimum", path),
                    message: format!("Field '{}' minimum constraint removed (was {})", name, v),
                    old_value: Some(v.to_string()),
                    new_value: None,
                });
            }
            (Some(old_v), Some(new_v)) if (old_v - new_v).abs() > f64::EPSILON => {
                let kind = if new_v < old_v {
                    ChangeKind::RelaxValidation
                } else {
                    ChangeKind::TightenValidation
                };
                changes.push(CrdChange {
                    kind,
                    path: format!("{}.minimum", path),
                    message: format!("Field '{}' minimum changed ({} → {})", name, old_v, new_v),
                    old_value: Some(old_v.to_string()),
                    new_value: Some(new_v.to_string()),
                });
            }
            _ => {}
        }

        // maximum
        match (old.maximum, new.maximum) {
            (None, Some(v)) => {
                changes.push(CrdChange {
                    kind: ChangeKind::TightenValidation,
                    path: format!("{}.maximum", path),
                    message: format!("Field '{}' maximum constraint added ({})", name, v),
                    old_value: None,
                    new_value: Some(v.to_string()),
                });
            }
            (Some(v), None) => {
                changes.push(CrdChange {
                    kind: ChangeKind::RelaxValidation,
                    path: format!("{}.maximum", path),
                    message: format!("Field '{}' maximum constraint removed (was {})", name, v),
                    old_value: Some(v.to_string()),
                    new_value: None,
                });
            }
            (Some(old_v), Some(new_v)) if (old_v - new_v).abs() > f64::EPSILON => {
                let kind = if new_v > old_v {
                    ChangeKind::RelaxValidation
                } else {
                    ChangeKind::TightenValidation
                };
                changes.push(CrdChange {
                    kind,
                    path: format!("{}.maximum", path),
                    message: format!("Field '{}' maximum changed ({} → {})", name, old_v, new_v),
                    old_value: Some(old_v.to_string()),
                    new_value: Some(new_v.to_string()),
                });
            }
            _ => {}
        }
    }

    /// Compare enum values
    fn compare_enums(
        old: &SchemaProperty,
        new: &SchemaProperty,
        name: &str,
        path: &str,
        changes: &mut Vec<CrdChange>,
    ) {
        match (&old.enum_values, &new.enum_values) {
            (Some(old_enums), Some(new_enums)) => {
                let old_set: BTreeSet<_> = old_enums.iter().map(|v| v.to_string()).collect();
                let new_set: BTreeSet<_> = new_enums.iter().map(|v| v.to_string()).collect();

                // Removed values (dangerous)
                for val in old_set.difference(&new_set) {
                    changes.push(CrdChange {
                        kind: ChangeKind::RemoveEnumValue,
                        path: format!("{}.enum", path),
                        message: format!("Field '{}' enum value '{}' removed", name, val),
                        old_value: Some(val.clone()),
                        new_value: None,
                    });
                }

                // Added values (warning - could be safe but worth noting)
                if !new_set.difference(&old_set).count() == 0 {
                    let added: Vec<_> = new_set.difference(&old_set).collect();
                    changes.push(CrdChange {
                        kind: ChangeKind::ChangeEnumValues,
                        path: format!("{}.enum", path),
                        message: format!(
                            "Field '{}' enum values added: {}",
                            name,
                            added
                                .iter()
                                .map(|s| s.as_str())
                                .collect::<Vec<_>>()
                                .join(", ")
                        ),
                        old_value: None,
                        new_value: Some(
                            added
                                .iter()
                                .map(|s| s.as_str())
                                .collect::<Vec<_>>()
                                .join(", "),
                        ),
                    });
                }
            }
            (None, Some(enums)) => {
                changes.push(CrdChange {
                    kind: ChangeKind::TightenValidation,
                    path: format!("{}.enum", path),
                    message: format!("Field '{}' enum constraint added ({})", name, enums.len()),
                    old_value: None,
                    new_value: Some(format!("{} values", enums.len())),
                });
            }
            (Some(enums), None) => {
                changes.push(CrdChange {
                    kind: ChangeKind::RelaxValidation,
                    path: format!("{}.enum", path),
                    message: format!("Field '{}' enum constraint removed", name),
                    old_value: Some(format!("{} values", enums.len())),
                    new_value: None,
                });
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crd::parser::CrdParser;

    fn make_crd(yaml: &str) -> CrdSchema {
        CrdParser::parse(yaml).unwrap()
    }

    #[test]
    fn test_analyze_new_crd() {
        let new = make_crd(
            r#"
apiVersion: apiextensions.k8s.io/v1
kind: CustomResourceDefinition
metadata:
  name: tests.example.com
spec:
  group: example.com
  scope: Namespaced
  names:
    kind: Test
    plural: tests
  versions:
    - name: v1
      served: true
      storage: true
      schema:
        openAPIV3Schema:
          type: object
"#,
        );

        let analysis = CrdAnalyzer::analyze(None, &new);
        assert!(analysis.is_new);
        assert!(analysis.changes.is_empty());
    }

    #[test]
    fn test_detect_added_version() {
        let old = make_crd(
            r#"
apiVersion: apiextensions.k8s.io/v1
kind: CustomResourceDefinition
metadata:
  name: tests.example.com
spec:
  group: example.com
  scope: Namespaced
  names:
    kind: Test
    plural: tests
  versions:
    - name: v1beta1
      served: true
      storage: true
      schema:
        openAPIV3Schema:
          type: object
"#,
        );

        let new = make_crd(
            r#"
apiVersion: apiextensions.k8s.io/v1
kind: CustomResourceDefinition
metadata:
  name: tests.example.com
spec:
  group: example.com
  scope: Namespaced
  names:
    kind: Test
    plural: tests
  versions:
    - name: v1
      served: true
      storage: true
      schema:
        openAPIV3Schema:
          type: object
    - name: v1beta1
      served: true
      storage: false
      schema:
        openAPIV3Schema:
          type: object
"#,
        );

        let analysis = CrdAnalyzer::analyze(Some(&old), &new);
        assert!(!analysis.is_new);

        let add_version = analysis
            .changes
            .iter()
            .find(|c| c.kind == ChangeKind::AddVersion);
        assert!(add_version.is_some());
        assert_eq!(add_version.unwrap().new_value, Some("v1".to_string()));
    }

    #[test]
    fn test_detect_removed_version() {
        let old = make_crd(
            r#"
apiVersion: apiextensions.k8s.io/v1
kind: CustomResourceDefinition
metadata:
  name: tests.example.com
spec:
  group: example.com
  scope: Namespaced
  names:
    kind: Test
    plural: tests
  versions:
    - name: v1
      served: true
      storage: true
      schema:
        openAPIV3Schema:
          type: object
    - name: v1beta1
      served: true
      storage: false
      schema:
        openAPIV3Schema:
          type: object
"#,
        );

        let new = make_crd(
            r#"
apiVersion: apiextensions.k8s.io/v1
kind: CustomResourceDefinition
metadata:
  name: tests.example.com
spec:
  group: example.com
  scope: Namespaced
  names:
    kind: Test
    plural: tests
  versions:
    - name: v1
      served: true
      storage: true
      schema:
        openAPIV3Schema:
          type: object
"#,
        );

        let analysis = CrdAnalyzer::analyze(Some(&old), &new);
        assert!(analysis.has_dangerous_changes());

        let remove_version = analysis
            .changes
            .iter()
            .find(|c| c.kind == ChangeKind::RemoveVersion);
        assert!(remove_version.is_some());
        assert_eq!(
            remove_version.unwrap().severity(),
            ChangeSeverity::Dangerous
        );
    }

    #[test]
    fn test_detect_scope_change() {
        let old = make_crd(
            r#"
apiVersion: apiextensions.k8s.io/v1
kind: CustomResourceDefinition
metadata:
  name: tests.example.com
spec:
  group: example.com
  scope: Namespaced
  names:
    kind: Test
    plural: tests
  versions:
    - name: v1
      served: true
      storage: true
      schema:
        openAPIV3Schema:
          type: object
"#,
        );

        let new = make_crd(
            r#"
apiVersion: apiextensions.k8s.io/v1
kind: CustomResourceDefinition
metadata:
  name: tests.example.com
spec:
  group: example.com
  scope: Cluster
  names:
    kind: Test
    plural: tests
  versions:
    - name: v1
      served: true
      storage: true
      schema:
        openAPIV3Schema:
          type: object
"#,
        );

        let analysis = CrdAnalyzer::analyze(Some(&old), &new);
        assert!(analysis.has_dangerous_changes());

        let scope_change = analysis
            .changes
            .iter()
            .find(|c| c.kind == ChangeKind::ChangeScope);
        assert!(scope_change.is_some());
    }

    #[test]
    fn test_detect_added_field() {
        let old = make_crd(
            r#"
apiVersion: apiextensions.k8s.io/v1
kind: CustomResourceDefinition
metadata:
  name: tests.example.com
spec:
  group: example.com
  scope: Namespaced
  names:
    kind: Test
    plural: tests
  versions:
    - name: v1
      served: true
      storage: true
      schema:
        openAPIV3Schema:
          type: object
          properties:
            spec:
              type: object
              properties:
                name:
                  type: string
"#,
        );

        let new = make_crd(
            r#"
apiVersion: apiextensions.k8s.io/v1
kind: CustomResourceDefinition
metadata:
  name: tests.example.com
spec:
  group: example.com
  scope: Namespaced
  names:
    kind: Test
    plural: tests
  versions:
    - name: v1
      served: true
      storage: true
      schema:
        openAPIV3Schema:
          type: object
          properties:
            spec:
              type: object
              properties:
                name:
                  type: string
                replicas:
                  type: integer
"#,
        );

        let analysis = CrdAnalyzer::analyze(Some(&old), &new);

        let add_field = analysis
            .changes
            .iter()
            .find(|c| c.kind == ChangeKind::AddOptionalField);
        assert!(add_field.is_some());
        assert_eq!(add_field.unwrap().severity(), ChangeSeverity::Safe);
    }

    #[test]
    fn test_detect_field_type_change() {
        let old = make_crd(
            r#"
apiVersion: apiextensions.k8s.io/v1
kind: CustomResourceDefinition
metadata:
  name: tests.example.com
spec:
  group: example.com
  scope: Namespaced
  names:
    kind: Test
    plural: tests
  versions:
    - name: v1
      served: true
      storage: true
      schema:
        openAPIV3Schema:
          type: object
          properties:
            spec:
              type: object
              properties:
                replicas:
                  type: string
"#,
        );

        let new = make_crd(
            r#"
apiVersion: apiextensions.k8s.io/v1
kind: CustomResourceDefinition
metadata:
  name: tests.example.com
spec:
  group: example.com
  scope: Namespaced
  names:
    kind: Test
    plural: tests
  versions:
    - name: v1
      served: true
      storage: true
      schema:
        openAPIV3Schema:
          type: object
          properties:
            spec:
              type: object
              properties:
                replicas:
                  type: integer
"#,
        );

        let analysis = CrdAnalyzer::analyze(Some(&old), &new);
        assert!(analysis.has_dangerous_changes());

        let type_change = analysis
            .changes
            .iter()
            .find(|c| c.kind == ChangeKind::ChangeFieldType);
        assert!(type_change.is_some());
        assert_eq!(type_change.unwrap().old_value, Some("string".to_string()));
        assert_eq!(type_change.unwrap().new_value, Some("integer".to_string()));
    }

    #[test]
    fn test_detect_validation_changes() {
        let old = make_crd(
            r#"
apiVersion: apiextensions.k8s.io/v1
kind: CustomResourceDefinition
metadata:
  name: tests.example.com
spec:
  group: example.com
  scope: Namespaced
  names:
    kind: Test
    plural: tests
  versions:
    - name: v1
      served: true
      storage: true
      schema:
        openAPIV3Schema:
          type: object
          properties:
            spec:
              type: object
              properties:
                name:
                  type: string
                  maxLength: 253
"#,
        );

        let new = make_crd(
            r#"
apiVersion: apiextensions.k8s.io/v1
kind: CustomResourceDefinition
metadata:
  name: tests.example.com
spec:
  group: example.com
  scope: Namespaced
  names:
    kind: Test
    plural: tests
  versions:
    - name: v1
      served: true
      storage: true
      schema:
        openAPIV3Schema:
          type: object
          properties:
            spec:
              type: object
              properties:
                name:
                  type: string
                  maxLength: 63
"#,
        );

        let analysis = CrdAnalyzer::analyze(Some(&old), &new);

        let tighten = analysis
            .changes
            .iter()
            .find(|c| c.kind == ChangeKind::TightenValidation);
        assert!(tighten.is_some());
        assert_eq!(tighten.unwrap().severity(), ChangeSeverity::Warning);
    }

    #[test]
    fn test_count_by_severity() {
        let old = make_crd(
            r#"
apiVersion: apiextensions.k8s.io/v1
kind: CustomResourceDefinition
metadata:
  name: tests.example.com
spec:
  group: example.com
  scope: Namespaced
  names:
    kind: Test
    plural: tests
  versions:
    - name: v1
      served: true
      storage: true
      schema:
        openAPIV3Schema:
          type: object
          properties:
            spec:
              type: object
              properties:
                name:
                  type: string
"#,
        );

        let new = make_crd(
            r#"
apiVersion: apiextensions.k8s.io/v1
kind: CustomResourceDefinition
metadata:
  name: tests.example.com
spec:
  group: example.com
  scope: Cluster
  names:
    kind: Test
    plural: tests
    shortNames:
      - t
  versions:
    - name: v1
      served: true
      storage: true
      schema:
        openAPIV3Schema:
          type: object
          properties:
            spec:
              type: object
              properties:
                name:
                  type: string
                newField:
                  type: string
"#,
        );

        let analysis = CrdAnalyzer::analyze(Some(&old), &new);
        let (safe, _warn, danger) = analysis.count_by_severity();

        assert!(safe >= 2); // AddShortName + AddOptionalField
        assert!(danger >= 1); // ChangeScope
    }
}
