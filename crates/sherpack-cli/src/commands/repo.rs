//! Repository management commands

use std::path::{Path, PathBuf};

use crate::error::{CliError, Result};
use sherpack_repo::{
    CredentialStore, Credentials, IndexCache, IndexDependency, Maintainer, PackEntry, Repository,
    RepositoryConfig, RepositoryIndex, RepositoryType, create_backend,
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

/// Generate a repository index.yaml from a directory of *.tgz packs
///
/// Equivalent to `helm repo index`. Walks `dir` for `*.tgz` archives, extracts
/// `Pack.yaml` from each, computes the SHA256 digest of the archive, and writes
/// `<dir>/index.yaml`.
///
/// - `url`: prepended to each archive filename to form the entry URL.
///   If absent, only the archive filename is used (relative).
/// - `merge`: optional path to an existing index.yaml; entries from `dir`
///   are merged into it (existing same name+version entries are kept,
///   new ones are appended).
pub async fn index(dir: &Path, url: Option<&str>, merge: Option<&Path>) -> Result<()> {
    if !dir.is_dir() {
        return Err(CliError::input(format!(
            "{} is not a directory",
            dir.display()
        )));
    }

    // 1) Discover *.tgz files
    let mut archives: Vec<PathBuf> = Vec::new();
    for entry in std::fs::read_dir(dir).map_err(|e| CliError::internal(e.to_string()))? {
        let entry = entry.map_err(|e| CliError::internal(e.to_string()))?;
        let path = entry.path();
        if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("tgz") {
            archives.push(path);
        }
    }
    archives.sort();

    if archives.is_empty() {
        return Err(CliError::input(format!(
            "No *.tgz archives found in {}",
            dir.display()
        )));
    }

    // 2) Optionally start from an existing index
    let mut index = match merge {
        Some(path) => {
            let yaml = std::fs::read_to_string(path).map_err(|e| {
                CliError::input(format!(
                    "Failed to read merge index {}: {}",
                    path.display(),
                    e
                ))
            })?;
            RepositoryIndex::from_yaml(&yaml)
                .map_err(|e| CliError::input(format!("Invalid merge index: {}", e)))?
        }
        None => RepositoryIndex::default(),
    };

    // 3) Build a set of existing (name, version) so merge doesn't duplicate
    let mut existing: std::collections::HashSet<(String, String)> =
        std::collections::HashSet::new();
    for (name, versions) in &index.entries {
        for v in versions {
            existing.insert((name.clone(), v.version.clone()));
        }
    }

    // 4) Build entries from each archive
    let mut added = 0usize;
    let mut skipped = 0usize;
    for archive in &archives {
        let entry = build_pack_entry(archive, url)?;
        let key = (entry.name.clone(), entry.version.clone());
        if existing.contains(&key) {
            skipped += 1;
            continue;
        }
        existing.insert(key);
        index.add_entry(entry);
        added += 1;
    }

    // 5) Refresh generated timestamp and write
    index.generated = chrono::Utc::now();
    let yaml = serde_yaml::to_string(&index)
        .map_err(|e| CliError::internal(format!("Failed to serialize index: {}", e)))?;
    let out = dir.join("index.yaml");
    std::fs::write(&out, yaml)
        .map_err(|e| CliError::internal(format!("Failed to write {}: {}", out.display(), e)))?;

    println!(
        "Wrote {} ({} new, {} already in merge index, {} archives total)",
        out.display(),
        added,
        skipped,
        archives.len()
    );
    Ok(())
}

/// Construct a PackEntry by reading Pack.yaml from inside the archive and
/// hashing the archive bytes.
fn build_pack_entry(archive: &Path, url_base: Option<&str>) -> Result<PackEntry> {
    use sha2::Digest;

    // Read Pack.yaml from inside the archive
    let pack_yaml_bytes = sherpack_core::read_file_from_archive(archive, "Pack.yaml")
        .map_err(|e| CliError::input(format!("{}: {}", archive.display(), e)))?;
    let pack: sherpack_core::Pack = serde_yaml::from_slice(&pack_yaml_bytes).map_err(|e| {
        CliError::input(format!(
            "Invalid Pack.yaml in {}: {}",
            archive.display(),
            e
        ))
    })?;

    // SHA256 of the archive bytes (Helm-compatible digest)
    let archive_bytes = std::fs::read(archive).map_err(|e| {
        CliError::internal(format!("Failed to read {}: {}", archive.display(), e))
    })?;
    let mut hasher = sha2::Sha256::new();
    hasher.update(&archive_bytes);
    let digest = format!("{:x}", hasher.finalize());

    // URL: <base>/<filename> if base is provided, else bare filename
    let filename = archive
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_string();
    let entry_url = match url_base {
        Some(base) => format!("{}/{}", base.trim_end_matches('/'), filename),
        None => filename,
    };

    let m = &pack.metadata;
    Ok(PackEntry {
        name: m.name.clone(),
        version: m.version.to_string(),
        app_version: m.app_version.clone(),
        description: m.description.clone(),
        home: m.home.clone(),
        icon: m.icon.clone(),
        sources: m.sources.clone(),
        keywords: m.keywords.clone(),
        maintainers: m
            .maintainers
            .iter()
            .map(|mt| Maintainer {
                name: mt.name.clone(),
                email: mt.email.clone(),
                url: mt.url.clone(),
            })
            .collect(),
        urls: vec![entry_url],
        digest: Some(digest),
        created: Some(chrono::Utc::now()),
        deprecated: false,
        dependencies: pack
            .dependencies
            .iter()
            .map(|d| IndexDependency {
                name: d.name.clone(),
                version: d.version.clone(),
                repository: Some(d.repository.clone()),
                condition: d.condition.clone(),
                tags: d.tags.clone(),
                alias: d.alias.clone(),
            })
            .collect(),
        annotations: m.annotations.clone(),
        api_version: Some(pack.api_version.clone()),
        r#type: None,
    })
}
