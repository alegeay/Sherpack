//! Recover command - recover a stuck release

use console::style;
use miette::IntoDiagnostic;
use sherpack_kube::{
    KubeClient,
    storage::{FileDriver, StorageConfig},
};

use crate::error::Result;

/// Run the recover command
pub async fn run(
    name: &str,
    namespace: &str,
) -> Result<()> {
    println!(
        "{} Attempting to recover release {}",
        style("→").blue().bold(),
        style(name).cyan()
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

    // Attempt recovery
    let release = client.recover(namespace, name).await.into_diagnostic()?;

    println!(
        "{} Successfully recovered {} (now marked as {})",
        style("✓").green().bold(),
        style(&release.name).cyan(),
        style(release.state.status_name()).yellow()
    );

    println!("\nYou can now retry the operation:");
    println!("  sherpack upgrade {} <pack>", name);

    Ok(())
}
