//! Core error types

use thiserror::Error;

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
}

pub type Result<T> = std::result::Result<T, CoreError>;
