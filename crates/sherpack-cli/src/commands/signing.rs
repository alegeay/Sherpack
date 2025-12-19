//! Shared signing utilities for package and sign commands

use console::style;
use miette::{IntoDiagnostic, Result};
use minisign::{SecretKey, SecretKeyBox};
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};

/// Load a minisign secret key from a file
///
/// Tries empty password first (for unencrypted keys), then prompts for password.
pub fn load_secret_key(key_path: &Path) -> Result<SecretKey> {
    let key_content = std::fs::read_to_string(key_path).into_diagnostic()?;

    // Try with empty password first (unencrypted keys)
    if let Ok(sk) = try_decrypt(&key_content, Some(String::new())) {
        return Ok(sk);
    }

    // Key is encrypted, prompt for password
    let password = rpassword::prompt_password("Enter key password: ").into_diagnostic()?;
    try_decrypt(&key_content, Some(password))
}

/// Try to decrypt a secret key with a given password
fn try_decrypt(content: &str, password: Option<String>) -> Result<SecretKey> {
    let sk_box = SecretKeyBox::from_string(content)
        .map_err(|e| miette::miette!("Failed to parse secret key: {}", e))?;
    sk_box
        .into_secret_key(password)
        .map_err(|e| miette::miette!("Failed to decrypt key: {}", e))
}

/// Sign an archive file and create a .minisig signature file
///
/// Returns the path to the created signature file.
pub fn sign_archive(
    archive_path: &Path,
    key_path: &Path,
    trusted_comment: Option<&str>,
) -> Result<PathBuf> {
    println!();
    println!("{} archive...", style("Signing").cyan().bold());

    // Load the secret key
    let sk = load_secret_key(key_path)?;

    // Read archive content
    let mut file = std::fs::File::open(archive_path).into_diagnostic()?;
    let mut data = Vec::new();
    file.read_to_end(&mut data).into_diagnostic()?;

    // Build trusted comment from manifest if not provided
    let comment = match trusted_comment {
        Some(c) => c.to_string(),
        None => build_default_comment(archive_path)?,
    };

    // Sign using Cursor for Read trait
    let mut cursor = Cursor::new(&data);
    let signature_box = minisign::sign(
        None, // public key (not included in signature)
        &sk,
        &mut cursor,
        Some(&comment),
        None, // untrusted comment
    )
    .map_err(|e| miette::miette!("Failed to sign: {}", e))?;

    // Write signature file
    let sig_path = PathBuf::from(format!("{}.minisig", archive_path.display()));
    std::fs::write(&sig_path, signature_box.to_string()).into_diagnostic()?;

    println!(
        "  {} {}",
        style("Created").green().bold(),
        sig_path.display()
    );

    Ok(sig_path)
}

/// Build a default trusted comment from the archive manifest
fn build_default_comment(archive_path: &Path) -> Result<String> {
    match sherpack_core::read_manifest_from_archive(archive_path) {
        Ok(manifest) => {
            let digest_preview = crate::util::truncate_hash(&manifest.digest, 16);
            Ok(format!(
                "sherpack:{} v{} digest:{}",
                manifest.name, manifest.pack_version, digest_preview
            ))
        }
        Err(_) => {
            // Not a sherpack archive, use filename
            let filename = archive_path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "unknown".to_string());
            Ok(format!("file:{}", filename))
        }
    }
}
