//! Sherpack Kube - Kubernetes integration for Sherpack
//!
//! This crate provides:
//! - **Storage Drivers**: Persist release information in Kubernetes (Secrets, ConfigMaps) or locally
//! - **Release Management**: Full lifecycle management with state machine and auto-recovery
//! - **Hooks System**: Improved hooks with policies and better error handling
//! - **Diff Engine**: Compare releases and detect cluster drift
//! - **Health Checks**: Validate deployments and auto-rollback on failure

pub mod error;
pub mod release;
pub mod storage;
pub mod hooks;
pub mod diff;
pub mod health;
pub mod actions;
pub mod resources;
pub mod client;

pub use error::{KubeError, Result};
pub use release::{StoredRelease, ReleaseState, ValueSource, ValuesProvenance};
pub use storage::{StorageDriver, StorageConfig, CompressionMethod, LargeReleaseStrategy, MockStorageDriver, OperationCounts};
pub use hooks::{Hook, HookPhase, HookFailurePolicy, HookCleanupPolicy, HookExecutor};
pub use diff::{DiffEngine, DiffResult, ResourceChange, ChangeType};
pub use health::{HealthChecker, HealthCheckConfig, HealthStatus, ResourceHealth};
pub use actions::{InstallOptions, UpgradeOptions, UninstallOptions, RollbackOptions};
pub use resources::{ResourceManager, ApplyResult, DeleteResult, OperationSummary};
pub use client::KubeClient;
