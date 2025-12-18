//! Sherpack Repository Management
//!
//! This crate provides repository management for Sherpack, including:
//!
//! - **HTTP repositories**: Traditional Helm-style repos with index.yaml
//! - **OCI registries**: Push/pull from Docker Hub, GHCR, ECR, etc.
//! - **Local file repositories**: For development and testing
//!
//! ## Key Features
//!
//! - **Unified interface**: Same commands work for HTTP and OCI
//! - **Secure credentials**: Scoped credentials with redirect protection
//! - **SQLite cache**: Fast local search with FTS5
//! - **Lock files**: Reproducible builds with integrity verification
//! - **Diamond detection**: Catch version conflicts before they cause problems
//!
//! ## Example
//!
//! ```rust,no_run
//! use sherpack_repo::{RepositoryConfig, Repository, RepositoryBackend, create_backend};
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! // Add a repository
//! let repo = Repository::new("bitnami", "https://charts.bitnami.com/bitnami")?;
//!
//! // Create backend (works for HTTP, OCI, or file repos)
//! let mut backend = create_backend(repo, None).await?;
//!
//! // Search for packs
//! let results = backend.search("nginx").await?;
//!
//! // Download a pack
//! let data = backend.download("nginx", "15.0.0").await?;
//! # Ok(())
//! # }
//! ```
//!
//! ## Security Notes
//!
//! - Credentials are NEVER sent after cross-origin redirects
//! - Lock files verify SHA256 integrity by default
//! - Diamond dependencies cause errors, not silent conflicts

pub mod error;
pub mod config;
pub mod credentials;
pub mod index;
pub mod http;
pub mod oci;
pub mod cache;
pub mod lock;
pub mod dependency;
pub mod backend;

// Re-exports for convenience
pub use error::{RepoError, Result};
pub use config::{Repository, RepositoryConfig, RepositoryType};
pub use credentials::{
    Credentials, CredentialStore, ResolvedCredentials, ScopedCredentials, SecureHttpClient,
};
pub use index::{PackEntry, RepositoryIndex};
pub use http::HttpRepository;
pub use oci::{OciRegistry, OciReference};
pub use cache::{IndexCache, CachedPack, CacheStats};
pub use lock::{LockFile, LockedDependency, LockPolicy, VerifyResult};
pub use dependency::{DependencyResolver, DependencyGraph, DependencySpec, ResolvedDependency};
pub use backend::{RepositoryBackend, create_backend, create_backend_by_name};
