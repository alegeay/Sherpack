//! Sherpack Core - Core types and utilities for the Kubernetes package manager
//!
//! This crate provides the foundational types used throughout Sherpack:
//! - `Pack`: The package definition (equivalent to Helm Chart)
//! - `Values`: Configuration values with deep merge support
//! - `Release`: Deployment state tracking
//! - `Context`: Template rendering context
//! - `Schema`: Values schema validation
//! - `Files`: Sandboxed file access for templates

pub mod archive;
pub mod context;
pub mod error;
pub mod files;
pub mod manifest;
pub mod pack;
pub mod release;
pub mod schema;
pub mod secrets;
pub mod values;

pub use archive::{
    ArchiveEntry, create_archive, default_archive_name, extract_archive, list_archive,
    read_file_from_archive, read_manifest_from_archive, verify_archive,
};
pub use context::TemplateContext;
pub use error::{CoreError, ValidationErrorInfo};
pub use files::FileEntry as FilesFileEntry;
pub use files::{FileProvider, Files, MockFileProvider, SandboxedFileProvider};
pub use manifest::FileEntry as ManifestFileEntry;
pub use manifest::{Manifest, MismatchedFile, VerificationResult};
pub use pack::{
    CrdConfig, CrdManifest, CrdUninstallConfig, CrdUpgradeConfig, CrdUpgradeStrategy, Dependency,
    LoadedPack, Pack, PackKind, PackMetadata, ResolvePolicy,
};
pub use release::{Release, ReleaseInfo, ReleaseStatus};
pub use schema::{
    Schema, SchemaValidator, SherpProperty, SherpSchema, SherpType, ValidationResult,
};
pub use secrets::{SecretCharset, SecretEntry, SecretGenerator, SecretState};
pub use values::{Values, parse_set_values};
