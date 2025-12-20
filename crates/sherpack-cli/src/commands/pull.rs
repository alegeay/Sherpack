//! Pull command - download a pack from a repository

use std::path::PathBuf;

use crate::error::{CliError, Result};
use sherpack_repo::{CredentialStore, IndexCache, Repository, RepositoryConfig, create_backend};

/// Pull a pack from a repository
pub async fn run(
    pack_ref: &str,
    version: Option<&str>,
    output: Option<&PathBuf>,
    untar: bool,
) -> Result<()> {
    // Parse pack reference: [repo/]name[:version] or oci://registry/repo:tag
    let (repo_name, pack_name, pack_version) = parse_pack_ref(pack_ref, version)?;

    let config = RepositoryConfig::load().map_err(|e| CliError::internal(e.to_string()))?;
    let cred_store = CredentialStore::load().unwrap_or_default();

    // Get repository
    let repo = if pack_ref.starts_with("oci://") {
        // Direct OCI reference
        Repository::new("_oci", pack_ref.split(':').next().unwrap_or(pack_ref))
            .map_err(|e| CliError::input(e.to_string()))?
    } else if let Some(repo_name) = repo_name {
        config
            .get(&repo_name)
            .cloned()
            .ok_or_else(|| CliError::input(format!("Repository '{}' not found", repo_name)))?
    } else {
        // Try to find pack in any repo
        let cache = IndexCache::open().map_err(|e| CliError::internal(e.to_string()))?;
        let results = cache
            .search(&pack_name)
            .map_err(|e| CliError::internal(e.to_string()))?;

        let found = results.iter().find(|p| p.name == pack_name);

        if let Some(pack) = found {
            config
                .get(&pack.repo_name)
                .cloned()
                .ok_or_else(|| CliError::input("Pack found but repository not configured"))?
        } else {
            return Err(CliError::input(format!(
                "Pack '{}' not found. Specify a repository: sherpack pull <repo>/{}",
                pack_name, pack_name
            )));
        }
    };

    let credentials = cred_store.get(&repo.name).and_then(|c| c.resolve().ok());

    let mut backend = create_backend(repo.clone(), credentials)
        .await
        .map_err(|e| CliError::internal(e.to_string()))?;

    // Get pack info
    let pack_entry = if let Some(version) = &pack_version {
        backend
            .get_version(&pack_name, version)
            .await
            .map_err(|e| CliError::internal(e.to_string()))?
    } else {
        backend
            .get_latest(&pack_name)
            .await
            .map_err(|e| CliError::internal(e.to_string()))?
    };

    println!(
        "Pulling {}/{}:{}...",
        repo.name, pack_entry.name, pack_entry.version
    );

    // Download
    let data = backend
        .download(&pack_entry.name, &pack_entry.version)
        .await
        .map_err(|e| CliError::internal(e.to_string()))?;

    // Save to file
    let output_path = if let Some(output) = output {
        output.clone()
    } else if untar {
        PathBuf::from(&pack_entry.name)
    } else {
        PathBuf::from(format!("{}-{}.tgz", pack_entry.name, pack_entry.version))
    };

    if untar {
        // Extract to directory
        std::fs::create_dir_all(&output_path)?;
        extract_archive(&data, &output_path)?;
        println!("Extracted to {}/", output_path.display());
    } else {
        // Save archive
        std::fs::write(&output_path, &data)?;
        println!("Saved to {}", output_path.display());
    }

    Ok(())
}

/// Parse pack reference into (repo, name, version)
fn parse_pack_ref(
    pack_ref: &str,
    version_flag: Option<&str>,
) -> Result<(Option<String>, String, Option<String>)> {
    // Handle OCI reference
    if pack_ref.starts_with("oci://") {
        let without_prefix = pack_ref.trim_start_matches("oci://");
        let (path, tag) = if let Some((p, t)) = without_prefix.rsplit_once(':') {
            (p, Some(t.to_string()))
        } else {
            (without_prefix, None)
        };

        let name = path.rsplit('/').next().unwrap_or(path).to_string();
        return Ok((None, name, tag.or_else(|| version_flag.map(String::from))));
    }

    // Handle repo/name:version format
    if let Some((repo, rest)) = pack_ref.split_once('/') {
        let (name, version) = if let Some((n, v)) = rest.rsplit_once(':') {
            (n.to_string(), Some(v.to_string()))
        } else {
            (rest.to_string(), None)
        };
        return Ok((
            Some(repo.to_string()),
            name,
            version.or_else(|| version_flag.map(String::from)),
        ));
    }

    // Handle name:version format
    if let Some((name, version)) = pack_ref.rsplit_once(':') {
        return Ok((None, name.to_string(), Some(version.to_string())));
    }

    // Just name
    Ok((None, pack_ref.to_string(), version_flag.map(String::from)))
}

fn extract_archive(data: &[u8], dest: &PathBuf) -> Result<()> {
    use flate2::read::GzDecoder;
    use tar::Archive;

    let gz = GzDecoder::new(std::io::Cursor::new(data));
    let mut archive = Archive::new(gz);

    std::fs::create_dir_all(dest)?;
    archive.unpack(dest)?;

    Ok(())
}
