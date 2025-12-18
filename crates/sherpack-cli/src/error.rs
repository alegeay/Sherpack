//! CLI error types with exit code handling
//!
//! This module provides a unified error type for CLI operations that
//! maps errors to appropriate exit codes.

#![allow(dead_code)] // Some variants/methods are for future use

use miette::Diagnostic;
use thiserror::Error;

use crate::exit_codes;

/// CLI-specific error type that includes exit code information
#[derive(Error, Debug, Diagnostic, Clone)]
pub enum CliError {
    /// Validation failed (schema or values)
    #[error("Validation failed: {message}")]
    #[diagnostic(code(sherpack::cli::validation))]
    Validation {
        message: String,
        #[help]
        help: Option<String>,
    },

    /// Template rendering failed
    #[error("Template error: {message}")]
    #[diagnostic(code(sherpack::cli::template))]
    Template {
        message: String,
        #[help]
        help: Option<String>,
    },

    /// Pack structure or loading error
    #[error("Pack error: {message}")]
    #[diagnostic(code(sherpack::cli::pack))]
    Pack {
        message: String,
        #[help]
        help: Option<String>,
    },

    /// Linting failed with errors
    #[error("Linting failed with {errors} error(s) and {warnings} warning(s)")]
    #[diagnostic(code(sherpack::cli::lint))]
    LintFailed { errors: usize, warnings: usize },

    /// IO error (file not found, permissions, etc.)
    #[error("IO error: {message}")]
    #[diagnostic(code(sherpack::cli::io))]
    Io { message: String },

    /// Wrapped error for passthrough (stores the formatted message)
    #[error("{message}")]
    #[diagnostic(code(sherpack::cli::error))]
    Other { message: String },

    /// Internal error (runtime, unexpected failure)
    #[error("Internal error: {message}")]
    #[diagnostic(code(sherpack::cli::internal))]
    Internal { message: String },
}

impl CliError {
    /// Get the exit code for this error
    pub fn exit_code(&self) -> i32 {
        match self {
            CliError::Validation { .. } => exit_codes::VALIDATION_ERROR,
            CliError::Template { .. } => exit_codes::TEMPLATE_ERROR,
            CliError::Pack { .. } => exit_codes::PACK_ERROR,
            CliError::LintFailed { .. } => exit_codes::ERROR,
            CliError::Io { .. } => exit_codes::IO_ERROR,
            CliError::Other { .. } => exit_codes::ERROR,
            CliError::Internal { .. } => exit_codes::ERROR,
        }
    }

    /// Create an internal error
    pub fn internal(message: impl Into<String>) -> Self {
        Self::Internal {
            message: message.into(),
        }
    }

    /// Create a validation error
    pub fn validation(message: impl Into<String>) -> Self {
        Self::Validation {
            message: message.into(),
            help: None,
        }
    }

    /// Create a validation error with help text
    pub fn validation_with_help(message: impl Into<String>, help: impl Into<String>) -> Self {
        Self::Validation {
            message: message.into(),
            help: Some(help.into()),
        }
    }

    /// Create a template error
    pub fn template(message: impl Into<String>) -> Self {
        Self::Template {
            message: message.into(),
            help: None,
        }
    }

    /// Create a pack error
    pub fn pack(message: impl Into<String>) -> Self {
        Self::Pack {
            message: message.into(),
            help: None,
        }
    }

    /// Create a lint failure error
    pub fn lint_failed(errors: usize, warnings: usize) -> Self {
        Self::LintFailed { errors, warnings }
    }

    /// Create an input error (user provided invalid input)
    pub fn input(message: impl Into<String>) -> Self {
        Self::Validation {
            message: message.into(),
            help: None,
        }
    }

    /// Create an IO error from std::io::Error
    pub fn io(err: std::io::Error) -> Self {
        Self::Io {
            message: err.to_string(),
        }
    }
}

impl From<std::io::Error> for CliError {
    fn from(err: std::io::Error) -> Self {
        CliError::Io {
            message: err.to_string(),
        }
    }
}

impl From<miette::Report> for CliError {
    fn from(err: miette::Report) -> Self {
        CliError::Other {
            message: format!("{:?}", err),
        }
    }
}

/// Result type for CLI operations
pub type Result<T> = std::result::Result<T, CliError>;

/// Extension trait to convert miette Results to CliError Results
pub trait IntoCliResult<T> {
    fn into_cli_result(self) -> Result<T>;
}

impl<T> IntoCliResult<T> for miette::Result<T> {
    fn into_cli_result(self) -> Result<T> {
        self.map_err(CliError::from)
    }
}
