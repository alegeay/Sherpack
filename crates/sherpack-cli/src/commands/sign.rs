//! Sign command - sign an archive with minisign

use console::style;
use miette::Result;
use std::path::Path;

use super::keygen::default_key_dir;
use super::signing::sign_archive;

pub fn run(
    archive_path: &Path,
    key_path: Option<&Path>,
    comment: Option<&str>,
) -> Result<()> {
    // Determine key path
    let key_path = key_path
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| default_key_dir().join("sherpack.key"));

    if !key_path.exists() {
        return Err(miette::miette!(
            "Secret key not found at {}.\nRun 'sherpack keygen' to generate keys.",
            key_path.display()
        ));
    }

    if !archive_path.exists() {
        return Err(miette::miette!(
            "Archive not found: {}",
            archive_path.display()
        ));
    }

    println!("{} {}...", style("Signing").cyan().bold(), archive_path.display());

    // Sign the archive
    let sig_path = sign_archive(archive_path, &key_path, comment)?;

    // Show trusted comment if we can read the signature
    if let Ok(sig_content) = std::fs::read_to_string(&sig_path) {
        if let Ok(sig_box) = minisign::SignatureBox::from_string(&sig_content) {
            if let Ok(trusted_comment) = sig_box.trusted_comment() {
                println!();
                println!(
                    "{}: {}",
                    style("Trusted comment").dim(),
                    trusted_comment
                );
            }
        }
    }

    Ok(())
}
