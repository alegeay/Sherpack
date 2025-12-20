//! Search command

use crate::error::{CliError, Result};
use sherpack_repo::{
    CredentialStore, IndexCache, RepositoryConfig, RepositoryType, create_backend,
};

/// Search for packs across repositories
pub async fn run(
    query: &str,
    repo_name: Option<&str>,
    versions: bool,
    json_output: bool,
) -> Result<()> {
    // First try local cache (fast)
    let cache = IndexCache::open().map_err(|e| CliError::internal(e.to_string()))?;

    let results = if let Some(repo_name) = repo_name {
        cache
            .search_in_repo(repo_name, query)
            .map_err(|e| CliError::internal(e.to_string()))?
    } else {
        cache
            .search(query)
            .map_err(|e| CliError::internal(e.to_string()))?
    };

    if results.is_empty() {
        // Try online search if cache is empty
        println!("No results in local cache. Searching online...");
        return search_online(query, repo_name).await;
    }

    if json_output {
        let json = serde_json::to_string_pretty(
            &results
                .iter()
                .map(|r| {
                    serde_json::json!({
                        "name": r.name,
                        "version": r.version,
                        "description": r.description,
                        "repository": r.repo_name,
                    })
                })
                .collect::<Vec<_>>(),
        )
        .unwrap_or_default();
        println!("{}", json);
        return Ok(());
    }

    // Print results
    println!(
        "{:<30} {:<15} {:<15} DESCRIPTION",
        "NAME", "VERSION", "REPO"
    );
    println!("{}", "-".repeat(90));

    for pack in results {
        let desc = pack
            .description
            .as_deref()
            .unwrap_or("")
            .chars()
            .take(35)
            .collect::<String>();

        println!(
            "{:<30} {:<15} {:<15} {}",
            format!("{}/{}", pack.repo_name, pack.name),
            pack.version,
            pack.repo_name,
            desc
        );

        if versions {
            // Show all versions
            if let Ok(all_versions) = cache.get_pack_versions(&pack.repo_name, &pack.name) {
                for v in all_versions.iter().skip(1).take(5) {
                    println!("  └── {}", v.version);
                }
                if all_versions.len() > 6 {
                    println!("  └── ... and {} more", all_versions.len() - 6);
                }
            }
        }
    }

    Ok(())
}

async fn search_online(query: &str, repo_name: Option<&str>) -> Result<()> {
    let config = RepositoryConfig::load().map_err(|e| CliError::internal(e.to_string()))?;
    let cred_store = CredentialStore::load().unwrap_or_default();

    let repos: Vec<_> = if let Some(name) = repo_name {
        config
            .get(name)
            .map(|r| vec![r.clone()])
            .ok_or_else(|| CliError::input(format!("Repository '{}' not found", name)))?
    } else {
        config.repositories.clone()
    };

    if repos.is_empty() {
        println!("No repositories configured. Add one with: sherpack repo add <name> <url>");
        return Ok(());
    }

    let mut found_any = false;

    for repo in repos {
        // OCI repos don't support search
        if repo.repo_type == RepositoryType::Oci {
            println!(
                "Note: Search not supported for OCI registry '{}'. Use Artifact Hub for discovery.",
                repo.name
            );
            continue;
        }

        let credentials = cred_store.get(&repo.name).and_then(|c| c.resolve().ok());

        match create_backend(repo.clone(), credentials).await {
            Ok(mut backend) => match backend.search(query).await {
                Ok(results) => {
                    for pack in results {
                        found_any = true;
                        let desc = pack
                            .description
                            .as_deref()
                            .unwrap_or("")
                            .chars()
                            .take(40)
                            .collect::<String>();

                        println!("{}/{}\t{}\t{}", repo.name, pack.name, pack.version, desc);
                    }
                }
                Err(e) => {
                    eprintln!("Search failed for {}: {}", repo.name, e);
                }
            },
            Err(e) => {
                eprintln!("Failed to connect to {}: {}", repo.name, e);
            }
        }
    }

    if !found_any {
        println!("No packs found matching '{}'", query);
        println!();
        println!("Tips:");
        println!("  - Run 'sherpack repo update' to refresh indices");
        println!("  - Try a different search term");
        println!("  - Check https://artifacthub.io for more packages");
    }

    Ok(())
}
