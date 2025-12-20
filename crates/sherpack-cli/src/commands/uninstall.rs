//! Uninstall command - remove a release from Kubernetes

use console::style;
use miette::IntoDiagnostic;
use sherpack_kube::{
    KubeClient, UninstallOptions,
    storage::{FileDriver, StorageConfig},
};

use crate::error::Result;

/// Run the uninstall command
#[allow(clippy::too_many_arguments)]
pub async fn run(
    name: &str,
    namespace: &str,
    wait: bool,
    timeout: Option<u64>,
    keep_history: bool,
    no_hooks: bool,
    dry_run: bool,
    delete_crds: bool,
    confirm_crd_deletion: bool,
) -> Result<()> {
    // Validate CRD deletion flags
    if delete_crds && !confirm_crd_deletion {
        eprintln!(
            "{} Cannot delete CRDs without confirmation",
            style("✗").red().bold()
        );
        eprintln!("  CRD deletion will also delete ALL CustomResources of those types!");
        eprintln!("  Use --confirm-crd-deletion to proceed.");
        return Err(miette::miette!("CRD deletion requires --confirm-crd-deletion flag").into());
    }

    println!(
        "{} Uninstalling release {} from namespace {}",
        style("→").blue().bold(),
        style(name).cyan(),
        style(namespace).yellow()
    );

    if delete_crds && confirm_crd_deletion {
        println!(
            "{} CRDs will be deleted (--delete-crds --confirm-crd-deletion)",
            style("⚠").yellow()
        );
    }

    // Create storage driver
    let storage_path = dirs::data_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("sherpack")
        .join("releases");

    let storage = FileDriver::new(storage_path, StorageConfig::default()).into_diagnostic()?;

    // Create client
    let client = KubeClient::new(storage).await.into_diagnostic()?;

    // Build uninstall options
    let mut options = UninstallOptions::new(name, namespace);
    options.wait = wait;
    options.keep_history = keep_history;
    options.no_hooks = no_hooks;
    options.dry_run = dry_run;

    if let Some(t) = timeout {
        options.timeout = Some(chrono::Duration::seconds(t as i64));
    }

    // Execute uninstall
    let release = client.uninstall(&options).await.into_diagnostic()?;

    if dry_run {
        println!(
            "{} Dry run - would uninstall {}",
            style("✓").green().bold(),
            style(name).cyan()
        );
    } else {
        println!(
            "{} Successfully uninstalled {} (was revision {})",
            style("✓").green().bold(),
            style(&release.name).cyan(),
            style(release.version).yellow()
        );

        if keep_history {
            println!("  History preserved (use --purge to remove completely)");
        }
    }

    Ok(())
}
