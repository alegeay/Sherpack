//! Sherpack Core - Core types and utilities for the Kubernetes package manager
//!
//! This crate provides the foundational types used throughout Sherpack:
//! - `Pack`: The package definition (equivalent to Helm Chart)
//! - `Values`: Configuration values with deep merge support
//! - `Release`: Deployment state tracking
//! - `Context`: Template rendering context

pub mod pack;
pub mod values;
pub mod release;
pub mod context;
pub mod error;

pub use pack::{Pack, PackMetadata, PackKind, Dependency, LoadedPack};
pub use values::{Values, parse_set_values};
pub use release::{Release, ReleaseStatus, ReleaseInfo};
pub use context::TemplateContext;
pub use error::CoreError;
