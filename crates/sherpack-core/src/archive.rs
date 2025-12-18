//! Archive creation and extraction for Sherpack packages
//!
//! Provides functionality to create and extract `.tar.gz` archives
//! with the standard Sherpack archive structure.

use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use flate2::Compression;
use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use tar::{Archive, Builder, Header};

use crate::error::{CoreError, Result};
use crate::manifest::Manifest;
use crate::pack::LoadedPack;

/// Create a tar.gz archive from a loaded pack
///
/// Returns the path to the created archive file.
/// The archive includes:
/// - MANIFEST (generated)
/// - Pack.yaml
/// - values.yaml
/// - values.schema.yaml (if present)
/// - templates/* (all template files)
pub fn create_archive(pack: &LoadedPack, output: &Path) -> Result<PathBuf> {
    // Generate manifest
    let manifest = Manifest::generate(pack)?;
    let manifest_content = manifest.to_string();

    // Create output file
    let file = File::create(output)?;
    let encoder = GzEncoder::new(file, Compression::default());
    let mut builder = Builder::new(encoder);

    // Add MANIFEST first
    add_bytes_to_archive(&mut builder, "MANIFEST", manifest_content.as_bytes())?;

    // Add Pack.yaml
    let pack_yaml = pack.root.join("Pack.yaml");
    if pack_yaml.exists() {
        add_file_to_archive(&mut builder, &pack_yaml, "Pack.yaml")?;
    }

    // Add values.yaml
    if pack.values_path.exists() {
        add_file_to_archive(&mut builder, &pack.values_path, "values.yaml")?;
    }

    // Add schema file if present
    if let Some(schema_path) = &pack.schema_path {
        if schema_path.exists() {
            let schema_name = schema_path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "values.schema.yaml".to_string());
            add_file_to_archive(&mut builder, schema_path, &schema_name)?;
        }
    }

    // Add template files
    let template_files = pack.template_files()?;
    for file_path in template_files {
        let rel_path = file_path
            .strip_prefix(&pack.root)
            .unwrap_or(&file_path)
            .to_string_lossy()
            .to_string();
        add_file_to_archive(&mut builder, &file_path, &rel_path)?;
    }

    // Finish the archive
    let encoder = builder.into_inner()?;
    encoder.finish()?;

    Ok(output.to_path_buf())
}

/// Extract an archive to a destination directory
pub fn extract_archive(archive_path: &Path, dest: &Path) -> Result<()> {
    let file = File::open(archive_path)?;
    let decoder = GzDecoder::new(file);
    let mut archive = Archive::new(decoder);

    // Create destination directory if it doesn't exist
    std::fs::create_dir_all(dest)?;

    archive.unpack(dest)?;

    Ok(())
}

/// List files in an archive
pub fn list_archive(archive_path: &Path) -> Result<Vec<ArchiveEntry>> {
    let file = File::open(archive_path)?;
    let decoder = GzDecoder::new(file);
    let mut archive = Archive::new(decoder);

    let mut entries = Vec::new();

    for entry in archive.entries()? {
        let entry = entry?;
        let path = entry.path()?.to_string_lossy().to_string();
        let size = entry.header().size()?;
        let is_dir = entry.header().entry_type().is_dir();

        entries.push(ArchiveEntry {
            path,
            size,
            is_dir,
        });
    }

    Ok(entries)
}

/// Read a specific file from an archive
pub fn read_file_from_archive(archive_path: &Path, file_path: &str) -> Result<Vec<u8>> {
    let file = File::open(archive_path)?;
    let decoder = GzDecoder::new(file);
    let mut archive = Archive::new(decoder);

    for entry in archive.entries()? {
        let mut entry = entry?;
        let path = entry.path()?.to_string_lossy().to_string();

        if path == file_path {
            let mut content = Vec::new();
            entry.read_to_end(&mut content)?;
            return Ok(content);
        }
    }

    Err(CoreError::Archive {
        message: format!("File not found in archive: {}", file_path),
    })
}

/// Read the MANIFEST from an archive
pub fn read_manifest_from_archive(archive_path: &Path) -> Result<Manifest> {
    let content = read_file_from_archive(archive_path, "MANIFEST")?;
    let text = String::from_utf8(content).map_err(|e| CoreError::Archive {
        message: format!("Invalid UTF-8 in MANIFEST: {}", e),
    })?;
    Manifest::parse(&text)
}

/// Read all files from an archive in a single pass
///
/// Returns a HashMap mapping file paths to their contents.
/// This is more efficient than multiple calls to `read_file_from_archive`.
fn read_all_files_from_archive(archive_path: &Path) -> Result<HashMap<String, Vec<u8>>> {
    let file = File::open(archive_path)?;
    let decoder = GzDecoder::new(file);
    let mut archive = Archive::new(decoder);
    let mut contents = HashMap::new();

    for entry in archive.entries()? {
        let mut entry = entry?;
        if entry.header().entry_type().is_dir() {
            continue;
        }

        let path = entry.path()?.to_string_lossy().to_string();
        let mut data = Vec::new();
        entry.read_to_end(&mut data)?;
        contents.insert(path, data);
    }

    Ok(contents)
}

/// Verify archive integrity by checking all file checksums
///
/// Uses single-pass reading for O(n) performance instead of O(n²).
pub fn verify_archive(archive_path: &Path) -> Result<crate::manifest::VerificationResult> {
    let manifest = read_manifest_from_archive(archive_path)?;

    // Read all files in a single pass for O(n) performance
    let file_contents = read_all_files_from_archive(archive_path)?;

    manifest.verify_files(|path| {
        file_contents
            .get(path)
            .cloned()
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "file not found"))
    })
}

/// Information about a file in an archive
#[derive(Debug, Clone)]
pub struct ArchiveEntry {
    /// Relative path within the archive
    pub path: String,
    /// File size in bytes
    pub size: u64,
    /// Whether this is a directory
    pub is_dir: bool,
}

/// Add a file to a tar archive
fn add_file_to_archive<W: Write>(
    builder: &mut Builder<W>,
    file_path: &Path,
    archive_path: &str,
) -> Result<()> {
    let content = std::fs::read(file_path)?;
    add_bytes_to_archive(builder, archive_path, &content)
}

/// Add bytes to a tar archive with a given path
fn add_bytes_to_archive<W: Write>(
    builder: &mut Builder<W>,
    archive_path: &str,
    content: &[u8],
) -> Result<()> {
    let mut header = Header::new_gnu();
    header.set_size(content.len() as u64);
    header.set_mode(0o644);
    header.set_mtime(0); // Reproducible builds: use epoch time
    header.set_cksum();

    builder.append_data(&mut header, archive_path, content)?;

    Ok(())
}

/// Generate the default archive filename for a pack
#[must_use]
pub fn default_archive_name(pack: &LoadedPack) -> String {
    format!(
        "{}-{}.tar.gz",
        pack.pack.metadata.name, pack.pack.metadata.version
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_pack(dir: &Path) {
        // Create Pack.yaml
        std::fs::write(
            dir.join("Pack.yaml"),
            r#"apiVersion: sherpack/v1
kind: application
metadata:
  name: testpack
  version: 1.0.0
"#,
        )
        .unwrap();

        // Create values.yaml
        std::fs::write(dir.join("values.yaml"), "replicas: 3\n").unwrap();

        // Create templates directory
        let templates_dir = dir.join("templates");
        std::fs::create_dir_all(&templates_dir).unwrap();

        // Create a template file
        std::fs::write(
            templates_dir.join("deployment.yaml"),
            "apiVersion: apps/v1\nkind: Deployment\n",
        )
        .unwrap();
    }

    #[test]
    fn test_create_and_extract_archive() {
        let temp = TempDir::new().unwrap();
        let pack_dir = temp.path().join("pack");
        std::fs::create_dir_all(&pack_dir).unwrap();
        create_test_pack(&pack_dir);

        // Load pack
        let pack = LoadedPack::load(&pack_dir).unwrap();

        // Create archive
        let archive_path = temp.path().join("test.tar.gz");
        create_archive(&pack, &archive_path).unwrap();

        assert!(archive_path.exists());

        // List archive contents
        let entries = list_archive(&archive_path).unwrap();
        let paths: Vec<_> = entries.iter().map(|e| e.path.as_str()).collect();

        assert!(paths.contains(&"MANIFEST"));
        assert!(paths.contains(&"Pack.yaml"));
        assert!(paths.contains(&"values.yaml"));
        assert!(paths.iter().any(|p| p.contains("deployment.yaml")));

        // Extract archive
        let extract_dir = temp.path().join("extracted");
        extract_archive(&archive_path, &extract_dir).unwrap();

        assert!(extract_dir.join("MANIFEST").exists());
        assert!(extract_dir.join("Pack.yaml").exists());
        assert!(extract_dir.join("values.yaml").exists());
    }

    #[test]
    fn test_read_manifest_from_archive() {
        let temp = TempDir::new().unwrap();
        let pack_dir = temp.path().join("pack");
        std::fs::create_dir_all(&pack_dir).unwrap();
        create_test_pack(&pack_dir);

        let pack = LoadedPack::load(&pack_dir).unwrap();
        let archive_path = temp.path().join("test.tar.gz");
        create_archive(&pack, &archive_path).unwrap();

        // Read manifest
        let manifest = read_manifest_from_archive(&archive_path).unwrap();
        assert_eq!(manifest.name, "testpack");
        assert_eq!(manifest.pack_version.to_string(), "1.0.0");
    }

    #[test]
    fn test_verify_archive() {
        let temp = TempDir::new().unwrap();
        let pack_dir = temp.path().join("pack");
        std::fs::create_dir_all(&pack_dir).unwrap();
        create_test_pack(&pack_dir);

        let pack = LoadedPack::load(&pack_dir).unwrap();
        let archive_path = temp.path().join("test.tar.gz");
        create_archive(&pack, &archive_path).unwrap();

        // Verify archive
        let result = verify_archive(&archive_path).unwrap();
        assert!(result.valid);
        assert!(result.mismatched.is_empty());
        assert!(result.missing.is_empty());
    }

    #[test]
    fn test_default_archive_name() {
        let temp = TempDir::new().unwrap();
        create_test_pack(temp.path());

        let pack = LoadedPack::load(temp.path()).unwrap();
        let name = default_archive_name(&pack);

        assert_eq!(name, "testpack-1.0.0.tar.gz");
    }
}
