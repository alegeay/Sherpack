//! Error types for sherpack-kube

use thiserror::Error;

/// Result type for sherpack-kube operations
pub type Result<T> = std::result::Result<T, KubeError>;

/// Errors that can occur during Kubernetes operations
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum KubeError {
    /// Kubernetes API error
    #[error("Kubernetes API error: {0}")]
    Api(#[from] kube::Error),

    /// Release not found
    #[error("release '{name}' not found in namespace '{namespace}'")]
    ReleaseNotFound { name: String, namespace: String },

    /// Release already exists
    #[error("release '{name}' already exists in namespace '{namespace}'")]
    ReleaseAlreadyExists { name: String, namespace: String },

    /// Another operation is in progress (with recovery hint)
    #[error("another operation is in progress for release '{name}': {status}\nHint: Run `sherpack recover {name}` to recover from stuck state")]
    OperationInProgress { name: String, status: String },

    /// Release is in a stuck state (can be auto-recovered)
    #[error("release '{name}' is stuck in state '{status}' (started {elapsed} ago)\nHint: Run `sherpack recover {name}` to mark as failed and retry")]
    StuckRelease {
        name: String,
        status: String,
        elapsed: String,
    },

    /// Hook execution failed
    #[error("hook '{hook_name}' failed during {phase}: {message}")]
    HookFailed {
        hook_name: String,
        phase: String,
        message: String,
    },

    /// Health check failed
    #[error("health check failed for release '{name}': {message}")]
    HealthCheckFailed { name: String, message: String },

    /// Rollback not possible
    #[error("cannot rollback release '{name}': {reason}")]
    RollbackNotPossible { name: String, reason: String },

    /// Storage error
    #[error("storage error: {0}")]
    Storage(String),

    /// Release data too large
    #[error("release data too large ({size} bytes, max {max} bytes)\nHint: Use --large-release-strategy=chunked or external storage")]
    ReleaseTooLarge { size: usize, max: usize },

    /// Serialization error
    #[error("serialization error: {0}")]
    Serialization(String),

    /// Compression error
    #[error("compression error: {0}")]
    Compression(String),

    /// Diff error
    #[error("diff error: {0}")]
    Diff(String),

    /// Template rendering error
    #[error("template error: {0}")]
    Template(String),

    /// Pack loading error
    #[error("pack error: {0}")]
    Pack(String),

    /// Invalid configuration
    #[error("invalid configuration: {0}")]
    InvalidConfig(String),

    /// Timeout
    #[error("operation timed out after {0}")]
    Timeout(String),

    /// IO error
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// Immutable field conflict during rollback/upgrade
    #[error("cannot modify immutable field '{field}' in {resource}\nHint: Use --immutable-strategy=recreate to delete and recreate the resource")]
    ImmutableFieldConflict { resource: String, field: String },

    /// Drift detected
    #[error("drift detected in {count} resource(s)\nHint: Use `sherpack diff {name}` to see changes, or --force to override")]
    DriftDetected { name: String, count: usize },

    /// Invalid manifest
    #[error("invalid manifest: {0}")]
    InvalidManifest(String),

    /// Wrapped kube error (for direct use without From trait)
    #[error("Kubernetes API error: {0}")]
    KubeApi(#[source] kube::Error),
}

impl From<serde_json::Error> for KubeError {
    fn from(e: serde_json::Error) -> Self {
        KubeError::Serialization(e.to_string())
    }
}

impl From<serde_yaml::Error> for KubeError {
    fn from(e: serde_yaml::Error) -> Self {
        KubeError::Serialization(e.to_string())
    }
}

impl From<sherpack_core::CoreError> for KubeError {
    fn from(e: sherpack_core::CoreError) -> Self {
        KubeError::Pack(e.to_string())
    }
}

impl From<sherpack_engine::EngineError> for KubeError {
    fn from(e: sherpack_engine::EngineError) -> Self {
        KubeError::Template(e.to_string())
    }
}

impl KubeError {
    /// Check if this is a Kubernetes 404 Not Found error
    pub fn is_not_found(&self) -> bool {
        matches!(self, KubeError::Api(kube::Error::Api(resp)) if resp.code == 404)
    }

    /// Check if this is a conflict error (409)
    pub fn is_conflict(&self) -> bool {
        matches!(self, KubeError::Api(kube::Error::Api(resp)) if resp.code == 409)
    }
}
