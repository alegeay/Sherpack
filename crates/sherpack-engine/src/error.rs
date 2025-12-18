//! Engine error types with beautiful formatting

use indexmap::IndexMap;
use miette::{Diagnostic, NamedSource, SourceSpan};
use thiserror::Error;

use crate::suggestions::{
    extract_filter_name, extract_function_name, extract_variable_name, suggest_iteration_fix,
    suggest_undefined_variable, suggest_unknown_filter, suggest_unknown_function,
    AVAILABLE_FILTERS,
};

/// Main engine error type
#[derive(Error, Debug)]
pub enum EngineError {
    #[error("Template error")]
    Template(#[from] TemplateError),

    #[error("Filter error: {message}")]
    Filter { message: String },

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("YAML error: {0}")]
    Yaml(#[from] serde_yaml::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Multiple template errors occurred")]
    MultipleErrors(RenderReport),
}

/// Error kind for categorizing template errors
///
/// Note: This enum is non-exhaustive - new variants may be added in future versions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum TemplateErrorKind {
    UndefinedVariable,
    UnknownFilter,
    UnknownFunction,
    SyntaxError,
    TypeError,
    InvalidOperation,
    YamlParseError,
    Other,
}

impl TemplateErrorKind {
    /// Convert to a code string for diagnostics
    pub fn to_code_string(&self) -> &'static str {
        match self {
            Self::UndefinedVariable => "undefined_variable",
            Self::UnknownFilter => "unknown_filter",
            Self::UnknownFunction => "unknown_function",
            Self::SyntaxError => "syntax",
            Self::TypeError => "type",
            Self::InvalidOperation => "invalid_operation",
            Self::YamlParseError => "yaml_parse",
            Self::Other => "render",
        }
    }
}

/// Template-specific error with source information
#[derive(Error, Debug, Diagnostic, Clone)]
#[error("{message}")]
#[diagnostic(code(sherpack::template::render))]
pub struct TemplateError {
    /// Error message
    pub message: String,

    /// Error kind for categorization
    pub kind: TemplateErrorKind,

    /// Template source code
    #[source_code]
    pub src: NamedSource<String>,

    /// Error location in source
    #[label("error occurred here")]
    pub span: Option<SourceSpan>,

    /// Suggestion for fixing the error
    #[help]
    pub suggestion: Option<String>,

    /// Additional context (available values, etc.)
    pub context: Option<String>,
}

impl TemplateError {
    /// Create a new template error from a MiniJinja error
    pub fn from_minijinja(
        err: minijinja::Error,
        template_name: &str,
        template_source: &str,
    ) -> Self {
        let (kind, message) = categorize_minijinja_error(&err);
        let line = err.line();

        // Calculate source span from line number
        let span = line.and_then(|line_num| calculate_span(template_source, line_num));

        // Generate suggestion based on error kind
        let suggestion = generate_suggestion(&err, &kind, None);

        Self {
            message,
            kind,
            src: NamedSource::new(template_name, template_source.to_string()),
            span,
            suggestion,
            context: None,
        }
    }

    /// Create a new template error from a MiniJinja error with enhanced context-aware suggestions
    pub fn from_minijinja_enhanced(
        err: minijinja::Error,
        template_name: &str,
        template_source: &str,
        values: Option<&serde_json::Value>,
    ) -> Self {
        let (kind, message) = categorize_minijinja_error(&err);
        let line = err.line();

        // Calculate source span from line number
        let span = line.and_then(|line_num| calculate_span(template_source, line_num));

        // Generate context-aware suggestion
        let suggestion = generate_suggestion(&err, &kind, values);

        Self {
            message,
            kind,
            src: NamedSource::new(template_name, template_source.to_string()),
            span,
            suggestion,
            context: None,
        }
    }

    /// Create a simple error without source mapping
    pub fn simple(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            kind: TemplateErrorKind::Other,
            src: NamedSource::new("<unknown>", String::new()),
            span: None,
            suggestion: None,
            context: None,
        }
    }

    /// Add context information
    pub fn with_context(mut self, context: impl Into<String>) -> Self {
        self.context = Some(context.into());
        self
    }

    /// Add a suggestion
    pub fn with_suggestion(mut self, suggestion: impl Into<String>) -> Self {
        self.suggestion = Some(suggestion.into());
        self
    }

    /// Get the error kind
    pub fn kind(&self) -> TemplateErrorKind {
        self.kind
    }
}

/// Categorize a MiniJinja error into our error kinds
fn categorize_minijinja_error(err: &minijinja::Error) -> (TemplateErrorKind, String) {
    let msg = err.to_string();
    let msg_lower = msg.to_lowercase();

    // Get the detailed display which contains more info
    let detailed = format!("{:#}", err);

    let kind = match err.kind() {
        minijinja::ErrorKind::UndefinedError => TemplateErrorKind::UndefinedVariable,
        minijinja::ErrorKind::UnknownFilter => TemplateErrorKind::UnknownFilter,
        minijinja::ErrorKind::UnknownFunction => TemplateErrorKind::UnknownFunction,
        minijinja::ErrorKind::SyntaxError => TemplateErrorKind::SyntaxError,
        minijinja::ErrorKind::InvalidOperation => TemplateErrorKind::InvalidOperation,
        minijinja::ErrorKind::NonPrimitive | minijinja::ErrorKind::NonKey => {
            TemplateErrorKind::TypeError
        }
        _ => {
            // Fallback to string matching for other cases
            if msg_lower.contains("undefined") || msg_lower.contains("unknown variable") {
                TemplateErrorKind::UndefinedVariable
            } else if msg_lower.contains("filter") {
                TemplateErrorKind::UnknownFilter
            } else if msg_lower.contains("function") {
                TemplateErrorKind::UnknownFunction
            } else if msg_lower.contains("syntax") || msg_lower.contains("expected") {
                TemplateErrorKind::SyntaxError
            } else if msg_lower.contains("not iterable") || msg_lower.contains("cannot") {
                TemplateErrorKind::TypeError
            } else {
                TemplateErrorKind::Other
            }
        }
    };

    // Extract the actual expression from the detailed error if possible
    // MiniJinja format shows: "   8 >   typo: {{ value.app.name }}"
    // followed by: "     i            ^^^^^^^^^ undefined value"
    let enhanced_msg = match kind {
        TemplateErrorKind::UndefinedVariable => {
            if let Some(expr) = extract_expression_from_display(&detailed) {
                format!("undefined variable `{}`", expr)
            } else {
                msg.replace("undefined value", "undefined variable")
            }
        }
        TemplateErrorKind::UnknownFilter => {
            if let Some(filter) = extract_filter_from_display(&detailed) {
                format!("unknown filter `{}`", filter)
            } else {
                msg.clone()
            }
        }
        _ => msg
            .replace("invalid operation: ", "")
            .replace("syntax error: ", "")
            .replace("undefined value", "undefined variable"),
    };

    (kind, enhanced_msg)
}

/// Extract the problematic expression from MiniJinja's detailed display
fn extract_expression_from_display(display: &str) -> Option<String> {
    // MiniJinja format:
    //    8 >   typo: {{ value.app.name }}
    //      i            ^^^^^^^^^ undefined value
    // The `>` marker shows the error line

    let lines: Vec<&str> = display.lines().collect();

    // First, find the line with the `>` marker (error line)
    for (i, line) in lines.iter().enumerate() {
        // Look for pattern like "   8 >   " at the start
        let trimmed = line.trim_start();
        if trimmed.contains(" > ") || trimmed.starts_with("> ") {
            // This is the error line - extract expression
            if let Some(start) = line.find("{{") {
                if let Some(end) = line[start..].find("}}") {
                    let expr = line[start + 2..start + end].trim();
                    // Get the first part before any filter (for undefined var)
                    let expr_part = expr.split('|').next().unwrap_or(expr).trim();
                    if !expr_part.is_empty() {
                        return Some(expr_part.to_string());
                    }
                }
            }
        }

        // Also check the line after a ^^^^^ marker for the error line
        if line.contains("^^^^^") {
            // The line with ^^^^^ follows the error line, so check i-1
            if i > 0 {
                let prev_line = lines[i - 1];
                if let Some(start) = prev_line.find("{{") {
                    if let Some(end) = prev_line[start..].find("}}") {
                        let expr = prev_line[start + 2..start + end].trim();
                        let expr_part = expr.split('|').next().unwrap_or(expr).trim();
                        if !expr_part.is_empty() {
                            return Some(expr_part.to_string());
                        }
                    }
                }
            }
        }
    }

    None
}

/// Extract the filter name from MiniJinja's detailed display
fn extract_filter_from_display(display: &str) -> Option<String> {
    // Look for the error line (marked with >) and find the filter
    // MiniJinja format:
    //    8 >   badFilter: {{ values.app.name | toyml }}
    //      i                                   ^^^^^ unknown filter

    let lines: Vec<&str> = display.lines().collect();

    // Find the error line
    for line in &lines {
        let trimmed = line.trim_start();
        if trimmed.contains(" > ") || trimmed.starts_with("> ") {
            // Look for {{ ... | filter }} pattern
            if let Some(start) = line.find("{{") {
                if let Some(end) = line[start..].find("}}") {
                    let expr = &line[start + 2..start + end];
                    // Find the pipe and get the filter name
                    if let Some(pipe_pos) = expr.rfind('|') {
                        let filter_part = expr[pipe_pos + 1..].trim();
                        // Filter name is the first word
                        let filter_name = filter_part.split_whitespace().next();
                        if let Some(name) = filter_name {
                            if !name.is_empty() {
                                return Some(name.to_string());
                            }
                        }
                    }
                }
            }
        }
    }

    // Fallback: look for pattern "unknown filter" in error message
    for line in &lines {
        if line.contains("unknown filter") {
            // Try to extract from the ^^^^^ marker line
            continue;
        }
    }

    None
}

/// Calculate the source span for a given line number
fn calculate_span(source: &str, line_num: usize) -> Option<SourceSpan> {
    let mut offset = 0;
    let mut current_line = 1;

    for line in source.lines() {
        if current_line == line_num {
            // Return span for the entire line
            return Some(SourceSpan::new(offset.into(), line.len().into()));
        }
        offset += line.len() + 1; // +1 for newline
        current_line += 1;
    }

    None
}

/// Generate context-aware suggestions based on error kind
fn generate_suggestion(
    err: &minijinja::Error,
    kind: &TemplateErrorKind,
    values: Option<&serde_json::Value>,
) -> Option<String> {
    let msg = err.to_string();
    let detailed = format!("{:#}", err);

    match kind {
        TemplateErrorKind::UndefinedVariable => {
            // Try to extract variable name from detailed display first
            let var_name = extract_expression_from_display(&detailed)
                .or_else(|| extract_variable_name(&msg));

            if let Some(var_name) = var_name {
                // Check for common typo: "value" instead of "values"
                if var_name == "value" || var_name.starts_with("value.") {
                    let corrected = var_name.replacen("value", "values", 1);
                    return Some(format!(
                        "Did you mean `{}`? Use `values` (plural) to access the values object.",
                        corrected
                    ));
                }

                // Check for property access on values
                if let Some(path) = var_name.strip_prefix("values.") {
                    let parts: Vec<&str> = path.split('.').collect();

                    if let Some(vals) = values {
                        // Navigate to find where the path breaks
                        let mut current = vals;
                        let mut valid_parts = vec![];

                        for part in &parts {
                            if let Some(next) = current.get(part) {
                                valid_parts.push(*part);
                                current = next;
                            } else {
                                // This part doesn't exist - suggest alternatives
                                if let Some(obj) = current.as_object() {
                                    let available: Vec<&str> =
                                        obj.keys().map(|s| s.as_str()).collect();

                                    // Find closest match
                                    let matches = crate::suggestions::find_closest_matches(
                                        part,
                                        &available,
                                        3,
                                        crate::suggestions::SuggestionCategory::Property,
                                    );

                                    let prefix = if valid_parts.is_empty() {
                                        "values".to_string()
                                    } else {
                                        format!("values.{}", valid_parts.join("."))
                                    };

                                    if !matches.is_empty() {
                                        let suggestions: Vec<String> = matches
                                            .iter()
                                            .map(|m| format!("`{}.{}`", prefix, m.text))
                                            .collect();
                                        return Some(format!(
                                            "Key `{}` not found. Did you mean {}? Available: {}",
                                            part,
                                            suggestions.join(" or "),
                                            available.join(", ")
                                        ));
                                    } else {
                                        return Some(format!(
                                            "Key `{}` not found in `{}`. Available keys: {}",
                                            part,
                                            prefix,
                                            available.join(", ")
                                        ));
                                    }
                                }
                                break;
                            }
                        }
                    }
                }

                // General undefined variable suggestion
                let available = values
                    .and_then(|v| v.as_object())
                    .map(|obj| obj.keys().cloned().collect::<Vec<_>>())
                    .unwrap_or_default();

                return suggest_undefined_variable(&var_name, &available).or_else(|| {
                    Some(format!(
                        "Variable `{}` is not defined. Check spelling or use `| default(\"fallback\")`.",
                        var_name
                    ))
                });
            }
            Some("Variable is not defined. Check spelling or use the `default` filter.".to_string())
        }

        TemplateErrorKind::UnknownFilter => {
            // Try to extract filter name from detailed display first
            let filter_name = extract_filter_from_display(&detailed)
                .or_else(|| extract_filter_name(&msg));

            if let Some(filter_name) = filter_name {
                return suggest_unknown_filter(&filter_name);
            }
            Some(format!(
                "Unknown filter. Available: {}",
                AVAILABLE_FILTERS.join(", ")
            ))
        }

        TemplateErrorKind::UnknownFunction => {
            if let Some(func_name) = extract_function_name(&msg) {
                return suggest_unknown_function(&func_name);
            }
            Some("Unknown function. Check the function name and arguments.".to_string())
        }

        TemplateErrorKind::SyntaxError => {
            if msg.contains("}") || msg.contains("%") {
                Some(
                    "Check bracket matching: `{{ }}` for expressions, `{% %}` for statements, `{# #}` for comments".to_string(),
                )
            } else if msg.contains("expected") {
                Some(
                    "Syntax error. Check for missing closing tags or mismatched brackets."
                        .to_string(),
                )
            } else {
                None
            }
        }

        TemplateErrorKind::TypeError => {
            if msg.to_lowercase().contains("not iterable") {
                Some(suggest_iteration_fix("object"))
            } else if msg.to_lowercase().contains("not callable") {
                Some(
                    "Use `{{ value }}` for variables, `{{ func() }}` for function calls."
                        .to_string(),
                )
            } else {
                None
            }
        }

        _ => None,
    }
}

/// A collection of errors from rendering multiple templates
#[derive(Debug, Default)]
pub struct RenderReport {
    /// Errors grouped by template file (IndexMap preserves insertion order)
    pub errors_by_template: IndexMap<String, Vec<TemplateError>>,

    /// Successfully rendered templates
    pub successful_templates: Vec<String>,

    /// Total error count
    pub total_errors: usize,
}

impl RenderReport {
    /// Create a new empty report
    pub fn new() -> Self {
        Self::default()
    }

    /// Add an error for a specific template
    pub fn add_error(&mut self, template_name: String, error: TemplateError) {
        self.errors_by_template
            .entry(template_name)
            .or_default()
            .push(error);
        self.total_errors += 1;
    }

    /// Mark a template as successfully rendered
    pub fn add_success(&mut self, template_name: String) {
        self.successful_templates.push(template_name);
    }

    /// Check if there are any errors
    pub fn has_errors(&self) -> bool {
        self.total_errors > 0
    }

    /// Get count of templates with errors
    pub fn templates_with_errors(&self) -> usize {
        self.errors_by_template.len()
    }

    /// Generate summary message: "5 errors in 3 templates"
    pub fn summary(&self) -> String {
        let template_word = if self.templates_with_errors() == 1 {
            "template"
        } else {
            "templates"
        };
        let error_word = if self.total_errors == 1 {
            "error"
        } else {
            "errors"
        };
        format!(
            "{} {} in {} {}",
            self.total_errors,
            error_word,
            self.templates_with_errors(),
            template_word
        )
    }
}

/// Result type that includes both successful renders and collected errors
#[derive(Debug)]
pub struct RenderResultWithReport {
    /// Rendered manifests (may be partial if errors occurred, IndexMap preserves order)
    pub manifests: IndexMap<String, String>,

    /// Post-install notes
    pub notes: Option<String>,

    /// Error report (empty if all templates rendered successfully)
    pub report: RenderReport,
}

impl RenderResultWithReport {
    /// Check if rendering was fully successful (no errors)
    pub fn is_success(&self) -> bool {
        !self.report.has_errors()
    }
}

/// Result type for engine operations
pub type Result<T> = std::result::Result<T, EngineError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_render_report_new() {
        let report = RenderReport::new();
        assert!(!report.has_errors());
        assert_eq!(report.total_errors, 0);
        assert_eq!(report.templates_with_errors(), 0);
        assert!(report.successful_templates.is_empty());
    }

    #[test]
    fn test_render_report_add_error() {
        let mut report = RenderReport::new();

        let error = TemplateError::simple("test error");
        report.add_error("template.yaml".to_string(), error);

        assert!(report.has_errors());
        assert_eq!(report.total_errors, 1);
        assert_eq!(report.templates_with_errors(), 1);
    }

    #[test]
    fn test_render_report_multiple_errors_same_template() {
        let mut report = RenderReport::new();

        report.add_error("template.yaml".to_string(), TemplateError::simple("error 1"));
        report.add_error("template.yaml".to_string(), TemplateError::simple("error 2"));

        assert_eq!(report.total_errors, 2);
        assert_eq!(report.templates_with_errors(), 1);
        assert_eq!(report.errors_by_template["template.yaml"].len(), 2);
    }

    #[test]
    fn test_render_report_multiple_templates() {
        let mut report = RenderReport::new();

        report.add_error("a.yaml".to_string(), TemplateError::simple("error 1"));
        report.add_error("b.yaml".to_string(), TemplateError::simple("error 2"));
        report.add_error("c.yaml".to_string(), TemplateError::simple("error 3"));

        assert_eq!(report.total_errors, 3);
        assert_eq!(report.templates_with_errors(), 3);
    }

    #[test]
    fn test_render_report_add_success() {
        let mut report = RenderReport::new();

        report.add_success("good.yaml".to_string());
        report.add_success("also-good.yaml".to_string());

        assert!(!report.has_errors());
        assert_eq!(report.successful_templates.len(), 2);
    }

    #[test]
    fn test_render_report_summary_singular() {
        let mut report = RenderReport::new();
        report.add_error("template.yaml".to_string(), TemplateError::simple("error"));

        assert_eq!(report.summary(), "1 error in 1 template");
    }

    #[test]
    fn test_render_report_summary_plural() {
        let mut report = RenderReport::new();
        report.add_error("a.yaml".to_string(), TemplateError::simple("error 1"));
        report.add_error("a.yaml".to_string(), TemplateError::simple("error 2"));
        report.add_error("b.yaml".to_string(), TemplateError::simple("error 3"));

        assert_eq!(report.summary(), "3 errors in 2 templates");
    }

    #[test]
    fn test_render_result_with_report_success() {
        let result = RenderResultWithReport {
            manifests: IndexMap::new(),
            notes: None,
            report: RenderReport::new(),
        };
        assert!(result.is_success());
    }

    #[test]
    fn test_render_result_with_report_failure() {
        let mut report = RenderReport::new();
        report.add_error("test.yaml".to_string(), TemplateError::simple("error"));

        let result = RenderResultWithReport {
            manifests: IndexMap::new(),
            notes: None,
            report,
        };
        assert!(!result.is_success());
    }

    #[test]
    fn test_template_error_simple() {
        let error = TemplateError::simple("test message");
        assert_eq!(error.message, "test message");
        assert_eq!(error.kind, TemplateErrorKind::Other);
        assert!(error.suggestion.is_none());
    }

    #[test]
    fn test_template_error_with_suggestion() {
        let error = TemplateError::simple("test").with_suggestion("try this");
        assert_eq!(error.suggestion, Some("try this".to_string()));
    }

    #[test]
    fn test_template_error_with_context() {
        let error = TemplateError::simple("test").with_context("additional info");
        assert_eq!(error.context, Some("additional info".to_string()));
    }

    #[test]
    fn test_template_error_kind() {
        let error = TemplateError {
            message: "test".to_string(),
            kind: TemplateErrorKind::UndefinedVariable,
            src: NamedSource::new("test", String::new()),
            span: None,
            suggestion: None,
            context: None,
        };
        assert_eq!(error.kind(), TemplateErrorKind::UndefinedVariable);
    }

    #[test]
    fn test_template_error_kind_to_code_string() {
        assert_eq!(
            TemplateErrorKind::UndefinedVariable.to_code_string(),
            "undefined_variable"
        );
        assert_eq!(
            TemplateErrorKind::UnknownFilter.to_code_string(),
            "unknown_filter"
        );
        assert_eq!(
            TemplateErrorKind::SyntaxError.to_code_string(),
            "syntax"
        );
    }

    #[test]
    fn test_extract_expression_from_display_with_marker() {
        let display = r#"
   8 >   typo: {{ value.app.name }}
     i            ^^^^^^^^^ undefined value
"#;
        let expr = extract_expression_from_display(display);
        assert_eq!(expr, Some("value.app.name".to_string()));
    }

    #[test]
    fn test_extract_expression_with_filter() {
        let display = r#"
   8 >   data: {{ values.app.name | upper }}
     i                              ^^^^^ unknown filter
"#;
        let expr = extract_expression_from_display(display);
        assert_eq!(expr, Some("values.app.name".to_string()));
    }

    #[test]
    fn test_extract_filter_from_display() {
        let display = r#"
   8 >   data: {{ values.name | toyml }}
     i                          ^^^^^ unknown filter
"#;
        let filter = extract_filter_from_display(display);
        assert_eq!(filter, Some("toyml".to_string()));
    }
}
