//! Improved hooks system with better policies and error handling
//!
//! Key improvements over Helm:
//! - Unique hook names per revision (prevents "already exists" errors)
//! - Configurable failure policies (fail, continue, rollback, retry)
//! - Better cleanup policies including "keep last N"
//! - "During" phase hooks (after resources created, before ready)
//! - Explicit timeouts

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

/// Hook execution phase
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub enum HookPhase {
    /// Before installation begins
    PreInstall,
    /// After resources created, before they're ready
    DuringInstall,
    /// After installation completes successfully
    PostInstall,

    /// Before upgrade begins
    PreUpgrade,
    /// After new resources created, before ready
    DuringUpgrade,
    /// After upgrade completes successfully
    PostUpgrade,

    /// Before rollback begins
    PreRollback,
    /// After rollback completes
    PostRollback,

    /// Before uninstall begins
    PreDelete,
    /// After uninstall completes
    PostDelete,

    /// Test hooks (run on demand)
    Test,
}

impl HookPhase {
    /// Get all phases for an install operation
    pub fn install_phases() -> &'static [HookPhase] {
        &[
            HookPhase::PreInstall,
            HookPhase::DuringInstall,
            HookPhase::PostInstall,
        ]
    }

    /// Get all phases for an upgrade operation
    pub fn upgrade_phases() -> &'static [HookPhase] {
        &[
            HookPhase::PreUpgrade,
            HookPhase::DuringUpgrade,
            HookPhase::PostUpgrade,
        ]
    }

    /// Get all phases for a rollback operation
    pub fn rollback_phases() -> &'static [HookPhase] {
        &[HookPhase::PreRollback, HookPhase::PostRollback]
    }

    /// Get all phases for an uninstall operation
    pub fn delete_phases() -> &'static [HookPhase] {
        &[HookPhase::PreDelete, HookPhase::PostDelete]
    }

    /// Is this a "pre" phase (before the operation)?
    pub fn is_pre(&self) -> bool {
        matches!(
            self,
            HookPhase::PreInstall
                | HookPhase::PreUpgrade
                | HookPhase::PreRollback
                | HookPhase::PreDelete
        )
    }

    /// Is this a "post" phase (after the operation)?
    pub fn is_post(&self) -> bool {
        matches!(
            self,
            HookPhase::PostInstall
                | HookPhase::PostUpgrade
                | HookPhase::PostRollback
                | HookPhase::PostDelete
        )
    }

    /// Is this a "during" phase?
    pub fn is_during(&self) -> bool {
        matches!(self, HookPhase::DuringInstall | HookPhase::DuringUpgrade)
    }
}

impl std::fmt::Display for HookPhase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            HookPhase::PreInstall => "pre-install",
            HookPhase::DuringInstall => "during-install",
            HookPhase::PostInstall => "post-install",
            HookPhase::PreUpgrade => "pre-upgrade",
            HookPhase::DuringUpgrade => "during-upgrade",
            HookPhase::PostUpgrade => "post-upgrade",
            HookPhase::PreRollback => "pre-rollback",
            HookPhase::PostRollback => "post-rollback",
            HookPhase::PreDelete => "pre-delete",
            HookPhase::PostDelete => "post-delete",
            HookPhase::Test => "test",
        };
        write!(f, "{}", s)
    }
}

/// Hook definition
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Hook {
    /// Hook name (used to generate unique resource names)
    pub name: String,

    /// Phases when this hook should run
    pub phases: Vec<HookPhase>,

    /// The Kubernetes resource to create (as YAML)
    pub resource: String,

    /// Weight for ordering (lower = runs first)
    #[serde(default)]
    pub weight: i32,

    /// What to do if the hook fails
    #[serde(default)]
    pub on_failure: HookFailurePolicy,

    /// Timeout for hook execution
    #[serde(default = "default_hook_timeout")]
    #[serde(with = "duration_serde")]
    pub timeout: Duration,

    /// Cleanup policy after hook completes
    #[serde(default)]
    pub cleanup: HookCleanupPolicy,
}

fn default_hook_timeout() -> Duration {
    Duration::minutes(5)
}

impl Hook {
    /// Generate a unique resource name for this hook
    ///
    /// Format: {release}-{hook_name}-{phase}-{revision}
    /// This prevents "already exists" errors that plague Helm
    pub fn unique_name(&self, release: &str, phase: HookPhase, revision: u32) -> String {
        format!("{}-{}-{}-v{}", release, self.name, phase, revision)
    }

    /// Check if this hook should run for a given phase
    pub fn runs_in_phase(&self, phase: HookPhase) -> bool {
        self.phases.contains(&phase)
    }
}

/// What to do when a hook fails
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum HookFailurePolicy {
    /// Fail the entire operation (default for pre-* hooks)
    #[default]
    FailOperation,

    /// Log the error but continue (default for post-* hooks)
    Continue,

    /// Automatically rollback the operation
    Rollback,

    /// Retry the hook N times before failing
    Retry {
        max_attempts: u32,
        #[serde(default = "default_retry_backoff")]
        #[serde(with = "duration_serde")]
        backoff: Duration,
    },
}

fn default_retry_backoff() -> Duration {
    Duration::seconds(5)
}

/// When to clean up hook resources
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum HookCleanupPolicy {
    /// Never delete the hook resource
    Never,

    /// Delete before the next run of this hook (Helm's before-hook-creation)
    #[default]
    BeforeNextRun,

    /// Delete immediately after successful completion
    OnSuccess,

    /// Delete after a delay (useful for debugging)
    AfterDelay(#[serde(with = "duration_serde")] Duration),

    /// Keep the last N executions (useful for auditing)
    KeepLast(u32),
}

/// Result of executing a hook
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookResult {
    /// Hook name
    pub name: String,

    /// Phase it ran in
    pub phase: HookPhase,

    /// Whether it succeeded
    pub success: bool,

    /// Error message if failed
    pub error: Option<String>,

    /// When execution started
    pub started_at: DateTime<Utc>,

    /// When execution completed
    pub completed_at: DateTime<Utc>,

    /// Number of retry attempts
    pub attempts: u32,
}

impl HookResult {
    /// Create a successful result
    pub fn success(name: String, phase: HookPhase, started_at: DateTime<Utc>) -> Self {
        Self {
            name,
            phase,
            success: true,
            error: None,
            started_at,
            completed_at: Utc::now(),
            attempts: 1,
        }
    }

    /// Create a failed result
    pub fn failure(
        name: String,
        phase: HookPhase,
        error: String,
        started_at: DateTime<Utc>,
        attempts: u32,
    ) -> Self {
        Self {
            name,
            phase,
            success: false,
            error: Some(error),
            started_at,
            completed_at: Utc::now(),
            attempts,
        }
    }

    /// Duration of execution
    pub fn duration(&self) -> Duration {
        self.completed_at.signed_duration_since(self.started_at)
    }
}

/// Hook executor for running hooks against a Kubernetes cluster
pub struct HookExecutor {
    /// Results of executed hooks
    pub results: Vec<HookResult>,
}

impl HookExecutor {
    /// Create a new hook executor
    pub fn new() -> Self {
        Self {
            results: Vec::new(),
        }
    }

    /// Execute all hooks for a given phase
    ///
    /// Returns Ok(()) if all hooks succeeded or were configured to continue on failure.
    /// Returns Err if any hook failed with FailOperation policy.
    pub async fn execute_phase(
        &mut self,
        hooks: &[Hook],
        phase: HookPhase,
        release_name: &str,
        revision: u32,
        _client: &kube::Client,
    ) -> crate::Result<()> {
        // Filter and sort hooks for this phase
        let mut phase_hooks: Vec<&Hook> = hooks
            .iter()
            .filter(|h| h.runs_in_phase(phase))
            .collect();

        phase_hooks.sort_by_key(|h| h.weight);

        for hook in phase_hooks {
            let started_at = Utc::now();
            let unique_name = hook.unique_name(release_name, phase, revision);

            // TODO: Actually execute the hook against Kubernetes
            // For now, we simulate success
            let result = self.execute_single_hook(hook, &unique_name, phase, started_at).await;

            match result {
                Ok(r) => self.results.push(r),
                Err(e) => {
                    let error_msg = e.to_string();
                    match hook.on_failure {
                        HookFailurePolicy::FailOperation => {
                            self.results.push(HookResult::failure(
                                hook.name.clone(),
                                phase,
                                error_msg.clone(),
                                started_at,
                                1,
                            ));
                            return Err(crate::KubeError::HookFailed {
                                hook_name: hook.name.clone(),
                                phase: phase.to_string(),
                                message: error_msg,
                            });
                        }
                        HookFailurePolicy::Continue => {
                            self.results.push(HookResult::failure(
                                hook.name.clone(),
                                phase,
                                error_msg,
                                started_at,
                                1,
                            ));
                            // Continue to next hook
                        }
                        HookFailurePolicy::Rollback => {
                            self.results.push(HookResult::failure(
                                hook.name.clone(),
                                phase,
                                error_msg.clone(),
                                started_at,
                                1,
                            ));
                            return Err(crate::KubeError::HookFailed {
                                hook_name: hook.name.clone(),
                                phase: phase.to_string(),
                                message: format!("{} (triggering rollback)", error_msg),
                            });
                        }
                        HookFailurePolicy::Retry { max_attempts, backoff } => {
                            let mut attempts = 1;
                            #[allow(unused_assignments)]
                            let mut last_error = error_msg; // Initial error that triggered retry

                            while attempts < max_attempts {
                                tokio::time::sleep(backoff.to_std().unwrap_or_default()).await;
                                attempts += 1;

                                match self
                                    .execute_single_hook(hook, &unique_name, phase, started_at)
                                    .await
                                {
                                    Ok(r) => {
                                        let mut success_result = r;
                                        success_result.attempts = attempts;
                                        self.results.push(success_result);
                                        break;
                                    }
                                    Err(e) => {
                                        last_error = e.to_string();
                                        if attempts >= max_attempts {
                                            self.results.push(HookResult::failure(
                                                hook.name.clone(),
                                                phase,
                                                last_error.clone(),
                                                started_at,
                                                attempts,
                                            ));
                                            return Err(crate::KubeError::HookFailed {
                                                hook_name: hook.name.clone(),
                                                phase: phase.to_string(),
                                                message: format!(
                                                    "{} (after {} attempts)",
                                                    last_error, attempts
                                                ),
                                            });
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Execute a single hook
    async fn execute_single_hook(
        &self,
        hook: &Hook,
        _unique_name: &str,
        phase: HookPhase,
        started_at: DateTime<Utc>,
    ) -> crate::Result<HookResult> {
        // TODO: Implement actual hook execution:
        // 1. Parse hook.resource as YAML
        // 2. Set metadata.name to unique_name
        // 3. Apply labels for tracking
        // 4. Create the resource
        // 5. If Job/Pod, wait for completion with timeout
        // 6. Handle cleanup based on policy

        // For now, return success
        Ok(HookResult::success(hook.name.clone(), phase, started_at))
    }

    /// Get all results for a phase
    pub fn results_for_phase(&self, phase: HookPhase) -> Vec<&HookResult> {
        self.results.iter().filter(|r| r.phase == phase).collect()
    }

    /// Check if any hooks failed
    pub fn has_failures(&self) -> bool {
        self.results.iter().any(|r| !r.success)
    }

    /// Get all failed hooks
    pub fn failures(&self) -> Vec<&HookResult> {
        self.results.iter().filter(|r| !r.success).collect()
    }
}

impl Default for HookExecutor {
    fn default() -> Self {
        Self::new()
    }
}

/// Parse hooks from template annotations
pub fn parse_hooks_from_manifest(manifest: &str) -> Vec<Hook> {
    let mut hooks = Vec::new();

    // Split manifest into documents
    for doc in manifest.split("---") {
        let doc = doc.trim();
        if doc.is_empty() {
            continue;
        }

        // Parse as YAML
        let yaml: serde_yaml::Value = match serde_yaml::from_str(doc) {
            Ok(v) => v,
            Err(_) => continue,
        };

        // Check for hook annotations
        let annotations = yaml
            .get("metadata")
            .and_then(|m| m.get("annotations"))
            .and_then(|a| a.as_mapping());

        if let Some(annotations) = annotations {
            // Check for sherpack.io/hook or helm.sh/hook (for compatibility)
            let hook_phases: Option<Vec<HookPhase>> = annotations
                .get(&serde_yaml::Value::String("sherpack.io/hook".to_string()))
                .or_else(|| {
                    annotations.get(&serde_yaml::Value::String("helm.sh/hook".to_string()))
                })
                .and_then(|v| v.as_str())
                .map(|s| parse_hook_phases(s));

            if let Some(phases) = hook_phases {
                let name = yaml
                    .get("metadata")
                    .and_then(|m| m.get("name"))
                    .and_then(|n| n.as_str())
                    .unwrap_or("unnamed-hook")
                    .to_string();

                let weight = annotations
                    .get(&serde_yaml::Value::String("sherpack.io/hook-weight".to_string()))
                    .or_else(|| {
                        annotations
                            .get(&serde_yaml::Value::String("helm.sh/hook-weight".to_string()))
                    })
                    .and_then(|v| v.as_str())
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(0);

                let cleanup = annotations
                    .get(&serde_yaml::Value::String(
                        "sherpack.io/hook-delete-policy".to_string(),
                    ))
                    .or_else(|| {
                        annotations.get(&serde_yaml::Value::String(
                            "helm.sh/hook-delete-policy".to_string(),
                        ))
                    })
                    .and_then(|v| v.as_str())
                    .map(parse_cleanup_policy)
                    .unwrap_or_default();

                hooks.push(Hook {
                    name,
                    phases,
                    resource: doc.to_string(),
                    weight,
                    on_failure: HookFailurePolicy::default(),
                    timeout: default_hook_timeout(),
                    cleanup,
                });
            }
        }
    }

    hooks
}

/// Parse comma-separated hook phases
fn parse_hook_phases(s: &str) -> Vec<HookPhase> {
    s.split(',')
        .filter_map(|p| match p.trim() {
            "pre-install" => Some(HookPhase::PreInstall),
            "during-install" => Some(HookPhase::DuringInstall),
            "post-install" => Some(HookPhase::PostInstall),
            "pre-upgrade" => Some(HookPhase::PreUpgrade),
            "during-upgrade" => Some(HookPhase::DuringUpgrade),
            "post-upgrade" => Some(HookPhase::PostUpgrade),
            "pre-rollback" => Some(HookPhase::PreRollback),
            "post-rollback" => Some(HookPhase::PostRollback),
            "pre-delete" => Some(HookPhase::PreDelete),
            "post-delete" => Some(HookPhase::PostDelete),
            "test" | "test-success" => Some(HookPhase::Test),
            _ => None,
        })
        .collect()
}

/// Parse hook cleanup policy
fn parse_cleanup_policy(s: &str) -> HookCleanupPolicy {
    match s.trim() {
        "before-hook-creation" => HookCleanupPolicy::BeforeNextRun,
        "hook-succeeded" => HookCleanupPolicy::OnSuccess,
        "hook-failed" => HookCleanupPolicy::Never, // Keep on failure for debugging
        _ => HookCleanupPolicy::default(),
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

    fn test_hook(name: &str, phases: Vec<HookPhase>) -> Hook {
        Hook {
            name: name.to_string(),
            phases,
            resource: "apiVersion: batch/v1\nkind: Job".to_string(),
            weight: 0,
            on_failure: HookFailurePolicy::default(),
            timeout: default_hook_timeout(),
            cleanup: HookCleanupPolicy::default(),
        }
    }

    #[test]
    fn test_unique_hook_name() {
        let hook = test_hook("migrate-db", vec![HookPhase::PreUpgrade]);

        let name = hook.unique_name("myapp", HookPhase::PreUpgrade, 5);
        assert_eq!(name, "myapp-migrate-db-pre-upgrade-v5");
    }

    #[test]
    fn test_unique_hook_name_different_phases() {
        let hook = test_hook("backup", vec![HookPhase::PreDelete]);

        assert_eq!(
            hook.unique_name("release", HookPhase::PreDelete, 1),
            "release-backup-pre-delete-v1"
        );
        assert_eq!(
            hook.unique_name("release", HookPhase::PreInstall, 3),
            "release-backup-pre-install-v3"
        );
    }

    #[test]
    fn test_parse_hook_phases() {
        let phases = parse_hook_phases("pre-install,post-install,pre-upgrade");
        assert_eq!(
            phases,
            vec![
                HookPhase::PreInstall,
                HookPhase::PostInstall,
                HookPhase::PreUpgrade
            ]
        );
    }

    #[test]
    fn test_parse_hook_phases_with_spaces() {
        let phases = parse_hook_phases("pre-install, post-install, pre-upgrade");
        assert_eq!(phases.len(), 3);
    }

    #[test]
    fn test_parse_hook_phases_invalid() {
        let phases = parse_hook_phases("invalid-phase,also-invalid");
        assert!(phases.is_empty());
    }

    #[test]
    fn test_parse_hook_phases_all() {
        let phases = parse_hook_phases(
            "pre-install,during-install,post-install,pre-upgrade,during-upgrade,post-upgrade,pre-rollback,post-rollback,pre-delete,post-delete,test"
        );
        assert_eq!(phases.len(), 11);
    }

    #[test]
    fn test_parse_hooks_from_manifest() {
        let manifest = r#"
apiVersion: batch/v1
kind: Job
metadata:
  name: db-migration
  annotations:
    sherpack.io/hook: pre-upgrade
    sherpack.io/hook-weight: "-5"
    sherpack.io/hook-delete-policy: before-hook-creation
spec:
  template:
    spec:
      containers:
      - name: migrate
        image: myapp:migrate
"#;

        let hooks = parse_hooks_from_manifest(manifest);
        assert_eq!(hooks.len(), 1);

        let hook = &hooks[0];
        assert_eq!(hook.name, "db-migration");
        assert_eq!(hook.phases, vec![HookPhase::PreUpgrade]);
        assert_eq!(hook.weight, -5);
        assert_eq!(hook.cleanup, HookCleanupPolicy::BeforeNextRun);
    }

    #[test]
    fn test_parse_multiple_hooks() {
        let manifest = r#"
---
apiVersion: batch/v1
kind: Job
metadata:
  name: pre-hook
  annotations:
    sherpack.io/hook: pre-install
spec:
  template:
    spec:
      containers:
      - name: pre
        image: pre:latest
---
apiVersion: batch/v1
kind: Job
metadata:
  name: post-hook
  annotations:
    sherpack.io/hook: post-install
spec:
  template:
    spec:
      containers:
      - name: post
        image: post:latest
"#;

        let hooks = parse_hooks_from_manifest(manifest);
        assert_eq!(hooks.len(), 2);
        assert_eq!(hooks[0].name, "pre-hook");
        assert_eq!(hooks[1].name, "post-hook");
    }

    #[test]
    fn test_helm_compatibility() {
        let manifest = r#"
apiVersion: batch/v1
kind: Job
metadata:
  name: test-job
  annotations:
    helm.sh/hook: test-success
    helm.sh/hook-weight: "0"
spec:
  template:
    spec:
      containers:
      - name: test
        image: test:latest
"#;

        let hooks = parse_hooks_from_manifest(manifest);
        assert_eq!(hooks.len(), 1);
        assert_eq!(hooks[0].phases, vec![HookPhase::Test]);
    }

    #[test]
    fn test_hook_phase_is_pre() {
        assert!(HookPhase::PreInstall.is_pre());
        assert!(HookPhase::PreUpgrade.is_pre());
        assert!(HookPhase::PreRollback.is_pre());
        assert!(HookPhase::PreDelete.is_pre());
        assert!(!HookPhase::PostInstall.is_pre());
        assert!(!HookPhase::DuringInstall.is_pre());
    }

    #[test]
    fn test_hook_phase_is_post() {
        assert!(HookPhase::PostInstall.is_post());
        assert!(HookPhase::PostUpgrade.is_post());
        assert!(HookPhase::PostRollback.is_post());
        assert!(HookPhase::PostDelete.is_post());
        assert!(!HookPhase::PreInstall.is_post());
        assert!(!HookPhase::DuringInstall.is_post());
    }

    #[test]
    fn test_hook_phase_is_during() {
        assert!(HookPhase::DuringInstall.is_during());
        assert!(HookPhase::DuringUpgrade.is_during());
        assert!(!HookPhase::PreInstall.is_during());
        assert!(!HookPhase::PostInstall.is_during());
    }

    #[test]
    fn test_hook_phases_lists() {
        assert_eq!(HookPhase::install_phases().len(), 3);
        assert_eq!(HookPhase::upgrade_phases().len(), 3);
        assert_eq!(HookPhase::rollback_phases().len(), 2);
        assert_eq!(HookPhase::delete_phases().len(), 2);
    }

    #[test]
    fn test_hook_runs_in_phase() {
        let hook = test_hook("test", vec![HookPhase::PreInstall, HookPhase::PreUpgrade]);

        assert!(hook.runs_in_phase(HookPhase::PreInstall));
        assert!(hook.runs_in_phase(HookPhase::PreUpgrade));
        assert!(!hook.runs_in_phase(HookPhase::PostInstall));
    }

    #[test]
    fn test_hook_phase_display() {
        assert_eq!(HookPhase::PreInstall.to_string(), "pre-install");
        assert_eq!(HookPhase::DuringInstall.to_string(), "during-install");
        assert_eq!(HookPhase::PostUpgrade.to_string(), "post-upgrade");
        assert_eq!(HookPhase::Test.to_string(), "test");
    }

    #[test]
    fn test_hook_result_success() {
        let started = Utc::now();
        let result = HookResult::success("my-hook".to_string(), HookPhase::PreInstall, started);

        assert!(result.success);
        assert!(result.error.is_none());
        assert_eq!(result.attempts, 1);
        assert!(result.duration().num_milliseconds() >= 0);
    }

    #[test]
    fn test_hook_result_failure() {
        let started = Utc::now();
        let result = HookResult::failure(
            "my-hook".to_string(),
            HookPhase::PreInstall,
            "Connection refused".to_string(),
            started,
            3,
        );

        assert!(!result.success);
        assert_eq!(result.error, Some("Connection refused".to_string()));
        assert_eq!(result.attempts, 3);
    }

    #[test]
    fn test_hook_executor_new() {
        let executor = HookExecutor::new();
        assert!(executor.results.is_empty());
        assert!(!executor.has_failures());
    }

    #[test]
    fn test_hook_executor_results_for_phase() {
        let mut executor = HookExecutor::new();
        executor.results.push(HookResult::success(
            "hook1".to_string(),
            HookPhase::PreInstall,
            Utc::now(),
        ));
        executor.results.push(HookResult::success(
            "hook2".to_string(),
            HookPhase::PostInstall,
            Utc::now(),
        ));
        executor.results.push(HookResult::success(
            "hook3".to_string(),
            HookPhase::PreInstall,
            Utc::now(),
        ));

        let pre_results = executor.results_for_phase(HookPhase::PreInstall);
        assert_eq!(pre_results.len(), 2);

        let post_results = executor.results_for_phase(HookPhase::PostInstall);
        assert_eq!(post_results.len(), 1);
    }

    #[test]
    fn test_hook_executor_failures() {
        let mut executor = HookExecutor::new();
        executor.results.push(HookResult::success(
            "hook1".to_string(),
            HookPhase::PreInstall,
            Utc::now(),
        ));
        executor.results.push(HookResult::failure(
            "hook2".to_string(),
            HookPhase::PreInstall,
            "Error".to_string(),
            Utc::now(),
            1,
        ));

        assert!(executor.has_failures());
        assert_eq!(executor.failures().len(), 1);
        assert_eq!(executor.failures()[0].name, "hook2");
    }

    #[test]
    fn test_cleanup_policy_parsing() {
        assert_eq!(
            parse_cleanup_policy("before-hook-creation"),
            HookCleanupPolicy::BeforeNextRun
        );
        assert_eq!(
            parse_cleanup_policy("hook-succeeded"),
            HookCleanupPolicy::OnSuccess
        );
        assert_eq!(
            parse_cleanup_policy("hook-failed"),
            HookCleanupPolicy::Never
        );
        assert_eq!(
            parse_cleanup_policy("unknown"),
            HookCleanupPolicy::default()
        );
    }

    #[test]
    fn test_hook_failure_policy_default() {
        assert!(matches!(
            HookFailurePolicy::default(),
            HookFailurePolicy::FailOperation
        ));
    }

    #[test]
    fn test_hook_serialization() {
        let hook = Hook {
            name: "test".to_string(),
            phases: vec![HookPhase::PreInstall],
            resource: "apiVersion: v1".to_string(),
            weight: -5,
            on_failure: HookFailurePolicy::Retry {
                max_attempts: 3,
                backoff: Duration::seconds(10),
            },
            timeout: Duration::minutes(2),
            cleanup: HookCleanupPolicy::KeepLast(3),
        };

        let json = serde_json::to_string(&hook).unwrap();
        let deserialized: Hook = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.name, "test");
        assert_eq!(deserialized.weight, -5);
        assert!(matches!(
            deserialized.on_failure,
            HookFailurePolicy::Retry { max_attempts: 3, .. }
        ));
    }

    #[test]
    fn test_parse_empty_manifest() {
        let hooks = parse_hooks_from_manifest("");
        assert!(hooks.is_empty());
    }

    #[test]
    fn test_parse_manifest_without_hooks() {
        let manifest = r#"
apiVersion: v1
kind: ConfigMap
metadata:
  name: my-config
data:
  key: value
"#;
        let hooks = parse_hooks_from_manifest(manifest);
        assert!(hooks.is_empty());
    }

    #[tokio::test]
    async fn test_hook_executor_execute_phase_empty() {
        let mut executor = HookExecutor::new();
        let client = kube::Client::try_default().await.ok();

        // Skip if no cluster available
        if let Some(client) = client {
            let result = executor
                .execute_phase(&[], HookPhase::PreInstall, "test", 1, &client)
                .await;
            assert!(result.is_ok());
            assert!(executor.results.is_empty());
        }
    }
}
