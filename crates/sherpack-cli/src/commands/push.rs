//! Push command - push a pack to an OCI registry

use std::path::Path;

use crate::error::{CliError, Result};
use sherpack_repo::{CredentialStore, OciRegistry, Repository};

/// Push a pack archive to an OCI registry
pub async fn run(archive: &Path, destination: &str) -> Result<()> {
    // Validate archive exists
    if !archive.exists() {
        return Err(CliError::input(format!(
            "Archive not found: {}",
            archive.display()
        )));
    }

    // Parse destination
    if !destination.starts_with("oci://") {
        return Err(CliError::input(
            "Destination must be an OCI reference (oci://registry/repo:tag)",
        ));
    }

    let (base_url, name, tag) = parse_oci_destination(destination)?;

    println!("Pushing {} to {}...", archive.display(), destination);

    // Read archive
    let data = std::fs::read(archive)?;

    // Get credentials
    let cred_store = CredentialStore::load().unwrap_or_default();

    // Extract registry from URL for credential lookup
    let registry = base_url
        .trim_start_matches("oci://")
        .split('/')
        .next()
        .unwrap_or("");

    let credentials = cred_store.get(registry).and_then(|c| c.resolve().ok());

    // Create OCI client
    let repo = Repository::new("_push", &base_url).map_err(|e| CliError::input(e.to_string()))?;

    let oci = OciRegistry::new(repo, credentials).map_err(|e| CliError::internal(e.to_string()))?;

    // Push
    let manifest_url = oci
        .push(&name, &tag, &data)
        .await
        .map_err(|e| CliError::internal(e.to_string()))?;

    println!("Successfully pushed!");
    println!("  Manifest: {}", manifest_url);
    println!();
    println!("To install: sherpack install <name> {}", destination);

    Ok(())
}

fn parse_oci_destination(dest: &str) -> Result<(String, String, String)> {
    // oci://registry/path/name:tag
    let without_prefix = dest.trim_start_matches("oci://");

    let (path_with_name, tag) = without_prefix
        .rsplit_once(':')
        .ok_or_else(|| CliError::input("OCI reference must include a tag (e.g., :1.0.0)"))?;

    let (path, name) = path_with_name
        .rsplit_once('/')
        .ok_or_else(|| CliError::input("Invalid OCI reference format"))?;

    let base_url = format!("oci://{}", path);

    Ok((base_url, name.to_string(), tag.to_string()))
}
