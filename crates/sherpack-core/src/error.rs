//! Core error types

use thiserror::Error;

/// Information about a single validation error
#[derive(Debug, Clone)]
pub struct ValidationErrorInfo {
    /// JSON path where the error occurred (e.g., "/image/tag")
    pub path: String,
    /// Human-readable error message
    pub message: String,
    /// Expected value/type (if applicable)
    pub expected: Option<String>,
    /// Actual value/type (if applicable)
    pub actual: Option<String>,
}

#[derive(Error, Debug)]
pub enum CoreError {
    #[error("Pack not found: {path}")]
    PackNotFound { path: String },

    #[error("Invalid Pack.yaml: {message}")]
    InvalidPack { message: String },

    #[error("Failed to parse Pack.yaml: {0}")]
    YamlParse(#[from] serde_yaml::Error),

    #[error("Failed to parse JSON: {0}")]
    JsonParse(#[from] serde_json::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Invalid version: {0}")]
    InvalidVersion(#[from] semver::Error),

    #[error("Values merge error: {message}")]
    ValuesMerge { message: String },

    #[error("Missing required field: {field}")]
    MissingField { field: String },

    #[error("Invalid schema: {message}")]
    InvalidSchema { message: String },

    #[error("Schema validation failed")]
    SchemaValidation { errors: Vec<ValidationErrorInfo> },

    #[error("Schema file not found: {path}")]
    SchemaNotFound { path: String },
}

impl CoreError {
    /// Format validation errors for display
    pub fn format_validation_errors(errors: &[ValidationErrorInfo]) -> String {
        errors
            .iter()
            .map(|e| format!("  - {}: {}", e.path, e.message))
            .collect::<Vec<_>>()
            .join("\n")
    }
}

pub type Result<T> = std::result::Result<T, CoreError>;
