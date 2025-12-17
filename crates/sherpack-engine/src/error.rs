//! Engine error types with beautiful formatting

use miette::{Diagnostic, NamedSource, SourceSpan};
use thiserror::Error;

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
}

/// Template-specific error with source information
#[derive(Error, Debug, Diagnostic)]
#[error("{message}")]
#[diagnostic(code(sherpack::template::render))]
pub struct TemplateError {
    /// Error message
    pub message: String,

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
        let message = format_minijinja_error(&err);
        let line = err.line();

        // Calculate source span from line number
        let span = line.and_then(|line_num| {
            calculate_span(template_source, line_num)
        });

        // Generate suggestion based on error kind
        let suggestion = generate_suggestion(&err);

        Self {
            message,
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
}

/// Format a MiniJinja error into a readable message
fn format_minijinja_error(err: &minijinja::Error) -> String {
    // Get the main error message
    let mut msg = err.to_string();

    // Clean up common patterns
    msg = msg
        .replace("invalid operation: ", "")
        .replace("syntax error: ", "")
        .replace("undefined value", "undefined variable");

    msg
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

/// Generate a suggestion based on the error
fn generate_suggestion(err: &minijinja::Error) -> Option<String> {
    let msg = err.to_string().to_lowercase();

    if msg.contains("undefined") {
        Some("Check that the variable is defined in values.yaml or use the `default` filter: {{ variable | default(\"fallback\") }}".to_string())
    } else if msg.contains("not found") && msg.contains("filter") {
        Some("Unknown filter. Common filters: toyaml, tojson, b64encode, quote, default, upper, lower".to_string())
    } else if msg.contains("expected") && (msg.contains("}") || msg.contains("%")) {
        Some("Check for matching brackets: {{ }} for expressions, {% %} for statements".to_string())
    } else if msg.contains("not iterable") {
        Some("The value is not a list. Use {% if value %} for conditionals or ensure the value is a list".to_string())
    } else if msg.contains("not callable") {
        Some("Use {{ value }} for variables, {{ func() }} for function calls".to_string())
    } else {
        None
    }
}

/// Result type for engine operations
pub type Result<T> = std::result::Result<T, EngineError>;
