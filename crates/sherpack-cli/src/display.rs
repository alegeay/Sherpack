//! Display formatting for CLI output
//!
//! Provides structured display for:
//! - Validation errors with grouped display
//! - CRD change analysis with severity colors
//! - Render reports with suggestions

#![allow(dead_code)]

use console::{style, Style};
use sherpack_engine::RenderReport;
use sherpack_kube::crd::{ChangeSeverity, CrdAnalysis, CrdChange, ChangeKind};
use sherpack_kube::{CrdDeletionImpact, DeletionConfirmation, DeletionImpactSummary};
use std::collections::BTreeMap;
use std::io::{self, Write};

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

// ═══════════════════════════════════════════════════════════════════════════
// CRD Diff Display
// ═══════════════════════════════════════════════════════════════════════════

/// Renderer for CRD change analysis
pub struct CrdDiffRenderer {
    writer: Box<dyn Write>,
}

impl Default for CrdDiffRenderer {
    fn default() -> Self {
        Self::new()
    }
}

impl CrdDiffRenderer {
    /// Create a new renderer that writes to stderr
    pub fn new() -> Self {
        Self {
            writer: Box::new(io::stderr()),
        }
    }

    /// Create a renderer that writes to a custom writer (for testing)
    pub fn with_writer<W: Write + 'static>(writer: W) -> Self {
        Self {
            writer: Box::new(writer),
        }
    }

    /// Render a CRD analysis to the terminal
    pub fn render(&mut self, analysis: &CrdAnalysis) -> io::Result<()> {
        // Header
        writeln!(self.writer)?;
        writeln!(
            self.writer,
            "CRD Analysis: {}",
            style(&analysis.crd_name).cyan().bold()
        )?;
        writeln!(self.writer, "{}", "═".repeat(66))?;

        if analysis.is_new {
            writeln!(
                self.writer,
                "  {} New CRD - will be created",
                style("✓").green()
            )?;
            return Ok(());
        }

        if analysis.changes.is_empty() {
            writeln!(
                self.writer,
                "  {} No changes detected",
                style("✓").green()
            )?;
            return Ok(());
        }

        // Group changes by category
        self.render_changes_by_category(analysis)?;

        // Summary section
        self.render_summary(analysis)?;

        Ok(())
    }

    /// Render changes grouped by category
    fn render_changes_by_category(&mut self, analysis: &CrdAnalysis) -> io::Result<()> {
        // Version changes
        let version_changes: Vec<_> = analysis
            .changes
            .iter()
            .filter(|c| {
                matches!(
                    c.kind,
                    ChangeKind::AddVersion
                        | ChangeKind::RemoveVersion
                        | ChangeKind::DeprecateVersion
                        | ChangeKind::ChangeStorageVersion
                )
            })
            .collect();

        if !version_changes.is_empty() {
            writeln!(self.writer)?;
            writeln!(self.writer, "{}:", style("Versions").bold())?;
            for change in version_changes {
                self.render_change(change)?;
            }
        }

        // Schema changes
        let schema_changes: Vec<_> = analysis
            .changes
            .iter()
            .filter(|c| {
                matches!(
                    c.kind,
                    ChangeKind::AddOptionalField
                        | ChangeKind::AddRequiredField
                        | ChangeKind::RemoveField
                        | ChangeKind::RemoveRequiredField
                        | ChangeKind::ChangeFieldType
                        | ChangeKind::MakeRequired
                        | ChangeKind::RelaxValidation
                        | ChangeKind::TightenValidation
                        | ChangeKind::ChangeDefault
                        | ChangeKind::AddDefault
                        | ChangeKind::UpdateDescription
                        | ChangeKind::ChangeEnumValues
                        | ChangeKind::RemoveEnumValue
                )
            })
            .collect();

        if !schema_changes.is_empty() {
            writeln!(self.writer)?;
            writeln!(self.writer, "{}:", style("Schema Changes").bold())?;
            for change in schema_changes {
                self.render_change(change)?;
            }
        }

        // Subresource changes
        let subresource_changes: Vec<_> = analysis
            .changes
            .iter()
            .filter(|c| {
                matches!(
                    c.kind,
                    ChangeKind::AddSubresource | ChangeKind::RemoveSubresource
                )
            })
            .collect();

        if !subresource_changes.is_empty() {
            writeln!(self.writer)?;
            writeln!(self.writer, "{}:", style("Subresources").bold())?;
            for change in subresource_changes {
                self.render_change(change)?;
            }
        }

        // Printer column changes
        let printer_changes: Vec<_> = analysis
            .changes
            .iter()
            .filter(|c| matches!(c.kind, ChangeKind::AddPrinterColumn))
            .collect();

        if !printer_changes.is_empty() {
            writeln!(self.writer)?;
            writeln!(self.writer, "{}:", style("Printer Columns").bold())?;
            for change in printer_changes {
                self.render_change(change)?;
            }
        }

        // Other changes (scope, group, kind changes)
        let other_changes: Vec<_> = analysis
            .changes
            .iter()
            .filter(|c| {
                matches!(
                    c.kind,
                    ChangeKind::ChangeScope
                        | ChangeKind::ChangeGroup
                        | ChangeKind::ChangeKindName
                        | ChangeKind::AddShortName
                        | ChangeKind::AddCategory
                )
            })
            .collect();

        if !other_changes.is_empty() {
            writeln!(self.writer)?;
            writeln!(self.writer, "{}:", style("Other Changes").bold())?;
            for change in other_changes {
                self.render_change(change)?;
            }
        }

        Ok(())
    }

    /// Render a single change
    fn render_change(&mut self, change: &CrdChange) -> io::Result<()> {
        let (icon, color) = severity_style(change.severity());

        let prefix = match (&change.old_value, &change.new_value) {
            (None, Some(_)) => "+",
            (Some(_), None) => "-",
            _ => "~",
        };

        writeln!(
            self.writer,
            "  {} {} {}",
            color.apply_to(icon),
            color.apply_to(prefix),
            change.message
        )?;

        // Show old -> new for modifications
        if let (Some(old), Some(new)) = (&change.old_value, &change.new_value) {
            writeln!(
                self.writer,
                "      {} → {}",
                style(old).dim(),
                style(new).bold()
            )?;
        }

        Ok(())
    }

    /// Render summary section
    fn render_summary(&mut self, analysis: &CrdAnalysis) -> io::Result<()> {
        let (safe, warn, danger) = analysis.count_by_severity();

        writeln!(self.writer)?;
        writeln!(self.writer, "{}", "─".repeat(66))?;
        writeln!(self.writer, "{}:", style("Summary").bold())?;

        if safe > 0 {
            writeln!(
                self.writer,
                "  {} {} safe change(s)",
                style("✓").green(),
                safe
            )?;
        }
        if warn > 0 {
            writeln!(
                self.writer,
                "  {} {} warning(s)",
                style("⚠").yellow(),
                warn
            )?;
        }
        if danger > 0 {
            writeln!(
                self.writer,
                "  {} {} dangerous (require --force-crd-update)",
                style("✗").red(),
                danger
            )?;
        }

        writeln!(self.writer, "{}", "─".repeat(66))?;

        Ok(())
    }
}

/// Get style for a severity level
fn severity_style(severity: ChangeSeverity) -> (&'static str, Style) {
    match severity {
        ChangeSeverity::Safe => ("✓", Style::new().green()),
        ChangeSeverity::Warning => ("⚠", Style::new().yellow()),
        ChangeSeverity::Dangerous => ("✗", Style::new().red()),
    }
}

/// Display CRD analysis to stderr
///
/// Convenience function for quick display without creating a renderer.
pub fn display_crd_analysis(analysis: &CrdAnalysis) {
    let mut renderer = CrdDiffRenderer::new();
    if let Err(e) = renderer.render(analysis) {
        eprintln!("Failed to render CRD analysis: {}", e);
    }
}

/// Display multiple CRD analyses
pub fn display_crd_analyses(analyses: &[CrdAnalysis]) {
    for analysis in analyses {
        display_crd_analysis(analysis);
    }
}

/// Format a CRD upgrade decision for display
pub fn format_upgrade_decision(
    analyses: &[CrdAnalysis],
    force_update: bool,
) -> String {
    let total_safe: usize = analyses.iter().map(|a| a.count_by_severity().0).sum();
    let total_warn: usize = analyses.iter().map(|a| a.count_by_severity().1).sum();
    let total_danger: usize = analyses.iter().map(|a| a.count_by_severity().2).sum();

    if total_danger > 0 && !force_update {
        format!(
            "{} CRD update blocked: {} dangerous change(s). Use --force-crd-update to override.",
            style("✗").red(),
            total_danger
        )
    } else if total_danger > 0 && force_update {
        format!(
            "{} CRD update proceeding with {} dangerous change(s) (--force-crd-update)",
            style("⚠").yellow(),
            total_danger
        )
    } else if total_warn > 0 {
        format!(
            "{} CRD update proceeding with {} warning(s)",
            style("⚠").yellow(),
            total_warn
        )
    } else if total_safe > 0 {
        format!(
            "{} CRD update: {} safe change(s)",
            style("✓").green(),
            total_safe
        )
    } else {
        format!("{} No CRD changes detected", style("✓").green())
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// CRD Deletion Impact Display (Phase 3)
// ═══════════════════════════════════════════════════════════════════════════

/// Display CRD deletion impact analysis
pub fn display_deletion_impact(summary: &DeletionImpactSummary) {
    println!();
    println!(
        "{} CRD Deletion Impact Analysis",
        style("⚠").yellow().bold()
    );
    println!("{}", "═".repeat(50));

    for impact in &summary.crds {
        display_single_crd_impact(impact);
    }

    // Summary line
    println!();
    println!("{}", "─".repeat(50));
    if summary.has_blocked() {
        println!(
            "  {} {} CRD(s) blocked by policy",
            style("✗").red(),
            summary.blocked_crds.len()
        );
    }
    if summary.has_data_loss() {
        println!(
            "  {} {} CustomResource(s) will be PERMANENTLY DELETED",
            style("⚠").yellow(),
            summary.total_resources
        );
    }
    if !summary.has_blocked() && !summary.has_data_loss() {
        println!(
            "  {} No data loss expected",
            style("✓").green()
        );
    }
}

/// Display impact for a single CRD
fn display_single_crd_impact(impact: &CrdDeletionImpact) {
    println!();
    let icon = if impact.deletion_allowed {
        if impact.has_data_loss() {
            style("⚠").yellow()
        } else {
            style("✓").green()
        }
    } else {
        style("✗").red()
    };

    println!(
        "  {} {}",
        icon,
        style(&impact.crd_name).cyan().bold()
    );

    // Show policy
    println!(
        "    Policy: {}",
        style(format!("{}", impact.policy)).dim()
    );

    // Show if blocked
    if let Some(reason) = &impact.blocked_reason {
        println!("    {}: {}", style("Blocked").red(), reason);
        return;
    }

    // Show resource counts
    if impact.total_resources == 0 {
        println!("    Existing resources: {}", style("none").dim());
    } else {
        println!(
            "    Existing resources: {} across {} namespace(s)",
            style(impact.total_resources).yellow().bold(),
            impact.by_namespace.len()
        );

        // Show top namespaces (limit to 5)
        let sorted = impact.sorted_namespaces();
        let display_count = sorted.len().min(5);

        for (ns, count) in sorted.iter().take(display_count) {
            let ns_display = if ns.is_empty() {
                "(cluster-scoped)".to_string()
            } else {
                ns.to_string()
            };
            println!(
                "      - {} ({} resources)",
                ns_display,
                count
            );
        }

        if sorted.len() > 5 {
            println!(
                "      ... and {} more namespace(s)",
                sorted.len() - 5
            );
        }
    }
}

/// Display deletion confirmation requirements
pub fn display_deletion_confirmation(confirmation: &DeletionConfirmation) {
    if !confirmation.required {
        return;
    }

    println!();
    println!(
        "{} {}",
        style("!").red().bold(),
        confirmation.explanation
    );

    if !confirmation.required_flags.is_empty() {
        println!();
        println!("  To proceed, add the following flags:");
        for flag in &confirmation.required_flags {
            println!("    {}", style(flag).cyan());
        }
    } else {
        println!();
        println!("  {} Cannot proceed - policy blocks deletion", style("✗").red());
    }
}

/// Format deletion warning message
pub fn format_deletion_warning(summary: &DeletionImpactSummary) -> String {
    if summary.has_blocked() {
        format!(
            "{} Cannot delete {} CRD(s): blocked by policy ({}).",
            style("✗").red(),
            summary.blocked_crds.len(),
            summary.blocked_crds.join(", ")
        )
    } else if summary.has_data_loss() {
        format!(
            "{} Deleting {} CRD(s) will permanently delete {} CustomResource(s).",
            style("⚠").yellow(),
            summary.total_crds,
            summary.total_resources
        )
    } else if summary.total_crds > 0 {
        format!(
            "{} Deleting {} CRD(s) (no existing CustomResources).",
            style("ℹ").blue(),
            summary.total_crds
        )
    } else {
        format!("{} No CRDs to delete.", style("✓").green())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    use std::sync::{Arc, Mutex};

    /// A thread-safe buffer for testing
    #[derive(Clone, Default)]
    struct TestBuffer {
        inner: Arc<Mutex<Vec<u8>>>,
    }

    impl TestBuffer {
        fn new() -> Self {
            Self::default()
        }

        fn to_string(&self) -> String {
            let guard = self.inner.lock().unwrap();
            String::from_utf8(guard.clone()).unwrap()
        }
    }

    impl Write for TestBuffer {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.inner.lock().unwrap().write(buf)
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn test_severity_style() {
        let (icon, _) = severity_style(ChangeSeverity::Safe);
        assert_eq!(icon, "✓");

        let (icon, _) = severity_style(ChangeSeverity::Warning);
        assert_eq!(icon, "⚠");

        let (icon, _) = severity_style(ChangeSeverity::Dangerous);
        assert_eq!(icon, "✗");
    }

    #[test]
    fn test_format_upgrade_decision_no_changes() {
        let analyses: Vec<CrdAnalysis> = vec![];
        let msg = format_upgrade_decision(&analyses, false);
        assert!(msg.contains("No CRD changes"));
    }

    #[test]
    fn test_crd_diff_renderer_new_crd() {
        let buffer = TestBuffer::new();
        let mut renderer = CrdDiffRenderer::with_writer(buffer.clone());

        let analysis = CrdAnalysis {
            crd_name: "tests.example.com".to_string(),
            changes: vec![],
            is_new: true,
        };

        renderer.render(&analysis).unwrap();
        let output_str = buffer.to_string();

        assert!(output_str.contains("tests.example.com"));
        assert!(output_str.contains("New CRD"));
    }

    #[test]
    fn test_crd_diff_renderer_no_changes() {
        let buffer = TestBuffer::new();
        let mut renderer = CrdDiffRenderer::with_writer(buffer.clone());

        let analysis = CrdAnalysis {
            crd_name: "tests.example.com".to_string(),
            changes: vec![],
            is_new: false,
        };

        renderer.render(&analysis).unwrap();
        let output_str = buffer.to_string();

        assert!(output_str.contains("No changes detected"));
    }
}
