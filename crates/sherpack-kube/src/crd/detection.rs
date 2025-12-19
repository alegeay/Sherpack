//! CRD detection in templates and templating detection in crds/
//!
//! This module provides detection capabilities for:
//! - Finding CRDs in rendered templates
//! - Detecting Jinja templating syntax in crds/ files
//! - Analyzing CRD locations for lint warnings
//!
//! # Design Philosophy
//!
//! Unlike Helm which forces a choice between templating (templates/) and
//! protection (crds/), Sherpack allows both. This module enables:
//!
//! 1. **Templated CRDs in crds/**: Files with `{{` or `{%` are rendered
//! 2. **Protected CRDs in templates/**: Auto-detected and protected
//! 3. **Smart lint warnings**: Suggest optimal placement

use super::policy::{CrdLocation, CrdPolicy, DetectedCrd};

/// Check if content contains Jinja templating syntax
///
/// Returns true if the content contains `{{`, `{%`, or `{#` markers.
pub fn contains_jinja_syntax(content: &str) -> bool {
    content.contains("{{") || content.contains("{%") || content.contains("{#")
}

/// Detect CRDs in rendered manifest content
///
/// Parses multi-document YAML and identifies CustomResourceDefinitions.
/// Accepts any iterable of (path, content) pairs (HashMap, IndexMap, etc.)
pub fn detect_crds_in_manifests<'a, I>(manifests: I) -> Vec<DetectedCrd>
where
    I: IntoIterator<Item = (&'a String, &'a String)>,
{
    let mut crds = Vec::new();

    for (template_path, content) in manifests {
        // Split multi-document YAML
        for doc in content.split("---") {
            let doc = doc.trim();
            if doc.is_empty() || doc.lines().all(|l| l.trim().is_empty() || l.trim().starts_with('#')) {
                continue;
            }

            // Try to parse as YAML
            let Ok(parsed): Result<serde_yaml::Value, _> = serde_yaml::from_str(doc) else {
                continue;
            };

            // Check if it's a CRD
            let kind = parsed.get("kind").and_then(|k| k.as_str());
            if kind != Some("CustomResourceDefinition") {
                continue;
            }

            // Extract name
            let name = parsed
                .get("metadata")
                .and_then(|m| m.get("name"))
                .and_then(|n| n.as_str())
                .unwrap_or("unknown")
                .to_string();

            let location = CrdLocation::templates(template_path.clone());
            crds.push(DetectedCrd::new(name, doc, location));
        }
    }

    crds
}

/// Result of scanning a crds/ directory
#[derive(Debug, Default)]
pub struct CrdsScanResult {
    /// Static CRD files (no templating)
    pub static_crds: Vec<DetectedCrd>,
    /// Templated CRD files (contain Jinja syntax)
    pub templated_crds: Vec<TemplatedCrdFile>,
    /// Files that are not CRDs (error)
    pub non_crd_files: Vec<NonCrdFile>,
}

/// A CRD file that needs templating
#[derive(Debug, Clone)]
pub struct TemplatedCrdFile {
    /// Relative path in crds/
    pub path: String,
    /// Raw content (with Jinja syntax)
    pub content: String,
    /// Detected Jinja constructs
    pub jinja_constructs: Vec<JinjaConstruct>,
}

/// A file in crds/ that isn't a CRD
#[derive(Debug, Clone)]
pub struct NonCrdFile {
    /// Relative path
    pub path: String,
    /// Detected kind
    pub kind: String,
}

/// Types of Jinja constructs found
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JinjaConstruct {
    /// Variable expression: `{{ ... }}`
    Variable { line: usize },
    /// Control flow: `{% ... %}`
    Control { line: usize },
    /// Comment: `{# ... #}`
    Comment { line: usize },
}

impl TemplatedCrdFile {
    /// Analyze content for Jinja constructs
    pub fn analyze(path: impl Into<String>, content: impl Into<String>) -> Self {
        let content = content.into();
        let mut constructs = Vec::new();

        for (line_num, line) in content.lines().enumerate() {
            let line_num = line_num + 1; // 1-indexed

            if line.contains("{{") {
                constructs.push(JinjaConstruct::Variable { line: line_num });
            }
            if line.contains("{%") {
                constructs.push(JinjaConstruct::Control { line: line_num });
            }
            if line.contains("{#") {
                constructs.push(JinjaConstruct::Comment { line: line_num });
            }
        }

        Self {
            path: path.into(),
            content,
            jinja_constructs: constructs,
        }
    }

    /// Check if this file has variable expressions
    pub fn has_variables(&self) -> bool {
        self.jinja_constructs
            .iter()
            .any(|c| matches!(c, JinjaConstruct::Variable { .. }))
    }

    /// Check if this file has control flow
    pub fn has_control_flow(&self) -> bool {
        self.jinja_constructs
            .iter()
            .any(|c| matches!(c, JinjaConstruct::Control { .. }))
    }
}

/// Lint warning for CRD placement
#[derive(Debug, Clone)]
pub struct CrdLintWarning {
    /// Warning code
    pub code: CrdLintCode,
    /// Affected file path
    pub path: String,
    /// CRD name (if known)
    pub crd_name: Option<String>,
    /// Human-readable message
    pub message: String,
    /// Suggested action
    pub suggestion: Option<String>,
}

/// CRD lint warning codes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CrdLintCode {
    /// CRD found in templates/ instead of crds/
    CrdInTemplates,
    /// Templated CRD in crds/ (informational)
    TemplatedCrdInCrdsDir,
    /// Non-CRD file in crds/ directory
    NonCrdInCrdsDir,
    /// CRD without policy annotation
    NoPolicyAnnotation,
    /// Shared CRD in templates/ (risky)
    SharedCrdInTemplates,
    /// External policy but CRD is defined in pack
    ExternalPolicyInPack,
}

impl CrdLintWarning {
    /// Create a new lint warning
    pub fn new(code: CrdLintCode, path: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code,
            path: path.into(),
            crd_name: None,
            message: message.into(),
            suggestion: None,
        }
    }

    /// Add CRD name
    pub fn with_crd_name(mut self, name: impl Into<String>) -> Self {
        self.crd_name = Some(name.into());
        self
    }

    /// Add suggestion
    pub fn with_suggestion(mut self, suggestion: impl Into<String>) -> Self {
        self.suggestion = Some(suggestion.into());
        self
    }

    /// Get severity (for display)
    pub fn severity(&self) -> LintSeverity {
        match self.code {
            CrdLintCode::CrdInTemplates => LintSeverity::Info,
            CrdLintCode::TemplatedCrdInCrdsDir => LintSeverity::Info,
            CrdLintCode::NonCrdInCrdsDir => LintSeverity::Error,
            CrdLintCode::NoPolicyAnnotation => LintSeverity::Info,
            CrdLintCode::SharedCrdInTemplates => LintSeverity::Warning,
            CrdLintCode::ExternalPolicyInPack => LintSeverity::Warning,
        }
    }
}

/// Lint severity levels
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LintSeverity {
    /// Informational note
    Info,
    /// Warning (may cause issues)
    Warning,
    /// Error (will cause issues)
    Error,
}

/// Generate lint warnings for detected CRDs
pub fn lint_crds(
    crds_dir_crds: &[DetectedCrd],
    templates_crds: &[DetectedCrd],
    templated_files: &[TemplatedCrdFile],
) -> Vec<CrdLintWarning> {
    let mut warnings = Vec::new();

    // Check CRDs in templates/
    for crd in templates_crds {
        let warning = CrdLintWarning::new(
            CrdLintCode::CrdInTemplates,
            crd.location.path().display().to_string(),
            "CRD detected in templates/ directory",
        )
        .with_crd_name(&crd.name)
        .with_suggestion(
            "Consider moving to crds/ for clearer organization. \
             Protection is automatic regardless of location.",
        );
        warnings.push(warning);

        // Check for shared policy in templates (risky)
        if crd.policy == CrdPolicy::Shared {
            warnings.push(
                CrdLintWarning::new(
                    CrdLintCode::SharedCrdInTemplates,
                    crd.location.path().display().to_string(),
                    "Shared CRD in templates/ may cause confusion",
                )
                .with_crd_name(&crd.name)
                .with_suggestion(
                    "Shared CRDs are typically managed in crds/ or externally. \
                     Consider using 'external' policy if CRD is managed by GitOps.",
                ),
            );
        }
    }

    // Check templated CRDs in crds/
    for templated in templated_files {
        let mut warning = CrdLintWarning::new(
            CrdLintCode::TemplatedCrdInCrdsDir,
            &templated.path,
            "Templated CRD in crds/ directory",
        );

        if templated.has_control_flow() {
            warning.suggestion = Some(
                "File uses control flow ({% ... %}). Ensure conditionals \
                 don't accidentally exclude required CRDs."
                    .to_string(),
            );
        } else {
            warning.suggestion = Some(
                "File uses templating. Will be rendered before installation.".to_string(),
            );
        }

        warnings.push(warning);
    }

    // Check for external policy on pack-defined CRDs
    for crd in crds_dir_crds.iter().chain(templates_crds.iter()) {
        if crd.policy == CrdPolicy::External {
            warnings.push(
                CrdLintWarning::new(
                    CrdLintCode::ExternalPolicyInPack,
                    crd.location.path().display().to_string(),
                    "CRD has 'external' policy but is defined in pack",
                )
                .with_crd_name(&crd.name)
                .with_suggestion(
                    "External policy means Sherpack won't manage this CRD. \
                     Remove from pack if managed externally, or use 'managed' or 'shared'.",
                ),
            );
        }
    }

    warnings
}

/// Check if a manifest is a CRD
pub fn is_crd_manifest(content: &str) -> bool {
    // Quick check before parsing
    if !content.contains("CustomResourceDefinition") {
        return false;
    }

    // Parse and verify
    let Ok(parsed): Result<serde_yaml::Value, _> = serde_yaml::from_str(content) else {
        return false;
    };

    parsed.get("kind").and_then(|k| k.as_str()) == Some("CustomResourceDefinition")
}

/// Extract CRD name from manifest content
pub fn extract_crd_name(content: &str) -> Option<String> {
    let parsed: serde_yaml::Value = serde_yaml::from_str(content).ok()?;

    parsed
        .get("metadata")
        .and_then(|m| m.get("name"))
        .and_then(|n| n.as_str())
        .map(|s| s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_contains_jinja_syntax() {
        assert!(contains_jinja_syntax("{{ values.name }}"));
        assert!(contains_jinja_syntax("{% if values.enabled %}"));
        assert!(contains_jinja_syntax("{# comment #}"));
        assert!(!contains_jinja_syntax("plain: yaml"));
    }

    #[test]
    fn test_detect_crds_in_manifests() {
        let mut manifests = HashMap::new();
        manifests.insert(
            "deployment.yaml".to_string(),
            r#"
apiVersion: apps/v1
kind: Deployment
metadata:
  name: test
"#
            .to_string(),
        );
        manifests.insert(
            "crd.yaml".to_string(),
            r#"
apiVersion: apiextensions.k8s.io/v1
kind: CustomResourceDefinition
metadata:
  name: tests.example.com
spec:
  group: example.com
"#
            .to_string(),
        );

        let crds = detect_crds_in_manifests(&manifests);

        assert_eq!(crds.len(), 1);
        assert_eq!(crds[0].name, "tests.example.com");
        assert!(matches!(crds[0].location, CrdLocation::Templates { .. }));
    }

    #[test]
    fn test_detect_multiple_crds_in_single_file() {
        let mut manifests = HashMap::new();
        manifests.insert(
            "crds.yaml".to_string(),
            r#"
apiVersion: apiextensions.k8s.io/v1
kind: CustomResourceDefinition
metadata:
  name: first.example.com
---
apiVersion: apiextensions.k8s.io/v1
kind: CustomResourceDefinition
metadata:
  name: second.example.com
"#
            .to_string(),
        );

        let crds = detect_crds_in_manifests(&manifests);

        assert_eq!(crds.len(), 2);
        let names: Vec<_> = crds.iter().map(|c| c.name.as_str()).collect();
        assert!(names.contains(&"first.example.com"));
        assert!(names.contains(&"second.example.com"));
    }

    #[test]
    fn test_templated_crd_file_analysis() {
        let content = r#"
apiVersion: apiextensions.k8s.io/v1
kind: CustomResourceDefinition
metadata:
  name: {{ values.crdName }}.{{ values.group }}
  labels:
    {% for key, val in values.labels %}
    {{ key }}: {{ val }}
    {% endfor %}
"#;

        let templated = TemplatedCrdFile::analyze("mycrd.yaml", content);

        assert!(templated.has_variables());
        assert!(templated.has_control_flow());
        assert!(!templated.jinja_constructs.is_empty());
    }

    #[test]
    fn test_lint_crd_in_templates() {
        let crd = DetectedCrd::new(
            "tests.example.com",
            "kind: CustomResourceDefinition",
            CrdLocation::templates("crd.yaml"),
        );

        let warnings = lint_crds(&[], &[crd], &[]);

        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].code, CrdLintCode::CrdInTemplates);
    }

    #[test]
    fn test_lint_templated_crd_in_crds_dir() {
        let templated = TemplatedCrdFile::analyze(
            "mycrd.yaml",
            "name: {{ values.name }}",
        );

        let warnings = lint_crds(&[], &[], &[templated]);

        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].code, CrdLintCode::TemplatedCrdInCrdsDir);
    }

    #[test]
    fn test_lint_external_policy_in_pack() {
        let content = r#"
apiVersion: apiextensions.k8s.io/v1
kind: CustomResourceDefinition
metadata:
  name: tests.example.com
  annotations:
    sherpack.io/crd-policy: external
"#;
        let crd = DetectedCrd::new(
            "tests.example.com",
            content,
            CrdLocation::crds_directory("test.yaml", false),
        );

        let warnings = lint_crds(&[crd], &[], &[]);

        assert!(warnings.iter().any(|w| w.code == CrdLintCode::ExternalPolicyInPack));
    }

    #[test]
    fn test_is_crd_manifest() {
        let crd = r#"
apiVersion: apiextensions.k8s.io/v1
kind: CustomResourceDefinition
metadata:
  name: test
"#;
        assert!(is_crd_manifest(crd));

        let deployment = r#"
apiVersion: apps/v1
kind: Deployment
metadata:
  name: test
"#;
        assert!(!is_crd_manifest(deployment));
    }

    #[test]
    fn test_extract_crd_name() {
        let content = r#"
apiVersion: apiextensions.k8s.io/v1
kind: CustomResourceDefinition
metadata:
  name: tests.example.com
"#;
        assert_eq!(
            extract_crd_name(content),
            Some("tests.example.com".to_string())
        );
    }

    #[test]
    fn test_lint_severity() {
        assert_eq!(
            CrdLintWarning::new(CrdLintCode::CrdInTemplates, "", "").severity(),
            LintSeverity::Info
        );
        assert_eq!(
            CrdLintWarning::new(CrdLintCode::NonCrdInCrdsDir, "", "").severity(),
            LintSeverity::Error
        );
        assert_eq!(
            CrdLintWarning::new(CrdLintCode::SharedCrdInTemplates, "", "").severity(),
            LintSeverity::Warning
        );
    }
}
