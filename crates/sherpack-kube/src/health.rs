//! Health check system for validating deployments
//!
//! Key features:
//! - Check deployment/statefulset readiness with REAL Kubernetes API calls
//! - Custom HTTP health checks
//! - Command-based health checks (exec into pods)
//! - Automatic rollback on failure
//!
//! Unlike Helm's broken `--wait` flag, Sherpack properly verifies:
//! - All replicas are updated (new version)
//! - All replicas are ready (passing readiness probe)
//! - All replicas are available (not being terminated)

use chrono::{DateTime, Duration, Utc};
use k8s_openapi::api::apps::v1::{DaemonSet, Deployment, StatefulSet};
use k8s_openapi::api::batch::v1::Job;
use k8s_openapi::api::core::v1::Pod;
use kube::api::{Api, ListParams};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::error::{KubeError, Result};
use crate::release::StoredRelease;

/// Health check configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthCheckConfig {
    /// Check that Deployments are ready
    #[serde(default = "default_true")]
    pub check_deployments: bool,

    /// Check that StatefulSets are ready
    #[serde(default = "default_true")]
    pub check_statefulsets: bool,

    /// Check that DaemonSets are ready
    #[serde(default)]
    pub check_daemonsets: bool,

    /// Custom HTTP health checks
    #[serde(default)]
    pub http_checks: Vec<HttpHealthCheck>,

    /// Command-based health checks
    #[serde(default)]
    pub command_checks: Vec<CommandHealthCheck>,

    /// Timeout for all checks
    #[serde(default = "default_health_timeout")]
    #[serde(with = "duration_serde")]
    pub timeout: Duration,

    /// Interval between retry attempts
    #[serde(default = "default_health_interval")]
    #[serde(with = "duration_serde")]
    pub interval: Duration,
}

impl Default for HealthCheckConfig {
    fn default() -> Self {
        Self {
            check_deployments: true,
            check_statefulsets: true,
            check_daemonsets: false,
            http_checks: Vec::new(),
            command_checks: Vec::new(),
            timeout: default_health_timeout(),
            interval: default_health_interval(),
        }
    }
}

fn default_true() -> bool {
    true
}

fn default_health_timeout() -> Duration {
    Duration::minutes(5)
}

fn default_health_interval() -> Duration {
    Duration::seconds(5)
}

/// HTTP health check configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HttpHealthCheck {
    /// Name for this check
    pub name: String,

    /// URL to check (can use service name if in-cluster)
    pub url: String,

    /// Expected HTTP status code
    #[serde(default = "default_http_status")]
    pub expected_status: u16,

    /// Timeout for this specific check
    #[serde(default = "default_check_timeout")]
    #[serde(with = "duration_serde")]
    pub timeout: Duration,

    /// Optional headers to send
    #[serde(default)]
    pub headers: HashMap<String, String>,
}

fn default_http_status() -> u16 {
    200
}

fn default_check_timeout() -> Duration {
    Duration::seconds(30)
}

/// Command-based health check (runs in a pod)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandHealthCheck {
    /// Name for this check
    pub name: String,

    /// Pod selector (label selector)
    pub pod_selector: String,

    /// Container name (optional, uses first container if not specified)
    pub container: Option<String>,

    /// Command to execute
    pub command: Vec<String>,

    /// Expected exit code (default: 0)
    #[serde(default)]
    pub expected_exit_code: i32,

    /// Timeout for this check
    #[serde(default = "default_check_timeout")]
    #[serde(with = "duration_serde")]
    pub timeout: Duration,
}

/// Overall health status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthStatus {
    /// Whether overall health is good
    pub healthy: bool,

    /// Individual resource health
    pub resources: Vec<ResourceHealth>,

    /// HTTP check results
    pub http_checks: Vec<CheckResult>,

    /// Command check results
    pub command_checks: Vec<CheckResult>,

    /// When the check was performed
    pub checked_at: DateTime<Utc>,

    /// How long the check took
    #[serde(with = "duration_serde")]
    pub duration: Duration,
}

impl HealthStatus {
    /// Get all unhealthy resources
    pub fn unhealthy_resources(&self) -> Vec<&ResourceHealth> {
        self.resources.iter().filter(|r| !r.healthy).collect()
    }

    /// Get all failed checks
    pub fn failed_checks(&self) -> Vec<&CheckResult> {
        self.http_checks
            .iter()
            .chain(self.command_checks.iter())
            .filter(|c| !c.success)
            .collect()
    }

    /// Generate a human-readable summary
    pub fn summary(&self) -> String {
        if self.healthy {
            format!("Healthy: {} resources ready", self.resources.len())
        } else {
            let unhealthy = self.unhealthy_resources();
            let failed = self.failed_checks();

            let mut parts = Vec::new();
            if !unhealthy.is_empty() {
                parts.push(format!("{} resources not ready", unhealthy.len()));
            }
            if !failed.is_empty() {
                parts.push(format!("{} checks failed", failed.len()));
            }

            format!("Unhealthy: {}", parts.join(", "))
        }
    }
}

/// Health of a single Kubernetes resource
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceHealth {
    /// Resource kind
    pub kind: String,

    /// Resource name
    pub name: String,

    /// Resource namespace
    pub namespace: String,

    /// Whether the resource is healthy
    pub healthy: bool,

    /// Ready replicas (for Deployments, etc.)
    pub ready: Option<i32>,

    /// Desired replicas
    pub desired: Option<i32>,

    /// Additional status message
    pub message: Option<String>,
}

impl ResourceHealth {
    /// Get a display string for readiness
    pub fn readiness_display(&self) -> String {
        match (self.ready, self.desired) {
            (Some(r), Some(d)) => format!("{}/{}", r, d),
            _ => if self.healthy { "Ready" } else { "Not Ready" }.to_string(),
        }
    }
}

/// Result of an individual health check
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckResult {
    /// Check name
    pub name: String,

    /// Whether the check passed
    pub success: bool,

    /// Error message if failed
    pub error: Option<String>,

    /// Response time
    #[serde(with = "duration_serde")]
    pub response_time: Duration,
}

/// Health checker implementation
pub struct HealthChecker {
    config: HealthCheckConfig,
}

impl HealthChecker {
    /// Create a new health checker
    pub fn new(config: HealthCheckConfig) -> Self {
        Self { config }
    }

    /// Check health of a release
    pub async fn check(
        &self,
        release: &StoredRelease,
        client: &kube::Client,
    ) -> Result<HealthStatus> {
        let start = Utc::now();
        let deadline = start + self.config.timeout;

        let mut resources = Vec::new();
        let mut http_checks = Vec::new();
        let mut command_checks = Vec::new();

        // Parse manifest to find resources to check
        let resource_refs = self.parse_resources(&release.manifest);

        // Check resources with retry
        loop {
            resources.clear();

            for (kind, name) in &resource_refs {
                let health = self
                    .check_resource(client, &release.namespace, kind, name)
                    .await?;
                resources.push(health);
            }

            // Check HTTP endpoints
            for http_check in &self.config.http_checks {
                let result = self.check_http(http_check).await;
                http_checks.push(result);
            }

            // Check commands
            for cmd_check in &self.config.command_checks {
                let result = self
                    .check_command(client, &release.namespace, cmd_check)
                    .await;
                command_checks.push(result);
            }

            // Check if all healthy
            let all_resources_healthy = resources.iter().all(|r| r.healthy);
            let all_http_healthy = http_checks.iter().all(|c| c.success);
            let all_cmd_healthy = command_checks.iter().all(|c| c.success);

            if all_resources_healthy && all_http_healthy && all_cmd_healthy {
                return Ok(HealthStatus {
                    healthy: true,
                    resources,
                    http_checks,
                    command_checks,
                    checked_at: Utc::now(),
                    duration: Utc::now().signed_duration_since(start),
                });
            }

            // Check timeout
            if Utc::now() >= deadline {
                return Ok(HealthStatus {
                    healthy: false,
                    resources,
                    http_checks,
                    command_checks,
                    checked_at: Utc::now(),
                    duration: Utc::now().signed_duration_since(start),
                });
            }

            // Wait before retry
            tokio::time::sleep(self.config.interval.to_std().unwrap_or_default()).await;
            http_checks.clear();
            command_checks.clear();
        }
    }

    /// Quick health check (no retries)
    pub async fn check_once(
        &self,
        release: &StoredRelease,
        client: &kube::Client,
    ) -> Result<HealthStatus> {
        let start = Utc::now();

        let mut resources = Vec::new();
        let resource_refs = self.parse_resources(&release.manifest);

        for (kind, name) in &resource_refs {
            let health = self
                .check_resource(client, &release.namespace, kind, name)
                .await?;
            resources.push(health);
        }

        let mut http_checks = Vec::new();
        for http_check in &self.config.http_checks {
            let result = self.check_http(http_check).await;
            http_checks.push(result);
        }

        let mut command_checks = Vec::new();
        for cmd_check in &self.config.command_checks {
            let result = self
                .check_command(client, &release.namespace, cmd_check)
                .await;
            command_checks.push(result);
        }

        let healthy = resources.iter().all(|r| r.healthy)
            && http_checks.iter().all(|c| c.success)
            && command_checks.iter().all(|c| c.success);

        Ok(HealthStatus {
            healthy,
            resources,
            http_checks,
            command_checks,
            checked_at: Utc::now(),
            duration: Utc::now().signed_duration_since(start),
        })
    }

    /// Parse manifest to find checkable resources
    fn parse_resources(&self, manifest: &str) -> Vec<(String, String)> {
        let mut resources = Vec::new();

        for doc in manifest.split("---") {
            let doc = doc.trim();
            if doc.is_empty() {
                continue;
            }

            let yaml: serde_yaml::Value = match serde_yaml::from_str(doc) {
                Ok(v) => v,
                Err(_) => continue,
            };

            let kind = yaml.get("kind").and_then(|v| v.as_str()).unwrap_or("");
            let name = yaml
                .get("metadata")
                .and_then(|m| m.get("name"))
                .and_then(|n| n.as_str())
                .unwrap_or("");

            // Only check supported resource types
            let should_check = match kind {
                "Deployment" => self.config.check_deployments,
                "StatefulSet" => self.config.check_statefulsets,
                "DaemonSet" => self.config.check_daemonsets,
                _ => false,
            };

            if should_check && !name.is_empty() {
                resources.push((kind.to_string(), name.to_string()));
            }
        }

        resources
    }

    /// Check a single Kubernetes resource
    async fn check_resource(
        &self,
        client: &kube::Client,
        namespace: &str,
        kind: &str,
        name: &str,
    ) -> Result<ResourceHealth> {
        match kind {
            "Deployment" => self.check_deployment(client, namespace, name).await,
            "StatefulSet" => self.check_statefulset(client, namespace, name).await,
            "DaemonSet" => self.check_daemonset(client, namespace, name).await,
            "Job" => self.check_job(client, namespace, name).await,
            _ => {
                // Unknown resource types are considered ready
                Ok(ResourceHealth {
                    kind: kind.to_string(),
                    name: name.to_string(),
                    namespace: namespace.to_string(),
                    healthy: true,
                    ready: None,
                    desired: None,
                    message: Some("Unknown resource type, skipping check".to_string()),
                })
            }
        }
    }

    /// Check Deployment readiness
    ///
    /// A Deployment is healthy when:
    /// - All replicas are updated (running new version)
    /// - All replicas are ready (passing readiness probe)
    /// - All replicas are available (not being terminated)
    async fn check_deployment(
        &self,
        client: &kube::Client,
        namespace: &str,
        name: &str,
    ) -> Result<ResourceHealth> {
        let api: Api<Deployment> = Api::namespaced(client.clone(), namespace);

        let deployment = match api.get(name).await {
            Ok(d) => d,
            Err(kube::Error::Api(ae)) if ae.code == 404 => {
                return Ok(ResourceHealth {
                    kind: "Deployment".to_string(),
                    name: name.to_string(),
                    namespace: namespace.to_string(),
                    healthy: false,
                    ready: Some(0),
                    desired: Some(0),
                    message: Some("Deployment not found".to_string()),
                });
            }
            Err(e) => return Err(KubeError::KubeApi(e)),
        };

        let spec = deployment.spec.as_ref();
        let status = deployment.status.as_ref();

        let desired = spec.and_then(|s| s.replicas).unwrap_or(1);
        let ready = status.and_then(|s| s.ready_replicas).unwrap_or(0);
        let updated = status.and_then(|s| s.updated_replicas).unwrap_or(0);
        let available = status.and_then(|s| s.available_replicas).unwrap_or(0);

        // CORRECT readiness check (unlike Helm):
        // - All replicas must be updated (new version)
        // - All replicas must be ready (passing readiness probe)
        // - All replicas must be available (not being terminated)
        let healthy = ready == desired && updated == desired && available == desired;

        let message = if !healthy {
            // Extract condition messages for better debugging
            let conditions = status
                .and_then(|s| s.conditions.as_ref())
                .map(|c| {
                    c.iter()
                        .filter(|cond| cond.status == "False")
                        .filter_map(|cond| {
                            cond.message
                                .as_ref()
                                .map(|m| format!("{}: {}", cond.type_, m))
                        })
                        .collect::<Vec<_>>()
                        .join("; ")
                })
                .filter(|s| !s.is_empty());

            Some(conditions.unwrap_or_else(|| {
                format!(
                    "Waiting: {}/{} ready, {}/{} updated, {}/{} available",
                    ready, desired, updated, desired, available, desired
                )
            }))
        } else {
            None
        };

        Ok(ResourceHealth {
            kind: "Deployment".to_string(),
            name: name.to_string(),
            namespace: namespace.to_string(),
            healthy,
            ready: Some(ready),
            desired: Some(desired),
            message,
        })
    }

    /// Check StatefulSet readiness
    ///
    /// A StatefulSet is healthy when:
    /// - All replicas are ready
    /// - Current revision matches update revision (rollout complete)
    async fn check_statefulset(
        &self,
        client: &kube::Client,
        namespace: &str,
        name: &str,
    ) -> Result<ResourceHealth> {
        let api: Api<StatefulSet> = Api::namespaced(client.clone(), namespace);

        let sts = match api.get(name).await {
            Ok(s) => s,
            Err(kube::Error::Api(ae)) if ae.code == 404 => {
                return Ok(ResourceHealth {
                    kind: "StatefulSet".to_string(),
                    name: name.to_string(),
                    namespace: namespace.to_string(),
                    healthy: false,
                    ready: Some(0),
                    desired: Some(0),
                    message: Some("StatefulSet not found".to_string()),
                });
            }
            Err(e) => return Err(KubeError::KubeApi(e)),
        };

        let spec = sts.spec.as_ref();
        let status = sts.status.as_ref();

        let desired = spec.and_then(|s| s.replicas).unwrap_or(1);
        let ready = status.and_then(|s| s.ready_replicas).unwrap_or(0);
        let current = status.and_then(|s| s.current_replicas).unwrap_or(0);

        // StatefulSets: also check currentRevision == updateRevision
        let current_rev = status.and_then(|s| s.current_revision.as_ref());
        let update_rev = status.and_then(|s| s.update_revision.as_ref());
        let revision_match = current_rev == update_rev;

        let healthy = ready == desired && current == desired && revision_match;

        let message = if !healthy {
            Some(format!(
                "Waiting: {}/{} ready, {}/{} current, revision match: {}",
                ready, desired, current, desired, revision_match
            ))
        } else {
            None
        };

        Ok(ResourceHealth {
            kind: "StatefulSet".to_string(),
            name: name.to_string(),
            namespace: namespace.to_string(),
            healthy,
            ready: Some(ready),
            desired: Some(desired),
            message,
        })
    }

    /// Check DaemonSet readiness
    ///
    /// A DaemonSet is healthy when:
    /// - Number of ready pods equals desired number scheduled
    /// - All pods are updated to current version
    async fn check_daemonset(
        &self,
        client: &kube::Client,
        namespace: &str,
        name: &str,
    ) -> Result<ResourceHealth> {
        let api: Api<DaemonSet> = Api::namespaced(client.clone(), namespace);

        let ds = match api.get(name).await {
            Ok(d) => d,
            Err(kube::Error::Api(ae)) if ae.code == 404 => {
                return Ok(ResourceHealth {
                    kind: "DaemonSet".to_string(),
                    name: name.to_string(),
                    namespace: namespace.to_string(),
                    healthy: false,
                    ready: Some(0),
                    desired: Some(0),
                    message: Some("DaemonSet not found".to_string()),
                });
            }
            Err(e) => return Err(KubeError::KubeApi(e)),
        };

        let status = ds.status.as_ref();

        let desired = status.map(|s| s.desired_number_scheduled).unwrap_or(0);
        let ready = status.map(|s| s.number_ready).unwrap_or(0);
        let updated = status.and_then(|s| s.updated_number_scheduled).unwrap_or(0);

        let healthy = ready == desired && updated == desired && desired > 0;

        let message = if !healthy {
            Some(format!(
                "Waiting: {}/{} ready, {}/{} updated",
                ready, desired, updated, desired
            ))
        } else {
            None
        };

        Ok(ResourceHealth {
            kind: "DaemonSet".to_string(),
            name: name.to_string(),
            namespace: namespace.to_string(),
            healthy,
            ready: Some(ready),
            desired: Some(desired),
            message,
        })
    }

    /// Check Job completion
    ///
    /// A Job is healthy when it has completed successfully.
    /// A Job is unhealthy if it has failed permanently.
    async fn check_job(
        &self,
        client: &kube::Client,
        namespace: &str,
        name: &str,
    ) -> Result<ResourceHealth> {
        let api: Api<Job> = Api::namespaced(client.clone(), namespace);

        let job = match api.get(name).await {
            Ok(j) => j,
            Err(kube::Error::Api(ae)) if ae.code == 404 => {
                return Ok(ResourceHealth {
                    kind: "Job".to_string(),
                    name: name.to_string(),
                    namespace: namespace.to_string(),
                    healthy: false,
                    ready: Some(0),
                    desired: Some(1),
                    message: Some("Job not found".to_string()),
                });
            }
            Err(e) => return Err(KubeError::KubeApi(e)),
        };

        let status = job.status.as_ref();
        let succeeded = status.and_then(|s| s.succeeded).unwrap_or(0);
        let failed = status.and_then(|s| s.failed).unwrap_or(0);
        let active = status.and_then(|s| s.active).unwrap_or(0);

        // Check job conditions for failure
        let has_failed_condition = status
            .and_then(|s| s.conditions.as_ref())
            .map(|c| {
                c.iter()
                    .any(|cond| cond.type_ == "Failed" && cond.status == "True")
            })
            .unwrap_or(false);

        let healthy = succeeded > 0;
        let failed_permanently = has_failed_condition || (failed > 0 && active == 0);

        let message = if failed_permanently {
            // Get failure reason
            let reason = status
                .and_then(|s| s.conditions.as_ref())
                .and_then(|c| {
                    c.iter()
                        .find(|cond| cond.type_ == "Failed")
                        .and_then(|cond| cond.message.clone())
                })
                .unwrap_or_else(|| format!("Job failed with {} failures", failed));
            Some(reason)
        } else if !healthy {
            Some(format!(
                "Running: {} active, {} succeeded",
                active, succeeded
            ))
        } else {
            None
        };

        Ok(ResourceHealth {
            kind: "Job".to_string(),
            name: name.to_string(),
            namespace: namespace.to_string(),
            healthy,
            ready: Some(succeeded),
            desired: Some(1),
            message,
        })
    }

    /// Execute an HTTP health check
    async fn check_http(&self, check: &HttpHealthCheck) -> CheckResult {
        let start = std::time::Instant::now();

        // Build request with timeout
        let client = match reqwest::Client::builder()
            .timeout(
                check
                    .timeout
                    .to_std()
                    .unwrap_or(std::time::Duration::from_secs(30)),
            )
            .build()
        {
            Ok(c) => c,
            Err(e) => {
                return CheckResult {
                    name: check.name.clone(),
                    success: false,
                    error: Some(format!("Failed to create HTTP client: {}", e)),
                    response_time: Duration::milliseconds(start.elapsed().as_millis() as i64),
                };
            }
        };

        let mut request = client.get(&check.url);

        // Add custom headers
        for (key, value) in &check.headers {
            request = request.header(key.as_str(), value.as_str());
        }

        match request.send().await {
            Ok(response) => {
                let status = response.status().as_u16();
                let success = status == check.expected_status;

                CheckResult {
                    name: check.name.clone(),
                    success,
                    error: if success {
                        None
                    } else {
                        Some(format!(
                            "Expected status {}, got {}",
                            check.expected_status, status
                        ))
                    },
                    response_time: Duration::milliseconds(start.elapsed().as_millis() as i64),
                }
            }
            Err(e) => CheckResult {
                name: check.name.clone(),
                success: false,
                error: Some(format!("HTTP request failed: {}", e)),
                response_time: Duration::milliseconds(start.elapsed().as_millis() as i64),
            },
        }
    }

    /// Execute a command health check by exec'ing into a pod
    async fn check_command(
        &self,
        client: &kube::Client,
        namespace: &str,
        check: &CommandHealthCheck,
    ) -> CheckResult {
        let start = std::time::Instant::now();

        // Find pods matching the selector
        let pods: Api<Pod> = Api::namespaced(client.clone(), namespace);
        let lp = ListParams::default().labels(&check.pod_selector);

        let pod_list = match pods.list(&lp).await {
            Ok(list) => list,
            Err(e) => {
                return CheckResult {
                    name: check.name.clone(),
                    success: false,
                    error: Some(format!("Failed to list pods: {}", e)),
                    response_time: Duration::milliseconds(start.elapsed().as_millis() as i64),
                };
            }
        };

        // Find a running pod
        let running_pod = pod_list.items.iter().find(|p| {
            p.status
                .as_ref()
                .and_then(|s| s.phase.as_ref())
                .map(|phase| phase == "Running")
                .unwrap_or(false)
        });

        let pod = match running_pod {
            Some(p) => p,
            None => {
                return CheckResult {
                    name: check.name.clone(),
                    success: false,
                    error: Some(format!(
                        "No running pods found matching selector: {}",
                        check.pod_selector
                    )),
                    response_time: Duration::milliseconds(start.elapsed().as_millis() as i64),
                };
            }
        };

        let pod_name = pod.metadata.name.as_deref().unwrap_or("unknown");

        // Get container name (first container if not specified)
        let container = check.container.clone().or_else(|| {
            pod.spec
                .as_ref()
                .and_then(|s| s.containers.first())
                .map(|c| c.name.clone())
        });

        // Note: Actual exec requires websocket support which is complex
        // For now, we'll use kubectl exec as a fallback
        // In production, this should use kube-rs's attach/exec API
        let container_arg = container.map(|c| format!("-c {}", c)).unwrap_or_default();
        let cmd = check.command.join(" ");

        let output = tokio::process::Command::new("kubectl")
            .args([
                "exec",
                "-n",
                namespace,
                pod_name,
                &container_arg,
                "--",
                "sh",
                "-c",
                &cmd,
            ])
            .output()
            .await;

        match output {
            Ok(out) => {
                let exit_code = out.status.code().unwrap_or(-1);
                let success = exit_code == check.expected_exit_code;

                CheckResult {
                    name: check.name.clone(),
                    success,
                    error: if success {
                        None
                    } else {
                        let stderr = String::from_utf8_lossy(&out.stderr);
                        Some(format!(
                            "Command exited with code {} (expected {}): {}",
                            exit_code, check.expected_exit_code, stderr
                        ))
                    },
                    response_time: Duration::milliseconds(start.elapsed().as_millis() as i64),
                }
            }
            Err(e) => CheckResult {
                name: check.name.clone(),
                success: false,
                error: Some(format!("Failed to exec command: {}", e)),
                response_time: Duration::milliseconds(start.elapsed().as_millis() as i64),
            },
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

    fn test_resource_health(kind: &str, name: &str, healthy: bool) -> ResourceHealth {
        ResourceHealth {
            kind: kind.to_string(),
            name: name.to_string(),
            namespace: "default".to_string(),
            healthy,
            ready: if healthy { Some(3) } else { Some(0) },
            desired: Some(3),
            message: if healthy {
                None
            } else {
                Some("Not ready".to_string())
            },
        }
    }

    #[test]
    fn test_health_status_summary_healthy() {
        let status = HealthStatus {
            healthy: true,
            resources: vec![ResourceHealth {
                kind: "Deployment".to_string(),
                name: "app".to_string(),
                namespace: "default".to_string(),
                healthy: true,
                ready: Some(3),
                desired: Some(3),
                message: None,
            }],
            http_checks: vec![],
            command_checks: vec![],
            checked_at: Utc::now(),
            duration: Duration::seconds(1),
        };

        let summary = status.summary();
        assert!(summary.contains("Healthy"));
    }

    #[test]
    fn test_health_status_summary_unhealthy() {
        let status = HealthStatus {
            healthy: false,
            resources: vec![ResourceHealth {
                kind: "Deployment".to_string(),
                name: "app".to_string(),
                namespace: "default".to_string(),
                healthy: false,
                ready: Some(0),
                desired: Some(3),
                message: Some("Waiting for pods".to_string()),
            }],
            http_checks: vec![],
            command_checks: vec![],
            checked_at: Utc::now(),
            duration: Duration::seconds(300),
        };

        let summary = status.summary();
        assert!(summary.contains("Unhealthy"));
        assert!(summary.contains("1 resources not ready"));
    }

    #[test]
    fn test_parse_resources() {
        let config = HealthCheckConfig::default();
        let checker = HealthChecker::new(config);

        let manifest = r#"
apiVersion: apps/v1
kind: Deployment
metadata:
  name: my-app
spec:
  replicas: 3
---
apiVersion: v1
kind: Service
metadata:
  name: my-service
"#;

        let resources = checker.parse_resources(manifest);
        assert_eq!(resources.len(), 1); // Only Deployment, not Service
        assert_eq!(
            resources[0],
            ("Deployment".to_string(), "my-app".to_string())
        );
    }

    #[test]
    fn test_health_check_config_default() {
        let config = HealthCheckConfig::default();

        assert!(config.check_deployments);
        assert!(config.check_statefulsets);
        assert!(!config.check_daemonsets);
        assert!(config.http_checks.is_empty());
        assert!(config.command_checks.is_empty());
        assert_eq!(config.timeout.num_seconds(), 300);
        // Interval defaults to 5 seconds based on actual implementation
        assert_eq!(config.interval.num_seconds(), 5);
    }

    #[test]
    fn test_health_checker_new() {
        let config = HealthCheckConfig::default();
        let checker = HealthChecker::new(config.clone());

        assert_eq!(
            checker.config.timeout.num_seconds(),
            config.timeout.num_seconds()
        );
    }

    #[test]
    fn test_parse_resources_statefulset() {
        let config = HealthCheckConfig {
            check_statefulsets: true,
            ..Default::default()
        };
        let checker = HealthChecker::new(config);

        let manifest = r#"
apiVersion: apps/v1
kind: StatefulSet
metadata:
  name: my-db
spec:
  replicas: 1
"#;

        let resources = checker.parse_resources(manifest);
        assert_eq!(resources.len(), 1);
        assert_eq!(
            resources[0],
            ("StatefulSet".to_string(), "my-db".to_string())
        );
    }

    #[test]
    fn test_parse_resources_daemonset() {
        let config = HealthCheckConfig {
            check_daemonsets: true,
            ..Default::default()
        };
        let checker = HealthChecker::new(config);

        let manifest = r#"
apiVersion: apps/v1
kind: DaemonSet
metadata:
  name: my-agent
spec:
  selector:
    matchLabels:
      app: agent
"#;

        let resources = checker.parse_resources(manifest);
        assert_eq!(resources.len(), 1);
        assert_eq!(
            resources[0],
            ("DaemonSet".to_string(), "my-agent".to_string())
        );
    }

    #[test]
    fn test_parse_resources_multiple() {
        let config = HealthCheckConfig::default();
        let checker = HealthChecker::new(config);

        let manifest = r#"
---
apiVersion: apps/v1
kind: Deployment
metadata:
  name: frontend
---
apiVersion: apps/v1
kind: Deployment
metadata:
  name: backend
---
apiVersion: v1
kind: ConfigMap
metadata:
  name: config
"#;

        let resources = checker.parse_resources(manifest);
        assert_eq!(resources.len(), 2); // Only Deployments
    }

    #[test]
    fn test_parse_resources_disabled() {
        let config = HealthCheckConfig {
            check_deployments: false,
            check_statefulsets: false,
            check_daemonsets: false,
            ..Default::default()
        };
        let checker = HealthChecker::new(config);

        let manifest = r#"
apiVersion: apps/v1
kind: Deployment
metadata:
  name: my-app
"#;

        let resources = checker.parse_resources(manifest);
        assert!(resources.is_empty());
    }

    #[test]
    fn test_parse_resources_empty_manifest() {
        let config = HealthCheckConfig::default();
        let checker = HealthChecker::new(config);

        let resources = checker.parse_resources("");
        assert!(resources.is_empty());
    }

    #[test]
    fn test_parse_resources_invalid_yaml() {
        let config = HealthCheckConfig::default();
        let checker = HealthChecker::new(config);

        let resources = checker.parse_resources("this is not valid yaml: {{{");
        assert!(resources.is_empty());
    }

    #[test]
    fn test_health_status_all_healthy() {
        let status = HealthStatus {
            healthy: true,
            resources: vec![
                test_resource_health("Deployment", "app1", true),
                test_resource_health("Deployment", "app2", true),
            ],
            http_checks: vec![CheckResult {
                name: "http-check".to_string(),
                success: true,
                error: None,
                response_time: Duration::milliseconds(50),
            }],
            command_checks: vec![],
            checked_at: Utc::now(),
            duration: Duration::seconds(1),
        };

        let summary = status.summary();
        assert!(summary.contains("Healthy"));
    }

    #[test]
    fn test_health_status_http_check_failed() {
        let status = HealthStatus {
            healthy: false,
            resources: vec![test_resource_health("Deployment", "app", true)],
            http_checks: vec![CheckResult {
                name: "http-check".to_string(),
                success: false,
                error: Some("Connection refused".to_string()),
                response_time: Duration::milliseconds(5000),
            }],
            command_checks: vec![],
            checked_at: Utc::now(),
            duration: Duration::seconds(5),
        };

        let summary = status.summary();
        assert!(summary.contains("Unhealthy") || summary.contains("not ready"));
    }

    #[test]
    fn test_http_health_check_struct() {
        let check = HttpHealthCheck {
            name: "api-health".to_string(),
            url: "http://localhost:8080/health".to_string(),
            expected_status: 200,
            timeout: Duration::seconds(5),
            headers: Default::default(),
        };

        assert_eq!(check.name, "api-health");
        assert_eq!(check.expected_status, 200);
    }

    #[test]
    fn test_command_health_check_struct() {
        let check = CommandHealthCheck {
            name: "db-ping".to_string(),
            pod_selector: "app=db".to_string(),
            container: Some("postgres".to_string()),
            command: vec!["pg_isready".to_string()],
            expected_exit_code: 0,
            timeout: Duration::seconds(10),
        };

        assert_eq!(check.pod_selector, "app=db");
        assert_eq!(check.container, Some("postgres".to_string()));
    }

    #[test]
    fn test_resource_health_display() {
        let health = ResourceHealth {
            kind: "Deployment".to_string(),
            name: "my-app".to_string(),
            namespace: "production".to_string(),
            healthy: true,
            ready: Some(3),
            desired: Some(3),
            message: None,
        };

        assert_eq!(health.kind, "Deployment");
        assert_eq!(health.namespace, "production");
        assert!(health.healthy);
    }

    #[test]
    fn test_check_result_success() {
        let result = CheckResult {
            name: "test-check".to_string(),
            success: true,
            error: None,
            response_time: Duration::milliseconds(100),
        };

        assert!(result.success);
        assert!(result.error.is_none());
    }

    #[test]
    fn test_check_result_failure() {
        let result = CheckResult {
            name: "test-check".to_string(),
            success: false,
            error: Some("Timeout".to_string()),
            response_time: Duration::seconds(30),
        };

        assert!(!result.success);
        assert_eq!(result.error, Some("Timeout".to_string()));
    }

    #[test]
    fn test_health_config_with_http_checks() {
        let config = HealthCheckConfig {
            http_checks: vec![
                HttpHealthCheck {
                    name: "check1".to_string(),
                    url: "http://localhost/health".to_string(),
                    expected_status: 200,
                    timeout: Duration::seconds(5),
                    headers: Default::default(),
                },
                HttpHealthCheck {
                    name: "check2".to_string(),
                    url: "http://localhost/ready".to_string(),
                    expected_status: 200,
                    timeout: Duration::seconds(5),
                    headers: Default::default(),
                },
            ],
            ..Default::default()
        };

        assert_eq!(config.http_checks.len(), 2);
    }

    #[test]
    fn test_health_status_serialization() {
        let status = HealthStatus {
            healthy: true,
            resources: vec![test_resource_health("Deployment", "app", true)],
            http_checks: vec![],
            command_checks: vec![],
            checked_at: Utc::now(),
            duration: Duration::seconds(1),
        };

        let json = serde_json::to_string(&status).unwrap();
        let deserialized: HealthStatus = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.healthy, status.healthy);
        assert_eq!(deserialized.resources.len(), 1);
    }

    #[test]
    fn test_unhealthy_count() {
        let status = HealthStatus {
            healthy: false,
            resources: vec![
                test_resource_health("Deployment", "app1", true),
                test_resource_health("Deployment", "app2", false),
                test_resource_health("StatefulSet", "db", false),
            ],
            http_checks: vec![],
            command_checks: vec![],
            checked_at: Utc::now(),
            duration: Duration::seconds(1),
        };

        let unhealthy = status.resources.iter().filter(|r| !r.healthy).count();
        assert_eq!(unhealthy, 2);
    }
}
