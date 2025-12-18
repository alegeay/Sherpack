//! Release types with improved state machine and provenance tracking

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use sherpack_core::{PackMetadata, Values};
use std::collections::HashMap;

/// Default timeout for pending operations (5 minutes)
pub const DEFAULT_OPERATION_TIMEOUT: Duration = Duration::minutes(5);

/// A stored release with full metadata and provenance tracking
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredRelease {
    /// Release name
    pub name: String,

    /// Kubernetes namespace
    pub namespace: String,

    /// Revision number (1-indexed, increments with each upgrade)
    pub version: u32,

    /// Current state with timing information
    pub state: ReleaseState,

    /// Pack metadata at deploy time
    pub pack: PackMetadata,

    /// Effective values (merged from all sources)
    pub values: Values,

    /// Provenance tracking for each value
    #[serde(default)]
    pub values_provenance: ValuesProvenance,

    /// Rendered manifest (all Kubernetes resources)
    pub manifest: String,

    /// Hooks defined in this release
    #[serde(default)]
    pub hooks: Vec<crate::hooks::Hook>,

    /// Custom labels for filtering/querying
    #[serde(default)]
    pub labels: HashMap<String, String>,

    /// Creation timestamp
    pub created_at: DateTime<Utc>,

    /// Last update timestamp
    pub updated_at: DateTime<Utc>,

    /// Notes from NOTES.txt (if present)
    #[serde(default)]
    pub notes: Option<String>,
}

impl StoredRelease {
    /// Create a new release for installation
    pub fn for_install(
        name: String,
        namespace: String,
        pack: PackMetadata,
        values: Values,
        manifest: String,
    ) -> Self {
        let now = Utc::now();
        Self {
            name,
            namespace,
            version: 1,
            state: ReleaseState::PendingInstall {
                started_at: now,
                timeout: DEFAULT_OPERATION_TIMEOUT,
            },
            pack,
            values,
            values_provenance: ValuesProvenance::default(),
            manifest,
            hooks: Vec::new(),
            labels: HashMap::new(),
            created_at: now,
            updated_at: now,
            notes: None,
        }
    }

    /// Create a new revision for upgrade
    pub fn for_upgrade(previous: &StoredRelease, values: Values, manifest: String) -> Self {
        let now = Utc::now();
        Self {
            name: previous.name.clone(),
            namespace: previous.namespace.clone(),
            version: previous.version + 1,
            state: ReleaseState::PendingUpgrade {
                started_at: now,
                timeout: DEFAULT_OPERATION_TIMEOUT,
                previous_version: previous.version,
            },
            pack: previous.pack.clone(),
            values,
            values_provenance: previous.values_provenance.clone(),
            manifest,
            hooks: previous.hooks.clone(),
            labels: previous.labels.clone(),
            created_at: now,
            updated_at: now,
            notes: previous.notes.clone(),
        }
    }

    /// Storage key for this release
    pub fn storage_key(&self) -> String {
        format!(
            "sh.sherpack.release.v1.{}.v{}",
            self.name, self.version
        )
    }

    /// Check if this release is in a terminal state
    pub fn is_terminal(&self) -> bool {
        matches!(
            self.state,
            ReleaseState::Deployed
                | ReleaseState::Failed { .. }
                | ReleaseState::Uninstalled
                | ReleaseState::Superseded
        )
    }

    /// Check if this release is stuck (pending operation timed out)
    pub fn is_stuck(&self) -> bool {
        self.state.is_stale()
    }

    /// Mark the release as deployed
    pub fn mark_deployed(&mut self) {
        self.state = ReleaseState::Deployed;
        self.updated_at = Utc::now();
    }

    /// Mark the release as failed
    pub fn mark_failed(&mut self, reason: String, recoverable: bool) {
        self.state = ReleaseState::Failed {
            reason,
            recoverable,
            failed_at: Utc::now(),
        };
        self.updated_at = Utc::now();
    }

    /// Mark the release as superseded (replaced by a newer version)
    pub fn mark_superseded(&mut self) {
        self.state = ReleaseState::Superseded;
        self.updated_at = Utc::now();
    }

    /// Mark the release as uninstalled
    pub fn mark_uninstalled(&mut self) {
        self.state = ReleaseState::Uninstalled;
        self.updated_at = Utc::now();
    }

    /// Attempt auto-recovery if stuck
    pub fn try_auto_recover(&mut self) -> bool {
        if let Some(new_state) = self.state.auto_recover() {
            self.state = new_state;
            self.updated_at = Utc::now();
            true
        } else {
            false
        }
    }
}

/// Release state with timing information for pending operations
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "status", rename_all = "kebab-case")]
#[non_exhaustive]
pub enum ReleaseState {
    /// Successfully deployed
    Deployed,

    /// Deployment failed
    Failed {
        reason: String,
        recoverable: bool,
        failed_at: DateTime<Utc>,
    },

    /// Release has been uninstalled
    Uninstalled,

    /// Replaced by a newer revision
    Superseded,

    /// Installation in progress
    PendingInstall {
        started_at: DateTime<Utc>,
        #[serde(with = "duration_serde")]
        timeout: Duration,
    },

    /// Upgrade in progress
    PendingUpgrade {
        started_at: DateTime<Utc>,
        #[serde(with = "duration_serde")]
        timeout: Duration,
        previous_version: u32,
    },

    /// Rollback in progress
    PendingRollback {
        started_at: DateTime<Utc>,
        #[serde(with = "duration_serde")]
        timeout: Duration,
        target_version: u32,
    },

    /// Uninstallation in progress
    PendingUninstall {
        started_at: DateTime<Utc>,
        #[serde(with = "duration_serde")]
        timeout: Duration,
    },

    /// Recovery in progress (from stuck state)
    Recovering {
        from_status: String,
        attempt: u32,
        started_at: DateTime<Utc>,
    },
}

impl ReleaseState {
    /// Check if this is a pending (transitional) state
    pub fn is_pending(&self) -> bool {
        matches!(
            self,
            Self::PendingInstall { .. }
                | Self::PendingUpgrade { .. }
                | Self::PendingRollback { .. }
                | Self::PendingUninstall { .. }
                | Self::Recovering { .. }
        )
    }

    /// Check if this pending state has timed out (is stale)
    pub fn is_stale(&self) -> bool {
        let now = Utc::now();
        match self {
            Self::PendingInstall { started_at, timeout }
            | Self::PendingUpgrade { started_at, timeout, .. }
            | Self::PendingRollback { started_at, timeout, .. }
            | Self::PendingUninstall { started_at, timeout } => {
                now.signed_duration_since(*started_at) > *timeout
            }
            Self::Recovering { started_at, .. } => {
                // Recovery timeout: 2 minutes
                now.signed_duration_since(*started_at) > Duration::minutes(2)
            }
            _ => false,
        }
    }

    /// Get elapsed time since operation started (for pending states)
    pub fn elapsed(&self) -> Option<Duration> {
        let now = Utc::now();
        match self {
            Self::PendingInstall { started_at, .. }
            | Self::PendingUpgrade { started_at, .. }
            | Self::PendingRollback { started_at, .. }
            | Self::PendingUninstall { started_at, .. }
            | Self::Recovering { started_at, .. } => {
                Some(now.signed_duration_since(*started_at))
            }
            _ => None,
        }
    }

    /// Auto-recover from stale pending state
    pub fn auto_recover(&self) -> Option<ReleaseState> {
        if self.is_stale() {
            Some(ReleaseState::Failed {
                reason: format!("Operation timed out (was: {})", self.status_name()),
                recoverable: true,
                failed_at: Utc::now(),
            })
        } else {
            None
        }
    }

    /// Human-readable status name
    pub fn status_name(&self) -> &'static str {
        match self {
            Self::Deployed => "deployed",
            Self::Failed { .. } => "failed",
            Self::Uninstalled => "uninstalled",
            Self::Superseded => "superseded",
            Self::PendingInstall { .. } => "pending-install",
            Self::PendingUpgrade { .. } => "pending-upgrade",
            Self::PendingRollback { .. } => "pending-rollback",
            Self::PendingUninstall { .. } => "pending-uninstall",
            Self::Recovering { .. } => "recovering",
        }
    }
}

impl std::fmt::Display for ReleaseState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Failed { reason, .. } => write!(f, "failed: {}", reason),
            Self::Recovering { from_status, attempt, .. } => {
                write!(f, "recovering from {} (attempt {})", from_status, attempt)
            }
            other => write!(f, "{}", other.status_name()),
        }
    }
}

impl Default for ReleaseState {
    fn default() -> Self {
        Self::Deployed
    }
}

/// Tracks where each value came from
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ValuesProvenance {
    /// Map from JSON path to source
    #[serde(default)]
    pub sources: HashMap<String, ValueSource>,
}

impl ValuesProvenance {
    /// Record the source of a value
    pub fn record(&mut self, path: &str, source: ValueSource) {
        self.sources.insert(path.to_string(), source);
    }

    /// Get the source of a value
    pub fn get_source(&self, path: &str) -> Option<&ValueSource> {
        self.sources.get(path)
    }
}

/// Source of a configuration value
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum ValueSource {
    /// Default from schema
    SchemaDefault,

    /// From pack's values.yaml
    PackDefault,

    /// From user's values file
    ValuesFile {
        path: String,
        line: Option<u32>,
    },

    /// From --set command line flag
    CommandLine {
        flag: String,
        timestamp: DateTime<Utc>,
        user: Option<String>,
    },

    /// From environment variable
    Environment { var: String },

    /// Merged from multiple sources
    Merged { sources: Vec<String> },
}

impl std::fmt::Display for ValueSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SchemaDefault => write!(f, "schema default"),
            Self::PackDefault => write!(f, "pack default"),
            Self::ValuesFile { path, line } => {
                if let Some(l) = line {
                    write!(f, "{}:{}", path, l)
                } else {
                    write!(f, "{}", path)
                }
            }
            Self::CommandLine { flag, timestamp, user } => {
                if let Some(u) = user {
                    write!(f, "--set {} by {} at {}", flag, u, timestamp.format("%Y-%m-%d %H:%M"))
                } else {
                    write!(f, "--set {} at {}", flag, timestamp.format("%Y-%m-%d %H:%M"))
                }
            }
            Self::Environment { var } => write!(f, "env ${}", var),
            Self::Merged { sources } => write!(f, "merged from: {}", sources.join(", ")),
        }
    }
}

/// Serialization helper for chrono::Duration
mod duration_serde {
    use chrono::Duration;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<S>(duration: &Duration, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        duration.num_seconds().serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Duration, D::Error>
    where
        D: Deserializer<'de>,
    {
        let seconds = i64::deserialize(deserializer)?;
        Ok(Duration::seconds(seconds))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_release_state_stale_detection() {
        // Create a state that started long ago
        let old_time = Utc::now() - Duration::minutes(10);
        let state = ReleaseState::PendingInstall {
            started_at: old_time,
            timeout: Duration::minutes(5),
        };

        assert!(state.is_stale());
        assert!(state.auto_recover().is_some());
    }

    #[test]
    fn test_release_state_not_stale() {
        let state = ReleaseState::PendingInstall {
            started_at: Utc::now(),
            timeout: Duration::minutes(5),
        };

        assert!(!state.is_stale());
        assert!(state.auto_recover().is_none());
    }

    #[test]
    fn test_terminal_states() {
        assert!(ReleaseState::Deployed.is_pending() == false);
        assert!(ReleaseState::PendingInstall {
            started_at: Utc::now(),
            timeout: Duration::minutes(5),
        }
        .is_pending());
    }

    #[test]
    fn test_storage_key() {
        let release = StoredRelease::for_install(
            "myapp".to_string(),
            "default".to_string(),
            PackMetadata {
                name: "test".to_string(),
                version: semver::Version::new(1, 0, 0),
                description: None,
                app_version: None,
                kube_version: None,
                home: None,
                icon: None,
                sources: vec![],
                keywords: vec![],
                maintainers: vec![],
                annotations: Default::default(),
            },
            Values::new(),
            "apiVersion: v1".to_string(),
        );

        assert_eq!(release.storage_key(), "sh.sherpack.release.v1.myapp.v1");
    }
}
