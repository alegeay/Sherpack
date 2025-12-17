//! Lint command - validate a pack

use console::style;
use miette::{IntoDiagnostic, Result};
use sherpack_core::{LoadedPack, ReleaseInfo, TemplateContext, Values};
use sherpack_engine::Engine;
use std::path::Path;

pub fn run(path: &Path, strict: bool) -> Result<()> {
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
        let entries: Vec<_> = std::fs::read_dir(&templates_dir)
            .into_diagnostic()?
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

    // Try to render templates with empty values
    if let Some(pack) = &pack {
        println!();
        println!("{} Testing template rendering...", style("→").blue());

        let values = if values_path.exists() {
            Values::from_file(&values_path).unwrap_or_else(|_| Values::new())
        } else {
            Values::new()
        };

        let release = ReleaseInfo::for_install("RELEASE-NAME", "NAMESPACE");
        let context = TemplateContext::new(values, release, &pack.pack.metadata);

        let engine = Engine::builder()
            .strict(strict || pack.pack.engine.strict)
            .build();

        match engine.render_pack(pack, &context) {
            Ok(result) => {
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
            }
            Err(e) => {
                // Use miette to display the error nicely
                let report = match e {
                    sherpack_engine::EngineError::Template(te) => miette::Report::new(te),
                    other => miette::miette!("{}", other),
                };
                println!("  {} Template rendering failed:", style("✗").red());
                println!("{:?}", report);
                errors += 1;
            }
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
        std::process::exit(1);
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
