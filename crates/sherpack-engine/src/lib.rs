//! Sherpack Engine - Jinja2 templating for Kubernetes
//!
//! This crate provides a MiniJinja-based template engine with:
//! - Kubernetes-specific filters (toYaml, b64encode, etc.)
//! - Human-readable error messages with suggestions
//! - Full Jinja2 syntax support

pub mod engine;
pub mod filters;
pub mod functions;
pub mod error;

pub use engine::{Engine, EngineBuilder, RenderResult};
pub use error::{EngineError, TemplateError};
