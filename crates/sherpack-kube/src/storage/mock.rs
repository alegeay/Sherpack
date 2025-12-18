//! Mock storage driver for testing
//!
//! This driver stores releases in memory, useful for unit tests
//! without requiring a Kubernetes cluster.

use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use super::StorageDriver;
use crate::error::{KubeError, Result};
use crate::release::StoredRelease;

/// In-memory storage driver for testing
#[derive(Clone)]
pub struct MockStorageDriver {
    /// Storage: namespace -> name -> version -> release
    store: Arc<RwLock<HashMap<String, HashMap<String, HashMap<u32, StoredRelease>>>>>,
    /// Track operation counts for assertions
    operations: Arc<RwLock<OperationCounts>>,
}

/// Counts of operations performed for testing assertions
#[derive(Debug, Default, Clone)]
pub struct OperationCounts {
    pub gets: usize,
    pub lists: usize,
    pub creates: usize,
    pub updates: usize,
    pub deletes: usize,
}

impl MockStorageDriver {
    /// Create a new empty mock driver
    pub fn new() -> Self {
        Self {
            store: Arc::new(RwLock::new(HashMap::new())),
            operations: Arc::new(RwLock::new(OperationCounts::default())),
        }
    }

    /// Create with pre-populated releases
    pub fn with_releases(releases: Vec<StoredRelease>) -> Self {
        let driver = Self::new();
        {
            let mut store = driver.store.write().unwrap();
            for release in releases {
                store
                    .entry(release.namespace.clone())
                    .or_default()
                    .entry(release.name.clone())
                    .or_default()
                    .insert(release.version, release);
            }
        }
        driver
    }

    /// Get operation counts for assertions
    pub fn operation_counts(&self) -> OperationCounts {
        self.operations.read().unwrap().clone()
    }

    /// Reset operation counts
    pub fn reset_counts(&self) {
        let mut ops = self.operations.write().unwrap();
        *ops = OperationCounts::default();
    }

    /// Get all releases (for testing)
    pub fn all_releases(&self) -> Vec<StoredRelease> {
        let store = self.store.read().unwrap();
        store
            .values()
            .flat_map(|ns| ns.values())
            .flat_map(|name| name.values())
            .cloned()
            .collect()
    }

    /// Count total releases
    pub fn release_count(&self) -> usize {
        let store = self.store.read().unwrap();
        store
            .values()
            .flat_map(|ns| ns.values())
            .map(|name| name.len())
            .sum()
    }
}

impl Default for MockStorageDriver {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl StorageDriver for MockStorageDriver {
    async fn get(&self, namespace: &str, name: &str, version: u32) -> Result<StoredRelease> {
        {
            let mut ops = self.operations.write().unwrap();
            ops.gets += 1;
        }

        let store = self.store.read().unwrap();
        store
            .get(namespace)
            .and_then(|ns| ns.get(name))
            .and_then(|versions| versions.get(&version))
            .cloned()
            .ok_or_else(|| KubeError::ReleaseNotFound {
                name: name.to_string(),
                namespace: namespace.to_string(),
            })
    }

    async fn get_latest(&self, namespace: &str, name: &str) -> Result<StoredRelease> {
        {
            let mut ops = self.operations.write().unwrap();
            ops.gets += 1;
        }

        let store = self.store.read().unwrap();
        store
            .get(namespace)
            .and_then(|ns| ns.get(name))
            .and_then(|versions| versions.values().max_by_key(|r| r.version))
            .cloned()
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
        {
            let mut ops = self.operations.write().unwrap();
            ops.lists += 1;
        }

        let store = self.store.read().unwrap();
        let mut releases: Vec<StoredRelease> = store
            .iter()
            .filter(|(ns, _)| namespace.map(|n| n == *ns).unwrap_or(true))
            .flat_map(|(_, names)| names.iter())
            .filter(|(n, _)| name.map(|filter| filter == *n).unwrap_or(true))
            .flat_map(|(_, versions)| versions.values())
            .cloned()
            .collect();

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
        {
            let mut ops = self.operations.write().unwrap();
            ops.lists += 1;
        }

        let store = self.store.read().unwrap();
        let mut releases: Vec<StoredRelease> = store
            .get(namespace)
            .and_then(|ns| ns.get(name))
            .map(|versions| versions.values().cloned().collect())
            .unwrap_or_default();

        if releases.is_empty() {
            return Err(KubeError::ReleaseNotFound {
                name: name.to_string(),
                namespace: namespace.to_string(),
            });
        }

        // Sort by version descending
        releases.sort_by(|a, b| b.version.cmp(&a.version));

        Ok(releases)
    }

    async fn create(&self, release: &StoredRelease) -> Result<()> {
        {
            let mut ops = self.operations.write().unwrap();
            ops.creates += 1;
        }

        let mut store = self.store.write().unwrap();
        let versions = store
            .entry(release.namespace.clone())
            .or_default()
            .entry(release.name.clone())
            .or_default();

        if versions.contains_key(&release.version) {
            return Err(KubeError::ReleaseAlreadyExists {
                name: release.name.clone(),
                namespace: release.namespace.clone(),
            });
        }

        versions.insert(release.version, release.clone());
        Ok(())
    }

    async fn update(&self, release: &StoredRelease) -> Result<()> {
        {
            let mut ops = self.operations.write().unwrap();
            ops.updates += 1;
        }

        let mut store = self.store.write().unwrap();
        let versions = store
            .entry(release.namespace.clone())
            .or_default()
            .entry(release.name.clone())
            .or_default();

        versions.insert(release.version, release.clone());
        Ok(())
    }

    async fn delete(&self, namespace: &str, name: &str, version: u32) -> Result<StoredRelease> {
        {
            let mut ops = self.operations.write().unwrap();
            ops.deletes += 1;
        }

        let mut store = self.store.write().unwrap();
        let release = store
            .get_mut(namespace)
            .and_then(|ns| ns.get_mut(name))
            .and_then(|versions| versions.remove(&version))
            .ok_or_else(|| KubeError::ReleaseNotFound {
                name: name.to_string(),
                namespace: namespace.to_string(),
            })?;

        Ok(release)
    }

    async fn delete_all(&self, namespace: &str, name: &str) -> Result<Vec<StoredRelease>> {
        {
            let mut ops = self.operations.write().unwrap();
            ops.deletes += 1;
        }

        let mut store = self.store.write().unwrap();
        let releases: Vec<StoredRelease> = store
            .get_mut(namespace)
            .and_then(|ns| ns.remove(name))
            .map(|versions| versions.into_values().collect())
            .unwrap_or_default();

        if releases.is_empty() {
            return Err(KubeError::ReleaseNotFound {
                name: name.to_string(),
                namespace: namespace.to_string(),
            });
        }

        Ok(releases)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::release::ReleaseState;

    fn create_test_release(name: &str, namespace: &str, version: u32) -> StoredRelease {
        StoredRelease {
            name: name.to_string(),
            namespace: namespace.to_string(),
            version,
            state: ReleaseState::Deployed,
            pack: sherpack_core::PackMetadata {
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
            values: sherpack_core::Values::new(),
            values_provenance: Default::default(),
            manifest: "apiVersion: v1\nkind: ConfigMap".to_string(),
            hooks: vec![],
            labels: Default::default(),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            notes: None,
        }
    }

    #[tokio::test]
    async fn test_mock_create_and_get() {
        let driver = MockStorageDriver::new();

        let release = create_test_release("myapp", "default", 1);
        driver.create(&release).await.unwrap();

        let retrieved = driver.get("default", "myapp", 1).await.unwrap();
        assert_eq!(retrieved.name, "myapp");
        assert_eq!(retrieved.version, 1);

        let counts = driver.operation_counts();
        assert_eq!(counts.creates, 1);
        assert_eq!(counts.gets, 1);
    }

    #[tokio::test]
    async fn test_mock_create_duplicate_fails() {
        let driver = MockStorageDriver::new();

        let release = create_test_release("myapp", "default", 1);
        driver.create(&release).await.unwrap();

        let result = driver.create(&release).await;
        assert!(matches!(result, Err(KubeError::ReleaseAlreadyExists { .. })));
    }

    #[tokio::test]
    async fn test_mock_get_not_found() {
        let driver = MockStorageDriver::new();

        let result = driver.get("default", "nonexistent", 1).await;
        assert!(matches!(result, Err(KubeError::ReleaseNotFound { .. })));
    }

    #[tokio::test]
    async fn test_mock_get_latest() {
        let driver = MockStorageDriver::new();

        driver.create(&create_test_release("myapp", "default", 1)).await.unwrap();
        driver.create(&create_test_release("myapp", "default", 2)).await.unwrap();
        driver.create(&create_test_release("myapp", "default", 3)).await.unwrap();

        let latest = driver.get_latest("default", "myapp").await.unwrap();
        assert_eq!(latest.version, 3);
    }

    #[tokio::test]
    async fn test_mock_list_all() {
        let driver = MockStorageDriver::new();

        driver.create(&create_test_release("app1", "default", 1)).await.unwrap();
        driver.create(&create_test_release("app2", "default", 1)).await.unwrap();
        driver.create(&create_test_release("app1", "staging", 1)).await.unwrap();

        let all = driver.list(None, None, false).await.unwrap();
        assert_eq!(all.len(), 3);
    }

    #[tokio::test]
    async fn test_mock_list_by_namespace() {
        let driver = MockStorageDriver::new();

        driver.create(&create_test_release("app1", "default", 1)).await.unwrap();
        driver.create(&create_test_release("app2", "default", 1)).await.unwrap();
        driver.create(&create_test_release("app1", "staging", 1)).await.unwrap();

        let in_default = driver.list(Some("default"), None, false).await.unwrap();
        assert_eq!(in_default.len(), 2);
    }

    #[tokio::test]
    async fn test_mock_list_excludes_superseded() {
        let driver = MockStorageDriver::new();

        driver.create(&create_test_release("myapp", "default", 1)).await.unwrap();
        driver.create(&create_test_release("myapp", "default", 2)).await.unwrap();
        driver.create(&create_test_release("myapp", "default", 3)).await.unwrap();

        // Without superseded, should only get latest
        let latest_only = driver.list(None, None, false).await.unwrap();
        assert_eq!(latest_only.len(), 1);
        assert_eq!(latest_only[0].version, 3);

        // With superseded, should get all
        let all = driver.list(None, None, true).await.unwrap();
        assert_eq!(all.len(), 3);
    }

    #[tokio::test]
    async fn test_mock_history() {
        let driver = MockStorageDriver::new();

        driver.create(&create_test_release("myapp", "default", 1)).await.unwrap();
        driver.create(&create_test_release("myapp", "default", 2)).await.unwrap();
        driver.create(&create_test_release("myapp", "default", 3)).await.unwrap();

        let history = driver.history("default", "myapp").await.unwrap();
        assert_eq!(history.len(), 3);
        // Should be sorted newest first
        assert_eq!(history[0].version, 3);
        assert_eq!(history[1].version, 2);
        assert_eq!(history[2].version, 1);
    }

    #[tokio::test]
    async fn test_mock_update() {
        let driver = MockStorageDriver::new();

        let mut release = create_test_release("myapp", "default", 1);
        driver.create(&release).await.unwrap();

        release.manifest = "updated manifest".to_string();
        driver.update(&release).await.unwrap();

        let retrieved = driver.get("default", "myapp", 1).await.unwrap();
        assert_eq!(retrieved.manifest, "updated manifest");
    }

    #[tokio::test]
    async fn test_mock_delete() {
        let driver = MockStorageDriver::new();

        driver.create(&create_test_release("myapp", "default", 1)).await.unwrap();
        driver.create(&create_test_release("myapp", "default", 2)).await.unwrap();

        let deleted = driver.delete("default", "myapp", 1).await.unwrap();
        assert_eq!(deleted.version, 1);

        // Should still have version 2
        let remaining = driver.get("default", "myapp", 2).await.unwrap();
        assert_eq!(remaining.version, 2);

        // Version 1 should be gone
        let result = driver.get("default", "myapp", 1).await;
        assert!(matches!(result, Err(KubeError::ReleaseNotFound { .. })));
    }

    #[tokio::test]
    async fn test_mock_delete_all() {
        let driver = MockStorageDriver::new();

        driver.create(&create_test_release("myapp", "default", 1)).await.unwrap();
        driver.create(&create_test_release("myapp", "default", 2)).await.unwrap();
        driver.create(&create_test_release("myapp", "default", 3)).await.unwrap();

        let deleted = driver.delete_all("default", "myapp").await.unwrap();
        assert_eq!(deleted.len(), 3);

        // Should be empty now
        let result = driver.history("default", "myapp").await;
        assert!(matches!(result, Err(KubeError::ReleaseNotFound { .. })));
    }

    #[tokio::test]
    async fn test_mock_with_releases() {
        let releases = vec![
            create_test_release("app1", "default", 1),
            create_test_release("app2", "default", 1),
        ];

        let driver = MockStorageDriver::with_releases(releases);
        assert_eq!(driver.release_count(), 2);

        let app1 = driver.get("default", "app1", 1).await.unwrap();
        assert_eq!(app1.name, "app1");
    }

    #[tokio::test]
    async fn test_operation_counts() {
        let driver = MockStorageDriver::new();

        driver.create(&create_test_release("myapp", "default", 1)).await.unwrap();
        let _ = driver.get("default", "myapp", 1).await;
        let _ = driver.list(None, None, false).await;
        driver.update(&create_test_release("myapp", "default", 1)).await.unwrap();
        let _ = driver.delete("default", "myapp", 1).await;

        let counts = driver.operation_counts();
        assert_eq!(counts.creates, 1);
        assert_eq!(counts.gets, 1);
        assert_eq!(counts.lists, 1);
        assert_eq!(counts.updates, 1);
        assert_eq!(counts.deletes, 1);

        driver.reset_counts();
        let counts = driver.operation_counts();
        assert_eq!(counts.creates, 0);
    }
}
