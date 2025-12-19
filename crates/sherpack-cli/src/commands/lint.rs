//! Lint command - validate a pack

use console::style;
use sherpack_core::{LoadedPack, ReleaseInfo, SchemaValidator, TemplateContext, Values};
use sherpack_engine::Engine;
use indexmap::IndexMap;
use sherpack_kube::{
    detect_crds_in_manifests, lint_crds, CrdLocation, DetectedCrd, LintSeverity,
    TemplatedCrdFile,
};
use std::path::Path;

use crate::display::display_render_report;
use crate::error::{CliError, Result};

pub fn run(path: &Path, strict: bool, skip_schema: bool) -> Result<()> {
    println!(
        "{} Linting pack at {}",
        style("→").blue(),
        path.display()
    );

    let mut errors = 0;
    let mut warnings = 0;

    // Check Pack.yaml exists and is valid
    let pack = match LoadedPack::load(path) {
        Ok(p) => {
            println!(
                "  {} Pack.yaml is valid ({} v{})",
                style("✓").green(),
                p.pack.metadata.name,
                p.pack.metadata.version
            );
            Some(p)
        }
        Err(e) => {
            println!("  {} Pack.yaml: {}", style("✗").red(), e);
            errors += 1;
            None
        }
    };

    // Check values.yaml exists
    let values_path = path.join("values.yaml");
    if values_path.exists() {
        match Values::from_file(&values_path) {
            Ok(_) => {
                println!("  {} values.yaml is valid", style("✓").green());
            }
            Err(e) => {
                println!("  {} values.yaml: {}", style("✗").red(), e);
                errors += 1;
            }
        }
    } else {
        println!(
            "  {} values.yaml not found (optional)",
            style("⚠").yellow()
        );
        warnings += 1;
    }

    // Check templates directory
    let templates_dir = path.join("templates");
    if templates_dir.exists() {
        let entries: Vec<_> = std::fs::read_dir(&templates_dir)?
            .filter_map(|e| e.ok())
            .collect();

        if entries.is_empty() {
            println!(
                "  {} templates/ directory is empty",
                style("⚠").yellow()
            );
            warnings += 1;
        } else {
            println!(
                "  {} templates/ contains {} file(s)",
                style("✓").green(),
                entries.len()
            );
        }
    } else {
        println!(
            "  {} templates/ directory not found",
            style("✗").red()
        );
        errors += 1;
    }

    // Check and validate schema if present
    let mut schema_validator = None;
    if let Some(pack) = &pack
        && !skip_schema {
            if let Some(schema_path) = &pack.schema_path {
                match pack.load_schema() {
                    Ok(Some(schema)) => {
                        println!(
                            "  {} {} is valid",
                            style("✓").green(),
                            schema_path
                                .file_name()
                                .map(|n| n.to_string_lossy())
                                .unwrap_or_else(|| "schema".into())
                        );
                        match SchemaValidator::new(schema) {
                            Ok(validator) => {
                                schema_validator = Some(validator);
                            }
                            Err(e) => {
                                println!("  {} Schema compilation failed: {}", style("✗").red(), e);
                                errors += 1;
                            }
                        }
                    }
                    Ok(None) => {}
                    Err(e) => {
                        println!("  {} Failed to load schema: {}", style("✗").red(), e);
                        errors += 1;
                    }
                }
            } else {
                println!(
                    "  {} No schema file found (optional)",
                    style("⚠").yellow()
                );
                warnings += 1;
            }
        }

    // Try to render templates with values
    if let Some(pack) = &pack {
        // Load values (with schema defaults if available)
        let mut values = if let Some(ref validator) = schema_validator {
            validator.defaults_as_values()
        } else {
            Values::new()
        };

        if values_path.exists() {
            let file_values = Values::from_file(&values_path).unwrap_or_else(|_| Values::new());
            values.merge(&file_values);
        }

        // Validate values against schema if present
        if let Some(ref validator) = schema_validator {
            println!();
            println!("{} Validating values against schema...", style("→").blue());

            let result = validator.validate(values.inner());
            if result.is_valid {
                println!("  {} Values match schema", style("✓").green());
            } else {
                println!("  {} Values do not match schema:", style("✗").red());
                for err in &result.errors {
                    println!("    - {}: {}", err.path, err.message);
                }
                errors += result.errors.len();
            }
        }

        println!();
        println!("{} Testing template rendering...", style("→").blue());

        let release = ReleaseInfo::for_install("RELEASE-NAME", "NAMESPACE");
        let context = TemplateContext::new(values, release, &pack.pack.metadata);

        let engine = Engine::builder()
            .strict(strict || pack.pack.engine.strict)
            .build();

        // Use the error-collecting render method
        let result = engine.render_pack_collect_errors(pack, &context);

        if result.is_success() {
            println!(
                "  {} Rendered {} template(s) successfully",
                style("✓").green(),
                result.manifests.len()
            );

            // Validate YAML output
            for (name, content) in &result.manifests {
                match serde_yaml::from_str::<serde_yaml::Value>(content) {
                    Ok(_) => {
                        println!(
                            "    {} {} produces valid YAML",
                            style("✓").green(),
                            name
                        );
                    }
                    Err(e) => {
                        println!(
                            "    {} {} produces invalid YAML: {}",
                            style("✗").red(),
                            name,
                            e
                        );
                        errors += 1;
                    }
                }
            }
        } else {
            // Display comprehensive error report
            display_render_report(&result.report);
            errors += result.report.total_errors;
        }

        // CRD Linting (Phase 3)
        if result.is_success() {
            let (crd_errors, crd_warnings) = lint_crds_in_pack(pack, &result.manifests);
            errors += crd_errors;
            warnings += crd_warnings;
        }
    }

    // Summary
    println!();
    if errors > 0 {
        println!(
            "{} Linting failed with {} error(s) and {} warning(s)",
            style("✗").red().bold(),
            errors,
            warnings
        );
        return Err(CliError::lint_failed(errors, warnings));
    } else if warnings > 0 {
        println!(
            "{} Linting passed with {} warning(s)",
            style("⚠").yellow().bold(),
            warnings
        );
    } else {
        println!(
            "{} Linting passed!",
            style("✓").green().bold()
        );
    }

    Ok(())
}

/// Lint CRDs in the pack
///
/// Returns (error_count, warning_count)
fn lint_crds_in_pack(pack: &LoadedPack, manifests: &IndexMap<String, String>) -> (usize, usize) {
    let mut errors = 0;
    let mut warnings = 0;

    // Collect CRDs from crds/ directory
    let crds_dir_crds: Vec<DetectedCrd> = match pack.load_crds() {
        Ok(crds) => crds
            .into_iter()
            .filter(|c| !c.is_templated)
            .map(|c| {
                let location = CrdLocation::crds_directory(&c.source_file, false);
                DetectedCrd::new(&c.name, &c.content, location)
            })
            .collect(),
        Err(_) => Vec::new(),
    };

    // Collect templated CRD files
    let templated_files: Vec<TemplatedCrdFile> = match pack.load_crds() {
        Ok(crds) => crds
            .into_iter()
            .filter(|c| c.is_templated)
            .map(|c| TemplatedCrdFile::analyze(c.source_file.display().to_string(), &c.content))
            .collect(),
        Err(_) => Vec::new(),
    };

    // Detect CRDs in rendered templates
    let templates_crds = detect_crds_in_manifests(manifests);

    // Skip if no CRDs found
    if crds_dir_crds.is_empty() && templated_files.is_empty() && templates_crds.is_empty() {
        return (0, 0);
    }

    println!();
    println!("{} Checking CRD configuration...", style("→").blue());

    // Show CRDs found
    let total_crds = crds_dir_crds.len() + templates_crds.len();
    if total_crds > 0 {
        println!(
            "  {} Found {} CRD(s)",
            style("✓").green(),
            total_crds
        );

        for crd in &crds_dir_crds {
            println!(
                "    {} {} ({})",
                style("•").dim(),
                crd.name,
                crd.location.description()
            );
        }

        for crd in &templates_crds {
            println!(
                "    {} {} ({})",
                style("•").dim(),
                crd.name,
                crd.location.description()
            );
        }
    }

    // Show templated CRD files
    if !templated_files.is_empty() {
        println!(
            "  {} Found {} templated CRD file(s) in crds/",
            style("ℹ").blue(),
            templated_files.len()
        );
    }

    // Run lint checks
    let lint_warnings = lint_crds(&crds_dir_crds, &templates_crds, &templated_files);

    if lint_warnings.is_empty() {
        println!("  {} No CRD issues found", style("✓").green());
        return (0, 0);
    }

    println!();
    println!("{} CRD Recommendations:", style("→").blue());

    for warning in &lint_warnings {
        let icon = match warning.severity() {
            LintSeverity::Error => style("✗").red(),
            LintSeverity::Warning => style("⚠").yellow(),
            LintSeverity::Info => style("ℹ").blue(),
        };

        // Count by severity
        match warning.severity() {
            LintSeverity::Error => errors += 1,
            LintSeverity::Warning => warnings += 1,
            LintSeverity::Info => {} // Info doesn't count as error or warning
        }

        println!();
        println!("  {} {}", icon, warning.path);
        if let Some(name) = &warning.crd_name {
            println!("    CRD: {}", name);
        }
        println!("    {}", warning.message);
        if let Some(suggestion) = &warning.suggestion {
            println!(
                "    {} {}",
                style("Tip:").dim(),
                suggestion
            );
        }
    }

    (errors, warnings)
}
