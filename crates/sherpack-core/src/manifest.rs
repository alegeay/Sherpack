//! Package manifest for archive integrity verification
//!
//! The MANIFEST file is a text file included in every Sherpack archive that provides:
//! - Package metadata (name, version, creation timestamp)
//! - SHA256 checksums for all files
//! - Overall archive digest for quick integrity verification

use chrono::{DateTime, Utc};
use semver::Version;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::io::{BufReader, Read};
use std::path::Path;

use crate::error::{CoreError, Result};
use crate::pack::LoadedPack;

/// Current manifest format version
pub const MANIFEST_VERSION: u32 = 1;

/// A file entry in the manifest
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileEntry {
    /// Relative path within the archive
    pub path: String,
    /// SHA256 hash of the file contents
    pub sha256: String,
}

/// Package manifest containing checksums and metadata
#[derive(Debug, Clone)]
pub struct Manifest {
    /// Manifest format version
    pub version: u32,
    /// Pack name
    pub name: String,
    /// Pack version
    pub pack_version: Version,
    /// Creation timestamp
    pub created: DateTime<Utc>,
    /// Files and their checksums (sorted by path)
    pub files: Vec<FileEntry>,
    /// Overall digest of all file checksums
    pub digest: String,
}

impl std::fmt::Display for Manifest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Header section
        writeln!(f, "sherpack-manifest-version: {}", self.version)?;
        writeln!(f, "name: {}", self.name)?;
        writeln!(f, "version: {}", self.pack_version)?;
        writeln!(f, "created: {}", self.created.to_rfc3339())?;
        writeln!(f)?;

        // Files section
        writeln!(f, "[files]")?;
        for entry in &self.files {
            writeln!(f, "{} sha256:{}", entry.path, entry.sha256)?;
        }
        writeln!(f)?;

        // Digest section
        writeln!(f, "[digest]")?;
        write!(f, "sha256:{}", self.digest)
    }
}

impl Manifest {
    /// Generate a manifest from a loaded pack
    pub fn generate(pack: &LoadedPack) -> Result<Self> {
        let mut files = BTreeMap::new();

        // Add Pack.yaml
        let pack_yaml_path = pack.root.join("Pack.yaml");
        if pack_yaml_path.exists() {
            let hash = hash_file(&pack_yaml_path)?;
            files.insert("Pack.yaml".to_string(), hash);
        }

        // Add values.yaml
        if pack.values_path.exists() {
            let hash = hash_file(&pack.values_path)?;
            files.insert("values.yaml".to_string(), hash);
        }

        // Add schema file if present
        if let Some(schema_path) = &pack.schema_path {
            if schema_path.exists() {
                let hash = hash_file(schema_path)?;
                let rel_path = schema_path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| "values.schema.yaml".to_string());
                files.insert(rel_path, hash);
            }
        }

        // Add template files
        let template_files = pack.template_files()?;
        for file_path in template_files {
            let hash = hash_file(&file_path)?;
            let rel_path = file_path
                .strip_prefix(&pack.root)
                .unwrap_or(&file_path)
                .to_string_lossy()
                .to_string();
            files.insert(rel_path, hash);
        }

        // Convert to FileEntry vec (already sorted by BTreeMap)
        let file_entries: Vec<FileEntry> = files
            .into_iter()
            .map(|(path, sha256)| FileEntry { path, sha256 })
            .collect();

        // Calculate overall digest from all file hashes
        let digest = calculate_digest(&file_entries);

        Ok(Self {
            version: MANIFEST_VERSION,
            name: pack.pack.metadata.name.clone(),
            pack_version: pack.pack.metadata.version.clone(),
            created: Utc::now(),
            files: file_entries,
            digest,
        })
    }

    /// Parse a manifest from its text representation
    pub fn parse(content: &str) -> Result<Self> {
        let mut version: Option<u32> = None;
        let mut name: Option<String> = None;
        let mut pack_version: Option<Version> = None;
        let mut created: Option<DateTime<Utc>> = None;
        let mut files = Vec::new();
        let mut digest: Option<String> = None;

        let mut in_files_section = false;
        let mut in_digest_section = false;

        for line in content.lines() {
            let line = line.trim();

            // Skip empty lines
            if line.is_empty() {
                continue;
            }

            // Section headers
            if line == "[files]" {
                in_files_section = true;
                in_digest_section = false;
                continue;
            }
            if line == "[digest]" {
                in_files_section = false;
                in_digest_section = true;
                continue;
            }

            // Parse content based on section
            if in_digest_section {
                // Digest line: sha256:HASH
                if let Some(hash) = line.strip_prefix("sha256:") {
                    digest = Some(hash.to_string());
                }
            } else if in_files_section {
                // File line: path sha256:HASH
                if let Some((path, hash_part)) = line.rsplit_once(' ') {
                    if let Some(hash) = hash_part.strip_prefix("sha256:") {
                        files.push(FileEntry {
                            path: path.to_string(),
                            sha256: hash.to_string(),
                        });
                    }
                }
            } else {
                // Header section: key: value
                if let Some((key, value)) = line.split_once(':') {
                    let key = key.trim();
                    let value = value.trim();

                    match key {
                        "sherpack-manifest-version" => {
                            version = value.parse().ok();
                        }
                        "name" => {
                            name = Some(value.to_string());
                        }
                        "version" => {
                            pack_version = Version::parse(value).ok();
                        }
                        "created" => {
                            created = DateTime::parse_from_rfc3339(value)
                                .ok()
                                .map(|dt| dt.with_timezone(&Utc));
                        }
                        _ => {}
                    }
                }
            }
        }

        // Validate required fields
        let version = version.ok_or_else(|| CoreError::InvalidManifest {
            message: "Missing sherpack-manifest-version".to_string(),
        })?;

        let name = name.ok_or_else(|| CoreError::InvalidManifest {
            message: "Missing name".to_string(),
        })?;

        let pack_version = pack_version.ok_or_else(|| CoreError::InvalidManifest {
            message: "Missing or invalid version".to_string(),
        })?;

        let created = created.ok_or_else(|| CoreError::InvalidManifest {
            message: "Missing or invalid created timestamp".to_string(),
        })?;

        let digest = digest.ok_or_else(|| CoreError::InvalidManifest {
            message: "Missing digest".to_string(),
        })?;

        Ok(Self {
            version,
            name,
            pack_version,
            created,
            files,
            digest,
        })
    }


    /// Verify that all files match their checksums
    ///
    /// Takes a function that reads file content given a relative path
    pub fn verify_files<F>(&self, read_file: F) -> Result<VerificationResult>
    where
        F: Fn(&str) -> std::io::Result<Vec<u8>>,
    {
        let mut result = VerificationResult {
            valid: true,
            mismatched: Vec::new(),
            missing: Vec::new(),
        };

        for entry in &self.files {
            match read_file(&entry.path) {
                Ok(content) => {
                    let actual_hash = hash_bytes(&content);
                    if actual_hash != entry.sha256 {
                        result.valid = false;
                        result.mismatched.push(MismatchedFile {
                            path: entry.path.clone(),
                            expected: entry.sha256.clone(),
                            actual: actual_hash,
                        });
                    }
                }
                Err(_) => {
                    result.valid = false;
                    result.missing.push(entry.path.clone());
                }
            }
        }

        // Verify overall digest
        let expected_digest = calculate_digest(&self.files);
        if expected_digest != self.digest {
            result.valid = false;
        }

        Ok(result)
    }
}

/// Result of manifest verification
#[derive(Debug, Clone)]
pub struct VerificationResult {
    /// Whether all verifications passed
    pub valid: bool,
    /// Files with mismatched checksums
    pub mismatched: Vec<MismatchedFile>,
    /// Files that are missing
    pub missing: Vec<String>,
}

/// A file with a mismatched checksum
#[derive(Debug, Clone)]
pub struct MismatchedFile {
    /// File path
    pub path: String,
    /// Expected SHA256 from manifest
    pub expected: String,
    /// Actual SHA256 of file
    pub actual: String,
}

/// Calculate SHA256 hash of a file
fn hash_file(path: &Path) -> Result<String> {
    let file = std::fs::File::open(path)?;
    let mut reader = BufReader::new(file);
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 8192];

    loop {
        let bytes_read = reader.read(&mut buffer)?;
        if bytes_read == 0 {
            break;
        }
        hasher.update(&buffer[..bytes_read]);
    }

    Ok(hex::encode(hasher.finalize()))
}

/// Calculate SHA256 hash of bytes
fn hash_bytes(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hex::encode(hasher.finalize())
}

/// Calculate overall digest from file entries
fn calculate_digest(files: &[FileEntry]) -> String {
    let mut hasher = Sha256::new();
    for entry in files {
        hasher.update(entry.path.as_bytes());
        hasher.update(b":");
        hasher.update(entry.sha256.as_bytes());
        hasher.update(b"\n");
    }
    hex::encode(hasher.finalize())
}

// We need hex encoding - add it inline to avoid another dependency
mod hex {
    const HEX_CHARS: &[u8; 16] = b"0123456789abcdef";

    pub fn encode<T: AsRef<[u8]>>(data: T) -> String {
        let bytes = data.as_ref();
        let mut hex = String::with_capacity(bytes.len() * 2);
        for &byte in bytes {
            hex.push(HEX_CHARS[(byte >> 4) as usize] as char);
            hex.push(HEX_CHARS[(byte & 0x0f) as usize] as char);
        }
        hex
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_manifest_roundtrip() {
        let manifest = Manifest {
            version: 1,
            name: "myapp".to_string(),
            pack_version: Version::new(1, 2, 3),
            created: Utc::now(),
            files: vec![
                FileEntry {
                    path: "Pack.yaml".to_string(),
                    sha256: "abc123".to_string(),
                },
                FileEntry {
                    path: "values.yaml".to_string(),
                    sha256: "def456".to_string(),
                },
            ],
            digest: "overall789".to_string(),
        };

        let text = manifest.to_string();
        let parsed = Manifest::parse(&text).unwrap();

        assert_eq!(parsed.version, manifest.version);
        assert_eq!(parsed.name, manifest.name);
        assert_eq!(parsed.pack_version, manifest.pack_version);
        assert_eq!(parsed.files.len(), manifest.files.len());
        assert_eq!(parsed.digest, manifest.digest);
    }

    #[test]
    fn test_manifest_parse() {
        let content = r#"sherpack-manifest-version: 1
name: testpack
version: 2.0.0
created: 2025-01-15T10:30:00Z

[files]
Pack.yaml sha256:abc123
values.yaml sha256:def456

[digest]
sha256:789xyz
"#;

        let manifest = Manifest::parse(content).unwrap();
        assert_eq!(manifest.version, 1);
        assert_eq!(manifest.name, "testpack");
        assert_eq!(manifest.pack_version, Version::new(2, 0, 0));
        assert_eq!(manifest.files.len(), 2);
        assert_eq!(manifest.files[0].path, "Pack.yaml");
        assert_eq!(manifest.files[0].sha256, "abc123");
        assert_eq!(manifest.digest, "789xyz");
    }

    #[test]
    fn test_hash_bytes() {
        let hash = hash_bytes(b"hello world");
        assert_eq!(
            hash,
            "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9"
        );
    }

    #[test]
    fn test_verification() {
        let files = vec![
            FileEntry {
                path: "test.txt".to_string(),
                sha256: hash_bytes(b"content"),
            },
        ];
        let digest = calculate_digest(&files);

        let manifest = Manifest {
            version: 1,
            name: "test".to_string(),
            pack_version: Version::new(1, 0, 0),
            created: Utc::now(),
            files,
            digest,
        };

        // Verify with correct content
        let result = manifest
            .verify_files(|path| {
                if path == "test.txt" {
                    Ok(b"content".to_vec())
                } else {
                    Err(std::io::Error::new(std::io::ErrorKind::NotFound, "not found"))
                }
            })
            .unwrap();

        assert!(result.valid);
        assert!(result.mismatched.is_empty());
        assert!(result.missing.is_empty());

        // Verify with wrong content
        let result = manifest
            .verify_files(|path| {
                if path == "test.txt" {
                    Ok(b"wrong content".to_vec())
                } else {
                    Err(std::io::Error::new(std::io::ErrorKind::NotFound, "not found"))
                }
            })
            .unwrap();

        assert!(!result.valid);
        assert_eq!(result.mismatched.len(), 1);
    }
}
