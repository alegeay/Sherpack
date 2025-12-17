//! Validate command - validate values against schema

use console::style;
use miette::{IntoDiagnostic, WrapErr};
use sherpack_core::{LoadedPack, Schema, SchemaValidator, Values};
use std::path::{Path, PathBuf};

use crate::display::ValidationReport;
use crate::error::{CliError, IntoCliResult, Result};

pub fn run(
    pack_path: &Path,
    external_schema: Option<&Path>,
    values_file: Option<&Path>,
    values_files: &[PathBuf],
    set_values: &[String],
    verbose: bool,
    json_output: bool,
    strict: bool,
) -> Result<()> {
    // Load pack
    let pack = LoadedPack::load(pack_path)
        .into_diagnostic()
        .wrap_err_with(|| format!("Failed to load pack from {}", pack_path.display()))
        .into_cli_result()?;

    if !json_output {
        println!(
            "{} Validating values for {} v{}",
            style("→").blue(),
            pack.pack.metadata.name,
            pack.pack.metadata.version
        );
    }

    // Determine schema source
    let schema_path = external_schema
        .map(|p| p.to_path_buf())
        .or_else(|| pack.schema_path.clone());

    let schema = match &schema_path {
        Some(path) => {
            if !json_output {
                println!(
                    "  {} Loading schema from {}",
                    style("→").blue(),
                    path.display()
                );
            }
            Some(
                Schema::from_file(path)
                    .into_diagnostic()
                    .wrap_err_with(|| format!("Failed to load schema from {}", path.display()))
                    .into_cli_result()?,
            )
        }
        None => {
            if !json_output {
                println!(
                    "  {} No schema found (values.schema.yaml or values.schema.json)",
                    style("⚠").yellow()
                );
            }
            return Ok(());
        }
    };

    // SAFETY: We already checked above that schema is Some and returned early if None
    let schema = schema.expect("schema should be Some after early return");

    // Create validator
    let validator = SchemaValidator::new(schema)
        .into_diagnostic()
        .wrap_err("Failed to compile schema")
        .into_cli_result()?;

    if !json_output {
        println!("  {} Schema compiled successfully", style("✓").green());
    }

    // Load and merge values
    let mut values = Values::new();

    // Apply schema defaults first
    let defaults = validator.defaults_as_values();
    if !defaults.is_empty() {
        values.merge(&defaults);
        if verbose && !json_output {
            println!(
                "  {} Applied defaults from schema",
                style("→").blue()
            );
        }
    }

    // Load values from specified file or pack's values.yaml
    let values_source = values_file.map(|p| p.to_path_buf()).unwrap_or_else(|| {
        if pack.values_path.exists() {
            pack.values_path.clone()
        } else {
            PathBuf::new()
        }
    });

    if values_source.exists() {
        let file_values = Values::from_file(&values_source)
            .into_diagnostic()
            .wrap_err_with(|| format!("Failed to load values from {}", values_source.display()))
            .into_cli_result()?;
        values.merge(&file_values);

        if verbose && !json_output {
            println!(
                "  {} Loaded values from {}",
                style("→").blue(),
                values_source.display()
            );
        }
    }

    // Merge additional values files
    for vf in values_files {
        let file_values = Values::from_file(vf)
            .into_diagnostic()
            .wrap_err_with(|| format!("Failed to load values from {}", vf.display()))
            .into_cli_result()?;
        values.merge(&file_values);

        if verbose && !json_output {
            println!("  {} Merged values from {}", style("→").blue(), vf.display());
        }
    }

    // Apply --set overrides
    if !set_values.is_empty() {
        let set_vals = sherpack_core::values::parse_set_values(set_values)
            .into_diagnostic()
            .wrap_err("Failed to parse --set values")
            .into_cli_result()?;
        values.merge(&set_vals);

        if verbose && !json_output {
            println!(
                "  {} Applied {} --set override(s)",
                style("→").blue(),
                set_values.len()
            );
        }
    }

    // Validate
    if !json_output {
        println!();
        println!("{} Validating values against schema...", style("→").blue());
    }

    let result = validator.validate(values.inner());

    if json_output {
        // Output as JSON
        let output = serde_json::json!({
            "valid": result.is_valid,
            "pack": {
                "name": pack.pack.metadata.name,
                "version": pack.pack.metadata.version.to_string(),
            },
            "errors": result.errors.iter().map(|e| {
                serde_json::json!({
                    "path": e.path,
                    "message": e.message,
                })
            }).collect::<Vec<_>>(),
        });
        // JSON serialization of our own struct should not fail
        println!(
            "{}",
            serde_json::to_string_pretty(&output)
                .expect("JSON serialization should not fail for known types")
        );

        if !result.is_valid {
            return Err(CliError::validation(format!(
                "{} validation error(s)",
                result.errors.len()
            )));
        }
    } else if result.is_valid {
        println!(
            "  {} Values are valid against schema",
            style("✓").green()
        );
        println!();
        println!("{} Validation passed!", style("✓").green().bold());
    } else {
        // Display errors
        let mut report = ValidationReport::new();

        for error in &result.errors {
            report.add_error(
                schema_path
                    .as_ref()
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_else(|| "schema".to_string())
                    .as_str(),
                &error.path,
                &error.message,
                None,
            );
        }

        report.display();
        println!();
        report.print_summary();

        if strict || report.has_errors() {
            return Err(CliError::validation(format!(
                "{} validation error(s)",
                result.errors.len()
            )));
        }
    }

    Ok(())
}
