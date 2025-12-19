//! CRD (CustomResourceDefinition) handling for Sherpack
//!
//! This module provides comprehensive CRD lifecycle management:
//!
//! - **Schema representation** (`schema`): Structured types for CRD schemas
//! - **Parsing** (`parser`): Parse CRD YAML into schema structures
//! - **Analysis** (`analyzer`): Detect and classify changes between CRD versions
//! - **Strategy** (`strategy`): Decide whether to apply changes based on safety
//! - **Application** (`apply`): Apply CRDs to the cluster with Server-Side Apply
//! - **Policy** (`policy`): Intent-based CRD management policies
//! - **Detection** (`detection`): CRD detection in templates and templating in crds/
//! - **Protection** (`protection`): Deletion protection and impact analysis
//!
//! # Safety-First Design
//!
//! Unlike Helm which never updates CRDs (leaving users to manual `kubectl apply`),
//! Sherpack provides smart CRD updates with safety analysis:
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │                    CRD Update Pipeline                      │
//! ├─────────────────────────────────────────────────────────────┤
//! │                                                             │
//! │   New CRD ──► Parser ──► Analyzer ──► Strategy ──► Apply   │
//! │     │                        │            │                 │
//! │     └──────────────┐         │            │                 │
//! │                    ▼         ▼            ▼                 │
//! │              Old CRD    CrdAnalysis   Decision              │
//! │              (from      (changes,     (Apply/               │
//! │               cluster)   severity)    Reject)               │
//! │                                                             │
//! └─────────────────────────────────────────────────────────────┘
//! ```
//!
//! # Intent-Based Policies (Phase 3)
//!
//! Unlike Helm's location-based rules, Sherpack uses explicit policies:
//!
//! - **managed**: This release owns the CRD (default, protected on uninstall)
//! - **shared**: CRD is shared between releases (never delete)
//! - **external**: CRD is managed externally (don't touch)
//!
//! Policies are set via annotations:
//! ```yaml
//! metadata:
//!   annotations:
//!     sherpack.io/crd-policy: shared
//! ```
//!
//! # Change Categories
//!
//! Changes are classified by severity:
//!
//! - **Safe** (✓): Adding optional fields, new versions, printer columns
//! - **Warning** (⚠): Tightening validation, adding required fields
//! - **Dangerous** (✗): Removing versions/fields, changing types, scope changes
//!
//! # Example
//!
//! ```ignore
//! use sherpack_kube::crd::{CrdParser, CrdAnalyzer, strategy_from_options};
//!
//! // Parse CRDs
//! let old_schema = CrdParser::parse(old_yaml)?;
//! let new_schema = CrdParser::parse(new_yaml)?;
//!
//! // Analyze changes
//! let analysis = CrdAnalyzer::analyze(Some(&old_schema), &new_schema);
//!
//! // Decide based on strategy
//! let strategy = strategy_from_options(skip_update, force_update);
//! let decision = strategy.decide(&analysis);
//!
//! // Apply if allowed
//! if decision.allows_apply() {
//!     crd_manager.apply_crd(new_yaml, dry_run).await?;
//! }
//! ```

mod analyzer;
mod apply;
mod detection;
mod parser;
mod policy;
mod protection;
mod schema;
mod strategy;

// Re-export main types for convenient access

// Schema types
pub use schema::{
    AdditionalProperties, CrdNames, CrdSchema, CrdScope, CrdVersionSchema, OpenApiSchema,
    PrinterColumn, PropertyType, ScaleSubresource, SchemaProperty, Subresources,
};

// Parser
pub use parser::CrdParser;

// Analyzer types
pub use analyzer::{ChangeSeverity, ChangeKind, CrdAnalysis, CrdAnalyzer, CrdChange};

// Strategy types
pub use strategy::{
    CautiousStrategy, CustomStrategy, ForceStrategy, SafeStrategy, SkipStrategy,
    SkippedChange, StrategyBuilder, UpgradeDecision, UpgradeStrategy, strategy_from_options,
};

// Apply types
pub use apply::{CrdApplyResult, CrdManager, CrdUpgradeResult, ResourceCategory};

// Policy types (Phase 3)
pub use policy::{
    CrdLocation, CrdOwnership, CrdPolicy, DetectedCrd, CRD_POLICY_ANNOTATION, HELM_RESOURCE_POLICY,
};

// Detection types (Phase 3)
pub use detection::{
    contains_jinja_syntax, detect_crds_in_manifests, extract_crd_name, is_crd_manifest, lint_crds,
    CrdLintCode, CrdLintWarning, CrdsScanResult, JinjaConstruct, LintSeverity, NonCrdFile,
    TemplatedCrdFile,
};

// Protection types (Phase 3)
pub use protection::{
    CrdDeletionImpact, CrdProtection, DeletionConfirmation, DeletionImpactSummary,
};
