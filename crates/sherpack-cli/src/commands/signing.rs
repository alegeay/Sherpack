//! Shared signing utilities for package and sign commands

use console::style;
use miette::{IntoDiagnostic, Result};
use minisign::{SecretKey, SecretKeyBox};
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};

/// Load a minisign secret key from a file
///
/// minisign 0.8: Try unencrypted first, then prompt for password if encrypted.
pub fn load_secret_key(key_path: &Path) -> Result<SecretKey> {
    let key_content = std::fs::read_to_string(key_path).into_diagnostic()?;

    let sk_box = SecretKeyBox::from_string(&key_content)
        .map_err(|e| miette::miette!("Failed to parse secret key: {}", e))?;

    // Try loading as unencrypted key first
    if let Ok(sk) = sk_box.clone().into_unencrypted_secret_key() {
        return Ok(sk);
    }

    // Key is encrypted, prompt for password
    let password = rpassword::prompt_password("Enter key password: ").into_diagnostic()?;
    sk_box
        .into_secret_key(Some(password))
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

#[cfg(test)]
mod tests {
    use super::*;
    use minisign::KeyPair;
    use std::io::Cursor;

    #[test]
    fn test_minisign_unencrypted_roundtrip() {
        // 1. Generate unencrypted keypair (for --no-password case)
        println!("1. Generating unencrypted keypair...");
        let KeyPair { pk: _, sk } =
            KeyPair::generate_unencrypted_keypair().expect("Failed to generate keypair");

        // 2. Create key box
        println!("2. Creating key box...");
        let sk_box = sk.to_box(Some("test")).expect("sk box");
        let sk_str = sk_box.to_string();
        println!(
            "Key string (first 100 chars): {}",
            &sk_str[..sk_str.len().min(100)]
        );

        // 3. Parse - use into_unencrypted_secret_key for unencrypted keys
        println!("3. Parsing secret key...");
        let parsed_sk_box = SecretKeyBox::from_string(&sk_str).expect("parse sk");
        let decrypted_sk = parsed_sk_box
            .into_unencrypted_secret_key()
            .expect("load unencrypted sk");

        // 4. Sign
        println!("4. Signing...");
        let data = b"test data";
        let mut cursor = Cursor::new(data.as_slice());
        let sig =
            minisign::sign(None, &decrypted_sk, &mut cursor, Some("comment"), None).expect("sign");

        println!("5. Success! Signature:\n{}", sig.to_string());
        assert!(sig.to_string().contains("untrusted comment"));
    }

    #[test]
    fn test_minisign_encrypted_roundtrip() {
        // 1. Generate encrypted keypair with actual password
        println!("1. Generating encrypted keypair...");
        let password = "test_password".to_string();
        let KeyPair { pk: _, sk } = KeyPair::generate_encrypted_keypair(Some(password.clone()))
            .expect("Failed to generate keypair");

        // 2. Create key box
        println!("2. Creating key box...");
        let sk_box = sk.to_box(Some("test")).expect("sk box");
        let sk_str = sk_box.to_string();
        println!(
            "Key string (first 100 chars): {}",
            &sk_str[..sk_str.len().min(100)]
        );

        // 3. Parse and decrypt with same password
        println!("3. Parsing and decrypting...");
        let parsed_sk_box = SecretKeyBox::from_string(&sk_str).expect("parse sk");
        let decrypted_sk = parsed_sk_box
            .into_secret_key(Some(password))
            .expect("decrypt sk");

        // 4. Sign
        println!("4. Signing...");
        let data = b"test data";
        let mut cursor = Cursor::new(data.as_slice());
        let sig =
            minisign::sign(None, &decrypted_sk, &mut cursor, Some("comment"), None).expect("sign");

        println!("5. Success! Signature:\n{}", sig.to_string());
        assert!(sig.to_string().contains("untrusted comment"));
    }
}
