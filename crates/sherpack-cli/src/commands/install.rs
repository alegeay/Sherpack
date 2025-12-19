//! Install command - deploy a pack to Kubernetes

use std::path::Path;
use console::style;
use miette::IntoDiagnostic;
use sherpack_core::{LoadedPack, Values, parse_set_values};
use sherpack_kube::{
    KubeClient, InstallOptions,
    storage::{FileDriver, StorageConfig},
};

use crate::error::Result;

/// Run the install command
#[allow(clippy::too_many_arguments)]
pub async fn run(
    name: &str,
    pack_path: &Path,
    values_files: &[std::path::PathBuf],
    set_values: &[String],
    namespace: &str,
    wait: bool,
    timeout: Option<u64>,
    atomic: bool,
    create_namespace: bool,
    dry_run: bool,
    show_diff: bool,
    skip_crds: bool,
) -> Result<()> {
    // Load the pack
    let pack = LoadedPack::load(pack_path).into_diagnostic()?;
    println!(
        "{} Installing pack {} version {}",
        style("→").blue().bold(),
        style(&pack.pack.metadata.name).cyan(),
        style(&pack.pack.metadata.version).yellow()
    );

    // Check for CRDs
    if pack.has_crds() {
        let crd_files = pack.crd_files().into_diagnostic()?;
        if skip_crds {
            println!(
                "{} Skipping {} CRD file(s) (--skip-crds)",
                style("⚠").yellow(),
                crd_files.len()
            );
        } else {
            println!(
                "{} Found {} CRD file(s) in crds/ directory",
                style("→").blue(),
                crd_files.len()
            );
        }
    }

    // Load and merge values
    let mut values = Values::from_file(&pack.values_path).into_diagnostic()?;

    // Apply schema defaults if available
    if let Some(schema) = pack.load_schema().into_diagnostic()? {
        let defaults = Values(schema.extract_defaults());
        values = Values::with_schema_defaults(defaults, values);
    }

    // Merge additional values files
    for vf in values_files {
        let overlay = Values::from_file(vf).into_diagnostic()?;
        values.merge(&overlay);
    }

    // Apply --set values
    if !set_values.is_empty() {
        let set_values_map = parse_set_values(set_values).into_diagnostic()?;
        values.merge(&set_values_map);
    }

    // Create storage driver (file-based for now, since we might not have a cluster)
    let storage_path = dirs::data_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("sherpack")
        .join("releases");

    let storage = FileDriver::new(storage_path, StorageConfig::default())
        .into_diagnostic()?;

    // Create client
    let client = KubeClient::new(storage).await.into_diagnostic()?;

    // Build install options
    let mut options = InstallOptions::new(name, namespace);
    options.wait = wait;
    options.atomic = atomic;
    options.create_namespace = create_namespace;
    options.dry_run = dry_run;
    options.show_diff = show_diff;

    if let Some(t) = timeout {
        options.timeout = Some(chrono::Duration::seconds(t as i64));
    }

    // Execute install
    let release = client.install(&pack, values, &options).await.into_diagnostic()?;

    if dry_run {
        println!(
            "{} Dry run - would install {} in namespace {}",
            style("✓").green().bold(),
            style(name).cyan(),
            style(namespace).yellow()
        );
    } else {
        println!(
            "{} Successfully installed {} (revision {}) in namespace {}",
            style("✓").green().bold(),
            style(&release.name).cyan(),
            style(release.version).yellow(),
            style(&release.namespace).yellow()
        );
    }

    // Show notes if present
    if let Some(notes) = &release.notes {
        println!("\n{}", style("NOTES:").bold());
        println!("{}", notes);
    }

    Ok(())
}
