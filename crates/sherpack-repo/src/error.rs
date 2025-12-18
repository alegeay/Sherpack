//! Error types for repository operations

use thiserror::Error;

/// Repository operation errors
#[derive(Debug, Error)]
pub enum RepoError {
    // ============ Configuration Errors ============
    #[error("Repository not found: {name}")]
    RepositoryNotFound { name: String },

    #[error("Repository already exists: {name}")]
    RepositoryAlreadyExists { name: String },

    #[error("Invalid repository URL: {url} - {reason}")]
    InvalidRepositoryUrl { url: String, reason: String },

    #[error("Invalid repository configuration: {message}")]
    InvalidConfig { message: String },

    // ============ Network Errors ============
    #[error("HTTP error: {status} - {message}")]
    HttpError { status: u16, message: String },

    #[error("Network error: {message}")]
    NetworkError { message: String },

    #[error("Request timeout after {seconds}s")]
    Timeout { seconds: u64 },

    #[error("Rate limited by server. Retry after {retry_after} seconds")]
    RateLimited { retry_after: u64 },

    // ============ Authentication Errors ============
    #[error("Authentication required for {url}")]
    AuthRequired { url: String },

    #[error("Authentication failed: {message}")]
    AuthFailed { message: String },

    #[error("Credential not found for repository: {name}")]
    CredentialNotFound { name: String },

    #[error("Token expired and refresh failed: {message}")]
    TokenExpired { message: String },

    // ============ Index Errors ============
    #[error("Index not found at {url}")]
    IndexNotFound { url: String },

    #[error("Invalid index format: {message}")]
    InvalidIndex { message: String },

    #[error("Index parse error: {message}")]
    IndexParseError { message: String },

    // ============ Pack Errors ============
    #[error("Pack not found: {name} in repository {repo}")]
    PackNotFound { name: String, repo: String },

    #[error("Version not found: {name}@{version} in repository {repo}")]
    VersionNotFound {
        name: String,
        version: String,
        repo: String,
    },

    #[error("No versions available for pack: {name}")]
    NoVersionsAvailable { name: String },

    // ============ Dependency Errors ============
    #[error("Dependency resolution failed: {message}")]
    ResolutionFailed { message: String },

    #[error("Diamond dependency conflict detected:\n{conflicts}")]
    DiamondConflict { conflicts: String },

    #[error("Circular dependency detected: {cycle}")]
    CircularDependency { cycle: String },

    #[error("Version constraint unsatisfiable: {name} requires {constraint}, available: {available}")]
    UnsatisfiableConstraint {
        name: String,
        constraint: String,
        available: String,
    },

    // ============ Lock File Errors ============
    #[error("Lock file not found: {path}")]
    LockFileNotFound { path: String },

    #[error("Lock file outdated - Pack.yaml has changed. Run 'sherpack dependency update'")]
    LockFileOutdated,

    #[error("Integrity check failed for {name}: expected {expected}, got {actual}")]
    IntegrityCheckFailed {
        name: String,
        expected: String,
        actual: String,
    },

    // ============ OCI Errors ============
    #[error("OCI registry error: {message}")]
    OciError { message: String },

    #[error("Invalid OCI reference: {reference}")]
    InvalidOciReference { reference: String },

    #[error("OCI manifest not found: {reference}")]
    OciManifestNotFound { reference: String },

    #[error("OCI push failed: {message}")]
    OciPushFailed { message: String },

    // ============ Cache Errors ============
    #[error("Cache error: {message}")]
    CacheError { message: String },

    #[error("Cache corrupted, rebuilding: {message}")]
    CacheCorrupted { message: String },

    // ============ IO Errors ============
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    Serialization(String),

    // ============ Other ============
    #[error("Operation cancelled")]
    Cancelled,

    #[error("{0}")]
    Other(String),
}

/// Result type for repository operations
pub type Result<T> = std::result::Result<T, RepoError>;

impl From<reqwest::Error> for RepoError {
    fn from(e: reqwest::Error) -> Self {
        if e.is_timeout() {
            RepoError::Timeout { seconds: 30 }
        } else if e.is_connect() {
            RepoError::NetworkError {
                message: format!("Connection failed: {}", e),
            }
        } else if let Some(status) = e.status() {
            RepoError::HttpError {
                status: status.as_u16(),
                message: e.to_string(),
            }
        } else {
            RepoError::NetworkError {
                message: e.to_string(),
            }
        }
    }
}

impl From<serde_yaml::Error> for RepoError {
    fn from(e: serde_yaml::Error) -> Self {
        RepoError::Serialization(e.to_string())
    }
}

impl From<serde_json::Error> for RepoError {
    fn from(e: serde_json::Error) -> Self {
        RepoError::Serialization(e.to_string())
    }
}

impl From<url::ParseError> for RepoError {
    fn from(e: url::ParseError) -> Self {
        RepoError::InvalidRepositoryUrl {
            url: String::new(),
            reason: e.to_string(),
        }
    }
}

impl From<rusqlite::Error> for RepoError {
    fn from(e: rusqlite::Error) -> Self {
        RepoError::CacheError {
            message: e.to_string(),
        }
    }
}

impl From<semver::Error> for RepoError {
    fn from(e: semver::Error) -> Self {
        RepoError::ResolutionFailed {
            message: format!("Invalid semver: {}", e),
        }
    }
}
