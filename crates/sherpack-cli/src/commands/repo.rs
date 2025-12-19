//! Repository management commands

use crate::error::{CliError, Result};
use sherpack_repo::{
    CredentialStore, Credentials, IndexCache, Repository, RepositoryConfig, RepositoryType,
    create_backend,
};

/// Add a new repository
pub async fn add(
    name: &str,
    url: &str,
    username: Option<&str>,
    password: Option<&str>,
    token: Option<&str>,
) -> Result<()> {
    let mut config = RepositoryConfig::load().map_err(|e| CliError::internal(e.to_string()))?;

    // Check if repo already exists
    if config.get(name).is_some() {
        return Err(CliError::input(format!(
            "Repository '{}' already exists. Use 'sherpack repo update' to modify it.",
            name
        )));
    }

    // Create repository
    let repo = Repository::new(name, url).map_err(|e| CliError::input(e.to_string()))?;

    // Handle credentials
    if username.is_some() || password.is_some() || token.is_some() {
        let mut cred_store =
            CredentialStore::load().map_err(|e| CliError::internal(e.to_string()))?;

        let creds = if let Some(token) = token {
            Credentials::bearer(token)
        } else if let (Some(user), Some(pass)) = (username, password) {
            Credentials::basic(user, pass)
        } else {
            return Err(CliError::input(
                "Please provide both username and password, or a token",
            ));
        };

        cred_store.set(name, creds);
        cred_store
            .save()
            .map_err(|e| CliError::internal(e.to_string()))?;
        println!("Credentials stored securely");
    }

    config
        .add(repo)
        .map_err(|e| CliError::internal(e.to_string()))?;
    config
        .save()
        .map_err(|e| CliError::internal(e.to_string()))?;

    let repo_type = match RepositoryType::detect(url) {
        Ok(RepositoryType::Http) => "HTTP",
        Ok(RepositoryType::Oci) => "OCI",
        Ok(RepositoryType::File) => "File",
        Err(_) => "Unknown",
    };

    println!(
        "\"{}\" has been added to your repositories ({})",
        name, repo_type
    );
    println!();
    println!("Run 'sherpack repo update {}' to fetch the index", name);

    Ok(())
}

/// List configured repositories
pub async fn list(show_auth: bool) -> Result<()> {
    let config = RepositoryConfig::load().map_err(|e| CliError::internal(e.to_string()))?;

    if config.repositories.is_empty() {
        println!("No repositories configured.");
        println!();
        println!("Add one with: sherpack repo add <name> <url>");
        return Ok(());
    }

    let cred_store = CredentialStore::load().unwrap_or_default();

    println!("{:<20} {:<10} {:<50}", "NAME", "TYPE", "URL");
    println!("{}", "-".repeat(80));

    for repo in &config.repositories {
        let repo_type = match repo.repo_type {
            RepositoryType::Http => "HTTP",
            RepositoryType::Oci => "OCI",
            RepositoryType::File => "File",
        };

        let auth_info = if show_auth {
            if cred_store.has(&repo.name) {
                " (authenticated)"
            } else {
                " (public)"
            }
        } else {
            ""
        };

        println!(
            "{:<20} {:<10} {}{}",
            repo.name, repo_type, repo.url, auth_info
        );
    }

    Ok(())
}

/// Update repository index
pub async fn update(name: Option<&str>) -> Result<()> {
    let config = RepositoryConfig::load().map_err(|e| CliError::internal(e.to_string()))?;
    let cred_store = CredentialStore::load().unwrap_or_default();
    let mut cache = IndexCache::open().map_err(|e| CliError::internal(e.to_string()))?;

    let repos_to_update: Vec<_> = if let Some(name) = name {
        let repo = config
            .get(name)
            .ok_or_else(|| CliError::input(format!("Repository '{}' not found", name)))?;
        vec![repo.clone()]
    } else {
        config.repositories.clone()
    };

    if repos_to_update.is_empty() {
        println!("No repositories to update.");
        return Ok(());
    }

    for repo in &repos_to_update {
        print!("Updating {}... ", repo.name);

        // Get credentials if available
        let credentials = cred_store.get(&repo.name).and_then(|c| c.resolve().ok());

        // Create backend and fetch index
        match create_backend(repo.clone(), credentials).await {
            Ok(mut backend) => match backend.refresh().await {
                Ok(()) => {
                    // For HTTP repos, cache the index
                    if repo.repo_type == RepositoryType::Http {
                        if let Ok(packs) = backend.list().await {
                            cache
                                .upsert_repository(&repo.name, &repo.url, "http", None)
                                .ok();
                            cache.add_packs(&repo.name, &packs).ok();
                            println!("done ({} packs)", packs.len());
                        } else {
                            println!("done (index cached)");
                        }
                    } else {
                        println!("done");
                    }
                }
                Err(e) => {
                    println!("failed");
                    eprintln!("  Error: {}", e);
                }
            },
            Err(e) => {
                println!("failed");
                eprintln!("  Error: {}", e);
            }
        }
    }

    Ok(())
}

/// Remove a repository
pub async fn remove(name: &str) -> Result<()> {
    let mut config = RepositoryConfig::load().map_err(|e| CliError::internal(e.to_string()))?;

    // Remove from config
    config
        .remove(name)
        .map_err(|e| CliError::input(e.to_string()))?;

    config
        .save()
        .map_err(|e| CliError::internal(e.to_string()))?;

    // Remove from cache
    if let Ok(mut cache) = IndexCache::open() {
        cache.remove_repository(name).ok();
    }

    // Remove credentials
    if let Ok(mut cred_store) = CredentialStore::load() {
        cred_store.remove(name);
        cred_store.save().ok();
    }

    println!("\"{}\" has been removed from your repositories", name);
    Ok(())
}
