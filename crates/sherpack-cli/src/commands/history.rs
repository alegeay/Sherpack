//! History command - show release history

use console::style;
use miette::IntoDiagnostic;
use sherpack_kube::{
    KubeClient,
    storage::{FileDriver, StorageConfig},
};

use crate::error::Result;

/// Run the history command
pub async fn run(
    name: &str,
    namespace: &str,
    max_revisions: Option<usize>,
    output_json: bool,
) -> Result<()> {
    // Create storage driver
    let storage_path = dirs::data_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("sherpack")
        .join("releases");

    let storage = FileDriver::new(storage_path, StorageConfig::default()).into_diagnostic()?;

    // Create client
    let client = KubeClient::new(storage).await.into_diagnostic()?;

    // Get history
    let mut history = client.history(namespace, name).await.into_diagnostic()?;

    // Limit if requested
    if let Some(max) = max_revisions {
        history.truncate(max);
    }

    if output_json {
        let json = serde_json::to_string_pretty(&history).into_diagnostic()?;
        println!("{}", json);
        return Ok(());
    }

    println!(
        "Release history for {} in namespace {}:\n",
        style(name).cyan(),
        style(namespace).yellow()
    );

    // Print header
    println!(
        "{:<10} {:<15} {:<30} {:<20}",
        style("REVISION").bold(),
        style("STATUS").bold(),
        style("PACK").bold(),
        style("UPDATED").bold()
    );

    // Print revisions
    for release in history {
        let status_style = match release.state.status_name() {
            "deployed" => style(release.state.status_name()).green(),
            "failed" => style(release.state.status_name()).red(),
            "superseded" => style(release.state.status_name()).dim(),
            s if s.starts_with("pending") => style(release.state.status_name()).yellow(),
            _ => style(release.state.status_name()).dim(),
        };

        let pack_info = format!("{}-{}", release.pack.name, release.pack.version);

        println!(
            "{:<10} {:<15} {:<30} {:<20}",
            release.version,
            status_style,
            pack_info,
            release.updated_at.format("%Y-%m-%d %H:%M:%S")
        );
    }

    Ok(())
}
