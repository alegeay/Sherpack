//! HTTP repository implementation
//!
//! Supports traditional Helm-style HTTP repositories with index.yaml

use std::path::PathBuf;

use crate::config::Repository;
use crate::credentials::{CachedResponse, ResolvedCredentials, ScopedCredentials, SecureHttpClient};
use crate::error::{RepoError, Result};
use crate::index::{PackEntry, RepositoryIndex};

/// HTTP repository client
pub struct HttpRepository {
    /// Repository configuration
    repo: Repository,
    /// HTTP client with secure credential handling
    client: SecureHttpClient,
    /// Cached index
    cached_index: Option<RepositoryIndex>,
}

impl HttpRepository {
    /// Create a new HTTP repository client
    pub fn new(repo: Repository, credentials: Option<ResolvedCredentials>) -> Result<Self> {
        let mut scoped = ScopedCredentials::default();
        if let Some(creds) = credentials {
            scoped.add(&repo.url, creds);
        }

        let client = SecureHttpClient::new(scoped)?;

        Ok(Self {
            repo,
            client,
            cached_index: None,
        })
    }

    /// Create for a public repository (no auth)
    pub fn public(repo: Repository) -> Result<Self> {
        Self::new(repo, None)
    }

    /// Get the repository name
    pub fn name(&self) -> &str {
        &self.repo.name
    }

    /// Get the repository URL
    pub fn url(&self) -> &str {
        &self.repo.url
    }

    /// Fetch or refresh the repository index
    pub async fn fetch_index(&mut self) -> Result<&RepositoryIndex> {
        let index_url = self.repo.index_url();

        // Use ETag for conditional request if we have a cached index
        let response = self
            .client
            .get_cached(&index_url, self.repo.etag.as_deref())
            .await?;

        match response {
            CachedResponse::NotModified => {
                // Index hasn't changed, use cached version
                if self.cached_index.is_none() {
                    return Err(RepoError::CacheError {
                        message: "Received 304 but no cached index".to_string(),
                    });
                }
            }
            CachedResponse::Fresh { data, etag } => {
                // Parse new index
                let index = RepositoryIndex::from_bytes(&data)?;
                self.cached_index = Some(index);

                // Store new ETag (would need to save to config)
                if etag.is_some() {
                    // Note: caller should save updated repo config
                    // self.repo.etag = etag;
                }
            }
        }

        self.cached_index
            .as_ref()
            .ok_or(RepoError::IndexNotFound {
                url: index_url,
            })
    }

    /// Get the cached index without fetching
    pub fn index(&self) -> Option<&RepositoryIndex> {
        self.cached_index.as_ref()
    }

    /// Search packs in the repository
    pub async fn search(&mut self, query: &str) -> Result<Vec<&PackEntry>> {
        let index = self.fetch_index().await?;
        Ok(index.search(query))
    }

    /// Get the latest version of a pack
    pub async fn get_latest(&mut self, name: &str) -> Result<PackEntry> {
        let repo_name = self.repo.name.clone();
        let index = self.fetch_index().await?;
        index.get_latest(name).cloned().ok_or_else(|| RepoError::PackNotFound {
            name: name.to_string(),
            repo: repo_name,
        })
    }

    /// Get a specific version of a pack
    pub async fn get_version(&mut self, name: &str, version: &str) -> Result<PackEntry> {
        let repo_name = self.repo.name.clone();
        let index = self.fetch_index().await?;
        index
            .get_version(name, version)
            .cloned()
            .ok_or_else(|| RepoError::VersionNotFound {
                name: name.to_string(),
                version: version.to_string(),
                repo: repo_name,
            })
    }

    /// Find best matching version for a constraint
    pub async fn find_best_match(&mut self, name: &str, constraint: &str) -> Result<PackEntry> {
        let index = self.fetch_index().await?;
        index.find_best_match(name, constraint).cloned()
    }

    /// Download a pack archive
    pub async fn download(&self, entry: &PackEntry) -> Result<Vec<u8>> {
        let url = entry.download_url().ok_or_else(|| RepoError::PackNotFound {
            name: entry.name.clone(),
            repo: self.repo.name.clone(),
        })?;

        // Resolve relative URLs
        let full_url = if url.starts_with("http://") || url.starts_with("https://") {
            url.to_string()
        } else {
            format!("{}/{}", self.repo.url.trim_end_matches('/'), url)
        };

        let data = self.client.get_bytes(&full_url).await?;

        // Verify digest if present
        if let Some(expected_digest) = &entry.digest {
            let actual_digest = compute_digest(&data);
            if !digest_matches(expected_digest, &actual_digest) {
                return Err(RepoError::IntegrityCheckFailed {
                    name: entry.name.clone(),
                    expected: expected_digest.clone(),
                    actual: actual_digest,
                });
            }
        }

        Ok(data)
    }

    /// Download and extract a pack to a directory
    pub async fn download_to(&self, entry: &PackEntry, dest: &PathBuf) -> Result<()> {
        let data = self.download(entry).await?;

        // Extract the archive
        extract_pack_archive(&data, dest)?;

        Ok(())
    }

    /// List all packs in the repository
    pub async fn list(&mut self) -> Result<Vec<&PackEntry>> {
        let index = self.fetch_index().await?;
        Ok(index
            .entries
            .values()
            .filter_map(|versions| {
                versions.iter().max_by(|a, b| {
                    let va = semver::Version::parse(&a.version).ok();
                    let vb = semver::Version::parse(&b.version).ok();
                    va.cmp(&vb)
                })
            })
            .collect())
    }
}

/// Compute SHA256 digest of data
fn compute_digest(data: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(data);
    let result = hasher.finalize();
    format!("sha256:{}", hex::encode(result))
}

/// Check if two digests match (supports various formats)
fn digest_matches(expected: &str, actual: &str) -> bool {
    // Normalize both digests
    let norm_expected = expected
        .trim()
        .to_lowercase()
        .replace("sha256:", "")
        .replace("sha256-", "");
    let norm_actual = actual
        .trim()
        .to_lowercase()
        .replace("sha256:", "")
        .replace("sha256-", "");

    norm_expected == norm_actual
}

/// Extract a pack archive (tar.gz) to a directory
fn extract_pack_archive(data: &[u8], dest: &PathBuf) -> Result<()> {
    use flate2::read::GzDecoder;
    use tar::Archive;

    let gz = GzDecoder::new(std::io::Cursor::new(data));
    let mut archive = Archive::new(gz);

    std::fs::create_dir_all(dest)?;
    archive.unpack(dest)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_digest() {
        let data = b"hello world";
        let digest = compute_digest(data);
        assert!(digest.starts_with("sha256:"));
        assert_eq!(digest.len(), 7 + 64); // "sha256:" + 64 hex chars
    }

    #[test]
    fn test_digest_matches() {
        let d1 = "sha256:abc123";
        let d2 = "sha256:ABC123";
        let d3 = "abc123";
        let d4 = "sha256-abc123";

        assert!(digest_matches(d1, d2));
        assert!(digest_matches(d1, d3));
        assert!(digest_matches(d1, d4));
        assert!(!digest_matches(d1, "sha256:xyz789"));
    }
}
