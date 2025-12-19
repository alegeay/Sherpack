//! Template command - render pack templates locally

use console::style;
use miette::{IntoDiagnostic, Result, WrapErr};
use sherpack_core::{LoadedPack, ReleaseInfo, SchemaValidator, TemplateContext, Values};
use sherpack_engine::{Engine, PackRenderer};
use std::fs;
use std::path::Path;

use crate::display::display_render_report;

#[allow(clippy::too_many_arguments)]
pub fn run(
    name: &str,
    pack_path: &Path,
    values_files: &[std::path::PathBuf],
    set_values: &[String],
    namespace: &str,
    output_dir: Option<&Path>,
    show_only: Option<&str>,
    show_values: bool,
    skip_schema: bool,
    debug: bool,
) -> Result<()> {
    // Load pack
    let pack = LoadedPack::load(pack_path)
        .into_diagnostic()
        .wrap_err_with(|| format!("Failed to load pack from {}", pack_path.display()))?;

    if debug {
        eprintln!(
            "{} Loaded pack: {} v{}",
            style("DEBUG").dim(),
            pack.pack.metadata.name,
            pack.pack.metadata.version
        );
    }

    // Load schema if present (for defaults and validation)
    let schema_validator = if !skip_schema {
        if let Some(schema_path) = &pack.schema_path {
            match pack.load_schema() {
                Ok(Some(schema)) => {
                    match SchemaValidator::new(schema) {
                        Ok(validator) => {
                            if debug {
                                eprintln!(
                                    "{} Loaded schema from {}",
                                    style("DEBUG").dim(),
                                    schema_path.display()
                                );
                            }
                            Some(validator)
                        }
                        Err(e) => {
                            eprintln!(
                                "{} Schema compilation warning: {}",
                                style("⚠").yellow(),
                                e
                            );
                            None
                        }
                    }
                }
                Ok(None) => None,
                Err(e) => {
                    eprintln!(
                        "{} Failed to load schema: {}",
                        style("⚠").yellow(),
                        e
                    );
                    None
                }
            }
        } else {
            None
        }
    } else {
        None
    };

    // Load and merge values
    // Order: schema defaults -> values.yaml -> -f files -> --set flags
    let mut values = if let Some(ref validator) = schema_validator {
        validator.defaults_as_values()
    } else {
        Values::new()
    };

    if debug && schema_validator.is_some() {
        eprintln!(
            "{} Applied schema defaults",
            style("DEBUG").dim()
        );
    }

    // 1. Load default values from pack
    if pack.values_path.exists() {
        let default_values = Values::from_file(&pack.values_path)
            .into_diagnostic()
            .wrap_err("Failed to load default values.yaml")?;
        values.merge(&default_values);

        if debug {
            eprintln!(
                "{} Loaded default values from {}",
                style("DEBUG").dim(),
                pack.values_path.display()
            );
        }
    }

    // 2. Merge values from -f/--values files
    for values_file in values_files {
        let file_values = Values::from_file(values_file)
            .into_diagnostic()
            .wrap_err_with(|| format!("Failed to load values file: {}", values_file.display()))?;
        values.merge(&file_values);

        if debug {
            eprintln!(
                "{} Merged values from {}",
                style("DEBUG").dim(),
                values_file.display()
            );
        }
    }

    // 3. Apply --set overrides
    if !set_values.is_empty() {
        let set_vals = sherpack_core::values::parse_set_values(set_values)
            .into_diagnostic()
            .wrap_err("Failed to parse --set values")?;
        values.merge(&set_vals);

        if debug {
            eprintln!(
                "{} Applied {} --set values",
                style("DEBUG").dim(),
                set_values.len()
            );
        }
    }

    // 4. Validate values against schema if present
    if let Some(ref validator) = schema_validator {
        let result = validator.validate(values.inner());
        if !result.is_valid {
            eprintln!(
                "{} Values validation failed:",
                style("✗").red()
            );
            for err in &result.errors {
                eprintln!("  - {}: {}", err.path, err.message);
            }
            return Err(miette::miette!(
                "Values do not match schema. Use --skip-schema to bypass validation."
            ));
        } else if debug {
            eprintln!(
                "{} Values validated against schema",
                style("DEBUG").dim()
            );
        }
    }

    // Show merged values if requested
    if show_values {
        println!("{}", style("# Computed Values").cyan().bold());
        println!("---");
        let yaml = serde_yaml::to_string(values.inner())
            .into_diagnostic()
            .wrap_err("Failed to serialize values")?;
        println!("{}", yaml);
        println!("---");
        println!();
    }

    // Create template context
    let release = ReleaseInfo::for_install(name, namespace);
    let context = TemplateContext::new(values, release, &pack.pack.metadata);

    // Create pack renderer (handles subcharts automatically)
    let engine = Engine::builder()
        .strict(pack.pack.engine.strict)
        .build();
    let renderer = PackRenderer::new(engine);

    // Render templates with subchart support and error collection
    let render_result = renderer.render_collect_errors(&pack, &context);

    // Show subchart discovery info in debug mode
    if debug {
        let discovery = &render_result.discovery;
        if !discovery.subcharts.is_empty() {
            eprintln!(
                "{} Found {} subchart(s):",
                style("DEBUG").dim(),
                discovery.subcharts.len()
            );
            for subchart in &discovery.subcharts {
                let status = if subchart.enabled {
                    style("enabled").green()
                } else {
                    style("disabled").dim()
                };
                eprintln!(
                    "  - {} ({})",
                    subchart.name,
                    status
                );
            }
        }
        for warning in &discovery.warnings {
            eprintln!(
                "{} {}",
                style("⚠").yellow(),
                warning
            );
        }
    }

    // Check for errors
    if !render_result.is_success() {
        display_render_report(&render_result.report);
        return Err(miette::miette!(
            "Template rendering failed with {} error(s)",
            render_result.report.total_errors
        ));
    }

    let result = sherpack_engine::RenderResult {
        manifests: render_result.manifests,
        notes: render_result.notes,
    };

    // Output results
    if let Some(output_path) = output_dir {
        // Write to directory
        fs::create_dir_all(output_path)
            .into_diagnostic()
            .wrap_err_with(|| format!("Failed to create output directory: {}", output_path.display()))?;

        for (filename, content) in &result.manifests {
            if let Some(filter) = show_only
                && !filename.contains(filter) {
                    continue;
                }

            let file_path = output_path.join(filename);

            // Create parent directories if needed
            if let Some(parent) = file_path.parent() {
                fs::create_dir_all(parent).into_diagnostic()?;
            }

            fs::write(&file_path, content)
                .into_diagnostic()
                .wrap_err_with(|| format!("Failed to write {}", file_path.display()))?;

            println!(
                "{} {}",
                style("wrote").green(),
                file_path.display()
            );
        }

        // Write notes if present
        if let Some(notes) = &result.notes {
            let notes_path = output_path.join("NOTES.txt");
            fs::write(&notes_path, notes).into_diagnostic()?;
            println!(
                "{} {}",
                style("wrote").green(),
                notes_path.display()
            );
        }
    } else {
        // Output to stdout
        let mut first = true;

        for (filename, content) in &result.manifests {
            if let Some(filter) = show_only
                && !filename.contains(filter) {
                    continue;
                }

            if !first {
                println!();
            }
            first = false;

            println!("{}", style(format!("# Source: {}", filename)).dim());
            println!("{}", content.trim());
        }

        // Show notes
        if let Some(notes) = &result.notes {
            if !first {
                println!();
            }
            println!("{}", style("# NOTES:").yellow().bold());
            println!("{}", notes);
        }
    }

    Ok(())
}
