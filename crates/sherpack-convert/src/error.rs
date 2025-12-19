//! Error and warning types for the converter
//!
//! This module provides rich error reporting and conversion warnings
//! with alternatives and documentation links.

use std::path::PathBuf;
use thiserror::Error;

/// Converter error
#[derive(Debug, Error)]
pub enum ConvertError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Failed to parse Chart.yaml: {0}")]
    ChartParse(#[from] crate::chart::ChartError),

    #[error("Failed to parse template: {0}")]
    TemplateParse(#[from] crate::parser::ParseError),

    #[error("YAML error: {0}")]
    Yaml(#[from] serde_yaml::Error),

    #[error("Invalid chart: {0}")]
    InvalidChart(String),

    #[error("File not found: {0}")]
    FileNotFound(PathBuf),

    #[error("Directory not found: {0}")]
    DirectoryNotFound(PathBuf),

    #[error("Not a Helm chart: missing {0}")]
    NotAChart(String),

    #[error("Output directory already exists: {0}")]
    OutputExists(PathBuf),

    #[error("Conversion failed for {file}: {message}")]
    ConversionFailed { file: PathBuf, message: String },
}

// =============================================================================
// WARNING SYSTEM
// =============================================================================

/// Warning severity levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum WarningSeverity {
    /// Informational - conversion succeeded, minor syntax change
    Info,
    /// Warning - conversion succeeded but manual review recommended
    Warning,
    /// Unsupported - feature not available, alternative provided
    Unsupported,
    /// Error - conversion failed for this element
    Error,
}

impl WarningSeverity {
    /// Get the display color for terminal output
    pub fn color(&self) -> &'static str {
        match self {
            Self::Info => "cyan",
            Self::Warning => "yellow",
            Self::Unsupported => "magenta",
            Self::Error => "red",
        }
    }

    /// Get the emoji icon for this severity
    pub fn icon(&self) -> &'static str {
        match self {
            Self::Info => "ℹ",
            Self::Warning => "⚠",
            Self::Unsupported => "✗",
            Self::Error => "✗",
        }
    }

    /// Get the label for this severity
    pub fn label(&self) -> &'static str {
        match self {
            Self::Info => "info",
            Self::Warning => "warning",
            Self::Unsupported => "unsupported",
            Self::Error => "error",
        }
    }
}

/// Warning category for grouping related warnings
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WarningCategory {
    /// Syntax conversion (Go template → Jinja2)
    Syntax,
    /// Unsupported function or feature
    UnsupportedFeature,
    /// Deprecated pattern (works but not recommended)
    Deprecated,
    /// Security concern (crypto in templates, etc.)
    Security,
    /// GitOps compatibility issue
    GitOps,
    /// Performance consideration
    Performance,
}

impl WarningCategory {
    /// Get the display label
    pub fn label(&self) -> &'static str {
        match self {
            Self::Syntax => "syntax",
            Self::UnsupportedFeature => "unsupported",
            Self::Deprecated => "deprecated",
            Self::Security => "security",
            Self::GitOps => "gitops",
            Self::Performance => "performance",
        }
    }
}

/// Rich warning with context and alternatives
#[derive(Debug, Clone)]
pub struct ConversionWarning {
    /// Warning severity
    pub severity: WarningSeverity,
    /// Warning category
    pub category: WarningCategory,
    /// File where the warning occurred
    pub file: PathBuf,
    /// Line number (if applicable)
    pub line: Option<usize>,
    /// The Helm pattern that triggered the warning
    pub pattern: String,
    /// Human-readable message
    pub message: String,
    /// Suggested alternative or fix
    pub suggestion: Option<String>,
    /// Link to documentation
    pub doc_link: Option<String>,
}

impl ConversionWarning {
    /// Create an info-level warning
    pub fn info(file: PathBuf, pattern: &str, message: &str) -> Self {
        Self {
            severity: WarningSeverity::Info,
            category: WarningCategory::Syntax,
            file,
            line: None,
            pattern: pattern.to_string(),
            message: message.to_string(),
            suggestion: None,
            doc_link: None,
        }
    }

    /// Create a warning-level warning
    pub fn warning(file: PathBuf, pattern: &str, message: &str) -> Self {
        Self {
            severity: WarningSeverity::Warning,
            category: WarningCategory::Syntax,
            file,
            line: None,
            pattern: pattern.to_string(),
            message: message.to_string(),
            suggestion: None,
            doc_link: None,
        }
    }

    /// Create an unsupported feature warning
    pub fn unsupported(file: PathBuf, pattern: &str, alternative: &str) -> Self {
        Self {
            severity: WarningSeverity::Unsupported,
            category: WarningCategory::UnsupportedFeature,
            file,
            line: None,
            pattern: pattern.to_string(),
            message: format!("'{}' is not supported in Sherpack", pattern),
            suggestion: Some(alternative.to_string()),
            doc_link: Some("https://sherpack.dev/docs/helm-migration".to_string()),
        }
    }

    /// Create a security warning
    pub fn security(file: PathBuf, pattern: &str, message: &str, alternative: &str) -> Self {
        Self {
            severity: WarningSeverity::Unsupported,
            category: WarningCategory::Security,
            file,
            line: None,
            pattern: pattern.to_string(),
            message: message.to_string(),
            suggestion: Some(alternative.to_string()),
            doc_link: Some("https://sherpack.dev/docs/security-best-practices".to_string()),
        }
    }

    /// Create a GitOps compatibility warning
    pub fn gitops(file: PathBuf, pattern: &str, message: &str, alternative: &str) -> Self {
        Self {
            severity: WarningSeverity::Warning,
            category: WarningCategory::GitOps,
            file,
            line: None,
            pattern: pattern.to_string(),
            message: message.to_string(),
            suggestion: Some(alternative.to_string()),
            doc_link: Some("https://sherpack.dev/docs/gitops-compatibility".to_string()),
        }
    }

    /// Add line number to warning
    pub fn at_line(mut self, line: usize) -> Self {
        self.line = Some(line);
        self
    }

    /// Add suggestion to warning
    pub fn with_suggestion(mut self, suggestion: &str) -> Self {
        self.suggestion = Some(suggestion.to_string());
        self
    }

    /// Add documentation link
    pub fn with_doc_link(mut self, url: &str) -> Self {
        self.doc_link = Some(url.to_string());
        self
    }

    /// Set category
    pub fn with_category(mut self, category: WarningCategory) -> Self {
        self.category = category;
        self
    }
}

impl std::fmt::Display for ConversionWarning {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Format: [severity] file:line - message
        write!(f, "[{}] ", self.severity.label())?;
        write!(f, "{}", self.file.display())?;

        if let Some(line) = self.line {
            write!(f, ":{}", line)?;
        }

        write!(f, " - {}", self.message)?;

        if let Some(ref suggestion) = self.suggestion {
            write!(f, "\n  {} {}", self.severity.icon(), suggestion)?;
        }

        if let Some(ref link) = self.doc_link {
            write!(f, "\n  See: {}", link)?;
        }

        Ok(())
    }
}

// =============================================================================
// CONVERSION RESULT
// =============================================================================

/// Summary of conversion results
#[derive(Debug, Default)]
pub struct ConversionSummary {
    /// Number of files successfully converted
    pub files_converted: usize,
    /// Number of files copied (non-template files)
    pub files_copied: usize,
    /// Number of files skipped
    pub files_skipped: usize,
    /// All warnings generated during conversion
    pub warnings: Vec<ConversionWarning>,
}

impl ConversionSummary {
    /// Create a new empty summary
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a warning
    pub fn add_warning(&mut self, warning: ConversionWarning) {
        self.warnings.push(warning);
    }

    /// Get warnings grouped by severity
    pub fn warnings_by_severity(&self) -> std::collections::HashMap<WarningSeverity, Vec<&ConversionWarning>> {
        let mut grouped = std::collections::HashMap::new();
        for warning in &self.warnings {
            grouped.entry(warning.severity).or_insert_with(Vec::new).push(warning);
        }
        grouped
    }

    /// Get warnings grouped by category
    pub fn warnings_by_category(&self) -> std::collections::HashMap<WarningCategory, Vec<&ConversionWarning>> {
        let mut grouped = std::collections::HashMap::new();
        for warning in &self.warnings {
            grouped.entry(warning.category).or_insert_with(Vec::new).push(warning);
        }
        grouped
    }

    /// Get count of warnings by severity
    pub fn count_by_severity(&self, severity: WarningSeverity) -> usize {
        self.warnings.iter().filter(|w| w.severity == severity).count()
    }

    /// Check if there are any errors
    pub fn has_errors(&self) -> bool {
        self.warnings.iter().any(|w| w.severity == WarningSeverity::Error)
    }

    /// Check if there are any unsupported features
    pub fn has_unsupported(&self) -> bool {
        self.warnings.iter().any(|w| w.severity == WarningSeverity::Unsupported)
    }

    /// Get a success message
    pub fn success_message(&self) -> String {
        let mut msg = format!(
            "Converted {} file{}, copied {} file{}",
            self.files_converted,
            if self.files_converted == 1 { "" } else { "s" },
            self.files_copied,
            if self.files_copied == 1 { "" } else { "s" },
        );

        if self.files_skipped > 0 {
            msg.push_str(&format!(", skipped {}", self.files_skipped));
        }

        let info_count = self.count_by_severity(WarningSeverity::Info);
        let warning_count = self.count_by_severity(WarningSeverity::Warning);
        let unsupported_count = self.count_by_severity(WarningSeverity::Unsupported);

        if info_count + warning_count + unsupported_count > 0 {
            msg.push_str(&format!(
                " with {} warning{}",
                info_count + warning_count + unsupported_count,
                if info_count + warning_count + unsupported_count == 1 { "" } else { "s" }
            ));
        }

        msg
    }
}

/// Result type for conversion operations
pub type Result<T> = std::result::Result<T, ConvertError>;

// =============================================================================
// PREDEFINED WARNINGS FOR COMMON PATTERNS
// =============================================================================

/// Factory functions for common warnings
pub mod warnings {
    use super::*;
    use std::path::Path;

    /// Create warning for crypto functions (genCA, genPrivateKey, etc.)
    pub fn crypto_in_template(file: &Path, func_name: &str) -> ConversionWarning {
        ConversionWarning::security(
            file.to_path_buf(),
            func_name,
            &format!(
                "'{}' generates cryptographic material in templates - this is insecure",
                func_name
            ),
            "Use cert-manager for certificates or external-secrets for keys",
        )
    }

    /// Create warning for Files.Get/Glob
    pub fn files_access(file: &Path, method: &str) -> ConversionWarning {
        ConversionWarning::unsupported(
            file.to_path_buf(),
            &format!(".Files.{}", method),
            "Embed file content in values.yaml or use ConfigMap/Secret resources",
        )
        .with_category(WarningCategory::UnsupportedFeature)
    }

    /// Create warning for lookup function
    pub fn lookup_function(file: &Path) -> ConversionWarning {
        ConversionWarning::gitops(
            file.to_path_buf(),
            "lookup",
            "'lookup' queries the cluster at render time - incompatible with GitOps",
            "Returns {} in template mode. Use explicit values for GitOps compatibility.",
        )
    }

    /// Create warning for tpl with dynamic input
    pub fn dynamic_tpl(file: &Path) -> ConversionWarning {
        ConversionWarning::warning(
            file.to_path_buf(),
            "tpl",
            "'tpl' with dynamic input may have security implications",
        )
        .with_suggestion("Sherpack limits tpl recursion depth to 10 for safety")
        .with_doc_link("https://sherpack.dev/docs/template-security")
    }

    /// Create warning for getHostByName
    pub fn dns_lookup(file: &Path) -> ConversionWarning {
        ConversionWarning::gitops(
            file.to_path_buf(),
            "getHostByName",
            "'getHostByName' performs DNS lookup at render time - non-deterministic",
            "Use explicit IP address or hostname in values.yaml",
        )
    }

    /// Create warning for random functions
    pub fn random_function(file: &Path, func_name: &str) -> ConversionWarning {
        ConversionWarning::gitops(
            file.to_path_buf(),
            func_name,
            &format!(
                "'{}' generates different values on each render - breaks GitOps",
                func_name
            ),
            "Pre-generate values and store in values.yaml or use external-secrets",
        )
    }

    /// Create info for successful syntax conversion
    pub fn syntax_converted(file: &Path, from: &str, to: &str) -> ConversionWarning {
        ConversionWarning::info(
            file.to_path_buf(),
            from,
            &format!("Converted '{}' to '{}'", from, to),
        )
        .with_category(WarningCategory::Syntax)
    }

    /// Create warning for 'with' block context issues
    pub fn with_block_context(file: &Path) -> ConversionWarning {
        ConversionWarning::warning(
            file.to_path_buf(),
            "with",
            "'with' block context scoping differs between Go templates and Jinja2",
        )
        .with_suggestion("Review converted template - use explicit variable names if needed")
        .with_category(WarningCategory::Syntax)
    }

    /// Create info for macro conversion
    pub fn macro_converted(file: &Path, helm_name: &str, jinja_name: &str) -> ConversionWarning {
        ConversionWarning::info(
            file.to_path_buf(),
            &format!("define \"{}\"", helm_name),
            &format!("Converted to Jinja2 macro '{}'", jinja_name),
        )
        .with_category(WarningCategory::Syntax)
    }
}
