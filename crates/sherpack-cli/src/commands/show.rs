//! Show command - display pack information

use console::style;
use miette::{IntoDiagnostic, Result};
use sherpack_core::LoadedPack;
use std::path::Path;

pub fn run(path: &Path, show_all: bool) -> Result<()> {
    let pack = LoadedPack::load(path).into_diagnostic()?;

    let meta = &pack.pack.metadata;

    println!("{}", style(&meta.name).cyan().bold());
    println!("{}", style("=".repeat(meta.name.len())).dim());
    println!();

    // Basic info
    println!("{}: {}", style("Version").bold(), meta.version);

    if let Some(desc) = &meta.description {
        println!("{}: {}", style("Description").bold(), desc);
    }

    if let Some(app_version) = &meta.app_version {
        println!("{}: {}", style("App Version").bold(), app_version);
    }

    println!("{}: {:?}", style("Type").bold(), pack.pack.kind);

    if let Some(home) = &meta.home {
        println!("{}: {}", style("Home").bold(), home);
    }

    if show_all {
        // Sources
        if !meta.sources.is_empty() {
            println!();
            println!("{}:", style("Sources").bold());
            for source in &meta.sources {
                println!("  - {}", source);
            }
        }

        // Keywords
        if !meta.keywords.is_empty() {
            println!();
            println!("{}: {}", style("Keywords").bold(), meta.keywords.join(", "));
        }

        // Maintainers
        if !meta.maintainers.is_empty() {
            println!();
            println!("{}:", style("Maintainers").bold());
            for maintainer in &meta.maintainers {
                let email = maintainer.email.as_deref().unwrap_or("");
                if email.is_empty() {
                    println!("  - {}", maintainer.name);
                } else {
                    println!("  - {} <{}>", maintainer.name, email);
                }
            }
        }

        // Dependencies
        if !pack.pack.dependencies.is_empty() {
            println!();
            println!("{}:", style("Dependencies").bold());
            for dep in &pack.pack.dependencies {
                println!("  - {} {} ({})", dep.name, dep.version, dep.repository);
            }
        }

        // Templates
        if let Ok(templates) = pack.template_files() {
            println!();
            println!("{}:", style("Templates").bold());
            for template in templates {
                let rel_path = template
                    .strip_prefix(&pack.templates_dir)
                    .unwrap_or(&template);
                println!("  - {}", rel_path.display());
            }
        }
    }

    Ok(())
}
