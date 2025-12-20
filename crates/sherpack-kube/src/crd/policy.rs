//! CRD policy and ownership types
//!
//! This module provides intent-based CRD handling, allowing users to explicitly
//! declare how CRDs should be managed during install, upgrade, and uninstall.
//!
//! # Policies
//!
//! Unlike Helm's location-based rules (crds/ vs templates/), Sherpack uses
//! explicit policies that can be set via annotations:
//!
//! - `managed`: This release owns the CRD (default, protected on uninstall)
//! - `shared`: CRD is shared between releases (never delete)
//! - `external`: CRD is managed externally (don't touch)
//!
//! # Example
//!
//! ```yaml
//! apiVersion: apiextensions.k8s.io/v1
//! kind: CustomResourceDefinition
//! metadata:
//!   name: myresources.example.com
//!   annotations:
//!     sherpack.io/crd-policy: shared
//! ```

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Annotation key for CRD policy
pub const CRD_POLICY_ANNOTATION: &str = "sherpack.io/crd-policy";

/// Annotation key for deletion protection (legacy Helm compatibility)
pub const HELM_RESOURCE_POLICY: &str = "helm.sh/resource-policy";

/// CRD management policy
///
/// Determines how Sherpack handles a CRD during lifecycle operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CrdPolicy {
    /// CRD is owned by this release (default)
    ///
    /// - Installed with the release
    /// - Updated during upgrades (subject to strategy)
    /// - Protected on uninstall (requires `--delete-crds --confirm-crd-deletion`)
    #[default]
    Managed,

    /// CRD is shared between multiple releases
    ///
    /// - Installed if not present
    /// - Updated during upgrades (subject to strategy)
    /// - Never deleted (even with `--delete-crds`)
    Shared,

    /// CRD is managed externally (GitOps, kubectl, etc.)
    ///
    /// - Never installed by Sherpack
    /// - Never updated by Sherpack
    /// - Never deleted by Sherpack
    External,
}

impl CrdPolicy {
    /// Parse policy from annotation value
    pub fn from_annotation(value: &str) -> Option<Self> {
        match value.to_lowercase().as_str() {
            "managed" => Some(Self::Managed),
            "shared" => Some(Self::Shared),
            "external" => Some(Self::External),
            _ => None,
        }
    }

    /// Check if this policy allows installation
    pub fn allows_install(&self) -> bool {
        matches!(self, Self::Managed | Self::Shared)
    }

    /// Check if this policy allows updates
    pub fn allows_update(&self) -> bool {
        matches!(self, Self::Managed | Self::Shared)
    }

    /// Check if this policy allows deletion
    pub fn allows_delete(&self) -> bool {
        matches!(self, Self::Managed)
    }

    /// Human-readable description
    pub fn description(&self) -> &'static str {
        match self {
            Self::Managed => "owned by this release",
            Self::Shared => "shared between releases",
            Self::External => "managed externally",
        }
    }
}

impl std::fmt::Display for CrdPolicy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Managed => write!(f, "managed"),
            Self::Shared => write!(f, "shared"),
            Self::External => write!(f, "external"),
        }
    }
}

/// Location where a CRD was found
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CrdLocation {
    /// CRD from crds/ directory (static, not templated)
    CrdsDirectory {
        /// Relative path within crds/
        path: PathBuf,
        /// Whether the file contains Jinja syntax (templated)
        templated: bool,
    },

    /// CRD detected in templates/ directory
    Templates {
        /// Relative path within templates/
        path: PathBuf,
    },

    /// CRD from a dependency pack
    Dependency {
        /// Dependency name
        dependency_name: String,
        /// Location within the dependency
        inner_location: Box<CrdLocation>,
    },
}

impl CrdLocation {
    /// Create a new crds/ directory location
    pub fn crds_directory(path: impl Into<PathBuf>, templated: bool) -> Self {
        Self::CrdsDirectory {
            path: path.into(),
            templated,
        }
    }

    /// Create a new templates/ directory location
    pub fn templates(path: impl Into<PathBuf>) -> Self {
        Self::Templates { path: path.into() }
    }

    /// Create a dependency location
    pub fn dependency(name: impl Into<String>, inner: CrdLocation) -> Self {
        Self::Dependency {
            dependency_name: name.into(),
            inner_location: Box::new(inner),
        }
    }

    /// Check if this CRD is from crds/ directory
    pub fn is_from_crds_dir(&self) -> bool {
        matches!(self, Self::CrdsDirectory { .. })
    }

    /// Check if this CRD is templated
    pub fn is_templated(&self) -> bool {
        match self {
            Self::CrdsDirectory { templated, .. } => *templated,
            Self::Templates { .. } => true, // Templates are always templated
            Self::Dependency { inner_location, .. } => inner_location.is_templated(),
        }
    }

    /// Get the file path
    pub fn path(&self) -> &PathBuf {
        match self {
            Self::CrdsDirectory { path, .. } => path,
            Self::Templates { path } => path,
            Self::Dependency { inner_location, .. } => inner_location.path(),
        }
    }

    /// Human-readable description
    pub fn description(&self) -> String {
        match self {
            Self::CrdsDirectory { path, templated } => {
                if *templated {
                    format!("crds/{} (templated)", path.display())
                } else {
                    format!("crds/{}", path.display())
                }
            }
            Self::Templates { path } => format!("templates/{}", path.display()),
            Self::Dependency {
                dependency_name,
                inner_location,
            } => format!(
                "dependency:{}/{}",
                dependency_name,
                inner_location.description()
            ),
        }
    }
}

/// Ownership information for a CRD
///
/// Tracks which release owns a CRD and its management policy.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrdOwnership {
    /// CRD name (e.g., "myresources.example.com")
    pub crd_name: String,

    /// Release that owns this CRD
    pub owning_release: String,

    /// Namespace of the owning release
    pub release_namespace: String,

    /// Management policy
    pub policy: CrdPolicy,

    /// Where the CRD was defined
    pub location: CrdLocation,

    /// Version when the CRD was installed
    pub installed_version: Option<String>,
}

impl CrdOwnership {
    /// Create new ownership info
    pub fn new(
        crd_name: impl Into<String>,
        release: impl Into<String>,
        namespace: impl Into<String>,
        policy: CrdPolicy,
        location: CrdLocation,
    ) -> Self {
        Self {
            crd_name: crd_name.into(),
            owning_release: release.into(),
            release_namespace: namespace.into(),
            policy,
            location,
            installed_version: None,
        }
    }

    /// Set the installed version
    pub fn with_version(mut self, version: impl Into<String>) -> Self {
        self.installed_version = Some(version.into());
        self
    }

    /// Check if a release can manage this CRD
    pub fn can_manage(&self, release: &str, namespace: &str) -> bool {
        self.owning_release == release && self.release_namespace == namespace
    }
}

/// A CRD that has been detected and analyzed
#[derive(Debug, Clone)]
pub struct DetectedCrd {
    /// CRD name (metadata.name)
    pub name: String,

    /// Raw YAML content
    pub content: String,

    /// Where the CRD was found
    pub location: CrdLocation,

    /// Detected policy (from annotation or default)
    pub policy: CrdPolicy,

    /// Whether deletion protection annotation is present
    pub has_keep_annotation: bool,
}

impl DetectedCrd {
    /// Create a new detected CRD
    pub fn new(name: impl Into<String>, content: impl Into<String>, location: CrdLocation) -> Self {
        let content = content.into();
        let (policy, has_keep_annotation) = Self::extract_policy(&content);

        Self {
            name: name.into(),
            content,
            location,
            policy,
            has_keep_annotation,
        }
    }

    /// Extract policy from CRD annotations
    fn extract_policy(content: &str) -> (CrdPolicy, bool) {
        // Parse YAML to extract annotations
        let parsed: Result<serde_yaml::Value, _> = serde_yaml::from_str(content);
        let Ok(value) = parsed else {
            return (CrdPolicy::default(), false);
        };

        let annotations = value
            .get("metadata")
            .and_then(|m| m.get("annotations"))
            .and_then(|a| a.as_mapping());

        let Some(annotations) = annotations else {
            return (CrdPolicy::default(), false);
        };

        // Check for sherpack.io/crd-policy
        let policy = annotations
            .get(serde_yaml::Value::String(CRD_POLICY_ANNOTATION.to_string()))
            .and_then(|v| v.as_str())
            .and_then(CrdPolicy::from_annotation)
            .unwrap_or_default();

        // Check for helm.sh/resource-policy: keep
        let has_keep = annotations
            .get(serde_yaml::Value::String(HELM_RESOURCE_POLICY.to_string()))
            .and_then(|v| v.as_str())
            .is_some_and(|v| v == "keep");

        (policy, has_keep)
    }

    /// Check if this CRD is protected from deletion
    pub fn is_protected(&self) -> bool {
        // Protected if:
        // - Policy doesn't allow delete (shared/external)
        // - Has helm.sh/resource-policy: keep annotation
        // - Policy is managed (protected by default, needs --delete-crds)
        !self.policy.allows_delete()
            || self.has_keep_annotation
            || self.policy == CrdPolicy::Managed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_crd_policy_default() {
        assert_eq!(CrdPolicy::default(), CrdPolicy::Managed);
    }

    #[test]
    fn test_crd_policy_from_annotation() {
        assert_eq!(
            CrdPolicy::from_annotation("managed"),
            Some(CrdPolicy::Managed)
        );
        assert_eq!(
            CrdPolicy::from_annotation("SHARED"),
            Some(CrdPolicy::Shared)
        );
        assert_eq!(
            CrdPolicy::from_annotation("External"),
            Some(CrdPolicy::External)
        );
        assert_eq!(CrdPolicy::from_annotation("invalid"), None);
    }

    #[test]
    fn test_crd_policy_permissions() {
        let managed = CrdPolicy::Managed;
        assert!(managed.allows_install());
        assert!(managed.allows_update());
        assert!(managed.allows_delete());

        let shared = CrdPolicy::Shared;
        assert!(shared.allows_install());
        assert!(shared.allows_update());
        assert!(!shared.allows_delete());

        let external = CrdPolicy::External;
        assert!(!external.allows_install());
        assert!(!external.allows_update());
        assert!(!external.allows_delete());
    }

    #[test]
    fn test_crd_location_crds_dir() {
        let loc = CrdLocation::crds_directory("mycrd.yaml", false);
        assert!(loc.is_from_crds_dir());
        assert!(!loc.is_templated());
        assert_eq!(loc.description(), "crds/mycrd.yaml");
    }

    #[test]
    fn test_crd_location_crds_dir_templated() {
        let loc = CrdLocation::crds_directory("mycrd.yaml", true);
        assert!(loc.is_from_crds_dir());
        assert!(loc.is_templated());
        assert_eq!(loc.description(), "crds/mycrd.yaml (templated)");
    }

    #[test]
    fn test_crd_location_templates() {
        let loc = CrdLocation::templates("operator-crd.yaml");
        assert!(!loc.is_from_crds_dir());
        assert!(loc.is_templated());
        assert_eq!(loc.description(), "templates/operator-crd.yaml");
    }

    #[test]
    fn test_crd_location_dependency() {
        let inner = CrdLocation::crds_directory("cert.yaml", false);
        let loc = CrdLocation::dependency("cert-manager", inner);
        assert!(!loc.is_templated());
        assert_eq!(loc.description(), "dependency:cert-manager/crds/cert.yaml");
    }

    #[test]
    fn test_detected_crd_extracts_policy() {
        let content = r#"
apiVersion: apiextensions.k8s.io/v1
kind: CustomResourceDefinition
metadata:
  name: tests.example.com
  annotations:
    sherpack.io/crd-policy: shared
"#;
        let crd = DetectedCrd::new(
            "tests.example.com",
            content,
            CrdLocation::crds_directory("test.yaml", false),
        );

        assert_eq!(crd.policy, CrdPolicy::Shared);
        assert!(!crd.has_keep_annotation);
    }

    #[test]
    fn test_detected_crd_extracts_keep_annotation() {
        let content = r#"
apiVersion: apiextensions.k8s.io/v1
kind: CustomResourceDefinition
metadata:
  name: tests.example.com
  annotations:
    helm.sh/resource-policy: keep
"#;
        let crd = DetectedCrd::new(
            "tests.example.com",
            content,
            CrdLocation::crds_directory("test.yaml", false),
        );

        assert_eq!(crd.policy, CrdPolicy::Managed); // default
        assert!(crd.has_keep_annotation);
        assert!(crd.is_protected());
    }

    #[test]
    fn test_crd_ownership() {
        let ownership = CrdOwnership::new(
            "tests.example.com",
            "my-release",
            "default",
            CrdPolicy::Managed,
            CrdLocation::crds_directory("test.yaml", false),
        )
        .with_version("1.0.0");

        assert!(ownership.can_manage("my-release", "default"));
        assert!(!ownership.can_manage("other-release", "default"));
        assert!(!ownership.can_manage("my-release", "other-namespace"));
    }

    #[test]
    fn test_crd_policy_serialization() {
        assert_eq!(
            serde_yaml::to_string(&CrdPolicy::Managed).unwrap().trim(),
            "managed"
        );
        assert_eq!(
            serde_yaml::to_string(&CrdPolicy::Shared).unwrap().trim(),
            "shared"
        );
        assert_eq!(
            serde_yaml::to_string(&CrdPolicy::External).unwrap().trim(),
            "external"
        );
    }
}
