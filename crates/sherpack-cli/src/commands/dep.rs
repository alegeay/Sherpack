//! Dependency management commands

use std::path::Path;

use crate::error::{CliError, Result};
use sherpack_core::{LoadedPack, ResolvePolicy, Values};
use sherpack_repo::{
    DependencyResolver, LockFile, RepositoryConfig, CredentialStore,
    create_backend, filter_dependencies,
};

/// List dependencies
pub async fn list(pack_path: &Path) -> Result<()> {
    let pack = LoadedPack::load(pack_path).map_err(|e| CliError::input(e.to_string()))?;

    if pack.pack.dependencies.is_empty() {
        println!("No dependencies defined in Pack.yaml");
        return Ok(());
    }

    // Load values for condition evaluation
    let values = Values::from_file(&pack.values_path)
        .map(|v| v.into_inner())
        .unwrap_or_else(|_| serde_json::json!({}));

    println!("Dependencies for {}:", pack.pack.metadata.name);
    println!();

    for dep in &pack.pack.dependencies {
        let alias_info = dep
            .alias
            .as_ref()
            .map(|a| format!(" (alias: {})", a))
            .unwrap_or_default();

        // Status indicators
        let status = if !dep.enabled {
            " [disabled]"
        } else if dep.resolve == ResolvePolicy::Never {
            " [resolve: never]"
        } else if dep.condition.is_some() && !dep.should_resolve(&values) {
            " [condition: false]"
        } else {
            ""
        };

        println!(
            "  {} @ {}{}{}",
            dep.name, dep.version, alias_info, status
        );
        println!("    repository: {}", dep.repository);

        if let Some(condition) = &dep.condition {
            println!("    condition: {}", condition);
        }
        if dep.resolve != ResolvePolicy::WhenEnabled {
            println!("    resolve: {:?}", dep.resolve);
        }
        if !dep.enabled {
            println!("    enabled: false");
        }
    }

    // Show filter summary
    let filter_result = filter_dependencies(&pack.pack.dependencies, &values);
    if filter_result.has_skipped() {
        println!();
        println!("Skipped dependencies ({}):", filter_result.skipped.len());
        println!("{}", filter_result.skipped_summary());
    }

    // Check if lock file exists
    let lock_path = pack_path.join("Pack.lock.yaml");
    if lock_path.exists() {
        println!();
        println!("Lock file: Pack.lock.yaml (exists)");

        let pack_yaml_content =
            std::fs::read_to_string(pack_path.join("Pack.yaml")).unwrap_or_default();
        if let Ok(lock) = LockFile::load(&lock_path)
            && lock.is_outdated(&pack_yaml_content) {
                println!("  WARNING: Lock file is outdated. Run 'sherpack dependency update'");
            }
    } else {
        println!();
        println!("Lock file: not found");
        println!("  Run 'sherpack dependency update' to create one");
    }

    Ok(())
}

/// Update dependencies and create lock file
pub async fn update(pack_path: &Path) -> Result<()> {
    let pack = LoadedPack::load(pack_path).map_err(|e| CliError::input(e.to_string()))?;

    if pack.pack.dependencies.is_empty() {
        println!("No dependencies to resolve");
        return Ok(());
    }

    println!("Resolving dependencies for {}...", pack.pack.metadata.name);

    // Filter dependencies based on enabled/resolve/condition
    let values = Values::from_file(&pack.values_path)
        .map(|v| v.into_inner())
        .unwrap_or_else(|_| serde_json::json!({}));
    let filter_result = filter_dependencies(&pack.pack.dependencies, &values);

    // Show skipped dependencies
    if filter_result.has_skipped() {
        println!();
        println!("Skipping {} dependencies:", filter_result.skipped.len());
        println!("{}", filter_result.skipped_summary());
    }

    if filter_result.to_resolve.is_empty() {
        println!();
        println!("No dependencies to resolve (all skipped)");

        // Still create an empty lock file
        let pack_yaml_content =
            std::fs::read_to_string(pack_path.join("Pack.yaml")).map_err(CliError::io)?;
        let lock = LockFile::new(&pack_yaml_content);
        let lock_path = pack_path.join("Pack.lock.yaml");
        lock.save(&lock_path)
            .map_err(|e| CliError::internal(e.to_string()))?;
        println!("Wrote empty Pack.lock.yaml");

        return Ok(());
    }

    let config = RepositoryConfig::load().map_err(|e| CliError::internal(e.to_string()))?;
    let cred_store = CredentialStore::load().unwrap_or_default();

    // Create resolver with fetch function
    let resolver = DependencyResolver::new(|repo_url, name, version| {
        // This is synchronous context, need to use futures
        let rt = tokio::runtime::Handle::current();
        rt.block_on(async {
            // Try to find repo in config
            let repo = if let Some(r) = config.repositories.iter().find(|r| r.url == repo_url) {
                r.clone()
            } else {
                // Create temporary repo for URL
                sherpack_repo::Repository::new("_temp", repo_url)?
            };

            let credentials = cred_store.get(&repo.name).and_then(|c| c.resolve().ok());
            let mut backend = create_backend(repo, credentials).await?;

            backend.find_best_match(name, version).await
        })
    });

    // Resolve only the filtered dependencies
    let graph = resolver
        .resolve(&filter_result.to_resolve)
        .map_err(|e| CliError::internal(e.to_string()))?;

    println!();
    println!("Resolved {} dependencies:", graph.len());

    for dep in graph.iter() {
        let alias_info = dep
            .alias
            .as_ref()
            .map(|a| format!(" (alias: {})", a))
            .unwrap_or_default();

        println!(
            "  {} @ {}{}",
            dep.name, dep.version, alias_info
        );
    }

    // Show dependency tree
    println!();
    println!("Dependency tree:");
    println!("{}", graph.render_tree());

    // Create lock file
    let pack_yaml_content =
        std::fs::read_to_string(pack_path.join("Pack.yaml")).map_err(CliError::io)?;

    let lock = graph.to_lock_file(&pack_yaml_content);
    let lock_path = pack_path.join("Pack.lock.yaml");
    lock.save(&lock_path)
        .map_err(|e| CliError::internal(e.to_string()))?;

    println!();
    println!("Wrote Pack.lock.yaml with {} locked dependencies", graph.len());

    Ok(())
}

/// Build (download) dependencies
pub async fn build(pack_path: &Path, verify: bool) -> Result<()> {
    // Validate pack exists (we only need to check if it loads)
    let _pack = LoadedPack::load(pack_path).map_err(|e| CliError::input(e.to_string()))?;

    let lock_path = pack_path.join("Pack.lock.yaml");
    if !lock_path.exists() {
        return Err(CliError::input(
            "Pack.lock.yaml not found. Run 'sherpack dependency update' first",
        ));
    }

    let lock = LockFile::load(&lock_path).map_err(|e| CliError::internal(e.to_string()))?;

    // Check if lock file is outdated
    let pack_yaml_content =
        std::fs::read_to_string(pack_path.join("Pack.yaml")).map_err(CliError::io)?;

    if lock.is_outdated(&pack_yaml_content) {
        return Err(CliError::input(
            "Pack.lock.yaml is outdated. Run 'sherpack dependency update' first",
        ));
    }

    if lock.dependencies.is_empty() {
        println!("No dependencies to download");
        return Ok(());
    }

    let config = RepositoryConfig::load().map_err(|e| CliError::internal(e.to_string()))?;
    let cred_store = CredentialStore::load().unwrap_or_default();

    // Create charts directory
    let charts_dir = pack_path.join("charts");
    std::fs::create_dir_all(&charts_dir)?;

    println!("Downloading {} dependencies...", lock.dependencies.len());

    for locked in &lock.dependencies {
        print!("  {} @ {}... ", locked.name, locked.version);

        // Find repository
        let repo = if let Some(r) = config
            .repositories
            .iter()
            .find(|r| r.url == locked.repository)
        {
            r.clone()
        } else {
            sherpack_repo::Repository::new("_temp", &locked.repository)
                .map_err(|e| CliError::internal(e.to_string()))?
        };

        let credentials = cred_store.get(&repo.name).and_then(|c| c.resolve().ok());

        let backend = create_backend(repo, credentials)
            .await
            .map_err(|e| CliError::internal(e.to_string()))?;

        // Download
        let data = backend
            .download(&locked.name, &locked.version.to_string())
            .await
            .map_err(|e| CliError::internal(e.to_string()))?;

        // Verify if requested
        if verify {
            match lock.verify(locked.effective_name(), &data) {
                Ok(sherpack_repo::VerifyResult::Match) => {
                    print!("verified... ");
                }
                Ok(sherpack_repo::VerifyResult::DigestChanged { .. }) => {
                    print!("(digest changed)... ");
                }
                Err(e) => {
                    println!("FAILED");
                    return Err(CliError::internal(format!("Integrity check failed: {}", e)));
                }
            }
        }

        // Extract to charts/
        let dest = charts_dir.join(locked.effective_name());
        extract_archive(&data, &dest)?;

        println!("OK");
    }

    println!();
    println!(
        "Downloaded {} dependencies to charts/",
        lock.dependencies.len()
    );

    Ok(())
}

/// Show dependency tree
pub async fn tree(pack_path: &Path) -> Result<()> {
    let pack = LoadedPack::load(pack_path).map_err(|e| CliError::input(e.to_string()))?;

    let lock_path = pack_path.join("Pack.lock.yaml");
    if !lock_path.exists() {
        // Just show from Pack.yaml without resolution
        println!("{}@{}", pack.pack.metadata.name, pack.pack.metadata.version);

        for (i, dep) in pack.pack.dependencies.iter().enumerate() {
            let is_last = i == pack.pack.dependencies.len() - 1;
            let prefix = if is_last { "└── " } else { "├── " };
            println!("{}{} @ {}", prefix, dep.name, dep.version);
        }

        println!();
        println!("Note: Run 'sherpack dependency update' to resolve transitive dependencies");
        return Ok(());
    }

    let lock = LockFile::load(&lock_path).map_err(|e| CliError::internal(e.to_string()))?;

    // Build graph from lock file
    let resolver = DependencyResolver::new(|_, _, _| {
        Err(sherpack_repo::RepoError::Other(
            "Tree display only".to_string(),
        ))
    });

    let graph = resolver
        .resolve_from_lock(&lock)
        .map_err(|e| CliError::internal(e.to_string()))?;

    println!("{}@{}", pack.pack.metadata.name, pack.pack.metadata.version);
    println!("{}", graph.render_tree());

    Ok(())
}

fn extract_archive(data: &[u8], dest: &std::path::PathBuf) -> Result<()> {
    use flate2::read::GzDecoder;
    use tar::Archive;

    let gz = GzDecoder::new(std::io::Cursor::new(data));
    let mut archive = Archive::new(gz);

    std::fs::create_dir_all(dest)?;
    archive.unpack(dest)?;

    Ok(())
}
