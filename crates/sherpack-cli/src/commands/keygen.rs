//! Keygen command - generate signing keys

use console::style;
use miette::{IntoDiagnostic, Result};
use minisign::KeyPair;
use std::path::Path;

/// Default directory for Sherpack keys
#[must_use]
pub fn default_key_dir() -> std::path::PathBuf {
    dirs::home_dir()
        .map(|h| h.join(".sherpack"))
        .unwrap_or_else(|| std::path::PathBuf::from(".sherpack"))
}

pub fn run(output_dir: Option<&Path>, force: bool, no_password: bool) -> Result<()> {
    let key_dir = output_dir
        .map(|p| p.to_path_buf())
        .unwrap_or_else(default_key_dir);

    let secret_key_path = key_dir.join("sherpack.key");
    let public_key_path = key_dir.join("sherpack.pub");

    // Check if keys already exist
    if !force && (secret_key_path.exists() || public_key_path.exists()) {
        return Err(miette::miette!(
            "Keys already exist at {}. Use --force to overwrite.",
            key_dir.display()
        ));
    }

    // Create output directory
    std::fs::create_dir_all(&key_dir).into_diagnostic()?;

    println!("{}", style("Generating signing keys...").cyan().bold());
    println!();

    // Get password (unless --no-password)
    let password: Option<String> = if no_password {
        None
    } else {
        let password = rpassword::prompt_password("Enter password to protect secret key (leave empty for no password): ")
            .into_diagnostic()?;

        if password.is_empty() {
            None
        } else {
            // Confirm password
            let confirm = rpassword::prompt_password("Confirm password: ")
                .into_diagnostic()?;

            if password != confirm {
                return Err(miette::miette!("Passwords do not match"));
            }

            Some(password)
        }
    };

    // Generate key pair
    // Note: minisign prompts interactively if None is passed, so we use empty string for no password
    let password_for_gen = if password.is_some() { password.clone() } else { Some(String::new()) };
    let KeyPair { pk, sk } = KeyPair::generate_encrypted_keypair(password_for_gen)
        .map_err(|e| miette::miette!("Failed to generate key pair: {}", e))?;

    // Create key boxes with comments
    let pk_box = pk.to_box()
        .map_err(|e| miette::miette!("Failed to create public key box: {}", e))?;

    let sk_box = sk.to_box(password.as_deref())
        .map_err(|e| miette::miette!("Failed to create secret key box: {}", e))?;

    // Write keys
    std::fs::write(&public_key_path, pk_box.to_string()).into_diagnostic()?;
    std::fs::write(&secret_key_path, sk_box.to_string()).into_diagnostic()?;

    // Set restrictive permissions on secret key (Unix only)
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&secret_key_path)
            .into_diagnostic()?
            .permissions();
        perms.set_mode(0o600);
        std::fs::set_permissions(&secret_key_path, perms).into_diagnostic()?;
    }

    println!("  {} {}", style("Secret key").green().bold(), secret_key_path.display());
    println!("  {} {}", style("Public key").green().bold(), public_key_path.display());
    println!();

    if password.is_some() {
        println!(
            "{}",
            style("Secret key is password-protected.").dim()
        );
    } else {
        println!(
            "{}",
            style("Warning: Secret key is NOT password-protected.").yellow()
        );
    }

    println!();
    println!("{}:", style("To sign a package").bold());
    println!("  sherpack sign mypack-1.0.0.tar.gz");
    println!();
    println!("{}:", style("To share your public key").bold());
    println!("  cat {}", public_key_path.display());

    Ok(())
}
