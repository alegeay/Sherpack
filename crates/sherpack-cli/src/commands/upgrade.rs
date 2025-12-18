//! Upgrade command - upgrade an existing release

use std::path::Path;
use console::style;
use miette::IntoDiagnostic;
use sherpack_core::{LoadedPack, Values, parse_set_values};
use sherpack_kube::{
    KubeClient, UpgradeOptions,
    storage::{FileDriver, StorageConfig},
    actions::ImmutableStrategy,
};

use crate::error::Result;

/// Run the upgrade command
pub async fn run(
    name: &str,
    pack_path: &Path,
    values_files: &[std::path::PathBuf],
    set_values: &[String],
    namespace: &str,
    wait: bool,
    timeout: Option<u64>,
    atomic: bool,
    install: bool,
    force: bool,
    reset_values: bool,
    reuse_values: bool,
    no_hooks: bool,
    dry_run: bool,
    show_diff: bool,
    immutable_strategy: Option<&str>,
    max_history: Option<u32>,
) -> Result<()> {
    // Load the pack
    let pack = LoadedPack::load(pack_path).into_diagnostic()?;
    println!(
        "{} Upgrading release {} with pack {} version {}",
        style("→").blue().bold(),
        style(name).cyan(),
        style(&pack.pack.metadata.name).cyan(),
        style(&pack.pack.metadata.version).yellow()
    );

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

    // Create storage driver
    let storage_path = dirs::data_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("sherpack")
        .join("releases");

    let storage = FileDriver::new(storage_path, StorageConfig::default())
        .into_diagnostic()?;

    // Create client
    let client = KubeClient::new(storage).await.into_diagnostic()?;

    // Build upgrade options
    let mut options = UpgradeOptions::new(name, namespace);
    options.wait = wait;
    options.atomic = atomic;
    options.install = install;
    options.force = force;
    options.reset_values = reset_values;
    options.reuse_values = reuse_values;
    options.no_hooks = no_hooks;
    options.dry_run = dry_run;
    options.show_diff = show_diff;
    options.max_history = max_history;

    if let Some(t) = timeout {
        options.timeout = Some(chrono::Duration::seconds(t as i64));
    }

    if let Some(strategy) = immutable_strategy {
        options.immutable_strategy = strategy.parse().unwrap_or(ImmutableStrategy::Fail);
    }

    // Execute upgrade
    let release = client.upgrade(&pack, values, &options).await.into_diagnostic()?;

    if dry_run {
        println!(
            "{} Dry run - would upgrade {} to revision {} in namespace {}",
            style("✓").green().bold(),
            style(name).cyan(),
            style(release.version).yellow(),
            style(namespace).yellow()
        );
    } else {
        println!(
            "{} Successfully upgraded {} to revision {} in namespace {}",
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
