//! OCI Registry client
//!
//! Basic push/pull operations for OCI-compliant registries.
//! NOTE: Search is NOT supported due to catalog API limitations across registries.

use oci_distribution::Reference;
use oci_distribution::client::{Client, ClientConfig, ClientProtocol};
use oci_distribution::secrets::RegistryAuth;
use std::path::Path;

use crate::config::Repository;
use crate::credentials::ResolvedCredentials;
use crate::error::{RepoError, Result};

/// Media types for Helm/Sherpack charts in OCI
pub mod media_types {
    /// Helm chart config
    pub const HELM_CONFIG: &str = "application/vnd.cncf.helm.config.v1+json";
    /// Helm chart content layer
    pub const HELM_CONTENT: &str = "application/vnd.cncf.helm.chart.content.v1.tar+gzip";
    /// Helm chart provenance layer
    pub const HELM_PROVENANCE: &str = "application/vnd.cncf.helm.chart.provenance.v1.prov";
}

/// OCI registry client
pub struct OciRegistry {
    /// Repository configuration
    repo: Repository,
    /// OCI client
    client: Client,
    /// Authentication
    auth: RegistryAuth,
}

impl OciRegistry {
    /// Create a new OCI registry client
    pub fn new(repo: Repository, credentials: Option<ResolvedCredentials>) -> Result<Self> {
        let auth = match credentials {
            Some(ResolvedCredentials::Basic { username, password }) => {
                RegistryAuth::Basic(username, password)
            }
            Some(ResolvedCredentials::Bearer { token }) => {
                // OCI registries typically use basic auth or token exchange
                // Bearer tokens need special handling depending on registry
                RegistryAuth::Basic(String::new(), token)
            }
            Some(ResolvedCredentials::DockerAuth(config)) => {
                // Try to get auth from docker config
                if let Some(auth_header) = config.auth_for_url(&repo.url) {
                    // Parse "Basic base64" format
                    if let Some(encoded) = auth_header.strip_prefix("Basic ") {
                        if let Ok(decoded) = base64::Engine::decode(
                            &base64::engine::general_purpose::STANDARD,
                            encoded,
                        ) {
                            if let Ok(creds) = String::from_utf8(decoded) {
                                if let Some((user, pass)) = creds.split_once(':') {
                                    return Ok(Self {
                                        repo,
                                        client: Self::create_client()?,
                                        auth: RegistryAuth::Basic(
                                            user.to_string(),
                                            pass.to_string(),
                                        ),
                                    });
                                }
                            }
                        }
                    }
                }
                RegistryAuth::Anonymous
            }
            None => RegistryAuth::Anonymous,
        };

        Ok(Self {
            repo,
            client: Self::create_client()?,
            auth,
        })
    }

    fn create_client() -> Result<Client> {
        let config = ClientConfig {
            protocol: ClientProtocol::Https,
            ..Default::default()
        };
        Ok(Client::new(config))
    }

    /// Get the repository name
    pub fn name(&self) -> &str {
        &self.repo.name
    }

    /// Get the repository URL
    pub fn url(&self) -> &str {
        &self.repo.url
    }

    /// Parse an OCI reference string
    ///
    /// Format: oci://registry/repo:tag or registry/repo:tag
    pub fn parse_reference(reference: &str) -> Result<Reference> {
        let clean = reference
            .trim_start_matches("oci://")
            .trim_start_matches("https://")
            .trim_start_matches("http://");

        Reference::try_from(clean).map_err(|e| RepoError::InvalidOciReference {
            reference: format!("{}: {}", reference, e),
        })
    }

    /// Pull a pack from the registry
    pub async fn pull(&self, name: &str, tag: &str) -> Result<Vec<u8>> {
        let reference = self.build_reference(name, tag)?;

        // Pull the image data (manifest + layers)
        let image_data = self
            .client
            .pull(
                &reference,
                &self.auth,
                vec![media_types::HELM_CONFIG, media_types::HELM_CONTENT],
            )
            .await
            .map_err(|e| RepoError::OciError {
                message: format!("Failed to pull: {}", e),
            })?;

        // Find the chart content layer
        let chart_layer = image_data
            .layers
            .iter()
            .find(|l| l.media_type == media_types::HELM_CONTENT)
            .ok_or_else(|| RepoError::OciError {
                message: "No chart content layer found in manifest".to_string(),
            })?;

        Ok(chart_layer.data.clone())
    }

    /// Pull and extract a pack to a directory
    pub async fn pull_to(&self, name: &str, tag: &str, dest: &Path) -> Result<()> {
        let data = self.pull(name, tag).await?;
        extract_pack_archive(&data, dest)?;
        Ok(())
    }

    /// Push a pack to the registry
    pub async fn push(&self, name: &str, tag: &str, archive_data: &[u8]) -> Result<String> {
        let reference = self.build_reference(name, tag)?;

        // Create config blob (minimal chart metadata)
        let config_data = b"{}";
        let config = oci_distribution::client::Config {
            data: config_data.to_vec(),
            media_type: media_types::HELM_CONFIG.to_string(),
            annotations: None,
        };

        // Create chart layer
        let layers = vec![oci_distribution::client::ImageLayer {
            data: archive_data.to_vec(),
            media_type: media_types::HELM_CONTENT.to_string(),
            annotations: None,
        }];

        // Push to registry
        let result = self
            .client
            .push(&reference, &layers, config, &self.auth, None)
            .await
            .map_err(|e| RepoError::OciPushFailed {
                message: e.to_string(),
            })?;

        Ok(result.manifest_url)
    }

    /// List tags for a pack in the registry
    pub async fn list_tags(&self, name: &str) -> Result<Vec<String>> {
        let reference = self.build_reference(name, "latest")?;

        let tags = self
            .client
            .list_tags(&reference, &self.auth, None, None)
            .await
            .map_err(|e| RepoError::OciError {
                message: format!("Failed to list tags: {}", e),
            })?;

        Ok(tags.tags)
    }

    /// Check if a specific tag exists
    pub async fn exists(&self, name: &str, tag: &str) -> Result<bool> {
        let reference = self.build_reference(name, tag)?;

        // Try to fetch manifest - if it succeeds, tag exists
        match self
            .client
            .fetch_manifest_digest(&reference, &self.auth)
            .await
        {
            Ok(_) => Ok(true),
            Err(e) => {
                let error_str = e.to_string().to_lowercase();
                // Check for various "not found" error messages
                if error_str.contains("not found")
                    || error_str.contains("manifest unknown")
                    || error_str.contains("404")
                {
                    Ok(false)
                } else {
                    Err(RepoError::OciError {
                        message: e.to_string(),
                    })
                }
            }
        }
    }

    /// Build an OCI reference from pack name and tag
    fn build_reference(&self, name: &str, tag: &str) -> Result<Reference> {
        // Extract registry and base path from repo URL
        // oci://ghcr.io/myorg/charts -> ghcr.io/myorg/charts/name:tag
        let base = self
            .repo
            .url
            .trim_start_matches("oci://")
            .trim_end_matches('/');

        let full_ref = format!("{}/{}:{}", base, name, tag);
        Self::parse_reference(&full_ref)
    }
}

/// OCI reference helper
#[derive(Debug, Clone)]
pub struct OciReference {
    pub registry: String,
    pub repository: String,
    pub tag: Option<String>,
    pub digest: Option<String>,
}

impl OciReference {
    /// Parse an OCI reference string
    pub fn parse(s: &str) -> Result<Self> {
        let clean = s
            .trim_start_matches("oci://")
            .trim_start_matches("https://")
            .trim_start_matches("http://");

        // Split registry from path
        let (registry, rest) =
            clean
                .split_once('/')
                .ok_or_else(|| RepoError::InvalidOciReference {
                    reference: s.to_string(),
                })?;

        // Check for digest
        if let Some((repo_tag, digest)) = rest.rsplit_once('@') {
            let (repository, tag) = if let Some((r, t)) = repo_tag.rsplit_once(':') {
                (r.to_string(), Some(t.to_string()))
            } else {
                (repo_tag.to_string(), None)
            };

            return Ok(Self {
                registry: registry.to_string(),
                repository,
                tag,
                digest: Some(digest.to_string()),
            });
        }

        // Check for tag
        if let Some((repository, tag)) = rest.rsplit_once(':') {
            Ok(Self {
                registry: registry.to_string(),
                repository: repository.to_string(),
                tag: Some(tag.to_string()),
                digest: None,
            })
        } else {
            Ok(Self {
                registry: registry.to_string(),
                repository: rest.to_string(),
                tag: None,
                digest: None,
            })
        }
    }

    /// Convert to string representation
    pub fn to_oci_string(&self) -> String {
        let mut s = format!("{}/{}", self.registry, self.repository);
        if let Some(tag) = &self.tag {
            s.push(':');
            s.push_str(tag);
        }
        if let Some(digest) = &self.digest {
            s.push('@');
            s.push_str(digest);
        }
        s
    }
}

/// Extract a pack archive to a directory
fn extract_pack_archive(data: &[u8], dest: &Path) -> Result<()> {
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
    fn test_oci_reference_parse() {
        // Full reference with tag
        let ref1 = OciReference::parse("oci://ghcr.io/myorg/charts/nginx:1.0.0").unwrap();
        assert_eq!(ref1.registry, "ghcr.io");
        assert_eq!(ref1.repository, "myorg/charts/nginx");
        assert_eq!(ref1.tag, Some("1.0.0".to_string()));
        assert!(ref1.digest.is_none());

        // Without oci:// prefix
        let ref2 = OciReference::parse("docker.io/library/nginx:latest").unwrap();
        assert_eq!(ref2.registry, "docker.io");
        assert_eq!(ref2.repository, "library/nginx");
        assert_eq!(ref2.tag, Some("latest".to_string()));

        // With digest
        let ref3 = OciReference::parse("ghcr.io/myorg/nginx:1.0@sha256:abc123").unwrap();
        assert_eq!(ref3.registry, "ghcr.io");
        assert_eq!(ref3.repository, "myorg/nginx");
        assert_eq!(ref3.tag, Some("1.0".to_string()));
        assert_eq!(ref3.digest, Some("sha256:abc123".to_string()));

        // Without tag
        let ref4 = OciReference::parse("ghcr.io/myorg/nginx").unwrap();
        assert!(ref4.tag.is_none());
    }

    #[test]
    fn test_oci_reference_to_string() {
        let r = OciReference {
            registry: "ghcr.io".to_string(),
            repository: "myorg/nginx".to_string(),
            tag: Some("1.0.0".to_string()),
            digest: None,
        };
        assert_eq!(r.to_oci_string(), "ghcr.io/myorg/nginx:1.0.0");

        let r2 = OciReference {
            registry: "docker.io".to_string(),
            repository: "library/nginx".to_string(),
            tag: Some("latest".to_string()),
            digest: Some("sha256:abc".to_string()),
        };
        assert_eq!(
            r2.to_oci_string(),
            "docker.io/library/nginx:latest@sha256:abc"
        );
    }
}
