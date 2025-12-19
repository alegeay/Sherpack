//! Sherpack Kube - Kubernetes integration for Sherpack
//!
//! This crate provides:
//! - **Storage Drivers**: Persist release information in Kubernetes (Secrets, ConfigMaps) or locally
//! - **Release Management**: Full lifecycle management with state machine and auto-recovery
//! - **Hooks System**: Improved hooks with policies and better error handling
//! - **Diff Engine**: Compare releases and detect cluster drift
//! - **Health Checks**: Validate deployments with REAL Kubernetes API calls
//! - **Sync Waves**: Resource ordering with wait-for dependencies
//! - **Progress Reporting**: Real-time feedback during deployment operations
//! - **Annotations**: Helm-compatible annotation parsing with Sherpack extensions

pub mod error;
pub mod release;
pub mod storage;
pub mod hooks;
pub mod diff;
pub mod health;
pub mod actions;
pub mod resources;
pub mod client;
pub mod annotations;
pub mod waves;
pub mod progress;
pub mod crd;

pub use error::{KubeError, Result};
pub use release::{StoredRelease, ReleaseState, ValueSource, ValuesProvenance};
pub use storage::{StorageDriver, StorageConfig, CompressionMethod, LargeReleaseStrategy, MockStorageDriver, OperationCounts};
pub use hooks::{Hook, HookPhase, HookFailurePolicy, HookCleanupPolicy, HookExecutor};
pub use diff::{DiffEngine, DiffResult, ResourceChange, ChangeType};
pub use health::{HealthChecker, HealthCheckConfig, HealthStatus, ResourceHealth};
pub use actions::{InstallOptions, UpgradeOptions, UninstallOptions, RollbackOptions};
pub use resources::{ResourceManager, ApplyResult, DeleteResult, OperationSummary};
pub use client::KubeClient;
pub use annotations::{ResourceRef, DeletePolicy, FailurePolicy};
pub use waves::{ExecutionPlan, Wave, Resource, WaveExecutionConfig};
pub use progress::{ProgressReporter, ResourceStatus, ResourceState};
// CRD handling - Phase 2 Safe Updates
pub use crd::{
    // Apply operations
    ResourceCategory, CrdManager, CrdApplyResult, CrdUpgradeResult,
    // Schema types
    CrdSchema, CrdScope, CrdParser,
    // Analysis types
    CrdAnalyzer, CrdAnalysis, CrdChange, ChangeKind, ChangeSeverity,
    // Strategy types
    UpgradeStrategy, UpgradeDecision, SafeStrategy, ForceStrategy, SkipStrategy,
    strategy_from_options,
};

// CRD handling - Phase 3 Templated CRDs
pub use crd::{
    // Policy types
    CrdPolicy, CrdLocation, CrdOwnership, DetectedCrd,
    CRD_POLICY_ANNOTATION, HELM_RESOURCE_POLICY,
    // Detection types
    CrdLintCode, CrdLintWarning, LintSeverity, TemplatedCrdFile, JinjaConstruct,
    contains_jinja_syntax, detect_crds_in_manifests, is_crd_manifest, lint_crds,
    // Protection types
    CrdProtection, CrdDeletionImpact, DeletionImpactSummary, DeletionConfirmation,
};
