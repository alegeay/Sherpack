//! Unified repository backend trait
//!
//! Provides a single interface for all repository types (HTTP, OCI, File)

use async_trait::async_trait;
use std::path::{Path, PathBuf};

use crate::config::{Repository, RepositoryType};
use crate::credentials::{CredentialStore, ResolvedCredentials};
use crate::error::{RepoError, Result};
use crate::http::HttpRepository;
use crate::index::PackEntry;
use crate::oci::OciRegistry;

/// Unified repository backend trait
#[async_trait]
pub trait RepositoryBackend: Send + Sync {
    /// Get repository name
    fn name(&self) -> &str;

    /// Get repository URL
    fn url(&self) -> &str;

    /// Get repository type
    fn repo_type(&self) -> RepositoryType;

    /// Refresh/update the repository index
    async fn refresh(&mut self) -> Result<()>;

    /// Search for packs
    async fn search(&mut self, query: &str) -> Result<Vec<PackEntry>>;

    /// List all packs (latest versions)
    async fn list(&mut self) -> Result<Vec<PackEntry>>;

    /// Get latest version of a pack
    async fn get_latest(&mut self, name: &str) -> Result<PackEntry>;

    /// Get a specific version of a pack
    async fn get_version(&mut self, name: &str, version: &str) -> Result<PackEntry>;

    /// Find best matching version for a constraint
    async fn find_best_match(&mut self, name: &str, constraint: &str) -> Result<PackEntry>;

    /// Download a pack archive
    async fn download(&self, name: &str, version: &str) -> Result<Vec<u8>>;

    /// Download and extract a pack to a directory
    async fn download_to(&self, name: &str, version: &str, dest: &Path) -> Result<()>;

    /// Check if a pack exists
    async fn exists(&mut self, name: &str, version: Option<&str>) -> Result<bool>;
}

/// Create a repository backend from configuration
pub async fn create_backend(
    repo: Repository,
    credentials: Option<ResolvedCredentials>,
) -> Result<Box<dyn RepositoryBackend>> {
    match repo.repo_type {
        RepositoryType::Http => {
            let http = HttpRepository::new(repo, credentials)?;
            Ok(Box::new(HttpBackend(http)))
        }
        RepositoryType::Oci => {
            let oci = OciRegistry::new(repo, credentials)?;
            Ok(Box::new(OciBackend(oci)))
        }
        RepositoryType::File => {
            let file = FileBackend::new(repo)?;
            Ok(Box::new(file))
        }
    }
}

/// Create a backend from repository name (loads from config)
pub async fn create_backend_by_name(
    name: &str,
    config: &crate::config::RepositoryConfig,
    cred_store: &CredentialStore,
) -> Result<Box<dyn RepositoryBackend>> {
    let repo = config
        .get(name)
        .ok_or_else(|| RepoError::RepositoryNotFound {
            name: name.to_string(),
        })?
        .clone();

    let credentials = if let Some(cred_ref) = &repo.credential_ref {
        cred_store.get(cred_ref).and_then(|c| c.resolve().ok())
    } else {
        cred_store.get(&repo.name).and_then(|c| c.resolve().ok())
    };

    create_backend(repo, credentials).await
}

// ============ HTTP Backend Wrapper ============

struct HttpBackend(HttpRepository);

#[async_trait]
impl RepositoryBackend for HttpBackend {
    fn name(&self) -> &str {
        self.0.name()
    }

    fn url(&self) -> &str {
        self.0.url()
    }

    fn repo_type(&self) -> RepositoryType {
        RepositoryType::Http
    }

    async fn refresh(&mut self) -> Result<()> {
        self.0.fetch_index().await?;
        Ok(())
    }

    async fn search(&mut self, query: &str) -> Result<Vec<PackEntry>> {
        let results = self.0.search(query).await?;
        Ok(results.into_iter().cloned().collect())
    }

    async fn list(&mut self) -> Result<Vec<PackEntry>> {
        let results = self.0.list().await?;
        Ok(results.into_iter().cloned().collect())
    }

    async fn get_latest(&mut self, name: &str) -> Result<PackEntry> {
        self.0.get_latest(name).await
    }

    async fn get_version(&mut self, name: &str, version: &str) -> Result<PackEntry> {
        self.0.get_version(name, version).await
    }

    async fn find_best_match(&mut self, name: &str, constraint: &str) -> Result<PackEntry> {
        self.0.find_best_match(name, constraint).await
    }

    async fn download(&self, name: &str, version: &str) -> Result<Vec<u8>> {
        let index = self.0.index().ok_or_else(|| RepoError::IndexNotFound {
            url: self.0.url().to_string(),
        })?;

        let entry = index
            .get_version(name, version)
            .ok_or_else(|| RepoError::VersionNotFound {
                name: name.to_string(),
                version: version.to_string(),
                repo: self.0.name().to_string(),
            })?;

        self.0.download(entry).await
    }

    async fn download_to(&self, name: &str, version: &str, dest: &Path) -> Result<()> {
        let data = self.download(name, version).await?;
        extract_archive(&data, dest)?;
        Ok(())
    }

    async fn exists(&mut self, name: &str, version: Option<&str>) -> Result<bool> {
        self.0.fetch_index().await?;
        let index = self.0.index().ok_or_else(|| RepoError::IndexNotFound {
            url: self.0.url().to_string(),
        })?;

        Ok(match version {
            Some(v) => index.get_version(name, v).is_some(),
            None => index.get(name).is_some(),
        })
    }
}

// ============ OCI Backend Wrapper ============

struct OciBackend(OciRegistry);

#[async_trait]
impl RepositoryBackend for OciBackend {
    fn name(&self) -> &str {
        self.0.name()
    }

    fn url(&self) -> &str {
        self.0.url()
    }

    fn repo_type(&self) -> RepositoryType {
        RepositoryType::Oci
    }

    async fn refresh(&mut self) -> Result<()> {
        // OCI doesn't have an index to refresh
        Ok(())
    }

    async fn search(&mut self, _query: &str) -> Result<Vec<PackEntry>> {
        // OCI search is limited - return warning
        Err(RepoError::OciError {
            message: "Search is not reliably supported for OCI registries. \
                     Use Artifact Hub (https://artifacthub.io) for chart discovery, \
                     or list tags with 'sherpack repo tags <name>'."
                .to_string(),
        })
    }

    async fn list(&mut self) -> Result<Vec<PackEntry>> {
        // Can't list all packs in OCI without catalog API
        Err(RepoError::OciError {
            message: "Listing all packs is not supported for OCI registries. \
                     Use 'sherpack repo tags <repo>/<name>' to list versions of a specific pack."
                .to_string(),
        })
    }

    async fn get_latest(&mut self, name: &str) -> Result<PackEntry> {
        // Try 'latest' tag
        let exists = self.0.exists(name, "latest").await?;
        if exists {
            Ok(PackEntry {
                name: name.to_string(),
                version: "latest".to_string(),
                app_version: None,
                description: None,
                home: None,
                icon: None,
                sources: vec![],
                keywords: vec![],
                maintainers: vec![],
                urls: vec![format!("{}/{}:latest", self.0.url(), name)],
                digest: None,
                created: None,
                deprecated: false,
                dependencies: vec![],
                annotations: std::collections::HashMap::new(),
                api_version: None,
                r#type: None,
            })
        } else {
            Err(RepoError::PackNotFound {
                name: name.to_string(),
                repo: self.0.name().to_string(),
            })
        }
    }

    async fn get_version(&mut self, name: &str, version: &str) -> Result<PackEntry> {
        let exists = self.0.exists(name, version).await?;
        if exists {
            Ok(PackEntry {
                name: name.to_string(),
                version: version.to_string(),
                app_version: None,
                description: None,
                home: None,
                icon: None,
                sources: vec![],
                keywords: vec![],
                maintainers: vec![],
                urls: vec![format!("{}/{}:{}", self.0.url(), name, version)],
                digest: None,
                created: None,
                deprecated: false,
                dependencies: vec![],
                annotations: std::collections::HashMap::new(),
                api_version: None,
                r#type: None,
            })
        } else {
            Err(RepoError::VersionNotFound {
                name: name.to_string(),
                version: version.to_string(),
                repo: self.0.name().to_string(),
            })
        }
    }

    async fn find_best_match(&mut self, name: &str, constraint: &str) -> Result<PackEntry> {
        // List tags and find best match
        let tags = self.0.list_tags(name).await?;

        let req =
            semver::VersionReq::parse(constraint).map_err(|e| RepoError::ResolutionFailed {
                message: format!("Invalid version constraint '{}': {}", constraint, e),
            })?;

        let best_version = tags
            .iter()
            .filter_map(|t| semver::Version::parse(t).ok())
            .filter(|v| req.matches(v))
            .max()
            .ok_or_else(|| RepoError::UnsatisfiableConstraint {
                name: name.to_string(),
                constraint: constraint.to_string(),
                available: tags.join(", "),
            })?;

        self.get_version(name, &best_version.to_string()).await
    }

    async fn download(&self, name: &str, version: &str) -> Result<Vec<u8>> {
        self.0.pull(name, version).await
    }

    async fn download_to(&self, name: &str, version: &str, dest: &Path) -> Result<()> {
        self.0.pull_to(name, version, dest).await
    }

    async fn exists(&mut self, name: &str, version: Option<&str>) -> Result<bool> {
        let tag = version.unwrap_or("latest");
        self.0.exists(name, tag).await
    }
}

// ============ File Backend ============

struct FileBackend {
    repo: Repository,
    root: PathBuf,
}

impl FileBackend {
    fn new(repo: Repository) -> Result<Self> {
        let root = PathBuf::from(
            repo.url
                .trim_start_matches("file://")
                .trim_start_matches('/'),
        );

        if !root.exists() {
            return Err(RepoError::RepositoryNotFound {
                name: repo.name.clone(),
            });
        }

        Ok(Self { repo, root })
    }
}

#[async_trait]
impl RepositoryBackend for FileBackend {
    fn name(&self) -> &str {
        &self.repo.name
    }

    fn url(&self) -> &str {
        &self.repo.url
    }

    fn repo_type(&self) -> RepositoryType {
        RepositoryType::File
    }

    async fn refresh(&mut self) -> Result<()> {
        Ok(())
    }

    async fn search(&mut self, query: &str) -> Result<Vec<PackEntry>> {
        let all = self.list().await?;
        Ok(all.into_iter().filter(|p| p.name.contains(query)).collect())
    }

    async fn list(&mut self) -> Result<Vec<PackEntry>> {
        let mut packs = Vec::new();

        for entry in std::fs::read_dir(&self.root)? {
            let entry = entry?;
            let path = entry.path();

            if path.is_dir() {
                let pack_yaml = path.join("Pack.yaml");
                if pack_yaml.exists() {
                    let content = std::fs::read_to_string(&pack_yaml)?;
                    if let Ok(pack) = serde_yaml::from_str::<sherpack_core::Pack>(&content) {
                        packs.push(PackEntry {
                            name: pack.metadata.name,
                            version: pack.metadata.version.to_string(),
                            app_version: pack.metadata.app_version,
                            description: pack.metadata.description,
                            home: pack.metadata.home,
                            icon: pack.metadata.icon,
                            sources: pack.metadata.sources,
                            keywords: pack.metadata.keywords,
                            maintainers: vec![],
                            urls: vec![format!("file://{}", path.display())],
                            digest: None,
                            created: None,
                            deprecated: false,
                            dependencies: vec![],
                            annotations: pack.metadata.annotations,
                            api_version: Some(pack.api_version),
                            r#type: None,
                        });
                    }
                }
            }
        }

        Ok(packs)
    }

    async fn get_latest(&mut self, name: &str) -> Result<PackEntry> {
        let all = self.list().await?;
        all.into_iter()
            .filter(|p| p.name == name)
            .max_by(|a, b| {
                let va = semver::Version::parse(&a.version).ok();
                let vb = semver::Version::parse(&b.version).ok();
                va.cmp(&vb)
            })
            .ok_or_else(|| RepoError::PackNotFound {
                name: name.to_string(),
                repo: self.repo.name.clone(),
            })
    }

    async fn get_version(&mut self, name: &str, version: &str) -> Result<PackEntry> {
        let all = self.list().await?;
        all.into_iter()
            .find(|p| p.name == name && p.version == version)
            .ok_or_else(|| RepoError::VersionNotFound {
                name: name.to_string(),
                version: version.to_string(),
                repo: self.repo.name.clone(),
            })
    }

    async fn find_best_match(&mut self, name: &str, constraint: &str) -> Result<PackEntry> {
        let req = semver::VersionReq::parse(constraint)?;
        let all = self.list().await?;

        all.into_iter()
            .filter(|p| p.name == name)
            .filter(|p| {
                semver::Version::parse(&p.version)
                    .map(|v| req.matches(&v))
                    .unwrap_or(false)
            })
            .max_by(|a, b| {
                let va = semver::Version::parse(&a.version).ok();
                let vb = semver::Version::parse(&b.version).ok();
                va.cmp(&vb)
            })
            .ok_or_else(|| RepoError::UnsatisfiableConstraint {
                name: name.to_string(),
                constraint: constraint.to_string(),
                available: "check local repository".to_string(),
            })
    }

    async fn download(&self, name: &str, _version: &str) -> Result<Vec<u8>> {
        // For file repos, we just return the path - actual "download" would be a copy
        Err(RepoError::Other(format!(
            "Use download_to for file repositories. Pack location: {}/{}",
            self.root.display(),
            name
        )))
    }

    async fn download_to(&self, name: &str, _version: &str, dest: &Path) -> Result<()> {
        let src = self.root.join(name);
        if !src.exists() {
            return Err(RepoError::PackNotFound {
                name: name.to_string(),
                repo: self.repo.name.clone(),
            });
        }

        // Copy directory
        copy_dir_recursive(&src, dest)?;
        Ok(())
    }

    async fn exists(&mut self, name: &str, version: Option<&str>) -> Result<bool> {
        let all = self.list().await?;
        Ok(all
            .iter()
            .any(|p| p.name == name && version.map(|v| p.version == v).unwrap_or(true)))
    }
}

/// Extract a tar.gz archive
fn extract_archive(data: &[u8], dest: &Path) -> Result<()> {
    use flate2::read::GzDecoder;
    use tar::Archive;

    let gz = GzDecoder::new(std::io::Cursor::new(data));
    let mut archive = Archive::new(gz);

    std::fs::create_dir_all(dest)?;
    archive.unpack(dest)?;

    Ok(())
}

/// Copy directory recursively
fn copy_dir_recursive(src: &Path, dest: &Path) -> Result<()> {
    std::fs::create_dir_all(dest)?;

    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let src_path = entry.path();
        let dest_path = dest.join(entry.file_name());

        if src_path.is_dir() {
            copy_dir_recursive(&src_path, &dest_path)?;
        } else {
            std::fs::copy(&src_path, &dest_path)?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_repository_type_detection() {
        let http_repo = Repository::new("test", "https://example.com").unwrap();
        assert_eq!(http_repo.repo_type, RepositoryType::Http);

        let oci_repo = Repository::new("test", "oci://ghcr.io/org/charts").unwrap();
        assert_eq!(oci_repo.repo_type, RepositoryType::Oci);

        let file_repo = Repository::new("test", "file:///path/to/repo").unwrap();
        assert_eq!(file_repo.repo_type, RepositoryType::File);
    }
}
