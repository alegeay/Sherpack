//! Kubernetes Secrets storage driver
//!
//! This is the default storage driver, storing release data in Kubernetes Secrets.
//! Similar to Helm's default behavior but with improvements:
//! - Zstd compression instead of gzip
//! - JSON format instead of protobuf
//! - Automatic chunking for large releases (>1MB)

use async_trait::async_trait;
use k8s_openapi::api::core::v1::Secret;
use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;
use kube::api::{Api, DeleteParams, ListParams, PostParams};
use kube::Client;
use std::collections::BTreeMap;

use super::chunked::{self, ChunkedStorage};
use super::{
    decode_from_storage, encode_for_storage, storage_labels, CompressionMethod,
    LargeReleaseStrategy, StorageConfig, StorageDriver, MAX_RESOURCE_SIZE,
};
use crate::error::{KubeError, Result};
use crate::release::StoredRelease;

/// Storage strategy based on size
enum StorageStrategy {
    /// Data fits in a single Secret
    Single(String),
    /// Data needs to be chunked across multiple Secrets
    Chunked(String),
}

/// Kubernetes Secrets storage driver
pub struct SecretsDriver {
    client: Client,
    config: StorageConfig,
}

impl SecretsDriver {
    /// Create a new Secrets driver
    pub async fn new(config: StorageConfig) -> Result<Self> {
        let client = Client::try_default().await?;
        Ok(Self { client, config })
    }

    /// Create with an existing client
    pub fn with_client(client: Client, config: StorageConfig) -> Self {
        Self { client, config }
    }

    /// Get the Secret API for a namespace
    fn secrets_api(&self, namespace: &str) -> Api<Secret> {
        Api::namespaced(self.client.clone(), namespace)
    }

    /// Check if encoded data needs chunking and return strategy
    fn check_size(&self, encoded: &str) -> Result<StorageStrategy> {
        if encoded.len() <= MAX_RESOURCE_SIZE {
            return Ok(StorageStrategy::Single(encoded.to_string()));
        }

        match &self.config.large_release_strategy {
            LargeReleaseStrategy::Fail => Err(KubeError::ReleaseTooLarge {
                size: encoded.len(),
                max: MAX_RESOURCE_SIZE,
            }),
            LargeReleaseStrategy::ChunkedSecrets => {
                Ok(StorageStrategy::Chunked(encoded.to_string()))
            }
            LargeReleaseStrategy::SeparateManifest => {
                // For now, fall back to chunking
                Ok(StorageStrategy::Chunked(encoded.to_string()))
            }
            LargeReleaseStrategy::ExternalReference { .. } => Err(KubeError::Storage(
                "External reference storage not yet implemented".to_string(),
            )),
        }
    }

    /// Build a Secret from a release (for non-chunked storage)
    fn build_secret(&self, release: &StoredRelease, encoded: &str) -> Secret {
        let mut labels = storage_labels(release);
        labels.insert("sherpack.io/storage-driver".to_string(), "secrets".to_string());

        // Add compression type for decoding
        let compression_type = match self.config.compression {
            CompressionMethod::None => "none",
            CompressionMethod::Gzip { .. } => "gzip",
            CompressionMethod::Zstd { .. } => "zstd",
        };
        labels.insert("sherpack.io/compression".to_string(), compression_type.to_string());

        let mut data = BTreeMap::new();
        data.insert("release".to_string(), k8s_openapi::ByteString(encoded.as_bytes().to_vec()));

        Secret {
            metadata: ObjectMeta {
                name: Some(release.storage_key()),
                namespace: Some(release.namespace.clone()),
                labels: Some(labels),
                ..Default::default()
            },
            type_: Some("sherpack.io/release.v1".to_string()),
            data: Some(data),
            ..Default::default()
        }
    }

    /// Get chunked storage helper
    fn chunked_storage(&self) -> ChunkedStorage {
        ChunkedStorage::new(self.client.clone())
    }

    /// Parse a release from a Secret (handles both regular and chunked)
    fn parse_secret(&self, secret: &Secret) -> Result<StoredRelease> {
        // Check if this is a chunked index
        if chunked::is_chunked_index(secret) {
            // This is handled separately in get() method
            return Err(KubeError::Storage(
                "Chunked release - use async parse".to_string(),
            ));
        }

        let data = secret
            .data
            .as_ref()
            .and_then(|d| d.get("release"))
            .ok_or_else(|| KubeError::Storage("Secret missing 'release' data".to_string()))?;

        let encoded = String::from_utf8(data.0.clone())
            .map_err(|e| KubeError::Storage(format!("Invalid UTF-8 in secret: {}", e)))?;

        // Determine compression from labels
        let compression = self.get_compression_from_labels(secret);

        decode_from_storage(&encoded, compression)
    }

    /// Parse a chunked release (requires async to fetch chunks)
    async fn parse_chunked_secret(&self, secret: &Secret) -> Result<StoredRelease> {
        let index = chunked::parse_chunked_index(secret)?;
        let namespace = secret
            .metadata
            .namespace
            .as_ref()
            .ok_or_else(|| KubeError::Storage("Secret has no namespace".to_string()))?;

        // Read all chunks
        let encoded = self.chunked_storage().read_chunked(namespace, secret).await?;

        // Decode using the compression method from the index
        decode_from_storage(&encoded, index.compression_method())
    }

    /// Get compression method from Secret labels
    fn get_compression_from_labels(&self, secret: &Secret) -> CompressionMethod {
        secret
            .metadata
            .labels
            .as_ref()
            .and_then(|l| l.get("sherpack.io/compression"))
            .map(|c| match c.as_str() {
                "none" => CompressionMethod::None,
                "gzip" => CompressionMethod::Gzip { level: 6 },
                "zstd" => CompressionMethod::Zstd { level: 3 },
                _ => self.config.compression,
            })
            .unwrap_or(self.config.compression)
    }
}

#[async_trait]
impl StorageDriver for SecretsDriver {
    async fn get(&self, namespace: &str, name: &str, version: u32) -> Result<StoredRelease> {
        let api = self.secrets_api(namespace);
        let key = format!("sh.sherpack.release.v1.{}.v{}", name, version);

        match api.get(&key).await {
            Ok(secret) => {
                // Check if this is a chunked release
                if chunked::is_chunked_index(&secret) {
                    self.parse_chunked_secret(&secret).await
                } else {
                    self.parse_secret(&secret)
                }
            }
            Err(kube::Error::Api(e)) if e.code == 404 => Err(KubeError::ReleaseNotFound {
                name: name.to_string(),
                namespace: namespace.to_string(),
            }),
            Err(e) => Err(e.into()),
        }
    }

    async fn get_latest(&self, namespace: &str, name: &str) -> Result<StoredRelease> {
        let history = self.history(namespace, name).await?;
        history.into_iter().next().ok_or_else(|| KubeError::ReleaseNotFound {
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
        // Exclude chunk secrets from listing
        let mut label_selector = "app.kubernetes.io/managed-by=sherpack".to_string();
        if let Some(n) = name {
            label_selector.push_str(&format!(",sherpack.io/release-name={}", n));
        }

        let lp = ListParams::default().labels(&label_selector);

        let secrets = if let Some(ns) = namespace {
            self.secrets_api(ns).list(&lp).await?
        } else {
            // List across all namespaces
            let api: Api<Secret> = Api::all(self.client.clone());
            api.list(&lp).await?
        };

        // Filter out chunk secrets and parse releases
        let mut releases: Vec<StoredRelease> = Vec::new();
        for secret in &secrets.items {
            // Skip chunk secrets (they have chunk-parent label)
            if secret
                .metadata
                .labels
                .as_ref()
                .map(|l| l.contains_key("sherpack.io/chunk-parent"))
                .unwrap_or(false)
            {
                continue;
            }

            // Parse the release (handle chunked)
            if chunked::is_chunked_index(secret) {
                if let Ok(release) = self.parse_chunked_secret(secret).await {
                    releases.push(release);
                }
            } else if let Ok(release) = self.parse_secret(secret) {
                releases.push(release);
            }
        }

        // Sort by version descending (newest first)
        releases.sort_by(|a, b| b.version.cmp(&a.version));

        // Group by name and take only the latest if not including superseded
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
        let label_selector = format!(
            "app.kubernetes.io/managed-by=sherpack,sherpack.io/release-name={}",
            name
        );
        let lp = ListParams::default().labels(&label_selector);

        let secrets = self.secrets_api(namespace).list(&lp).await?;

        // Filter out chunk secrets and parse releases
        let mut releases: Vec<StoredRelease> = Vec::new();
        for secret in &secrets.items {
            // Skip chunk secrets
            if secret
                .metadata
                .labels
                .as_ref()
                .map(|l| l.contains_key("sherpack.io/chunk-parent"))
                .unwrap_or(false)
            {
                continue;
            }

            // Parse the release (handle chunked)
            if chunked::is_chunked_index(secret) {
                if let Ok(release) = self.parse_chunked_secret(secret).await {
                    releases.push(release);
                }
            } else if let Ok(release) = self.parse_secret(secret) {
                releases.push(release);
            }
        }

        // Sort by version descending (newest first)
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
        let api = self.secrets_api(&release.namespace);

        // Check if it already exists
        match api.get(&release.storage_key()).await {
            Ok(_) => {
                return Err(KubeError::ReleaseAlreadyExists {
                    name: release.name.clone(),
                    namespace: release.namespace.clone(),
                });
            }
            Err(kube::Error::Api(e)) if e.code == 404 => {
                // Expected - doesn't exist
            }
            Err(e) => return Err(e.into()),
        }

        // Encode the release
        let encoded = encode_for_storage(release, &self.config)?;

        // Check size and determine strategy
        match self.check_size(&encoded)? {
            StorageStrategy::Single(data) => {
                let secret = self.build_secret(release, &data);
                api.create(&PostParams::default(), &secret).await?;
            }
            StorageStrategy::Chunked(data) => {
                self.chunked_storage()
                    .create_chunked(release, &data, self.config.compression)
                    .await?;
            }
        }

        Ok(())
    }

    async fn update(&self, release: &StoredRelease) -> Result<()> {
        let api = self.secrets_api(&release.namespace);
        let key = release.storage_key();

        // First, check if the existing release is chunked
        let is_currently_chunked = match api.get(&key).await {
            Ok(secret) => chunked::is_chunked_index(&secret),
            Err(kube::Error::Api(e)) if e.code == 404 => false,
            Err(e) => return Err(e.into()),
        };

        // Encode the new release
        let encoded = encode_for_storage(release, &self.config)?;

        match self.check_size(&encoded)? {
            StorageStrategy::Single(data) => {
                // If it was chunked before, delete chunks first
                if is_currently_chunked {
                    self.chunked_storage()
                        .delete_chunked(&release.namespace, &key)
                        .await?;
                }
                let secret = self.build_secret(release, &data);
                api.replace(&key, &PostParams::default(), &secret).await?;
            }
            StorageStrategy::Chunked(data) => {
                // Update chunked (handles cleanup of old chunks)
                self.chunked_storage()
                    .update_chunked(release, &data, self.config.compression)
                    .await?;
            }
        }

        Ok(())
    }

    async fn delete(&self, namespace: &str, name: &str, version: u32) -> Result<StoredRelease> {
        let release = self.get(namespace, name, version).await?;
        let api = self.secrets_api(namespace);
        let key = format!("sh.sherpack.release.v1.{}.v{}", name, version);

        // Check if it's chunked
        match api.get(&key).await {
            Ok(secret) if chunked::is_chunked_index(&secret) => {
                // Delete chunked release (index + chunks)
                self.chunked_storage().delete_chunked(namespace, &key).await?;
            }
            Ok(_) => {
                // Delete single secret
                api.delete(&key, &DeleteParams::default()).await?;
            }
            Err(kube::Error::Api(e)) if e.code == 404 => {
                // Already deleted
            }
            Err(e) => return Err(e.into()),
        }

        Ok(release)
    }

    async fn delete_all(&self, namespace: &str, name: &str) -> Result<Vec<StoredRelease>> {
        let releases = self.history(namespace, name).await?;

        for release in &releases {
            let key = release.storage_key();
            let api = self.secrets_api(namespace);

            // Check if chunked
            match api.get(&key).await {
                Ok(secret) if chunked::is_chunked_index(&secret) => {
                    let _ = self.chunked_storage().delete_chunked(namespace, &key).await;
                }
                Ok(_) => {
                    let _ = api.delete(&key, &DeleteParams::default()).await;
                }
                Err(_) => {}
            }
        }

        Ok(releases)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_storage_key_format() {
        let release = StoredRelease::for_install(
            "myapp".to_string(),
            "default".to_string(),
            sherpack_core::PackMetadata {
                name: "test".to_string(),
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
            sherpack_core::Values::new(),
            "".to_string(),
        );

        assert_eq!(release.storage_key(), "sh.sherpack.release.v1.myapp.v1");
    }
}
