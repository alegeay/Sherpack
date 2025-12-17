//! Template command - render pack templates locally

use console::style;
use miette::{IntoDiagnostic, Result, WrapErr};
use sherpack_core::{LoadedPack, ReleaseInfo, TemplateContext, Values};
use sherpack_engine::Engine;
use std::fs;
use std::path::Path;

pub fn run(
    name: &str,
    pack_path: &Path,
    values_files: &[std::path::PathBuf],
    set_values: &[String],
    namespace: &str,
    output_dir: Option<&Path>,
    show_only: Option<&str>,
    show_values: bool,
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

    // Load and merge values
    let mut values = Values::new();

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

    // Create engine
    let engine = Engine::builder()
        .strict(pack.pack.engine.strict)
        .build();

    // Render templates
    let result = engine
        .render_pack(&pack, &context)
        .map_err(|e| match e {
            sherpack_engine::EngineError::Template(te) => miette::Report::new(te),
            other => miette::miette!("{}", other),
        })?;

    // Output results
    if let Some(output_path) = output_dir {
        // Write to directory
        fs::create_dir_all(output_path)
            .into_diagnostic()
            .wrap_err_with(|| format!("Failed to create output directory: {}", output_path.display()))?;

        for (filename, content) in &result.manifests {
            if let Some(filter) = show_only {
                if !filename.contains(filter) {
                    continue;
                }
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
            if let Some(filter) = show_only {
                if !filename.contains(filter) {
                    continue;
                }
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
