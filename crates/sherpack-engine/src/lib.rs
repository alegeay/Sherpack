//! Sherpack Engine - Jinja2 templating for Kubernetes
//!
//! This crate provides a MiniJinja-based template engine with:
//! - Kubernetes-specific filters (toYaml, b64encode, etc.)
//! - Human-readable error messages with suggestions
//! - Full Jinja2 syntax support
//! - Multi-error collection for comprehensive error reporting
//! - Files API for accessing pack files from templates
//! - Subchart rendering with recursive support

pub mod engine;
pub mod filters;
pub mod functions;
pub mod error;
pub mod suggestions;
pub mod files_object;
pub mod subchart;
pub mod pack_renderer;

pub use engine::{Engine, EngineBuilder, RenderResult};
pub use error::{
    EngineError, TemplateError, TemplateErrorKind, RenderReport, RenderResultWithReport,
    RenderIssue, IssueSeverity,
};
pub use suggestions::{AVAILABLE_FILTERS, AVAILABLE_FUNCTIONS};
pub use files_object::{FilesObject, create_files_value, create_files_value_from_provider};
pub use subchart::{SubchartConfig, SubchartInfo, DiscoveryResult};
pub use pack_renderer::{PackRenderer, PackRendererBuilder, PackRenderResult, PackRenderResultWithReport};
