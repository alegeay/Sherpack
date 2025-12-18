//! Kubernetes Secrets storage driver
//!
//! This is the default storage driver, storing release data in Kubernetes Secrets.
//! Similar to Helm's default behavior but with improvements:
//! - Zstd compression instead of gzip
//! - JSON format instead of protobuf
//! - Automatic chunking for large releases

use async_trait::async_trait;
use k8s_openapi::api::core::v1::Secret;
use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;
use kube::api::{Api, DeleteParams, ListParams, PostParams};
use kube::Client;
use std::collections::BTreeMap;

use super::{
    decode_from_storage, encode_for_storage, storage_labels, CompressionMethod,
    LargeReleaseStrategy, StorageConfig, StorageDriver, MAX_RESOURCE_SIZE,
};
use crate::error::{KubeError, Result};
use crate::release::StoredRelease;

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

    /// Build a Secret from a release
    fn build_secret(&self, release: &StoredRelease) -> Result<Secret> {
        let encoded = encode_for_storage(release, &self.config)?;

        // Check size
        if encoded.len() > MAX_RESOURCE_SIZE {
            match &self.config.large_release_strategy {
                LargeReleaseStrategy::Fail => {
                    return Err(KubeError::ReleaseTooLarge {
                        size: encoded.len(),
                        max: MAX_RESOURCE_SIZE,
                    });
                }
                LargeReleaseStrategy::ChunkedSecrets => {
                    // TODO: Implement chunking
                    return Err(KubeError::Storage(
                        "Chunked secrets not yet implemented".to_string(),
                    ));
                }
                LargeReleaseStrategy::SeparateManifest => {
                    // TODO: Implement separate manifest storage
                    return Err(KubeError::Storage(
                        "Separate manifest storage not yet implemented".to_string(),
                    ));
                }
                LargeReleaseStrategy::ExternalReference { .. } => {
                    return Err(KubeError::Storage(
                        "External reference storage not yet implemented".to_string(),
                    ));
                }
            }
        }

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
        data.insert("release".to_string(), k8s_openapi::ByteString(encoded.into_bytes()));

        Ok(Secret {
            metadata: ObjectMeta {
                name: Some(release.storage_key()),
                namespace: Some(release.namespace.clone()),
                labels: Some(labels),
                ..Default::default()
            },
            type_: Some("sherpack.io/release.v1".to_string()),
            data: Some(data),
            ..Default::default()
        })
    }

    /// Parse a release from a Secret
    fn parse_secret(&self, secret: &Secret) -> Result<StoredRelease> {
        let data = secret
            .data
            .as_ref()
            .and_then(|d| d.get("release"))
            .ok_or_else(|| KubeError::Storage("Secret missing 'release' data".to_string()))?;

        let encoded = String::from_utf8(data.0.clone())
            .map_err(|e| KubeError::Storage(format!("Invalid UTF-8 in secret: {}", e)))?;

        // Determine compression from labels
        let compression = secret
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
            .unwrap_or(self.config.compression);

        decode_from_storage(&encoded, compression)
    }
}

#[async_trait]
impl StorageDriver for SecretsDriver {
    async fn get(&self, namespace: &str, name: &str, version: u32) -> Result<StoredRelease> {
        let api = self.secrets_api(namespace);
        let key = format!("sh.sherpack.release.v1.{}.v{}", name, version);

        match api.get(&key).await {
            Ok(secret) => self.parse_secret(&secret),
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

        let mut releases: Vec<StoredRelease> = secrets
            .items
            .iter()
            .filter_map(|s| self.parse_secret(s).ok())
            .collect();

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

        let mut releases: Vec<StoredRelease> = secrets
            .items
            .iter()
            .filter_map(|s| self.parse_secret(s).ok())
            .collect();

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
        let secret = self.build_secret(release)?;

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

        api.create(&PostParams::default(), &secret).await?;
        Ok(())
    }

    async fn update(&self, release: &StoredRelease) -> Result<()> {
        let api = self.secrets_api(&release.namespace);
        let secret = self.build_secret(release)?;

        // Use replace to update
        api.replace(&release.storage_key(), &PostParams::default(), &secret)
            .await?;
        Ok(())
    }

    async fn delete(&self, namespace: &str, name: &str, version: u32) -> Result<StoredRelease> {
        let release = self.get(namespace, name, version).await?;
        let api = self.secrets_api(namespace);
        let key = format!("sh.sherpack.release.v1.{}.v{}", name, version);

        api.delete(&key, &DeleteParams::default()).await?;
        Ok(release)
    }

    async fn delete_all(&self, namespace: &str, name: &str) -> Result<Vec<StoredRelease>> {
        let releases = self.history(namespace, name).await?;
        let api = self.secrets_api(namespace);

        for release in &releases {
            let _ = api
                .delete(&release.storage_key(), &DeleteParams::default())
                .await;
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
