//! Convert command - convert Helm charts to Sherpack packs
//!
//! This command transforms Helm charts into idiomatic Sherpack packs,
//! converting Go templates to Jinja2 syntax with elegant patterns.

use console::style;
use miette::{IntoDiagnostic, Result, WrapErr};
use sherpack_convert::{
    ConversionResult, ConvertOptions, WarningCategory, WarningSeverity, convert_with_options,
};
use std::collections::HashMap;
use std::path::Path;

pub fn run(
    chart_path: &Path,
    output: Option<&Path>,
    force: bool,
    dry_run: bool,
    verbose: bool,
) -> Result<()> {
    // Determine output path
    let output_path = if let Some(out) = output {
        out.to_path_buf()
    } else {
        // Default: chartname-sherpack in current directory
        let chart_name = chart_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("pack");
        std::env::current_dir()
            .into_diagnostic()?
            .join(format!("{}-sherpack", chart_name))
    };

    // Header
    print_header(chart_path, &output_path);

    let options = ConvertOptions {
        force,
        dry_run,
        verbose,
    };

    let result = convert_with_options(chart_path, &output_path, options)
        .into_diagnostic()
        .wrap_err("Conversion failed")?;

    // Print results
    print_files(&result, &output_path, chart_path);
    print_warnings(&result, &output_path, chart_path, verbose);
    print_summary(&result);
    print_next_steps(&result, &output_path, dry_run);

    Ok(())
}

fn print_header(chart_path: &Path, output_path: &Path) {
    println!();
    println!(
        "  {} {} {}",
        style("Sherpack Convert").bold().cyan(),
        style("─").dim(),
        style("Helm → Jinja2").dim()
    );
    println!();
    println!(
        "  {} {} {}",
        style("Source:").dim(),
        style(chart_path.display()).cyan(),
        style("(Helm chart)").dim()
    );
    println!(
        "  {} {} {}",
        style("Target:").dim(),
        style(output_path.display()).green(),
        style("(Sherpack pack)").dim()
    );
    println!();
}

fn print_files(result: &ConversionResult, output_path: &Path, chart_path: &Path) {
    println!("  {}", style("Converted Files").bold());
    println!("  {}", style("───────────────").dim());

    for file in &result.converted_files {
        let rel_path = file.strip_prefix(output_path).unwrap_or(file);
        println!("  {} {}", style("✓").green().bold(), rel_path.display());
    }

    if !result.copied_files.is_empty() {
        println!();
        println!("  {}", style("Copied Files").bold());
        println!("  {}", style("────────────").dim());

        for file in &result.copied_files {
            let rel_path = file.strip_prefix(output_path).unwrap_or(file);
            println!("  {} {}", style("→").blue(), rel_path.display());
        }
    }

    if !result.skipped_files.is_empty() {
        println!();
        println!("  {}", style("Skipped Files").bold().yellow());
        println!("  {}", style("─────────────").dim());

        for file in &result.skipped_files {
            let rel_path = file.strip_prefix(chart_path).unwrap_or(file);
            println!("  {} {}", style("○").yellow(), rel_path.display());
        }
    }

    println!();
}

fn print_warnings(result: &ConversionResult, output_path: &Path, chart_path: &Path, verbose: bool) {
    if result.warnings.is_empty() {
        return;
    }

    // Group warnings by category
    let mut by_category: HashMap<WarningCategory, Vec<_>> = HashMap::new();
    for warning in &result.warnings {
        by_category
            .entry(warning.category)
            .or_default()
            .push(warning);
    }

    // Skip info-level warnings unless verbose
    let has_significant_warnings = result
        .warnings
        .iter()
        .any(|w| w.severity != WarningSeverity::Info);

    if !has_significant_warnings && !verbose {
        let info_count = result
            .warnings
            .iter()
            .filter(|w| w.severity == WarningSeverity::Info)
            .count();
        if info_count > 0 {
            println!(
                "  {} {} {} {}",
                style("ℹ").cyan(),
                info_count,
                style("syntax conversions applied").dim(),
                style("(use --verbose to see details)").dim()
            );
            println!();
        }
        return;
    }

    println!("  {}", style("Conversion Notes").bold());
    println!("  {}", style("────────────────").dim());
    println!();

    // Print security/unsupported warnings first (most important)
    if let Some(security_warnings) = by_category.get(&WarningCategory::Security) {
        println!(
            "  {} {}",
            style("Security").red().bold(),
            style("─ requires attention").dim()
        );
        for warning in security_warnings {
            print_warning(warning, output_path, chart_path);
        }
        println!();
    }

    if let Some(unsupported) = by_category.get(&WarningCategory::UnsupportedFeature) {
        println!(
            "  {} {}",
            style("Unsupported").magenta().bold(),
            style("─ manual migration needed").dim()
        );
        for warning in unsupported {
            print_warning(warning, output_path, chart_path);
        }
        println!();
    }

    if let Some(gitops_warnings) = by_category.get(&WarningCategory::GitOps) {
        println!(
            "  {} {}",
            style("GitOps").yellow().bold(),
            style("─ review for compatibility").dim()
        );
        for warning in gitops_warnings {
            print_warning(warning, output_path, chart_path);
        }
        println!();
    }

    // Show syntax warnings only in verbose mode
    if verbose && let Some(syntax_warnings) = by_category.get(&WarningCategory::Syntax) {
        println!(
            "  {} {}",
            style("Syntax").cyan().bold(),
            style("─ automatic conversions").dim()
        );
        for warning in syntax_warnings {
            print_warning(warning, output_path, chart_path);
        }
        println!();
    }
}

fn print_warning(
    warning: &sherpack_convert::ConversionWarning,
    output_path: &Path,
    chart_path: &Path,
) {
    let icon = match warning.severity {
        WarningSeverity::Info => style("ℹ").cyan(),
        WarningSeverity::Warning => style("⚠").yellow(),
        WarningSeverity::Unsupported => style("✗").magenta(),
        WarningSeverity::Error => style("✗").red().bold(),
    };

    let rel_file = warning
        .file
        .strip_prefix(output_path)
        .or_else(|_| warning.file.strip_prefix(chart_path))
        .unwrap_or(&warning.file);

    let location = if let Some(line) = warning.line {
        format!("{}:{}", rel_file.display(), line)
    } else {
        format!("{}", rel_file.display())
    };

    println!(
        "    {} {} {}",
        icon,
        style(&warning.pattern).bold(),
        style(format!("in {}", location)).dim()
    );

    // Show message on next line, indented
    println!("      {}", style(&warning.message).dim());

    // Show suggestion with better formatting
    if let Some(ref suggestion) = warning.suggestion {
        println!("      {} {}", style("→").green(), suggestion);
    }

    // Show doc link if present
    if let Some(ref link) = warning.doc_link {
        println!(
            "      {} {}",
            style("📖").dim(),
            style(link).underlined().dim()
        );
    }
}

fn print_summary(result: &ConversionResult) {
    let converted = result.converted_files.len();
    let copied = result.copied_files.len();
    let skipped = result.skipped_files.len();

    let error_count = result
        .warnings
        .iter()
        .filter(|w| w.severity == WarningSeverity::Error)
        .count();
    let unsupported_count = result
        .warnings
        .iter()
        .filter(|w| w.severity == WarningSeverity::Unsupported)
        .count();
    let warning_count = result
        .warnings
        .iter()
        .filter(|w| w.severity == WarningSeverity::Warning)
        .count();

    // Summary box
    println!("  {}", style("Summary").bold());
    println!("  {}", style("───────").dim());

    // Files summary
    println!(
        "  {} {} template{} converted to Jinja2",
        style(format!("{:>3}", converted)).green().bold(),
        style("files").dim(),
        if converted == 1 { "" } else { "s" }
    );

    if copied > 0 {
        println!(
            "  {} {} copied unchanged",
            style(format!("{:>3}", copied)).blue().bold(),
            style("files").dim()
        );
    }

    if skipped > 0 {
        println!(
            "  {} {} skipped",
            style(format!("{:>3}", skipped)).yellow().bold(),
            style("files").dim()
        );
    }

    // Warnings summary
    if error_count > 0 {
        println!(
            "  {} {} conversion error{}",
            style(format!("{:>3}", error_count)).red().bold(),
            style("").dim(),
            if error_count == 1 { "" } else { "s" }
        );
    }

    if unsupported_count > 0 {
        println!(
            "  {} {} unsupported feature{} {}",
            style(format!("{:>3}", unsupported_count)).magenta().bold(),
            style("").dim(),
            if unsupported_count == 1 { "" } else { "s" },
            style("(needs manual fix)").dim()
        );
    }

    if warning_count > 0 {
        println!(
            "  {} {} warning{} {}",
            style(format!("{:>3}", warning_count)).yellow().bold(),
            style("").dim(),
            if warning_count == 1 { "" } else { "s" },
            style("(review recommended)").dim()
        );
    }

    println!();
}

fn print_next_steps(result: &ConversionResult, output_path: &Path, dry_run: bool) {
    if dry_run {
        println!(
            "  {} {}",
            style("ℹ").cyan(),
            style("Dry run mode - no files were written").dim()
        );
        println!();
        return;
    }

    let has_unsupported = result
        .warnings
        .iter()
        .any(|w| w.severity == WarningSeverity::Unsupported);

    println!("  {}", style("Next Steps").bold());
    println!("  {}", style("──────────").dim());

    // Step 1: Always validate
    println!(
        "  {} {}",
        style("1.").dim(),
        style(format!("sherpack lint {}", output_path.display())).cyan()
    );
    println!(
        "     {}",
        style("Validate the converted pack structure").dim()
    );

    // Step 2: If unsupported features, review them
    if has_unsupported {
        println!();
        println!(
            "  {} {}",
            style("2.").dim(),
            style("Review unsupported features above").yellow()
        );
        println!(
            "     {}",
            style("Replace with recommended alternatives").dim()
        );
    }

    // Step 3: Test render
    println!();
    println!(
        "  {} {}",
        style(if has_unsupported { "3." } else { "2." }).dim(),
        style(format!(
            "sherpack template test-release {} -f values.yaml",
            output_path.display()
        ))
        .cyan()
    );
    println!(
        "     {}",
        style("Test template rendering with your values").dim()
    );

    println!();

    // Show elegance comparison
    println!("  {}", style("Why Sherpack?").bold().green());
    println!("  {}", style("─────────────").dim());
    println!(
        "  {} Helm:     {}",
        style("  ").dim(),
        style("{{ index .Values.list 0 }}").dim()
    );
    println!(
        "  {} Sherpack: {}",
        style("→").green(),
        style("{{ values.list[0] }}").cyan()
    );
    println!();
    println!(
        "  {} Helm:     {}",
        style("  ").dim(),
        style("{{ ternary \"a\" \"b\" .cond }}").dim()
    );
    println!(
        "  {} Sherpack: {}",
        style("→").green(),
        style("{{ \"a\" if cond else \"b\" }}").cyan()
    );
    println!();
}
