//! Files API for accessing pack files from templates
//!
//! This module provides a sandboxed file access API that allows templates
//! to read files from within the pack directory. All operations are restricted
//! to the pack root to prevent path traversal attacks.
//!
//! # Security
//!
//! - All paths are resolved relative to the pack root
//! - Absolute paths are rejected
//! - Path traversal attempts (../) are detected and rejected
//! - Files outside the pack directory cannot be accessed
//!
//! # Example
//!
//! ```jinja2
//! {# Read a file #}
//! data:
//!   nginx.conf: {{ files.get("config/nginx.conf") | b64encode }}
//!
//! {# Check if file exists #}
//! {% if files.exists("config/custom.yaml") %}
//!   custom: {{ files.get("config/custom.yaml") }}
//! {% endif %}
//!
//! {# Iterate over files matching a glob pattern #}
//! {% for file in files.glob("scripts/*.sh") %}
//!   {{ file.name }}: {{ file.content | b64encode }}
//! {% endfor %}
//! ```

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use serde::{Deserialize, Serialize};

use crate::error::{CoreError, Result};

/// Trait for file access providers
///
/// This trait allows for different implementations:
/// - `SandboxedFileProvider`: Real filesystem access (sandboxed to pack root)
/// - `MockFileProvider`: In-memory files for testing
/// - `ArchiveFileProvider`: Read files from a tar.gz archive (future)
pub trait FileProvider: Send + Sync {
    /// Read the contents of a file as bytes
    fn get(&self, path: &str) -> Result<Vec<u8>>;

    /// Check if a file exists
    fn exists(&self, path: &str) -> bool;

    /// List files matching a glob pattern
    fn glob(&self, pattern: &str) -> Result<Vec<FileEntry>>;

    /// Read a file as lines
    fn lines(&self, path: &str) -> Result<Vec<String>>;

    /// Read the contents of a file as a string (UTF-8)
    fn get_string(&self, path: &str) -> Result<String> {
        let bytes = self.get(path)?;
        String::from_utf8(bytes).map_err(|e| CoreError::FileAccess {
            path: path.to_string(),
            message: format!("file is not valid UTF-8: {}", e),
        })
    }
}

/// A file entry returned by glob operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileEntry {
    /// Relative path from pack root
    pub path: String,
    /// File name (without directory)
    pub name: String,
    /// File content as string (UTF-8 lossy)
    pub content: String,
    /// File size in bytes
    pub size: usize,
}

/// Sandboxed file provider that restricts access to the pack directory
///
/// This is the default provider used during template rendering.
/// It ensures that templates cannot access files outside the pack root.
#[derive(Debug)]
pub struct SandboxedFileProvider {
    /// The root directory of the pack (all paths are relative to this)
    root: PathBuf,
    /// Canonicalized root for security checks
    canonical_root: PathBuf,
    /// Cache of file contents to avoid repeated reads
    cache: Arc<RwLock<HashMap<PathBuf, Vec<u8>>>>,
}

impl SandboxedFileProvider {
    /// Create a new sandboxed file provider
    ///
    /// # Arguments
    ///
    /// * `pack_root` - The root directory of the pack
    ///
    /// # Errors
    ///
    /// Returns an error if the pack root doesn't exist or cannot be canonicalized.
    pub fn new(pack_root: impl AsRef<Path>) -> Result<Self> {
        let root = pack_root.as_ref().to_path_buf();

        if !root.exists() {
            return Err(CoreError::FileAccess {
                path: root.display().to_string(),
                message: "pack root directory does not exist".to_string(),
            });
        }

        let canonical_root = root.canonicalize().map_err(|e| CoreError::FileAccess {
            path: root.display().to_string(),
            message: format!("failed to canonicalize pack root: {}", e),
        })?;

        Ok(Self {
            root,
            canonical_root,
            cache: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    /// Resolve a relative path and verify it's within the sandbox
    ///
    /// # Security
    ///
    /// This method:
    /// 1. Rejects absolute paths
    /// 2. Joins the path with the pack root
    /// 3. Canonicalizes to resolve symlinks and `..` components
    /// 4. Verifies the result is still within the pack root
    fn resolve_path(&self, relative: &str) -> Result<PathBuf> {
        let requested = Path::new(relative);

        // Reject absolute paths
        if requested.is_absolute() {
            return Err(CoreError::FileAccess {
                path: relative.to_string(),
                message: "absolute paths are not allowed in templates".to_string(),
            });
        }

        // Quick check for obvious traversal attempts
        if relative.contains("..") {
            // Still do the full check, but this catches simple cases early
        }

        // Build the full path
        let full_path = self.root.join(relative);

        // Check if the file exists before canonicalizing
        if !full_path.exists() {
            return Err(CoreError::FileAccess {
                path: relative.to_string(),
                message: "file not found".to_string(),
            });
        }

        // Canonicalize to resolve symlinks and .. components
        let canonical = full_path
            .canonicalize()
            .map_err(|e| CoreError::FileAccess {
                path: relative.to_string(),
                message: format!("failed to resolve path: {}", e),
            })?;

        // Verify the path is within the sandbox
        if !canonical.starts_with(&self.canonical_root) {
            return Err(CoreError::FileAccess {
                path: relative.to_string(),
                message: "path escapes pack directory (sandbox violation)".to_string(),
            });
        }

        Ok(canonical)
    }

    /// Check if a path is valid without reading the file
    fn is_valid_path(&self, relative: &str) -> bool {
        self.resolve_path(relative).is_ok()
    }
}

impl FileProvider for SandboxedFileProvider {
    fn get(&self, path: &str) -> Result<Vec<u8>> {
        let resolved = self.resolve_path(path)?;

        // Check cache first
        {
            let cache = self.cache.read().map_err(|_| CoreError::FileAccess {
                path: path.to_string(),
                message: "cache lock poisoned".to_string(),
            })?;

            if let Some(content) = cache.get(&resolved) {
                return Ok(content.clone());
            }
        }

        // Read the file
        let content = std::fs::read(&resolved).map_err(|e| CoreError::FileAccess {
            path: path.to_string(),
            message: format!("failed to read file: {}", e),
        })?;

        // Update cache
        {
            let mut cache = self.cache.write().map_err(|_| CoreError::FileAccess {
                path: path.to_string(),
                message: "cache lock poisoned".to_string(),
            })?;

            cache.insert(resolved, content.clone());
        }

        Ok(content)
    }

    fn exists(&self, path: &str) -> bool {
        self.is_valid_path(path)
    }

    fn glob(&self, pattern: &str) -> Result<Vec<FileEntry>> {
        // Validate the glob pattern
        let glob_pattern = glob::Pattern::new(pattern).map_err(|e| CoreError::GlobPattern {
            message: format!("invalid glob pattern '{}': {}", pattern, e),
        })?;

        let mut entries = Vec::new();

        // Walk the pack directory
        for entry in walkdir::WalkDir::new(&self.root)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
        {
            // Get relative path
            let rel_path = match entry.path().strip_prefix(&self.root) {
                Ok(p) => p,
                Err(_) => continue,
            };

            let rel_str = rel_path.to_string_lossy();

            // Check if it matches the pattern
            if glob_pattern.matches(&rel_str) {
                // Read the file content
                let content = match std::fs::read_to_string(entry.path()) {
                    Ok(c) => c,
                    Err(_) => {
                        // For binary files, use lossy conversion
                        match std::fs::read(entry.path()) {
                            Ok(bytes) => String::from_utf8_lossy(&bytes).to_string(),
                            Err(_) => continue,
                        }
                    }
                };

                let size = content.len();

                entries.push(FileEntry {
                    path: rel_str.to_string(),
                    name: entry.file_name().to_string_lossy().to_string(),
                    content,
                    size,
                });
            }
        }

        // Sort for deterministic output (important for reproducible templates)
        entries.sort_by(|a, b| a.path.cmp(&b.path));

        Ok(entries)
    }

    fn lines(&self, path: &str) -> Result<Vec<String>> {
        let content = self.get_string(path)?;
        Ok(content.lines().map(String::from).collect())
    }
}

/// Mock file provider for testing
///
/// This provider stores files in memory, allowing tests to run
/// without filesystem access.
#[derive(Debug, Default, Clone)]
pub struct MockFileProvider {
    files: HashMap<String, Vec<u8>>,
}

impl MockFileProvider {
    /// Create a new empty mock provider
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a file to the mock filesystem
    pub fn with_file(mut self, path: &str, content: impl Into<Vec<u8>>) -> Self {
        self.files.insert(path.to_string(), content.into());
        self
    }

    /// Add a text file to the mock filesystem
    pub fn with_text_file(self, path: &str, content: &str) -> Self {
        self.with_file(path, content.as_bytes().to_vec())
    }

    /// Add multiple files at once
    pub fn with_files(
        mut self,
        files: impl IntoIterator<Item = (&'static str, &'static str)>,
    ) -> Self {
        for (path, content) in files {
            self.files
                .insert(path.to_string(), content.as_bytes().to_vec());
        }
        self
    }
}

impl FileProvider for MockFileProvider {
    fn get(&self, path: &str) -> Result<Vec<u8>> {
        self.files
            .get(path)
            .cloned()
            .ok_or_else(|| CoreError::FileAccess {
                path: path.to_string(),
                message: "file not found".to_string(),
            })
    }

    fn exists(&self, path: &str) -> bool {
        self.files.contains_key(path)
    }

    fn glob(&self, pattern: &str) -> Result<Vec<FileEntry>> {
        let glob_pattern = glob::Pattern::new(pattern).map_err(|e| CoreError::GlobPattern {
            message: format!("invalid glob pattern '{}': {}", pattern, e),
        })?;

        let mut entries: Vec<_> = self
            .files
            .iter()
            .filter(|(path, _)| glob_pattern.matches(path))
            .map(|(path, content)| {
                let name = Path::new(path)
                    .file_name()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_default();

                FileEntry {
                    path: path.clone(),
                    name,
                    content: String::from_utf8_lossy(content).to_string(),
                    size: content.len(),
                }
            })
            .collect();

        // Sort for deterministic output
        entries.sort_by(|a, b| a.path.cmp(&b.path));

        Ok(entries)
    }

    fn lines(&self, path: &str) -> Result<Vec<String>> {
        let content = self.get_string(path)?;
        Ok(content.lines().map(String::from).collect())
    }
}

/// A wrapper that provides the Files API to templates
///
/// This struct is what gets injected into the template context as `files`.
/// It wraps any `FileProvider` implementation.
#[derive(Clone)]
pub struct Files {
    provider: Arc<dyn FileProvider>,
}

impl Files {
    /// Create a new Files wrapper from a provider
    pub fn new(provider: impl FileProvider + 'static) -> Self {
        Self {
            provider: Arc::new(provider),
        }
    }

    /// Create Files from an Arc'd provider (avoids double-Arc)
    pub fn from_arc(provider: Arc<dyn FileProvider>) -> Self {
        Self { provider }
    }

    /// Create a sandboxed Files instance for a pack
    pub fn for_pack(pack_root: impl AsRef<Path>) -> Result<Self> {
        let provider = SandboxedFileProvider::new(pack_root)?;
        Ok(Self::new(provider))
    }

    /// Create a mock Files instance for testing
    pub fn mock() -> MockFileProvider {
        MockFileProvider::new()
    }

    /// Get file contents as string
    pub fn get(&self, path: &str) -> Result<String> {
        self.provider.get_string(path)
    }

    /// Get file contents as bytes
    pub fn get_bytes(&self, path: &str) -> Result<Vec<u8>> {
        self.provider.get(path)
    }

    /// Check if file exists
    pub fn exists(&self, path: &str) -> bool {
        self.provider.exists(path)
    }

    /// Glob for files
    pub fn glob(&self, pattern: &str) -> Result<Vec<FileEntry>> {
        self.provider.glob(pattern)
    }

    /// Read file as lines
    pub fn lines(&self, path: &str) -> Result<Vec<String>> {
        self.provider.lines(path)
    }
}

impl std::fmt::Debug for Files {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Files").finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_pack() -> TempDir {
        let temp = TempDir::new().unwrap();

        // Create directory structure
        std::fs::create_dir_all(temp.path().join("config")).unwrap();
        std::fs::create_dir_all(temp.path().join("scripts")).unwrap();

        // Create test files
        std::fs::write(temp.path().join("config/app.yaml"), "key: value").unwrap();
        std::fs::write(temp.path().join("config/db.yaml"), "host: localhost").unwrap();
        std::fs::write(
            temp.path().join("scripts/init.sh"),
            "#!/bin/bash\necho hello",
        )
        .unwrap();
        std::fs::write(temp.path().join("README.md"), "# Test Pack").unwrap();

        temp
    }

    #[test]
    fn test_sandboxed_provider_read_file() {
        let temp = create_test_pack();
        let provider = SandboxedFileProvider::new(temp.path()).unwrap();

        let content = provider.get_string("config/app.yaml").unwrap();
        assert_eq!(content, "key: value");
    }

    #[test]
    fn test_sandboxed_provider_exists() {
        let temp = create_test_pack();
        let provider = SandboxedFileProvider::new(temp.path()).unwrap();

        assert!(provider.exists("config/app.yaml"));
        assert!(provider.exists("README.md"));
        assert!(!provider.exists("nonexistent.txt"));
    }

    #[test]
    fn test_sandboxed_provider_glob() {
        let temp = create_test_pack();
        let provider = SandboxedFileProvider::new(temp.path()).unwrap();

        let entries = provider.glob("config/*.yaml").unwrap();
        assert_eq!(entries.len(), 2);

        // Check sorted order
        assert_eq!(entries[0].name, "app.yaml");
        assert_eq!(entries[1].name, "db.yaml");
    }

    #[test]
    fn test_sandboxed_provider_lines() {
        let temp = create_test_pack();
        let provider = SandboxedFileProvider::new(temp.path()).unwrap();

        let lines = provider.lines("scripts/init.sh").unwrap();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0], "#!/bin/bash");
        assert_eq!(lines[1], "echo hello");
    }

    #[test]
    fn test_sandbox_prevents_absolute_paths() {
        let temp = create_test_pack();
        let provider = SandboxedFileProvider::new(temp.path()).unwrap();

        let result = provider.get("/etc/passwd");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("absolute paths"));
    }

    #[test]
    fn test_sandbox_prevents_path_traversal() {
        let temp = create_test_pack();
        let provider = SandboxedFileProvider::new(temp.path()).unwrap();

        // Create a file outside the pack
        let parent = temp.path().parent().unwrap();
        std::fs::write(parent.join("secret.txt"), "secret data").unwrap();

        // Try to access it via path traversal
        let result = provider.get("../secret.txt");
        assert!(result.is_err());

        let err = result.unwrap_err().to_string();
        // Either "sandbox violation" or "file not found" depending on resolution order
        assert!(err.contains("sandbox") || err.contains("not found"));
    }

    #[test]
    fn test_sandbox_prevents_deep_traversal() {
        let temp = create_test_pack();
        let provider = SandboxedFileProvider::new(temp.path()).unwrap();

        let result = provider.get("config/../../../../../../etc/passwd");
        assert!(result.is_err());
    }

    #[test]
    fn test_mock_provider() {
        let provider = MockFileProvider::new()
            .with_text_file("config/app.yaml", "key: value")
            .with_text_file("config/db.yaml", "host: localhost");

        assert!(provider.exists("config/app.yaml"));
        assert!(!provider.exists("nonexistent.txt"));

        let content = provider.get_string("config/app.yaml").unwrap();
        assert_eq!(content, "key: value");
    }

    #[test]
    fn test_mock_provider_glob() {
        let provider = MockFileProvider::new()
            .with_text_file("config/a.yaml", "a")
            .with_text_file("config/b.yaml", "b")
            .with_text_file("other/c.yaml", "c");

        let entries = provider.glob("config/*.yaml").unwrap();
        assert_eq!(entries.len(), 2);

        // Verify sorted order
        assert_eq!(entries[0].path, "config/a.yaml");
        assert_eq!(entries[1].path, "config/b.yaml");
    }

    #[test]
    fn test_files_wrapper() {
        let mock = MockFileProvider::new().with_text_file("test.txt", "hello world");

        let files = Files::new(mock);

        assert!(files.exists("test.txt"));
        assert_eq!(files.get("test.txt").unwrap(), "hello world");
    }

    #[test]
    fn test_glob_deterministic_order() {
        // Create files in non-alphabetical order
        let provider = MockFileProvider::new()
            .with_text_file("z.yaml", "z")
            .with_text_file("a.yaml", "a")
            .with_text_file("m.yaml", "m");

        let entries = provider.glob("*.yaml").unwrap();
        let paths: Vec<_> = entries.iter().map(|e| e.path.as_str()).collect();

        assert_eq!(paths, vec!["a.yaml", "m.yaml", "z.yaml"]);
    }

    #[test]
    fn test_file_caching() {
        let temp = create_test_pack();
        let provider = SandboxedFileProvider::new(temp.path()).unwrap();

        // First read
        let content1 = provider.get("config/app.yaml").unwrap();

        // Modify the file
        std::fs::write(temp.path().join("config/app.yaml"), "modified").unwrap();

        // Second read should return cached content
        let content2 = provider.get("config/app.yaml").unwrap();

        assert_eq!(content1, content2);
    }

    #[test]
    fn test_binary_file_handling() {
        let temp = TempDir::new().unwrap();

        // Create a binary file
        let binary_data = vec![0u8, 1, 2, 255, 254, 253];
        std::fs::write(temp.path().join("binary.bin"), &binary_data).unwrap();

        let provider = SandboxedFileProvider::new(temp.path()).unwrap();
        let content = provider.get("binary.bin").unwrap();

        assert_eq!(content, binary_data);
    }

    #[test]
    fn test_glob_pattern_validation() {
        let provider = MockFileProvider::new();

        // Invalid glob pattern (unclosed bracket)
        let result = provider.glob("[invalid");
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("invalid glob pattern")
        );
    }
}
