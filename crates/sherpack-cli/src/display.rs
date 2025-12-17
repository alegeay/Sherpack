//! Error display formatting for CLI output
//!
//! Provides structured error reporting with grouped display and severity levels.

#![allow(dead_code)]

use console::style;
use sherpack_engine::RenderReport;
use std::collections::BTreeMap;

/// Error severity levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Warning,
    Error,
}

/// A validation error with location information
#[derive(Debug, Clone)]
pub struct ValidationIssue {
    pub severity: Severity,
    pub file: String,
    pub path: String,
    pub message: String,
    pub suggestion: Option<String>,
}

/// Grouped validation results for display
#[derive(Debug, Default)]
pub struct ValidationReport {
    pub issues: Vec<ValidationIssue>,
    pub validated_count: usize,
}

impl ValidationReport {
    /// Create a new empty report
    pub fn new() -> Self {
        Self::default()
    }

    /// Add an issue to the report
    pub fn add_issue(&mut self, issue: ValidationIssue) {
        self.issues.push(issue);
    }

    /// Add an error
    pub fn add_error(&mut self, file: &str, path: &str, message: &str, suggestion: Option<String>) {
        self.issues.push(ValidationIssue {
            severity: Severity::Error,
            file: file.to_string(),
            path: path.to_string(),
            message: message.to_string(),
            suggestion,
        });
    }

    /// Add a warning
    pub fn add_warning(
        &mut self,
        file: &str,
        path: &str,
        message: &str,
        suggestion: Option<String>,
    ) {
        self.issues.push(ValidationIssue {
            severity: Severity::Warning,
            file: file.to_string(),
            path: path.to_string(),
            message: message.to_string(),
            suggestion,
        });
    }

    /// Display errors grouped by file
    pub fn display(&self) {
        // Group by file
        let mut by_file: BTreeMap<&str, Vec<&ValidationIssue>> = BTreeMap::new();
        for issue in &self.issues {
            by_file.entry(&issue.file).or_default().push(issue);
        }

        for (file, issues) in by_file {
            println!();
            println!("{}", style(file).cyan().bold());

            for issue in issues {
                let icon = match issue.severity {
                    Severity::Error => style("✗").red(),
                    Severity::Warning => style("⚠").yellow(),
                };

                let path_display = if issue.path.is_empty() {
                    String::new()
                } else {
                    format!(" at {}", style(&issue.path).dim())
                };

                println!("  {} {}{}", icon, issue.message, path_display);

                if let Some(suggestion) = &issue.suggestion {
                    println!("    {} {}", style("hint:").blue(), suggestion);
                }
            }
        }
    }

    /// Get summary counts
    pub fn summary(&self) -> (usize, usize) {
        let errors = self
            .issues
            .iter()
            .filter(|i| i.severity == Severity::Error)
            .count();
        let warnings = self
            .issues
            .iter()
            .filter(|i| i.severity == Severity::Warning)
            .count();
        (errors, warnings)
    }

    /// Print summary line
    pub fn print_summary(&self) {
        let (errors, warnings) = self.summary();
        if errors > 0 {
            println!(
                "{} Validation failed: {} error(s), {} warning(s)",
                style("✗").red().bold(),
                errors,
                warnings
            );
        } else if warnings > 0 {
            println!(
                "{} Validation passed with {} warning(s)",
                style("⚠").yellow().bold(),
                warnings
            );
        } else {
            println!("{} Validation passed!", style("✓").green().bold());
        }
    }

    /// Check if there are any errors (not warnings)
    pub fn has_errors(&self) -> bool {
        self.issues.iter().any(|i| i.severity == Severity::Error)
    }
}

/// Display a comprehensive render report with grouped errors
pub fn display_render_report(report: &RenderReport) {
    println!(
        "  {} Template rendering failed: {}",
        style("✗").red(),
        style(report.summary()).bold()
    );
    println!();

    // Group errors by template
    for (template_name, template_errors) in &report.errors_by_template {
        println!(
            "  {} {} ({} {})",
            style("→").blue(),
            style(template_name).yellow(),
            template_errors.len(),
            if template_errors.len() == 1 {
                "error"
            } else {
                "errors"
            }
        );

        for error in template_errors {
            // Display the error message
            println!("    {} {}", style("✗").red(), error.message);

            // Display suggestion if available
            if let Some(suggestion) = &error.suggestion {
                println!("      {} {}", style("hint:").blue(), suggestion);
            }
        }
        println!();
    }

    // Show successful templates if any
    if !report.successful_templates.is_empty() {
        println!(
            "  {} {} template(s) rendered successfully:",
            style("✓").green(),
            report.successful_templates.len()
        );
        for name in &report.successful_templates {
            println!("    - {}", name);
        }
    }
}

/// Format count with proper pluralization
pub fn pluralize(count: usize, singular: &str, plural: &str) -> String {
    if count == 1 {
        format!("{} {}", count, singular)
    } else {
        format!("{} {}", count, plural)
    }
}
