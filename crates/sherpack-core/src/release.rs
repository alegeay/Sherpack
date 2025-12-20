//! Release management types

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::pack::PackMetadata;
use crate::values::Values;

/// A deployed release of a pack
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Release {
    /// Release name
    pub name: String,

    /// Kubernetes namespace
    pub namespace: String,

    /// Revision number
    pub revision: u32,

    /// Current status
    pub status: ReleaseStatus,

    /// Pack metadata at deploy time
    pub pack: PackMetadata,

    /// Values used for this release
    pub values: Values,

    /// Rendered manifest
    pub manifest: String,

    /// Creation timestamp
    pub created_at: DateTime<Utc>,

    /// Last update timestamp
    pub updated_at: DateTime<Utc>,
}

/// Release status
///
/// Note: This enum is non-exhaustive - new variants may be added in future versions.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub enum ReleaseStatus {
    #[default]
    Unknown,
    Deployed,
    Uninstalled,
    Superseded,
    Failed,
    Uninstalling,
    PendingInstall,
    PendingUpgrade,
    PendingRollback,
}

impl std::fmt::Display for ReleaseStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Self::Unknown => "unknown",
            Self::Deployed => "deployed",
            Self::Uninstalled => "uninstalled",
            Self::Superseded => "superseded",
            Self::Failed => "failed",
            Self::Uninstalling => "uninstalling",
            Self::PendingInstall => "pending-install",
            Self::PendingUpgrade => "pending-upgrade",
            Self::PendingRollback => "pending-rollback",
        };
        write!(f, "{}", s)
    }
}

/// Release information for templates
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseInfo {
    /// Release name
    pub name: String,

    /// Target namespace
    pub namespace: String,

    /// Revision number
    pub revision: u32,

    /// Is this an install operation?
    pub is_install: bool,

    /// Is this an upgrade operation?
    pub is_upgrade: bool,

    /// Service (always "Sherpack")
    pub service: String,
}

impl ReleaseInfo {
    /// Create release info for a new install
    pub fn for_install(name: &str, namespace: &str) -> Self {
        Self {
            name: name.to_string(),
            namespace: namespace.to_string(),
            revision: 1,
            is_install: true,
            is_upgrade: false,
            service: "Sherpack".to_string(),
        }
    }

    /// Create release info for an upgrade
    pub fn for_upgrade(name: &str, namespace: &str, revision: u32) -> Self {
        Self {
            name: name.to_string(),
            namespace: namespace.to_string(),
            revision,
            is_install: false,
            is_upgrade: true,
            service: "Sherpack".to_string(),
        }
    }
}
