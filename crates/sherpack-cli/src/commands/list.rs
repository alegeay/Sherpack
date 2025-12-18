//! List command - list installed releases

use console::style;
use miette::IntoDiagnostic;
use sherpack_kube::{
    KubeClient,
    storage::{FileDriver, StorageConfig},
};

use crate::error::Result;

/// Run the list command
pub async fn run(
    namespace: Option<&str>,
    all_namespaces: bool,
    output_json: bool,
) -> Result<()> {
    // Create storage driver
    let storage_path = dirs::data_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("sherpack")
        .join("releases");

    let storage = FileDriver::new(storage_path, StorageConfig::default())
        .into_diagnostic()?;

    // Create client
    let client = KubeClient::new(storage).await.into_diagnostic()?;

    // List releases
    let releases = client.list(namespace, all_namespaces).await.into_diagnostic()?;

    if output_json {
        let json = serde_json::to_string_pretty(&releases).into_diagnostic()?;
        println!("{}", json);
        return Ok(());
    }

    if releases.is_empty() {
        if all_namespaces {
            println!("No releases found in any namespace");
        } else {
            println!(
                "No releases found in namespace {}",
                namespace.unwrap_or("default")
            );
        }
        return Ok(());
    }

    // Print header
    println!(
        "{:<20} {:<15} {:<10} {:<15} {:<20}",
        style("NAME").bold(),
        style("NAMESPACE").bold(),
        style("REVISION").bold(),
        style("STATUS").bold(),
        style("UPDATED").bold()
    );

    // Print releases
    for release in releases {
        let status_style = match release.state.status_name() {
            "deployed" => style(release.state.status_name()).green(),
            "failed" => style(release.state.status_name()).red(),
            s if s.starts_with("pending") => style(release.state.status_name()).yellow(),
            _ => style(release.state.status_name()).dim(),
        };

        println!(
            "{:<20} {:<15} {:<10} {:<15} {:<20}",
            release.name,
            release.namespace,
            release.version,
            status_style,
            release.updated_at.format("%Y-%m-%d %H:%M:%S")
        );
    }

    Ok(())
}
