//! Package command - create distributable archives

use console::style;
use miette::{IntoDiagnostic, Result};
use sherpack_core::{LoadedPack, create_archive, default_archive_name};
use std::path::Path;

use super::signing::sign_archive;
use crate::util::{format_size, truncate_hash};

pub fn run(path: &Path, output: Option<&Path>, sign_key: Option<&Path>) -> Result<()> {
    // Load the pack
    let pack = LoadedPack::load(path).into_diagnostic()?;

    // Determine output path
    let archive_name = default_archive_name(&pack);
    let output_path = match output {
        Some(p) => p.to_path_buf(),
        None => path.join(&archive_name),
    };

    // Create the archive
    println!(
        "{} {} v{}",
        style("Packaging").cyan().bold(),
        pack.pack.metadata.name,
        pack.pack.metadata.version
    );

    let created_path = create_archive(&pack, &output_path).into_diagnostic()?;

    // Get file size for display
    let metadata = std::fs::metadata(&created_path).into_diagnostic()?;
    let size = format_size(metadata.len());

    println!(
        "  {} {}",
        style("Created").green().bold(),
        created_path.display()
    );
    println!("  {} {}", style("Size").dim(), size);

    // Sign if key provided
    if let Some(key_path) = sign_key {
        sign_archive(&created_path, key_path, None)?;
    }

    // Print manifest info
    let manifest = sherpack_core::read_manifest_from_archive(&created_path).into_diagnostic()?;

    println!();
    println!("{}:", style("Contents").bold());
    for entry in &manifest.files {
        println!("  {}", entry.path);
    }

    println!();
    println!(
        "{}: sha256:{}",
        style("Digest").bold(),
        truncate_hash(&manifest.digest, 16)
    );

    Ok(())
}
