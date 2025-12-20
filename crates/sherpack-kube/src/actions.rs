//! Action options for install, upgrade, uninstall, and rollback operations

use chrono::Duration;
use serde::{Deserialize, Serialize};

use crate::health::HealthCheckConfig;
use crate::storage::LargeReleaseStrategy;

/// Options for install operation
#[derive(Debug, Clone, Default)]
pub struct InstallOptions {
    /// Release name
    pub name: String,

    /// Target namespace
    pub namespace: String,

    /// Wait for resources to be ready
    pub wait: bool,

    /// Timeout for wait
    pub timeout: Option<Duration>,

    /// Run health checks after install
    pub health_check: Option<HealthCheckConfig>,

    /// Automatically rollback on failure (only with wait=true)
    pub atomic: bool,

    /// Create namespace if it doesn't exist
    pub create_namespace: bool,

    /// Strategy for large releases
    pub large_release_strategy: LargeReleaseStrategy,

    /// Skip schema validation
    pub skip_schema_validation: bool,

    /// Dry run mode (don't actually apply)
    pub dry_run: bool,

    /// Show diff before applying
    pub show_diff: bool,

    /// Custom labels to add to the release
    pub labels: std::collections::HashMap<String, String>,

    /// Description for this release
    pub description: Option<String>,
}

impl InstallOptions {
    /// Create default install options with name and namespace
    pub fn new(name: impl Into<String>, namespace: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            namespace: namespace.into(),
            ..Default::default()
        }
    }

    /// Enable waiting for resources
    pub fn with_wait(mut self, timeout: Duration) -> Self {
        self.wait = true;
        self.timeout = Some(timeout);
        self
    }

    /// Enable atomic mode (auto-rollback on failure)
    pub fn with_atomic(mut self, timeout: Duration) -> Self {
        self.wait = true;
        self.atomic = true;
        self.timeout = Some(timeout);
        self
    }

    /// Enable health checks
    pub fn with_health_check(mut self, config: HealthCheckConfig) -> Self {
        self.health_check = Some(config);
        self
    }

    /// Enable dry-run mode
    pub fn dry_run(mut self) -> Self {
        self.dry_run = true;
        self
    }

    /// Show diff before applying
    pub fn with_diff(mut self) -> Self {
        self.show_diff = true;
        self
    }
}

/// Options for upgrade operation
#[derive(Debug, Clone, Default)]
pub struct UpgradeOptions {
    /// Release name
    pub name: String,

    /// Target namespace
    pub namespace: String,

    /// Wait for resources to be ready
    pub wait: bool,

    /// Timeout for wait
    pub timeout: Option<Duration>,

    /// Run health checks after upgrade
    pub health_check: Option<HealthCheckConfig>,

    /// Automatically rollback on failure
    pub atomic: bool,

    /// Install if release doesn't exist
    pub install: bool,

    /// Force resource updates through delete/recreate
    pub force: bool,

    /// Strategy for immutable field conflicts
    pub immutable_strategy: ImmutableStrategy,

    /// Skip schema validation
    pub skip_schema_validation: bool,

    /// Reset values to defaults (don't merge with previous)
    pub reset_values: bool,

    /// Reuse values from previous release
    pub reuse_values: bool,

    /// Dry run mode
    pub dry_run: bool,

    /// Show diff before applying
    pub show_diff: bool,

    /// Skip hooks
    pub no_hooks: bool,

    /// Maximum history to keep
    pub max_history: Option<u32>,

    /// Custom labels to add
    pub labels: std::collections::HashMap<String, String>,

    /// Description for this revision
    pub description: Option<String>,
}

impl UpgradeOptions {
    /// Create default upgrade options
    pub fn new(name: impl Into<String>, namespace: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            namespace: namespace.into(),
            ..Default::default()
        }
    }

    /// Enable install-if-not-exists
    pub fn with_install(mut self) -> Self {
        self.install = true;
        self
    }

    /// Enable atomic mode
    pub fn with_atomic(mut self, timeout: Duration) -> Self {
        self.wait = true;
        self.atomic = true;
        self.timeout = Some(timeout);
        self
    }

    /// Enable force mode
    pub fn with_force(mut self) -> Self {
        self.force = true;
        self
    }

    /// Set immutable strategy
    pub fn with_immutable_strategy(mut self, strategy: ImmutableStrategy) -> Self {
        self.immutable_strategy = strategy;
        self
    }
}

/// Options for uninstall operation
#[derive(Debug, Clone, Default)]
pub struct UninstallOptions {
    /// Release name
    pub name: String,

    /// Target namespace
    pub namespace: String,

    /// Wait for resources to be deleted
    pub wait: bool,

    /// Timeout for wait
    pub timeout: Option<Duration>,

    /// Keep release history (don't delete storage)
    pub keep_history: bool,

    /// Skip pre/post-delete hooks
    pub no_hooks: bool,

    /// Dry run mode
    pub dry_run: bool,

    /// Cascade deletion (delete dependents)
    pub cascade: DeletionCascade,

    /// Description for the uninstall
    pub description: Option<String>,
}

impl UninstallOptions {
    /// Create default uninstall options
    pub fn new(name: impl Into<String>, namespace: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            namespace: namespace.into(),
            cascade: DeletionCascade::Background,
            ..Default::default()
        }
    }

    /// Keep history after uninstall
    pub fn keep_history(mut self) -> Self {
        self.keep_history = true;
        self
    }

    /// Wait for deletion
    pub fn with_wait(mut self, timeout: Duration) -> Self {
        self.wait = true;
        self.timeout = Some(timeout);
        self
    }
}

/// Options for rollback operation
#[derive(Debug, Clone, Default)]
pub struct RollbackOptions {
    /// Release name
    pub name: String,

    /// Target namespace
    pub namespace: String,

    /// Target revision (0 = previous)
    pub revision: u32,

    /// Wait for resources to be ready
    pub wait: bool,

    /// Timeout for wait
    pub timeout: Option<Duration>,

    /// Run health checks after rollback
    pub health_check: Option<HealthCheckConfig>,

    /// Force rollback through delete/recreate
    pub force: bool,

    /// Strategy for immutable field conflicts
    pub immutable_strategy: ImmutableStrategy,

    /// Strategy for PVCs
    pub pvc_strategy: PvcStrategy,

    /// Skip hooks
    pub no_hooks: bool,

    /// Dry run mode
    pub dry_run: bool,

    /// Show diff before applying
    pub show_diff: bool,

    /// Recreate pods (delete existing pods)
    pub recreate_pods: bool,

    /// Maximum history to keep
    pub max_history: Option<u32>,

    /// Description for this rollback
    pub description: Option<String>,
}

impl RollbackOptions {
    /// Create default rollback options
    pub fn new(name: impl Into<String>, namespace: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            namespace: namespace.into(),
            ..Default::default()
        }
    }

    /// Set target revision
    pub fn to_revision(mut self, revision: u32) -> Self {
        self.revision = revision;
        self
    }

    /// Enable force mode
    pub fn with_force(mut self) -> Self {
        self.force = true;
        self
    }

    /// Wait for rollback
    pub fn with_wait(mut self, timeout: Duration) -> Self {
        self.wait = true;
        self.timeout = Some(timeout);
        self
    }
}

/// Strategy for handling immutable field conflicts
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ImmutableStrategy {
    /// Fail on immutable field conflict (default)
    #[default]
    Fail,

    /// Delete and recreate the resource
    Recreate,

    /// Skip resources with immutable conflicts
    Skip,
}

impl std::fmt::Display for ImmutableStrategy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Fail => write!(f, "fail"),
            Self::Recreate => write!(f, "recreate"),
            Self::Skip => write!(f, "skip"),
        }
    }
}

impl std::str::FromStr for ImmutableStrategy {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "fail" => Ok(Self::Fail),
            "recreate" => Ok(Self::Recreate),
            "skip" => Ok(Self::Skip),
            _ => Err(format!("unknown immutable strategy: {}", s)),
        }
    }
}

/// Strategy for handling PVCs during rollback
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PvcStrategy {
    /// Don't touch PVCs (default)
    #[default]
    Preserve,

    /// Warn that PVC data won't be rolled back
    WarnAndPreserve,

    /// Try to restore from snapshot
    RestoreSnapshot {
        /// Volume snapshot class to use
        snapshot_class: String,
    },
}

impl std::fmt::Display for PvcStrategy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Preserve => write!(f, "preserve"),
            Self::WarnAndPreserve => write!(f, "warn"),
            Self::RestoreSnapshot { .. } => write!(f, "restore-snapshot"),
        }
    }
}

/// Cascade deletion strategy
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DeletionCascade {
    /// Delete in background (default)
    #[default]
    Background,

    /// Delete in foreground (wait for dependents)
    Foreground,

    /// Orphan dependents (don't delete them)
    Orphan,
}

impl std::fmt::Display for DeletionCascade {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Background => write!(f, "background"),
            Self::Foreground => write!(f, "foreground"),
            Self::Orphan => write!(f, "orphan"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_install_options_builder() {
        let opts = InstallOptions::new("myapp", "default")
            .with_wait(Duration::minutes(5))
            .with_diff();

        assert_eq!(opts.name, "myapp");
        assert_eq!(opts.namespace, "default");
        assert!(opts.wait);
        assert!(opts.show_diff);
    }

    #[test]
    fn test_upgrade_options_atomic() {
        let opts = UpgradeOptions::new("myapp", "default")
            .with_atomic(Duration::minutes(10))
            .with_install();

        assert!(opts.wait);
        assert!(opts.atomic);
        assert!(opts.install);
    }

    #[test]
    fn test_rollback_options() {
        let opts = RollbackOptions::new("myapp", "default")
            .to_revision(3)
            .with_force();

        assert_eq!(opts.revision, 3);
        assert!(opts.force);
    }

    #[test]
    fn test_immutable_strategy_parse() {
        assert_eq!(
            "recreate".parse::<ImmutableStrategy>().unwrap(),
            ImmutableStrategy::Recreate
        );
        assert_eq!(
            "fail".parse::<ImmutableStrategy>().unwrap(),
            ImmutableStrategy::Fail
        );
    }
}
