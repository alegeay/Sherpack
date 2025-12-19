//! Status command - show status of a release

use console::style;
use miette::IntoDiagnostic;
use sherpack_kube::{
    KubeClient,
    health::HealthCheckConfig,
    storage::{FileDriver, StorageConfig},
};

use crate::error::Result;

/// Run the status command
pub async fn run(
    name: &str,
    namespace: &str,
    show_resources: bool,
    show_values: bool,
    show_manifest: bool,
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

    // Get release status
    let release = client.status(namespace, name).await.into_diagnostic()?;

    if output_json {
        let json = serde_json::to_string_pretty(&release).into_diagnostic()?;
        println!("{}", json);
        return Ok(());
    }

    // Print release info
    println!("{}", style("RELEASE INFO").bold().underlined());
    println!("  Name:       {}", style(&release.name).cyan());
    println!("  Namespace:  {}", style(&release.namespace).yellow());
    println!("  Revision:   {}", style(release.version).yellow());

    let status_style = match release.state.status_name() {
        "deployed" => style(release.state.to_string()).green(),
        "failed" => style(release.state.to_string()).red(),
        s if s.starts_with("pending") => style(release.state.to_string()).yellow(),
        _ => style(release.state.to_string()).dim(),
    };
    println!("  Status:     {}", status_style);
    println!(
        "  Created:    {}",
        release.created_at.format("%Y-%m-%d %H:%M:%S")
    );
    println!(
        "  Updated:    {}",
        release.updated_at.format("%Y-%m-%d %H:%M:%S")
    );

    // Pack info
    println!("\n{}", style("PACK").bold().underlined());
    println!("  Name:       {}", release.pack.name);
    println!("  Version:    {}", release.pack.version);
    if let Some(desc) = &release.pack.description {
        println!("  Description: {}", desc);
    }
    if let Some(app_ver) = &release.pack.app_version {
        println!("  App Version: {}", app_ver);
    }

    // Show values if requested
    if show_values {
        println!("\n{}", style("VALUES").bold().underlined());
        let yaml = serde_yaml::to_string(&release.values).into_diagnostic()?;
        println!("{}", yaml);
    }

    // Show manifest if requested
    if show_manifest {
        println!("\n{}", style("MANIFEST").bold().underlined());
        println!("{}", release.manifest);
    }

    // Show resources if requested
    if show_resources {
        println!("\n{}", style("RESOURCES").bold().underlined());

        // Get health status
        let health_config = HealthCheckConfig::default();
        match client.health(namespace, name, Some(health_config)).await {
            Ok(health) => {
                for resource in health.resources {
                    let icon = if resource.healthy { "✓" } else { "✗" };
                    let icon_style = if resource.healthy {
                        style(icon).green()
                    } else {
                        style(icon).red()
                    };
                    println!(
                        "  {} {}/{} - {}",
                        icon_style,
                        resource.kind,
                        resource.name,
                        resource.readiness_display()
                    );
                }
            }
            Err(e) => {
                println!(
                    "  {}",
                    style(format!("Unable to get resource status: {}", e)).dim()
                );
            }
        }
    }

    // Show notes if present
    if let Some(notes) = &release.notes {
        println!("\n{}", style("NOTES").bold().underlined());
        println!("{}", notes);
    }

    Ok(())
}
