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

pub mod actions;
pub mod annotations;
pub mod client;
pub mod crd;
pub mod diff;
pub mod error;
pub mod health;
pub mod hooks;
pub mod progress;
pub mod release;
pub mod resources;
pub mod storage;
pub mod waves;

pub use actions::{InstallOptions, RollbackOptions, UninstallOptions, UpgradeOptions};
pub use annotations::{DeletePolicy, FailurePolicy, ResourceRef};
pub use client::KubeClient;
pub use diff::{ChangeType, DiffEngine, DiffResult, ResourceChange};
pub use error::{KubeError, Result};
pub use health::{HealthCheckConfig, HealthChecker, HealthStatus, ResourceHealth};
pub use hooks::{Hook, HookCleanupPolicy, HookExecutor, HookFailurePolicy, HookPhase};
pub use progress::{ProgressReporter, ResourceState, ResourceStatus};
pub use release::{ReleaseState, StoredRelease, ValueSource, ValuesProvenance};
pub use resources::{ApplyResult, DeleteResult, OperationSummary, ResourceManager};
pub use storage::{
    CompressionMethod, LargeReleaseStrategy, MockStorageDriver, OperationCounts, StorageConfig,
    StorageDriver,
};
pub use waves::{ExecutionPlan, Resource, Wave, WaveExecutionConfig};
// CRD handling - Phase 2 Safe Updates
pub use crd::{
    ChangeKind,
    ChangeSeverity,
    CrdAnalysis,
    // Analysis types
    CrdAnalyzer,
    CrdApplyResult,
    CrdChange,
    CrdManager,
    CrdParser,
    // Schema types
    CrdSchema,
    CrdScope,
    CrdUpgradeResult,
    ForceStrategy,
    // Apply operations
    ResourceCategory,
    SafeStrategy,
    SkipStrategy,
    UpgradeDecision,
    // Strategy types
    UpgradeStrategy,
    strategy_from_options,
};

// CRD handling - Phase 3 Templated CRDs
pub use crd::{
    CRD_POLICY_ANNOTATION,
    CrdDeletionImpact,
    // Detection types
    CrdLintCode,
    CrdLintWarning,
    CrdLocation,
    CrdOwnership,
    // Policy types
    CrdPolicy,
    // Protection types
    CrdProtection,
    DeletionConfirmation,
    DeletionImpactSummary,
    DetectedCrd,
    HELM_RESOURCE_POLICY,
    JinjaConstruct,
    LintSeverity,
    TemplatedCrdFile,
    contains_jinja_syntax,
    detect_crds_in_manifests,
    is_crd_manifest,
    lint_crds,
};
