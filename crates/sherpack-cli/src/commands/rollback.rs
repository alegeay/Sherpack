//! Rollback command - roll back to a previous release revision

use console::style;
use miette::IntoDiagnostic;
use sherpack_kube::{
    KubeClient, RollbackOptions,
    storage::{FileDriver, StorageConfig},
    actions::ImmutableStrategy,
};

use crate::error::Result;

/// Run the rollback command
pub async fn run(
    name: &str,
    revision: u32,
    namespace: &str,
    wait: bool,
    timeout: Option<u64>,
    force: bool,
    no_hooks: bool,
    dry_run: bool,
    show_diff: bool,
    immutable_strategy: Option<&str>,
    max_history: Option<u32>,
) -> Result<()> {
    let target = if revision == 0 {
        "previous".to_string()
    } else {
        format!("revision {}", revision)
    };

    println!(
        "{} Rolling back release {} to {}",
        style("→").blue().bold(),
        style(name).cyan(),
        style(&target).yellow()
    );

    // Create storage driver
    let storage_path = dirs::data_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("sherpack")
        .join("releases");

    let storage = FileDriver::new(storage_path, StorageConfig::default())
        .into_diagnostic()?;

    // Create client
    let client = KubeClient::new(storage).await.into_diagnostic()?;

    // Build rollback options
    let mut options = RollbackOptions::new(name, namespace);
    options.revision = revision;
    options.wait = wait;
    options.force = force;
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

    // Execute rollback
    let release = client.rollback(&options).await.into_diagnostic()?;

    if dry_run {
        println!(
            "{} Dry run - would rollback {} to revision {}",
            style("✓").green().bold(),
            style(name).cyan(),
            style(release.version).yellow()
        );
    } else {
        println!(
            "{} Successfully rolled back {} to revision {}",
            style("✓").green().bold(),
            style(&release.name).cyan(),
            style(release.version).yellow()
        );
    }

    Ok(())
}
