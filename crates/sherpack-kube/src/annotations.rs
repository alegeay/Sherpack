//! Annotation parsing with Helm compatibility
//!
//! Sherpack supports both `sherpack.io/*` and `helm.sh/*` annotations
//! to facilitate migration from Helm charts.

use std::collections::BTreeMap;
use std::time::Duration;

/// Sherpack-native annotations
pub mod sherpack {
    /// Hook phase annotation
    pub const HOOK: &str = "sherpack.io/hook";
    /// Hook weight for ordering
    pub const HOOK_WEIGHT: &str = "sherpack.io/hook-weight";
    /// Hook timeout
    pub const HOOK_TIMEOUT: &str = "sherpack.io/hook-timeout";
    /// Hook delete policy
    pub const HOOK_DELETE_POLICY: &str = "sherpack.io/hook-delete-policy";
    /// Hook failure policy
    pub const HOOK_FAILURE_POLICY: &str = "sherpack.io/hook-failure-policy";
    /// Hook retry count
    pub const HOOK_RETRIES: &str = "sherpack.io/hook-retries";
    /// Sync wave for ordering resources
    pub const SYNC_WAVE: &str = "sherpack.io/sync-wave";
    /// Wait for another resource before applying
    pub const WAIT_FOR: &str = "sherpack.io/wait-for";
    /// Custom health check configuration
    pub const HEALTH_CHECK: &str = "sherpack.io/health-check";
    /// Skip waiting for this resource
    pub const SKIP_WAIT: &str = "sherpack.io/skip-wait";
}

/// Helm-compatible annotations (for migration)
pub mod helm {
    /// Hook phase annotation
    pub const HOOK: &str = "helm.sh/hook";
    /// Hook weight for ordering
    pub const HOOK_WEIGHT: &str = "helm.sh/hook-weight";
    /// Hook delete policy
    pub const HOOK_DELETE_POLICY: &str = "helm.sh/hook-delete-policy";
    /// Resource policy (keep on uninstall)
    pub const RESOURCE_POLICY: &str = "helm.sh/resource-policy";
}

/// Get annotation value, preferring Sherpack over Helm
pub fn get_annotation<'a>(
    annotations: &'a BTreeMap<String, String>,
    sherpack_key: &str,
    helm_key: &str,
) -> Option<&'a str> {
    annotations
        .get(sherpack_key)
        .or_else(|| annotations.get(helm_key))
        .map(|s| s.as_str())
}

/// Get annotation with only Sherpack key
pub fn get_sherpack_annotation<'a>(
    annotations: &'a BTreeMap<String, String>,
    key: &str,
) -> Option<&'a str> {
    annotations.get(key).map(|s| s.as_str())
}

/// Parse hook phases from annotation value
pub fn parse_hook_phases(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

/// Parse hook weight (default: 0)
pub fn parse_hook_weight(annotations: &BTreeMap<String, String>) -> i32 {
    get_annotation(annotations, sherpack::HOOK_WEIGHT, helm::HOOK_WEIGHT)
        .and_then(|s| s.parse().ok())
        .unwrap_or(0)
}

/// Parse sync wave (default: 0)
pub fn parse_sync_wave(annotations: &BTreeMap<String, String>) -> i32 {
    get_sherpack_annotation(annotations, sherpack::SYNC_WAVE)
        .and_then(|s| s.parse().ok())
        .unwrap_or(0)
}

/// Parse wait-for dependencies
/// Format: "kind/name" or "kind/name,kind/name"
pub fn parse_wait_for(annotations: &BTreeMap<String, String>) -> Vec<ResourceRef> {
    get_sherpack_annotation(annotations, sherpack::WAIT_FOR)
        .map(|s| {
            s.split(',')
                .filter_map(|dep| {
                    let dep = dep.trim();
                    let parts: Vec<&str> = dep.split('/').collect();
                    if parts.len() == 2 {
                        Some(ResourceRef {
                            kind: parts[0].to_string(),
                            name: parts[1].to_string(),
                        })
                    } else {
                        None
                    }
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Parse timeout duration from string (e.g., "5m", "300s", "1h")
pub fn parse_duration(value: &str) -> Option<Duration> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }

    let (num_str, unit) = if let Some(stripped) = value.strip_suffix("ms") {
        (stripped, "ms")
    } else if let Some(stripped) = value.strip_suffix('s') {
        (stripped, "s")
    } else if let Some(stripped) = value.strip_suffix('m') {
        (stripped, "m")
    } else if let Some(stripped) = value.strip_suffix('h') {
        (stripped, "h")
    } else {
        // Assume seconds if no unit
        (value, "s")
    };

    let num: u64 = num_str.parse().ok()?;

    Some(match unit {
        "ms" => Duration::from_millis(num),
        "s" => Duration::from_secs(num),
        "m" => Duration::from_secs(num * 60),
        "h" => Duration::from_secs(num * 3600),
        _ => return None,
    })
}

/// Parse hook timeout (default: 5 minutes)
pub fn parse_hook_timeout(annotations: &BTreeMap<String, String>) -> Duration {
    get_sherpack_annotation(annotations, sherpack::HOOK_TIMEOUT)
        .and_then(parse_duration)
        .unwrap_or(Duration::from_secs(300))
}

/// Parse hook delete policy
pub fn parse_delete_policy(annotations: &BTreeMap<String, String>) -> DeletePolicy {
    let value = get_annotation(
        annotations,
        sherpack::HOOK_DELETE_POLICY,
        helm::HOOK_DELETE_POLICY,
    );

    match value {
        Some(s) => {
            let policies: Vec<&str> = s.split(',').map(|p| p.trim()).collect();

            if policies.contains(&"before-hook-creation") {
                DeletePolicy::BeforeHookCreation
            } else if policies.contains(&"hook-succeeded") && policies.contains(&"hook-failed") {
                DeletePolicy::Always
            } else if policies.contains(&"hook-succeeded") {
                DeletePolicy::OnSuccess
            } else if policies.contains(&"hook-failed") {
                DeletePolicy::OnFailure
            } else {
                DeletePolicy::BeforeHookCreation // Default
            }
        }
        None => DeletePolicy::BeforeHookCreation,
    }
}

/// Parse failure policy
pub fn parse_failure_policy(annotations: &BTreeMap<String, String>) -> FailurePolicy {
    get_sherpack_annotation(annotations, sherpack::HOOK_FAILURE_POLICY)
        .map(|s| match s.to_lowercase().as_str() {
            "continue" => FailurePolicy::Continue,
            "rollback" => FailurePolicy::Rollback,
            "fail" | "abort" => FailurePolicy::Fail,
            s if s.starts_with("retry") => {
                // Parse "retry(3)" or "retry:3"
                let count = s
                    .trim_start_matches("retry")
                    .trim_start_matches('(')
                    .trim_start_matches(':')
                    .trim_end_matches(')')
                    .parse()
                    .unwrap_or(3);
                FailurePolicy::Retry(count)
            }
            _ => FailurePolicy::Fail,
        })
        .unwrap_or(FailurePolicy::Fail)
}

/// Check if resource should skip wait
pub fn should_skip_wait(annotations: &BTreeMap<String, String>) -> bool {
    get_sherpack_annotation(annotations, sherpack::SKIP_WAIT)
        .map(|s| s.to_lowercase() == "true" || s == "1")
        .unwrap_or(false)
}

/// Reference to a Kubernetes resource
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ResourceRef {
    pub kind: String,
    pub name: String,
}

impl ResourceRef {
    pub fn new(kind: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            kind: kind.into(),
            name: name.into(),
        }
    }
}

impl std::fmt::Display for ResourceRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}/{}", self.kind, self.name)
    }
}

/// Hook delete policy
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DeletePolicy {
    /// Delete before creating a new hook (default)
    #[default]
    BeforeHookCreation,
    /// Delete after successful completion
    OnSuccess,
    /// Delete after failure
    OnFailure,
    /// Always delete (success or failure)
    Always,
    /// Never delete
    Never,
}

/// Hook failure policy
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FailurePolicy {
    /// Fail the entire operation (default)
    #[default]
    Fail,
    /// Log and continue
    Continue,
    /// Trigger rollback
    Rollback,
    /// Retry N times
    Retry(u32),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_annotations(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn test_get_annotation_prefers_sherpack() {
        let annotations = make_annotations(&[
            ("sherpack.io/hook", "pre-install"),
            ("helm.sh/hook", "post-install"),
        ]);

        let result = get_annotation(&annotations, sherpack::HOOK, helm::HOOK);
        assert_eq!(result, Some("pre-install"));
    }

    #[test]
    fn test_get_annotation_falls_back_to_helm() {
        let annotations = make_annotations(&[("helm.sh/hook", "post-install")]);

        let result = get_annotation(&annotations, sherpack::HOOK, helm::HOOK);
        assert_eq!(result, Some("post-install"));
    }

    #[test]
    fn test_parse_hook_phases() {
        assert_eq!(
            parse_hook_phases("pre-install,post-upgrade"),
            vec!["pre-install", "post-upgrade"]
        );
        assert_eq!(parse_hook_phases("pre-install"), vec!["pre-install"]);
        assert_eq!(
            parse_hook_phases(" pre-install , post-install "),
            vec!["pre-install", "post-install"]
        );
    }

    #[test]
    fn test_parse_sync_wave() {
        let annotations = make_annotations(&[("sherpack.io/sync-wave", "2")]);
        assert_eq!(parse_sync_wave(&annotations), 2);

        let annotations = make_annotations(&[("sherpack.io/sync-wave", "-1")]);
        assert_eq!(parse_sync_wave(&annotations), -1);

        let empty: BTreeMap<String, String> = BTreeMap::new();
        assert_eq!(parse_sync_wave(&empty), 0);
    }

    #[test]
    fn test_parse_wait_for() {
        let annotations = make_annotations(&[("sherpack.io/wait-for", "Deployment/postgres")]);
        let deps = parse_wait_for(&annotations);
        assert_eq!(deps.len(), 1);
        assert_eq!(deps[0].kind, "Deployment");
        assert_eq!(deps[0].name, "postgres");

        let annotations =
            make_annotations(&[("sherpack.io/wait-for", "Deployment/db, Service/cache")]);
        let deps = parse_wait_for(&annotations);
        assert_eq!(deps.len(), 2);
    }

    #[test]
    fn test_parse_duration() {
        assert_eq!(parse_duration("5m"), Some(Duration::from_secs(300)));
        assert_eq!(parse_duration("30s"), Some(Duration::from_secs(30)));
        assert_eq!(parse_duration("1h"), Some(Duration::from_secs(3600)));
        assert_eq!(parse_duration("100ms"), Some(Duration::from_millis(100)));
        assert_eq!(parse_duration("60"), Some(Duration::from_secs(60)));
        assert_eq!(parse_duration(""), None);
    }

    #[test]
    fn test_parse_delete_policy() {
        let annotations = make_annotations(&[("helm.sh/hook-delete-policy", "hook-succeeded")]);
        assert_eq!(parse_delete_policy(&annotations), DeletePolicy::OnSuccess);

        let annotations =
            make_annotations(&[("helm.sh/hook-delete-policy", "hook-succeeded,hook-failed")]);
        assert_eq!(parse_delete_policy(&annotations), DeletePolicy::Always);

        let annotations =
            make_annotations(&[("helm.sh/hook-delete-policy", "before-hook-creation")]);
        assert_eq!(
            parse_delete_policy(&annotations),
            DeletePolicy::BeforeHookCreation
        );
    }

    #[test]
    fn test_parse_failure_policy() {
        let annotations = make_annotations(&[("sherpack.io/hook-failure-policy", "continue")]);
        assert_eq!(parse_failure_policy(&annotations), FailurePolicy::Continue);

        let annotations = make_annotations(&[("sherpack.io/hook-failure-policy", "retry(5)")]);
        assert_eq!(parse_failure_policy(&annotations), FailurePolicy::Retry(5));

        let annotations = make_annotations(&[("sherpack.io/hook-failure-policy", "retry:3")]);
        assert_eq!(parse_failure_policy(&annotations), FailurePolicy::Retry(3));
    }
}
