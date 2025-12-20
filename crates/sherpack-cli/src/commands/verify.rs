//! Verify command - verify archive integrity and signature

use console::style;
use miette::{IntoDiagnostic, Result};
use minisign::{PublicKeyBox, SignatureBox};
use std::io::{Cursor, Read};
use std::path::Path;

use super::keygen::default_key_dir;
use crate::util::truncate_hash;

pub fn run(archive_path: &Path, key_path: Option<&Path>, require_signature: bool) -> Result<()> {
    if !archive_path.exists() {
        return Err(miette::miette!(
            "Archive not found: {}",
            archive_path.display()
        ));
    }

    println!(
        "{} {}",
        style("Verifying").cyan().bold(),
        archive_path.display()
    );
    println!();

    // Step 1: Verify manifest checksums
    println!("{}:", style("Integrity check").bold());

    let verification_result = sherpack_core::verify_archive(archive_path).into_diagnostic()?;

    if verification_result.valid {
        println!(
            "  {} All file checksums match",
            style("[OK]").green().bold()
        );
    } else {
        println!(
            "  {} Checksum verification failed",
            style("[FAIL]").red().bold()
        );

        for mismatch in &verification_result.mismatched {
            println!(
                "    {} {}: expected {}, got {}",
                style("-").red(),
                mismatch.path,
                truncate_hash(&mismatch.expected, 16),
                truncate_hash(&mismatch.actual, 16)
            );
        }

        for missing in &verification_result.missing {
            println!("    {} {}: missing from archive", style("-").red(), missing);
        }

        return Err(miette::miette!("Archive integrity check failed"));
    }

    // Step 2: Check signature (if present or required)
    let sig_path = format!("{}.minisig", archive_path.display());
    let sig_exists = Path::new(&sig_path).exists();

    println!();
    println!("{}:", style("Signature check").bold());

    if !sig_exists {
        if require_signature {
            println!("  {} No signature found", style("[FAIL]").red().bold());
            return Err(miette::miette!(
                "Signature required but not found: {}",
                sig_path
            ));
        } else {
            println!(
                "  {} No signature file ({})",
                style("[SKIP]").yellow().bold(),
                sig_path
            );
            println!();
            println!(
                "{}",
                style("Archive integrity verified (no signature).").green()
            );
            return Ok(());
        }
    }

    // Find public key
    let key_path = key_path
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| default_key_dir().join("sherpack.pub"));

    if !key_path.exists() {
        println!(
            "  {} Public key not found at {}",
            style("[FAIL]").red().bold(),
            key_path.display()
        );
        return Err(miette::miette!(
            "Public key required for signature verification.\n\
             Use --key to specify a public key file."
        ));
    }

    // Read public key
    let pk_content = std::fs::read_to_string(&key_path).into_diagnostic()?;
    let pk_box = PublicKeyBox::from_string(&pk_content)
        .map_err(|e| miette::miette!("Failed to parse public key: {}", e))?;
    let pk = pk_box
        .into_public_key()
        .map_err(|e| miette::miette!("Invalid public key: {}", e))?;

    // Read signature
    let sig_content = std::fs::read_to_string(&sig_path).into_diagnostic()?;
    let sig_box = SignatureBox::from_string(&sig_content)
        .map_err(|e| miette::miette!("Failed to parse signature: {}", e))?;

    // Read archive data
    let mut file = std::fs::File::open(archive_path).into_diagnostic()?;
    let mut data = Vec::new();
    file.read_to_end(&mut data).into_diagnostic()?;

    // Verify signature using Cursor for Read + Seek
    let mut cursor = Cursor::new(&data);
    match minisign::verify(&pk, &sig_box, &mut cursor, true, false, false) {
        Ok(()) => {
            println!("  {} Signature valid", style("[OK]").green().bold());

            // Show trusted comment if available
            if let Ok(trusted_comment) = sig_box.trusted_comment() {
                println!("  {}: {}", style("Signed by").dim(), trusted_comment);
            }
        }
        Err(e) => {
            println!(
                "  {} Signature verification failed: {}",
                style("[FAIL]").red().bold(),
                e
            );
            return Err(miette::miette!("Signature verification failed"));
        }
    }

    println!();
    println!("{}", style("Archive verified successfully.").green().bold());

    Ok(())
}
