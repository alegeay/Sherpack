//! Storage drivers for persisting release information
//!
//! Sherpack supports multiple storage backends:
//! - **Secrets** (default): Store releases in Kubernetes Secrets (like Helm)
//! - **ConfigMap**: Store releases in ConfigMaps (less secure, but more accessible)
//! - **File**: Store releases in local files (for development/testing)
//!
//! ## Key Improvements over Helm
//!
//! - **Zstd compression**: Better compression ratio than gzip (~30% smaller)
//! - **Large release handling**: Automatic chunking or external storage for >1MB releases
//! - **JSON format**: Human-readable after decompression (vs Helm's protobuf)

mod secrets;
mod configmap;
mod file;
mod mock;
mod chunked;

pub use secrets::SecretsDriver;
pub use configmap::ConfigMapDriver;
pub use file::FileDriver;
pub use mock::{MockStorageDriver, OperationCounts};
pub use chunked::{ChunkedIndex, ChunkedStorage, CHUNK_SIZE};

use async_trait::async_trait;
use crate::error::{KubeError, Result};
use crate::release::StoredRelease;

/// Maximum size for a single Kubernetes Secret/ConfigMap (1MB - some overhead)
pub const MAX_RESOURCE_SIZE: usize = 1_000_000;

/// Storage driver trait for release persistence
///
/// Implementations must be Send + Sync for use across async tasks.
#[async_trait]
pub trait StorageDriver: Send + Sync {
    /// Get a specific release by name and version
    async fn get(&self, namespace: &str, name: &str, version: u32) -> Result<StoredRelease>;

    /// Get the latest release for a name
    async fn get_latest(&self, namespace: &str, name: &str) -> Result<StoredRelease>;

    /// List all releases, optionally filtered by namespace and/or name
    async fn list(
        &self,
        namespace: Option<&str>,
        name: Option<&str>,
        include_superseded: bool,
    ) -> Result<Vec<StoredRelease>>;

    /// Get release history (all versions for a name)
    async fn history(&self, namespace: &str, name: &str) -> Result<Vec<StoredRelease>>;

    /// Create a new release
    async fn create(&self, release: &StoredRelease) -> Result<()>;

    /// Update an existing release
    async fn update(&self, release: &StoredRelease) -> Result<()>;

    /// Delete a specific release version
    async fn delete(&self, namespace: &str, name: &str, version: u32) -> Result<StoredRelease>;

    /// Delete all versions of a release
    async fn delete_all(&self, namespace: &str, name: &str) -> Result<Vec<StoredRelease>>;

    /// Check if a release exists
    async fn exists(&self, namespace: &str, name: &str) -> Result<bool> {
        match self.get_latest(namespace, name).await {
            Ok(_) => Ok(true),
            Err(KubeError::ReleaseNotFound { .. }) => Ok(false),
            Err(e) => Err(e),
        }
    }
}

/// Storage configuration
#[derive(Debug, Clone)]
pub struct StorageConfig {
    /// Compression method
    pub compression: CompressionMethod,

    /// Strategy for handling large releases
    pub large_release_strategy: LargeReleaseStrategy,

    /// Maximum number of revisions to keep per release
    pub max_history: u32,
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            compression: CompressionMethod::Zstd { level: 3 },
            large_release_strategy: LargeReleaseStrategy::ChunkedSecrets,
            max_history: 10,
        }
    }
}

/// Compression method for release data
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompressionMethod {
    /// No compression
    None,

    /// Gzip compression (Helm-compatible)
    Gzip { level: u32 },

    /// Zstd compression (better ratio, faster)
    Zstd { level: i32 },
}

impl Default for CompressionMethod {
    fn default() -> Self {
        Self::Zstd { level: 3 }
    }
}

/// Strategy for handling releases larger than MAX_RESOURCE_SIZE
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LargeReleaseStrategy {
    /// Fail if release is too large (Helm default behavior)
    Fail,

    /// Split across multiple Secrets/ConfigMaps
    ChunkedSecrets,

    /// Store manifest separately in a ConfigMap
    SeparateManifest,

    /// Reference external storage (manifest stored elsewhere)
    ExternalReference {
        /// S3-compatible endpoint URL
        endpoint: String,
        /// Bucket name
        bucket: String,
    },
}

impl Default for LargeReleaseStrategy {
    fn default() -> Self {
        Self::ChunkedSecrets
    }
}

/// Compress data using the configured method
#[must_use = "compression result should be used"]
pub fn compress(data: &[u8], method: CompressionMethod) -> Result<Vec<u8>> {
    match method {
        CompressionMethod::None => Ok(data.to_vec()),
        CompressionMethod::Gzip { level } => {
            use std::io::Write;
            let mut encoder = flate2::write::GzEncoder::new(
                Vec::new(),
                flate2::Compression::new(level),
            );
            encoder.write_all(data).map_err(|e| KubeError::Compression(e.to_string()))?;
            encoder.finish().map_err(|e| KubeError::Compression(e.to_string()))
        }
        CompressionMethod::Zstd { level } => {
            zstd::encode_all(std::io::Cursor::new(data), level)
                .map_err(|e| KubeError::Compression(e.to_string()))
        }
    }
}

/// Decompress data
#[must_use = "decompression result should be used"]
pub fn decompress(data: &[u8], method: CompressionMethod) -> Result<Vec<u8>> {
    match method {
        CompressionMethod::None => Ok(data.to_vec()),
        CompressionMethod::Gzip { .. } => {
            use std::io::Read;
            let mut decoder = flate2::read::GzDecoder::new(data);
            let mut decompressed = Vec::new();
            decoder.read_to_end(&mut decompressed)
                .map_err(|e| KubeError::Compression(e.to_string()))?;
            Ok(decompressed)
        }
        CompressionMethod::Zstd { .. } => {
            zstd::decode_all(std::io::Cursor::new(data))
                .map_err(|e| KubeError::Compression(e.to_string()))
        }
    }
}

/// Serialize a release to JSON bytes
#[must_use = "serialization result should be used"]
pub fn serialize_release(release: &StoredRelease) -> Result<Vec<u8>> {
    serde_json::to_vec(release).map_err(|e| KubeError::Serialization(e.to_string()))
}

/// Deserialize a release from JSON bytes
#[must_use = "deserialization result should be used"]
pub fn deserialize_release(data: &[u8]) -> Result<StoredRelease> {
    serde_json::from_slice(data).map_err(|e| KubeError::Serialization(e.to_string()))
}

/// Encode data for storage (serialize + compress + base64)
#[must_use = "encoded data should be used for storage"]
pub fn encode_for_storage(release: &StoredRelease, config: &StorageConfig) -> Result<String> {
    let json = serialize_release(release)?;
    let compressed = compress(&json, config.compression)?;
    Ok(base64::Engine::encode(
        &base64::engine::general_purpose::STANDARD,
        &compressed,
    ))
}

/// Decode data from storage (base64 + decompress + deserialize)
#[must_use = "decoded release should be used"]
pub fn decode_from_storage(data: &str, compression: CompressionMethod) -> Result<StoredRelease> {
    let decoded = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, data)
        .map_err(|e| KubeError::Serialization(format!("base64 decode error: {}", e)))?;
    let decompressed = decompress(&decoded, compression)?;
    deserialize_release(&decompressed)
}

/// Labels applied to all storage resources
#[must_use = "labels should be applied to resources"]
pub fn storage_labels(release: &StoredRelease) -> std::collections::BTreeMap<String, String> {
    let mut labels = std::collections::BTreeMap::new();
    labels.insert("app.kubernetes.io/managed-by".to_string(), "sherpack".to_string());
    labels.insert("sherpack.io/release-name".to_string(), release.name.clone());
    labels.insert("sherpack.io/release-version".to_string(), release.version.to_string());
    labels.insert("sherpack.io/release-namespace".to_string(), release.namespace.clone());
    labels
}

#[cfg(test)]
mod tests {
    use super::*;
    use sherpack_core::{PackMetadata, Values};
    use crate::release::ReleaseState;

    fn test_release() -> StoredRelease {
        StoredRelease::for_install(
            "test".to_string(),
            "default".to_string(),
            PackMetadata {
                name: "test-pack".to_string(),
                version: semver::Version::new(1, 0, 0),
                description: Some("Test pack".to_string()),
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
            "apiVersion: v1\nkind: ConfigMap".to_string(),
        )
    }

    fn test_release_with_manifest(manifest: &str) -> StoredRelease {
        StoredRelease::for_install(
            "test".to_string(),
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
            manifest.to_string(),
        )
    }

    #[test]
    fn test_compression_roundtrip_zstd() {
        let data = b"Hello, World! This is test data for compression.";
        let compressed = compress(data, CompressionMethod::Zstd { level: 3 }).unwrap();
        let decompressed = decompress(&compressed, CompressionMethod::Zstd { level: 3 }).unwrap();
        assert_eq!(data.as_slice(), decompressed.as_slice());
    }

    #[test]
    fn test_compression_roundtrip_gzip() {
        let data = b"Hello, World! This is test data for compression.";
        let compressed = compress(data, CompressionMethod::Gzip { level: 6 }).unwrap();
        let decompressed = decompress(&compressed, CompressionMethod::Gzip { level: 6 }).unwrap();
        assert_eq!(data.as_slice(), decompressed.as_slice());
    }

    #[test]
    fn test_compression_none() {
        let data = b"No compression test data";
        let compressed = compress(data, CompressionMethod::None).unwrap();
        assert_eq!(data.as_slice(), compressed.as_slice());
        let decompressed = decompress(&compressed, CompressionMethod::None).unwrap();
        assert_eq!(data.as_slice(), decompressed.as_slice());
    }

    #[test]
    fn test_encode_decode_roundtrip() {
        let release = test_release();
        let config = StorageConfig::default();

        let encoded = encode_for_storage(&release, &config).unwrap();
        let decoded = decode_from_storage(&encoded, config.compression).unwrap();

        assert_eq!(release.name, decoded.name);
        assert_eq!(release.namespace, decoded.namespace);
        assert_eq!(release.version, decoded.version);
    }

    #[test]
    fn test_encode_decode_no_compression() {
        let release = test_release();
        let config = StorageConfig {
            compression: CompressionMethod::None,
            ..Default::default()
        };

        let encoded = encode_for_storage(&release, &config).unwrap();
        let decoded = decode_from_storage(&encoded, config.compression).unwrap();

        assert_eq!(release.name, decoded.name);
        assert_eq!(release.manifest, decoded.manifest);
    }

    #[test]
    fn test_encode_decode_gzip() {
        let release = test_release();
        let config = StorageConfig {
            compression: CompressionMethod::Gzip { level: 6 },
            ..Default::default()
        };

        let encoded = encode_for_storage(&release, &config).unwrap();
        let decoded = decode_from_storage(&encoded, config.compression).unwrap();

        assert_eq!(release.name, decoded.name);
    }

    #[test]
    fn test_zstd_smaller_than_gzip() {
        // Large data to show compression difference
        let data: Vec<u8> = (0..10000).map(|i| (i % 256) as u8).collect();

        let zstd_compressed = compress(&data, CompressionMethod::Zstd { level: 3 }).unwrap();
        let gzip_compressed = compress(&data, CompressionMethod::Gzip { level: 6 }).unwrap();

        // Zstd should be smaller or similar
        assert!(
            zstd_compressed.len() <= gzip_compressed.len() + 100,
            "Zstd: {}, Gzip: {}",
            zstd_compressed.len(),
            gzip_compressed.len()
        );
    }

    #[test]
    fn test_storage_labels() {
        let release = test_release();
        let labels = storage_labels(&release);

        assert_eq!(labels.get("app.kubernetes.io/managed-by"), Some(&"sherpack".to_string()));
        assert_eq!(labels.get("sherpack.io/release-name"), Some(&"test".to_string()));
        assert_eq!(labels.get("sherpack.io/release-version"), Some(&"1".to_string()));
        assert_eq!(labels.get("sherpack.io/release-namespace"), Some(&"default".to_string()));
    }

    #[test]
    fn test_serialize_deserialize_release() {
        let release = test_release();
        let serialized = serialize_release(&release).unwrap();
        let deserialized = deserialize_release(&serialized).unwrap();

        assert_eq!(release.name, deserialized.name);
        assert_eq!(release.namespace, deserialized.namespace);
        assert_eq!(release.version, deserialized.version);
        assert_eq!(release.manifest, deserialized.manifest);
    }

    #[test]
    fn test_serialize_release_with_all_fields() {
        let mut release = test_release();
        release.notes = Some("Installation notes".to_string());
        release.labels.insert("env".to_string(), "prod".to_string());

        let serialized = serialize_release(&release).unwrap();
        let deserialized = deserialize_release(&serialized).unwrap();

        assert_eq!(deserialized.notes, Some("Installation notes".to_string()));
        assert_eq!(deserialized.labels.get("env"), Some(&"prod".to_string()));
    }

    #[test]
    fn test_storage_config_default() {
        let config = StorageConfig::default();

        assert!(matches!(config.compression, CompressionMethod::Zstd { level: 3 }));
        assert!(matches!(config.large_release_strategy, LargeReleaseStrategy::ChunkedSecrets));
        assert_eq!(config.max_history, 10);
    }

    #[test]
    fn test_large_manifest_compression() {
        // Create a release with a large manifest
        let large_manifest = "apiVersion: v1\nkind: ConfigMap\n".repeat(1000);
        let release = test_release_with_manifest(&large_manifest);
        let config = StorageConfig::default();

        let encoded = encode_for_storage(&release, &config).unwrap();
        let decoded = decode_from_storage(&encoded, config.compression).unwrap();

        assert_eq!(release.manifest, decoded.manifest);

        // Compressed should be smaller than original JSON
        let json = serialize_release(&release).unwrap();
        let base64_decoded = base64::Engine::decode(
            &base64::engine::general_purpose::STANDARD,
            &encoded,
        ).unwrap();
        assert!(
            base64_decoded.len() < json.len(),
            "Compressed {} should be smaller than JSON {}",
            base64_decoded.len(),
            json.len()
        );
    }

    #[test]
    fn test_decode_invalid_base64() {
        let result = decode_from_storage("not valid base64!!!", CompressionMethod::None);
        assert!(result.is_err());
    }

    #[test]
    fn test_decode_invalid_json() {
        // Valid base64 but not valid JSON
        let invalid = base64::Engine::encode(
            &base64::engine::general_purpose::STANDARD,
            b"not json",
        );
        let result = decode_from_storage(&invalid, CompressionMethod::None);
        assert!(result.is_err());
    }

    #[test]
    fn test_release_state_preserved() {
        let mut release = test_release();
        release.state = ReleaseState::Failed {
            reason: "Test failure".to_string(),
            recoverable: true,
            failed_at: chrono::Utc::now(),
        };

        let serialized = serialize_release(&release).unwrap();
        let deserialized = deserialize_release(&serialized).unwrap();

        assert!(matches!(deserialized.state, ReleaseState::Failed { reason, .. } if reason == "Test failure"));
    }
}
