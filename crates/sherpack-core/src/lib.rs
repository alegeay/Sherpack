//! Sherpack Core - Core types and utilities for the Kubernetes package manager
//!
//! This crate provides the foundational types used throughout Sherpack:
//! - `Pack`: The package definition (equivalent to Helm Chart)
//! - `Values`: Configuration values with deep merge support
//! - `Release`: Deployment state tracking
//! - `Context`: Template rendering context
//! - `Schema`: Values schema validation
//! - `Files`: Sandboxed file access for templates

pub mod pack;
pub mod values;
pub mod release;
pub mod context;
pub mod error;
pub mod schema;
pub mod manifest;
pub mod archive;
pub mod files;

pub use pack::{
    Pack, PackMetadata, PackKind, Dependency, ResolvePolicy, LoadedPack,
    CrdConfig, CrdUpgradeConfig, CrdUninstallConfig, CrdUpgradeStrategy, CrdManifest,
};
pub use values::{Values, parse_set_values};
pub use release::{Release, ReleaseStatus, ReleaseInfo};
pub use context::TemplateContext;
pub use error::{CoreError, ValidationErrorInfo};
pub use schema::{Schema, SchemaValidator, ValidationResult, SherpSchema, SherpProperty, SherpType};
pub use manifest::{Manifest, VerificationResult, MismatchedFile};
pub use manifest::FileEntry as ManifestFileEntry;
pub use archive::{
    create_archive, extract_archive, list_archive, read_manifest_from_archive,
    read_file_from_archive, verify_archive, default_archive_name, ArchiveEntry,
};
pub use files::{Files, FileProvider, SandboxedFileProvider, MockFileProvider};
pub use files::FileEntry as FilesFileEntry;
