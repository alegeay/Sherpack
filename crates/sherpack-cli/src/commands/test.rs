//! Test command — run `test` hooks against an installed release
//!
//! Equivalent to `helm test`. Loads the latest stored release, parses its
//! manifests for hooks tagged with the `test` phase (annotation
//! `sherpack.io/hook: test` or `helm.sh/hook: test`), and executes them
//! against the cluster.

use console::style;
use miette::IntoDiagnostic;
use sherpack_kube::{
    KubeClient,
    hooks::{HookExecutor, HookPhase, parse_hooks_from_manifest},
    storage::{FileDriver, StorageConfig},
};

use crate::error::{CliError, Result};

/// Run the test command
pub async fn run(name: &str, namespace: &str) -> Result<()> {
    let storage_path = dirs::data_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("sherpack")
        .join("releases");

    let storage = FileDriver::new(storage_path, StorageConfig::default()).into_diagnostic()?;
    let client = KubeClient::new(storage).await.into_diagnostic()?;

    // Load the latest stored release
    let release = client.status(namespace, name).await.into_diagnostic()?;

    // Hooks may already be parsed in the stored release; if not, parse from manifest
    let hooks = if release.hooks.is_empty() {
        parse_hooks_from_manifest(&release.manifest)
    } else {
        release.hooks.clone()
    };

    let test_hooks: Vec<_> = hooks
        .iter()
        .filter(|h| h.runs_in_phase(HookPhase::Test))
        .collect();

    if test_hooks.is_empty() {
        println!(
            "Release {}/{} has no test hooks (looked for {})",
            namespace,
            name,
            style("sherpack.io/hook: test").cyan()
        );
        return Ok(());
    }

    println!(
        "Running {} test hook(s) for {}/{} (revision {})",
        style(test_hooks.len()).bold(),
        style(namespace).yellow(),
        style(name).cyan(),
        release.version
    );

    let mut executor = HookExecutor::with_namespace(namespace);
    let result = executor
        .execute_phase(
            &hooks,
            HookPhase::Test,
            name,
            release.version,
            client.kube_client(),
        )
        .await;

    // Always print results, success or failure
    println!();
    println!("{}", style("RESULTS").bold().underlined());
    for r in executor.results_for_phase(HookPhase::Test) {
        let status = if r.error.is_some() {
            style("FAIL").red().bold()
        } else {
            style("PASS").green().bold()
        };
        let duration_ms = r.duration().num_milliseconds();
        println!("  [{}] {}  ({}ms)", status, r.name, duration_ms);
        if let Some(err) = &r.error {
            println!("        {}", style(err).red().dim());
        }
    }

    match result {
        Ok(()) => {
            if executor.has_failures() {
                Err(CliError::internal(
                    "One or more test hooks reported failures",
                ))
            } else {
                Ok(())
            }
        }
        Err(e) => Err(CliError::internal(format!("Test execution aborted: {}", e))),
    }
}
