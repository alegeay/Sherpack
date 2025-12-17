//! Sherpack Engine - Jinja2 templating for Kubernetes
//!
//! This crate provides a MiniJinja-based template engine with:
//! - Kubernetes-specific filters (toYaml, b64encode, etc.)
//! - Human-readable error messages with suggestions
//! - Full Jinja2 syntax support
//! - Multi-error collection for comprehensive error reporting

pub mod engine;
pub mod filters;
pub mod functions;
pub mod error;
pub mod suggestions;

pub use engine::{Engine, EngineBuilder, RenderResult};
pub use error::{EngineError, TemplateError, TemplateErrorKind, RenderReport, RenderResultWithReport};
pub use suggestions::{AVAILABLE_FILTERS, AVAILABLE_FUNCTIONS};
