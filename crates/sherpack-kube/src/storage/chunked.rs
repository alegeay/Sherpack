//! Chunked storage for large releases
//!
//! When a release exceeds Kubernetes' 1MB Secret size limit, we split it
//! across multiple Secrets with an index Secret that references the chunks.
//!
//! ## Architecture
//!
//! ```text
//! Index Secret: sh.sherpack.release.v1.myapp.v3
//!   └─ Contains: chunk count, total size, checksum
//!
//! Chunk Secrets:
//!   ├─ sh.sherpack.release.v1.myapp.v3.chunk.0
//!   ├─ sh.sherpack.release.v1.myapp.v3.chunk.1
//!   └─ sh.sherpack.release.v1.myapp.v3.chunk.2
//! ```
//!
//! ## Key Properties
//!
//! - **Atomic visibility**: Index is created last, deleted first
//! - **Checksummed**: SHA-256 of complete data for integrity
//! - **Recoverable**: Orphaned chunks are cleaned up automatically

use k8s_openapi::api::core::v1::Secret;
use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;
use kube::Client;
use kube::api::{Api, DeleteParams, ListParams, PostParams};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

use super::{CompressionMethod, MAX_RESOURCE_SIZE, storage_labels};
use crate::error::{KubeError, Result};
use crate::release::StoredRelease;

/// Maximum size per chunk (after compression, before base64)
/// We use 700KB to leave room for Secret overhead and base64 expansion
pub const CHUNK_SIZE: usize = 700_000;

/// Index stored in the main Secret when data is chunked
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkedIndex {
    /// Format identifier
    pub format: String,
    /// Total size of concatenated chunk data (base64 encoded)
    pub total_size: usize,
    /// Number of chunks
    pub chunk_count: usize,
    /// Size of each chunk (last chunk may be smaller)
    pub chunk_size: usize,
    /// SHA-256 checksum of the complete data (before chunking)
    pub checksum: String,
    /// Compression method used
    pub compression: String,
}

impl ChunkedIndex {
    /// Create a new chunked index
    pub fn new(data: &str, chunk_count: usize, compression: CompressionMethod) -> Self {
        let checksum = compute_checksum(data.as_bytes());
        let compression_str = match compression {
            CompressionMethod::None => "none",
            CompressionMethod::Gzip { .. } => "gzip",
            CompressionMethod::Zstd { .. } => "zstd",
        };

        Self {
            format: "chunked".to_string(),
            total_size: data.len(),
            chunk_count,
            chunk_size: CHUNK_SIZE,
            checksum,
            compression: compression_str.to_string(),
        }
    }

    /// Parse compression method from string
    pub fn compression_method(&self) -> CompressionMethod {
        match self.compression.as_str() {
            "none" => CompressionMethod::None,
            "gzip" => CompressionMethod::Gzip { level: 6 },
            "zstd" => CompressionMethod::Zstd { level: 3 },
            _ => CompressionMethod::Zstd { level: 3 },
        }
    }
}

/// Compute SHA-256 checksum of data
pub fn compute_checksum(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    format!("sha256:{}", hex::encode(hasher.finalize()))
}

/// Verify checksum of data
pub fn verify_checksum(data: &[u8], expected: &str) -> bool {
    compute_checksum(data) == expected
}

/// Split data into chunks
pub fn split_into_chunks(data: &str) -> Vec<String> {
    data.as_bytes()
        .chunks(CHUNK_SIZE)
        .map(|chunk| String::from_utf8_lossy(chunk).into_owned())
        .collect()
}

/// Check if encoded data needs chunking
#[allow(dead_code)]
pub fn needs_chunking(encoded: &str) -> bool {
    encoded.len() > MAX_RESOURCE_SIZE
}

/// Generate chunk Secret name
pub fn chunk_secret_name(base_key: &str, index: usize) -> String {
    format!("{}.chunk.{}", base_key, index)
}

/// Labels for a chunk Secret
pub fn chunk_labels(
    release: &StoredRelease,
    parent_key: &str,
    chunk_index: usize,
) -> BTreeMap<String, String> {
    let mut labels = storage_labels(release);
    labels.insert(
        "sherpack.io/storage-driver".to_string(),
        "secrets".to_string(),
    );
    labels.insert("sherpack.io/chunked".to_string(), "true".to_string());
    labels.insert(
        "sherpack.io/chunk-index".to_string(),
        chunk_index.to_string(),
    );
    labels.insert(
        "sherpack.io/chunk-parent".to_string(),
        parent_key.to_string(),
    );
    labels
}

/// Labels for an index Secret (chunked release)
pub fn index_labels(release: &StoredRelease, compression: &str) -> BTreeMap<String, String> {
    let mut labels = storage_labels(release);
    labels.insert(
        "sherpack.io/storage-driver".to_string(),
        "secrets".to_string(),
    );
    labels.insert("sherpack.io/chunked".to_string(), "true".to_string());
    labels.insert(
        "sherpack.io/compression".to_string(),
        compression.to_string(),
    );
    labels
}

/// Build a chunk Secret
pub fn build_chunk_secret(
    release: &StoredRelease,
    parent_key: &str,
    chunk_index: usize,
    chunk_data: &str,
) -> Secret {
    let name = chunk_secret_name(parent_key, chunk_index);
    let labels = chunk_labels(release, parent_key, chunk_index);

    let mut data = BTreeMap::new();
    data.insert(
        "chunk".to_string(),
        k8s_openapi::ByteString(chunk_data.as_bytes().to_vec()),
    );
    data.insert(
        "index".to_string(),
        k8s_openapi::ByteString(chunk_index.to_string().into_bytes()),
    );

    Secret {
        metadata: ObjectMeta {
            name: Some(name),
            namespace: Some(release.namespace.clone()),
            labels: Some(labels),
            ..Default::default()
        },
        type_: Some("sherpack.io/release-chunk.v1".to_string()),
        data: Some(data),
        ..Default::default()
    }
}

/// Build an index Secret for chunked data
pub fn build_index_secret(release: &StoredRelease, index: &ChunkedIndex) -> Result<Secret> {
    let key = release.storage_key();
    let labels = index_labels(release, &index.compression);

    let index_json =
        serde_json::to_string(index).map_err(|e| KubeError::Serialization(e.to_string()))?;

    let mut data = BTreeMap::new();
    data.insert(
        "index".to_string(),
        k8s_openapi::ByteString(index_json.into_bytes()),
    );

    Ok(Secret {
        metadata: ObjectMeta {
            name: Some(key),
            namespace: Some(release.namespace.clone()),
            labels: Some(labels),
            ..Default::default()
        },
        type_: Some("sherpack.io/release.v1".to_string()),
        data: Some(data),
        ..Default::default()
    })
}

/// Check if a Secret is a chunked index
pub fn is_chunked_index(secret: &Secret) -> bool {
    secret
        .metadata
        .labels
        .as_ref()
        .and_then(|l| l.get("sherpack.io/chunked"))
        .map(|v| v == "true")
        .unwrap_or(false)
        && secret
            .data
            .as_ref()
            .map(|d| d.contains_key("index"))
            .unwrap_or(false)
        && !secret
            .data
            .as_ref()
            .map(|d| d.contains_key("release"))
            .unwrap_or(false)
}

/// Parse chunked index from Secret
pub fn parse_chunked_index(secret: &Secret) -> Result<ChunkedIndex> {
    let data = secret
        .data
        .as_ref()
        .and_then(|d| d.get("index"))
        .ok_or_else(|| KubeError::Storage("Chunked secret missing 'index' data".to_string()))?;

    let index_str = String::from_utf8(data.0.clone())
        .map_err(|e| KubeError::Storage(format!("Invalid UTF-8 in index: {}", e)))?;

    serde_json::from_str(&index_str)
        .map_err(|e| KubeError::Serialization(format!("Failed to parse chunked index: {}", e)))
}

/// Chunked storage operations
pub struct ChunkedStorage {
    client: Client,
}

impl ChunkedStorage {
    pub fn new(client: Client) -> Self {
        Self { client }
    }

    fn secrets_api(&self, namespace: &str) -> Api<Secret> {
        Api::namespaced(self.client.clone(), namespace)
    }

    /// Create a chunked release
    pub async fn create_chunked(
        &self,
        release: &StoredRelease,
        encoded_data: &str,
        compression: CompressionMethod,
    ) -> Result<()> {
        let key = release.storage_key();
        let api = self.secrets_api(&release.namespace);

        // Clean up any orphaned chunks from failed previous attempts
        self.cleanup_orphaned_chunks(&release.namespace, &key)
            .await?;

        // Split data into chunks
        let chunks = split_into_chunks(encoded_data);
        let chunk_count = chunks.len();

        // Create chunk index
        let index = ChunkedIndex::new(encoded_data, chunk_count, compression);

        // Create all chunk Secrets first
        let pp = PostParams::default();
        for (i, chunk_data) in chunks.iter().enumerate() {
            let chunk_secret = build_chunk_secret(release, &key, i, chunk_data);
            api.create(&pp, &chunk_secret)
                .await
                .map_err(|e| KubeError::Storage(format!("Failed to create chunk {}: {}", i, e)))?;
        }

        // Create index Secret last (makes release visible)
        let index_secret = build_index_secret(release, &index)?;
        api.create(&pp, &index_secret).await.map_err(|e| {
            // If index creation fails, clean up chunks
            KubeError::Storage(format!("Failed to create index: {}", e))
        })?;

        Ok(())
    }

    /// Read a chunked release
    pub async fn read_chunked(&self, namespace: &str, index_secret: &Secret) -> Result<String> {
        let index = parse_chunked_index(index_secret)?;
        let parent_key = index_secret
            .metadata
            .name
            .as_ref()
            .ok_or_else(|| KubeError::Storage("Index secret has no name".to_string()))?;

        let api = self.secrets_api(namespace);

        // List all chunks for this release
        let label_selector = format!("sherpack.io/chunk-parent={}", parent_key);
        let lp = ListParams::default().labels(&label_selector);
        let chunk_secrets = api.list(&lp).await?;

        if chunk_secrets.items.len() != index.chunk_count {
            return Err(KubeError::Storage(format!(
                "Expected {} chunks, found {}. Release may be corrupted.",
                index.chunk_count,
                chunk_secrets.items.len()
            )));
        }

        // Sort chunks by index
        let mut chunks: Vec<(usize, String)> = Vec::new();
        for secret in &chunk_secrets.items {
            let chunk_index = secret
                .metadata
                .labels
                .as_ref()
                .and_then(|l| l.get("sherpack.io/chunk-index"))
                .and_then(|i| i.parse::<usize>().ok())
                .ok_or_else(|| KubeError::Storage("Chunk missing index label".to_string()))?;

            let chunk_data = secret
                .data
                .as_ref()
                .and_then(|d| d.get("chunk"))
                .ok_or_else(|| KubeError::Storage("Chunk missing data".to_string()))?;

            let data_str = String::from_utf8(chunk_data.0.clone())
                .map_err(|e| KubeError::Storage(format!("Invalid UTF-8 in chunk: {}", e)))?;

            chunks.push((chunk_index, data_str));
        }

        // Sort by index
        chunks.sort_by_key(|(i, _)| *i);

        // Verify we have all chunks in order
        for (expected, (actual, _)) in chunks.iter().enumerate() {
            if expected != *actual {
                return Err(KubeError::Storage(format!(
                    "Missing chunk {}. Found chunks: {:?}",
                    expected,
                    chunks.iter().map(|(i, _)| i).collect::<Vec<_>>()
                )));
            }
        }

        // Concatenate chunks
        let complete_data: String = chunks.into_iter().map(|(_, data)| data).collect();

        // Verify checksum
        if !verify_checksum(complete_data.as_bytes(), &index.checksum) {
            return Err(KubeError::Storage(
                "Checksum mismatch. Release data may be corrupted.".to_string(),
            ));
        }

        Ok(complete_data)
    }

    /// Delete a chunked release
    pub async fn delete_chunked(&self, namespace: &str, key: &str) -> Result<()> {
        let api = self.secrets_api(namespace);
        let dp = DeleteParams::default();

        // Delete index first (makes release invisible)
        let _ = api.delete(key, &dp).await;

        // Delete all chunks
        let label_selector = format!("sherpack.io/chunk-parent={}", key);
        let lp = ListParams::default().labels(&label_selector);
        let chunk_secrets = api.list(&lp).await?;

        for chunk in &chunk_secrets.items {
            if let Some(name) = &chunk.metadata.name {
                let _ = api.delete(name, &dp).await;
            }
        }

        Ok(())
    }

    /// Clean up orphaned chunks (from failed creates)
    pub async fn cleanup_orphaned_chunks(&self, namespace: &str, parent_key: &str) -> Result<()> {
        let api = self.secrets_api(namespace);

        // Check if index exists
        match api.get(parent_key).await {
            Ok(_) => {
                // Index exists, chunks are not orphaned
                return Ok(());
            }
            Err(kube::Error::Api(e)) if e.code == 404 => {
                // Index doesn't exist, clean up any orphaned chunks
            }
            Err(e) => return Err(e.into()),
        }

        // Find and delete orphaned chunks
        let label_selector = format!("sherpack.io/chunk-parent={}", parent_key);
        let lp = ListParams::default().labels(&label_selector);
        let chunk_secrets = api.list(&lp).await?;

        if !chunk_secrets.items.is_empty() {
            let dp = DeleteParams::default();
            for chunk in &chunk_secrets.items {
                if let Some(name) = &chunk.metadata.name {
                    let _ = api.delete(name, &dp).await;
                }
            }
        }

        Ok(())
    }

    /// Update a chunked release
    pub async fn update_chunked(
        &self,
        release: &StoredRelease,
        encoded_data: &str,
        compression: CompressionMethod,
    ) -> Result<()> {
        let key = release.storage_key();

        // Delete old chunks and index
        self.delete_chunked(&release.namespace, &key).await?;

        // Create new chunked release
        self.create_chunked(release, encoded_data, compression)
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_checksum() {
        let data = b"Hello, World!";
        let checksum = compute_checksum(data);
        assert!(checksum.starts_with("sha256:"));
        assert_eq!(checksum.len(), 7 + 64); // "sha256:" + 64 hex chars
    }

    #[test]
    fn test_verify_checksum() {
        let data = b"Test data for checksum";
        let checksum = compute_checksum(data);
        assert!(verify_checksum(data, &checksum));
        assert!(!verify_checksum(b"Different data", &checksum));
    }

    #[test]
    fn test_split_into_chunks() {
        let data = "a".repeat(CHUNK_SIZE * 2 + 100);
        let chunks = split_into_chunks(&data);
        assert_eq!(chunks.len(), 3);
        assert_eq!(chunks[0].len(), CHUNK_SIZE);
        assert_eq!(chunks[1].len(), CHUNK_SIZE);
        assert_eq!(chunks[2].len(), 100);
    }

    #[test]
    fn test_split_small_data() {
        let data = "small data";
        let chunks = split_into_chunks(data);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0], "small data");
    }

    #[test]
    fn test_needs_chunking() {
        let small = "a".repeat(MAX_RESOURCE_SIZE - 1);
        assert!(!needs_chunking(&small));

        let large = "a".repeat(MAX_RESOURCE_SIZE + 1);
        assert!(needs_chunking(&large));
    }

    #[test]
    fn test_chunk_secret_name() {
        let name = chunk_secret_name("sh.sherpack.release.v1.myapp.v3", 2);
        assert_eq!(name, "sh.sherpack.release.v1.myapp.v3.chunk.2");
    }

    #[test]
    fn test_chunked_index_serialization() {
        let index = ChunkedIndex::new(
            &"a".repeat(2_000_000),
            3,
            CompressionMethod::Zstd { level: 3 },
        );

        let json = serde_json::to_string(&index).unwrap();
        let parsed: ChunkedIndex = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.format, "chunked");
        assert_eq!(parsed.chunk_count, 3);
        assert_eq!(parsed.compression, "zstd");
        assert!(parsed.checksum.starts_with("sha256:"));
    }

    #[test]
    fn test_compression_method_parsing() {
        let index = ChunkedIndex {
            format: "chunked".to_string(),
            total_size: 1000,
            chunk_count: 1,
            chunk_size: CHUNK_SIZE,
            checksum: "sha256:abc".to_string(),
            compression: "gzip".to_string(),
        };

        assert!(matches!(
            index.compression_method(),
            CompressionMethod::Gzip { .. }
        ));
    }

    #[test]
    fn test_chunk_labels() {
        use sherpack_core::{PackMetadata, Values};

        let release = StoredRelease::for_install(
            "myapp".to_string(),
            "default".to_string(),
            PackMetadata {
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
            Values::new(),
            "manifest".to_string(),
        );

        let labels = chunk_labels(&release, "sh.sherpack.release.v1.myapp.v1", 2);

        assert_eq!(labels.get("sherpack.io/chunked"), Some(&"true".to_string()));
        assert_eq!(
            labels.get("sherpack.io/chunk-index"),
            Some(&"2".to_string())
        );
        assert_eq!(
            labels.get("sherpack.io/chunk-parent"),
            Some(&"sh.sherpack.release.v1.myapp.v1".to_string())
        );
    }

    #[test]
    fn test_is_chunked_index() {
        // Not chunked - has "release" key
        let regular_secret = Secret {
            metadata: ObjectMeta::default(),
            data: Some({
                let mut data = BTreeMap::new();
                data.insert("release".to_string(), k8s_openapi::ByteString(vec![]));
                data
            }),
            ..Default::default()
        };
        assert!(!is_chunked_index(&regular_secret));

        // Chunked - has "index" key and chunked label
        let chunked_secret = Secret {
            metadata: ObjectMeta {
                labels: Some({
                    let mut labels = BTreeMap::new();
                    labels.insert("sherpack.io/chunked".to_string(), "true".to_string());
                    labels
                }),
                ..Default::default()
            },
            data: Some({
                let mut data = BTreeMap::new();
                data.insert("index".to_string(), k8s_openapi::ByteString(vec![]));
                data
            }),
            ..Default::default()
        };
        assert!(is_chunked_index(&chunked_secret));
    }
}
