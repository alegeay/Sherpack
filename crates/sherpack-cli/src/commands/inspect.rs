//! Inspect command - view archive contents without extracting

use console::style;
use miette::{IntoDiagnostic, Result};
use sherpack_core::{list_archive, read_file_from_archive, read_manifest_from_archive};
use std::path::Path;

use crate::util::{format_size, truncate_hash};

pub fn run(archive_path: &Path, show_manifest: bool, show_checksums: bool) -> Result<()> {
    // Read manifest
    let manifest = read_manifest_from_archive(archive_path).into_diagnostic()?;

    if show_manifest {
        // Just print the raw manifest
        let manifest_bytes = read_file_from_archive(archive_path, "MANIFEST").into_diagnostic()?;
        let manifest_text = String::from_utf8(manifest_bytes)
            .map_err(|e| miette::miette!("Invalid UTF-8 in MANIFEST: {}", e))?;
        println!("{}", manifest_text);
        return Ok(());
    }

    // Print header
    println!(
        "{} {} v{}",
        style("Archive").cyan().bold(),
        manifest.name,
        manifest.pack_version
    );
    println!();

    // Print metadata
    println!(
        "  {}: {}",
        style("Created").dim(),
        manifest.created.format("%Y-%m-%d %H:%M:%S UTC")
    );
    println!(
        "  {}: sha256:{}...",
        style("Digest").dim(),
        truncate_hash(&manifest.digest, 16)
    );
    println!();

    // List files
    let entries = list_archive(archive_path).into_diagnostic()?;

    println!("{}:", style("Files").bold());
    for entry in &entries {
        if entry.is_dir {
            continue;
        }

        let size = format_size(entry.size);

        if show_checksums {
            // Find checksum in manifest
            let checksum = manifest
                .files
                .iter()
                .find(|f| f.path == entry.path)
                .map(|f| format!("sha256:{}...", truncate_hash(&f.sha256, 12)))
                .unwrap_or_else(|| "N/A".to_string());

            println!(
                "  {:40} {:>10}  {}",
                entry.path,
                size,
                style(checksum).dim()
            );
        } else {
            println!("  {:40} {:>10}", entry.path, size);
        }
    }

    // Summary
    let file_count = entries.iter().filter(|e| !e.is_dir).count();
    let total_size: u64 = entries.iter().filter(|e| !e.is_dir).map(|e| e.size).sum();

    println!();
    println!("{} files, {} total", file_count, format_size(total_size));

    Ok(())
}
