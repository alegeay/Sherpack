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
pub mod error;
pub mod files_object;
pub mod filters;
pub mod functions;
pub mod pack_renderer;
pub mod secrets;
pub mod subchart;
pub mod suggestions;

pub use engine::{Engine, EngineBuilder, RenderResult};
pub use error::{
    EngineError, IssueSeverity, RenderIssue, RenderReport, RenderResultWithReport, TemplateError,
    TemplateErrorKind,
};
pub use files_object::{FilesObject, create_files_value, create_files_value_from_provider};
pub use pack_renderer::{
    PackRenderResult, PackRenderResultWithReport, PackRenderer, PackRendererBuilder,
};
pub use secrets::SecretFunctionState;
pub use subchart::{DiscoveryResult, SubchartConfig, SubchartInfo};
pub use suggestions::{AVAILABLE_FILTERS, AVAILABLE_FUNCTIONS};
