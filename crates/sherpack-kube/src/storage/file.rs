//! File-based storage driver
//!
//! Stores release data in local files. Useful for:
//! - Development and testing without a Kubernetes cluster
//! - Offline scenarios
//! - Backup/restore operations

use async_trait::async_trait;
use std::path::PathBuf;

use super::{
    StorageConfig, StorageDriver, compress, decompress, deserialize_release, serialize_release,
};
use crate::error::{KubeError, Result};
use crate::release::StoredRelease;

/// File-based storage driver
pub struct FileDriver {
    /// Base directory for storing releases
    base_dir: PathBuf,
    config: StorageConfig,
}

impl FileDriver {
    /// Create a new file driver
    pub fn new(base_dir: PathBuf, config: StorageConfig) -> Result<Self> {
        // Create base directory if it doesn't exist
        std::fs::create_dir_all(&base_dir)?;
        Ok(Self { base_dir, config })
    }

    /// Get the path for a release file
    fn release_path(&self, namespace: &str, name: &str, version: u32) -> PathBuf {
        self.base_dir
            .join(namespace)
            .join(name)
            .join(format!("v{}.json", version))
    }

    /// Get the directory for a release (all versions)
    fn release_dir(&self, namespace: &str, name: &str) -> PathBuf {
        self.base_dir.join(namespace).join(name)
    }

    /// Write a release to file
    fn write_release(&self, release: &StoredRelease) -> Result<()> {
        let path = self.release_path(&release.namespace, &release.name, release.version);

        // Create parent directories
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let json = serialize_release(release)?;
        let data = compress(&json, self.config.compression)?;
        std::fs::write(&path, data)?;

        Ok(())
    }

    /// Read a release from file
    fn read_release(&self, path: &PathBuf) -> Result<StoredRelease> {
        let data = std::fs::read(path)?;
        let decompressed = decompress(&data, self.config.compression)?;
        deserialize_release(&decompressed)
    }
}

#[async_trait]
impl StorageDriver for FileDriver {
    async fn get(&self, namespace: &str, name: &str, version: u32) -> Result<StoredRelease> {
        let path = self.release_path(namespace, name, version);

        if !path.exists() {
            return Err(KubeError::ReleaseNotFound {
                name: name.to_string(),
                namespace: namespace.to_string(),
            });
        }

        self.read_release(&path)
    }

    async fn get_latest(&self, namespace: &str, name: &str) -> Result<StoredRelease> {
        let history = self.history(namespace, name).await?;
        history
            .into_iter()
            .next()
            .ok_or_else(|| KubeError::ReleaseNotFound {
                name: name.to_string(),
                namespace: namespace.to_string(),
            })
    }

    async fn list(
        &self,
        namespace: Option<&str>,
        name: Option<&str>,
        include_superseded: bool,
    ) -> Result<Vec<StoredRelease>> {
        let mut releases = Vec::new();

        let namespaces: Vec<PathBuf> = if let Some(ns) = namespace {
            let path = self.base_dir.join(ns);
            if path.exists() { vec![path] } else { vec![] }
        } else {
            std::fs::read_dir(&self.base_dir)?
                .filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| p.is_dir())
                .collect()
        };

        for ns_path in namespaces {
            let names: Vec<PathBuf> = if let Some(n) = name {
                let path = ns_path.join(n);
                if path.exists() { vec![path] } else { vec![] }
            } else {
                std::fs::read_dir(&ns_path)?
                    .filter_map(|e| e.ok())
                    .map(|e| e.path())
                    .filter(|p| p.is_dir())
                    .collect()
            };

            for name_path in names {
                let files: Vec<PathBuf> = std::fs::read_dir(&name_path)?
                    .filter_map(|e| e.ok())
                    .map(|e| e.path())
                    .filter(|p| p.extension().map(|e| e == "json").unwrap_or(false))
                    .collect();

                for file in files {
                    if let Ok(release) = self.read_release(&file) {
                        releases.push(release);
                    }
                }
            }
        }

        // Sort by version descending
        releases.sort_by(|a, b| b.version.cmp(&a.version));

        // Filter to latest only if not including superseded
        if !include_superseded {
            let mut seen = std::collections::HashSet::new();
            releases.retain(|r| {
                let key = format!("{}/{}", r.namespace, r.name);
                seen.insert(key)
            });
        }

        Ok(releases)
    }

    async fn history(&self, namespace: &str, name: &str) -> Result<Vec<StoredRelease>> {
        let dir = self.release_dir(namespace, name);

        if !dir.exists() {
            return Err(KubeError::ReleaseNotFound {
                name: name.to_string(),
                namespace: namespace.to_string(),
            });
        }

        let mut releases: Vec<StoredRelease> = std::fs::read_dir(&dir)?
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.extension().map(|e| e == "json").unwrap_or(false))
            .filter_map(|p| self.read_release(&p).ok())
            .collect();

        // Sort by version descending
        releases.sort_by(|a, b| b.version.cmp(&a.version));

        if releases.is_empty() {
            return Err(KubeError::ReleaseNotFound {
                name: name.to_string(),
                namespace: namespace.to_string(),
            });
        }

        Ok(releases)
    }

    async fn create(&self, release: &StoredRelease) -> Result<()> {
        let path = self.release_path(&release.namespace, &release.name, release.version);

        if path.exists() {
            return Err(KubeError::ReleaseAlreadyExists {
                name: release.name.clone(),
                namespace: release.namespace.clone(),
            });
        }

        self.write_release(release)
    }

    async fn update(&self, release: &StoredRelease) -> Result<()> {
        self.write_release(release)
    }

    async fn delete(&self, namespace: &str, name: &str, version: u32) -> Result<StoredRelease> {
        let path = self.release_path(namespace, name, version);
        let release = self.get(namespace, name, version).await?;

        std::fs::remove_file(&path)?;

        // Clean up empty directories
        let name_dir = self.release_dir(namespace, name);
        if name_dir.exists() && std::fs::read_dir(&name_dir)?.next().is_none() {
            let _ = std::fs::remove_dir(&name_dir);
        }

        let ns_dir = self.base_dir.join(namespace);
        if ns_dir.exists() && std::fs::read_dir(&ns_dir)?.next().is_none() {
            let _ = std::fs::remove_dir(&ns_dir);
        }

        Ok(release)
    }

    async fn delete_all(&self, namespace: &str, name: &str) -> Result<Vec<StoredRelease>> {
        let releases = self.history(namespace, name).await?;
        let dir = self.release_dir(namespace, name);

        if dir.exists() {
            std::fs::remove_dir_all(&dir)?;
        }

        // Clean up empty namespace directory
        let ns_dir = self.base_dir.join(namespace);
        if ns_dir.exists() && std::fs::read_dir(&ns_dir)?.next().is_none() {
            let _ = std::fs::remove_dir(&ns_dir);
        }

        Ok(releases)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sherpack_core::{PackMetadata, Values};
    use tempfile::TempDir;

    fn test_release(name: &str, version: u32) -> StoredRelease {
        let mut release = StoredRelease::for_install(
            name.to_string(),
            "default".to_string(),
            PackMetadata {
                name: "test-pack".to_string(),
                version: semver::Version::new(1, 0, 0),
                description: None,
                app_version: None,
                kube_version: None,
                home: None,
                icon: None,
                sources: vec![],
                keywords: vec![],
                maintainers: vec![],
                annotations: Default::default(),
            },
            Values::new(),
            "apiVersion: v1".to_string(),
        );
        release.version = version;
        release
    }

    #[tokio::test]
    async fn test_file_driver_create_and_get() {
        let tmp = TempDir::new().unwrap();
        let driver = FileDriver::new(tmp.path().to_path_buf(), StorageConfig::default()).unwrap();

        let release = test_release("myapp", 1);
        driver.create(&release).await.unwrap();

        let retrieved = driver.get("default", "myapp", 1).await.unwrap();
        assert_eq!(retrieved.name, "myapp");
        assert_eq!(retrieved.version, 1);
    }

    #[tokio::test]
    async fn test_file_driver_history() {
        let tmp = TempDir::new().unwrap();
        let driver = FileDriver::new(tmp.path().to_path_buf(), StorageConfig::default()).unwrap();

        for v in 1..=3 {
            let release = test_release("myapp", v);
            driver.create(&release).await.unwrap();
        }

        let history = driver.history("default", "myapp").await.unwrap();
        assert_eq!(history.len(), 3);
        assert_eq!(history[0].version, 3); // Newest first
        assert_eq!(history[2].version, 1);
    }

    #[tokio::test]
    async fn test_file_driver_list() {
        let tmp = TempDir::new().unwrap();
        let driver = FileDriver::new(tmp.path().to_path_buf(), StorageConfig::default()).unwrap();

        driver.create(&test_release("app1", 1)).await.unwrap();
        driver.create(&test_release("app1", 2)).await.unwrap();
        driver.create(&test_release("app2", 1)).await.unwrap();

        // List latest only
        let releases = driver.list(Some("default"), None, false).await.unwrap();
        assert_eq!(releases.len(), 2);

        // List all including superseded
        let releases = driver.list(Some("default"), None, true).await.unwrap();
        assert_eq!(releases.len(), 3);
    }

    #[tokio::test]
    async fn test_file_driver_delete() {
        let tmp = TempDir::new().unwrap();
        let driver = FileDriver::new(tmp.path().to_path_buf(), StorageConfig::default()).unwrap();

        driver.create(&test_release("myapp", 1)).await.unwrap();
        driver.delete("default", "myapp", 1).await.unwrap();

        let result = driver.get("default", "myapp", 1).await;
        assert!(matches!(result, Err(KubeError::ReleaseNotFound { .. })));
    }
}
