//! Real-time progress reporting for Kubernetes operations
//!
//! Provides visual feedback during deployment operations, showing:
//! - Resource apply status
//! - Health check progress (ready/desired replicas)
//! - Wave execution progress
//! - Hook execution status

use std::collections::HashMap;
use std::io::{self, Write};
use std::time::{Duration, Instant};

use console::{Term, style};

/// Progress reporter for deployment operations
pub struct ProgressReporter {
    /// Terminal for output
    term: Term,
    /// Resource states
    resources: HashMap<String, ResourceState>,
    /// Current wave being processed
    current_wave: Option<i32>,
    /// Start time
    start_time: Instant,
    /// Whether colors are enabled
    #[allow(dead_code)]
    colors_enabled: bool,
    /// Whether to show verbose output
    verbose: bool,
}

/// State of a single resource
#[derive(Debug, Clone)]
pub struct ResourceState {
    pub kind: String,
    pub name: String,
    pub status: ResourceStatus,
    pub ready: Option<i32>,
    pub desired: Option<i32>,
    pub message: Option<String>,
    pub last_update: Instant,
}

/// Status of a resource
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceStatus {
    Pending,
    Applying,
    Applied,
    WaitingForReady,
    Ready,
    Failed,
    Skipped,
}

impl ResourceStatus {
    fn symbol(&self) -> &'static str {
        match self {
            ResourceStatus::Pending => "○",
            ResourceStatus::Applying => "◐",
            ResourceStatus::Applied => "◑",
            ResourceStatus::WaitingForReady => "◕",
            ResourceStatus::Ready => "●",
            ResourceStatus::Failed => "✗",
            ResourceStatus::Skipped => "⊘",
        }
    }

    fn styled_symbol(&self) -> console::StyledObject<&'static str> {
        match self {
            ResourceStatus::Pending => style(self.symbol()).dim(),
            ResourceStatus::Applying => style(self.symbol()).cyan(),
            ResourceStatus::Applied => style(self.symbol()).blue(),
            ResourceStatus::WaitingForReady => style(self.symbol()).yellow(),
            ResourceStatus::Ready => style(self.symbol()).green(),
            ResourceStatus::Failed => style(self.symbol()).red(),
            ResourceStatus::Skipped => style(self.symbol()).dim(),
        }
    }
}

impl ProgressReporter {
    /// Create a new progress reporter
    pub fn new() -> Self {
        Self {
            term: Term::stderr(),
            resources: HashMap::new(),
            current_wave: None,
            start_time: Instant::now(),
            colors_enabled: console::colors_enabled(),
            verbose: false,
        }
    }

    /// Create with verbose output
    pub fn verbose(mut self) -> Self {
        self.verbose = true;
        self
    }

    /// Add a resource to track
    pub fn add_resource(&mut self, kind: &str, name: &str) {
        let key = format!("{}/{}", kind, name);
        self.resources.insert(
            key,
            ResourceState {
                kind: kind.to_string(),
                name: name.to_string(),
                status: ResourceStatus::Pending,
                ready: None,
                desired: None,
                message: None,
                last_update: Instant::now(),
            },
        );
    }

    /// Set current wave
    pub fn set_wave(&mut self, wave: i32) {
        self.current_wave = Some(wave);
        self.print_wave_header(wave);
    }

    /// Update resource status
    pub fn update_status(&mut self, key: &str, status: ResourceStatus) {
        if let Some(resource) = self.resources.get_mut(key) {
            resource.status = status;
            resource.last_update = Instant::now();
            self.print_resource_update(key);
        }
    }

    /// Update resource readiness
    pub fn update_readiness(&mut self, key: &str, ready: i32, desired: i32, message: Option<&str>) {
        if let Some(resource) = self.resources.get_mut(key) {
            resource.ready = Some(ready);
            resource.desired = Some(desired);
            resource.message = message.map(String::from);
            resource.last_update = Instant::now();

            if ready == desired {
                resource.status = ResourceStatus::Ready;
            }

            self.print_resource_update(key);
        }
    }

    /// Mark resource as failed
    pub fn fail(&mut self, key: &str, error: &str) {
        if let Some(resource) = self.resources.get_mut(key) {
            resource.status = ResourceStatus::Failed;
            resource.message = Some(error.to_string());
            resource.last_update = Instant::now();
            self.print_resource_update(key);
        }
    }

    /// Print wave header
    fn print_wave_header(&self, wave: i32) {
        let wave_resources: Vec<_> = self
            .resources
            .values()
            .filter(|_| true) // In reality, filter by wave
            .collect();

        let _ = writeln!(
            io::stderr(),
            "\n{} Wave {} ({} resources)",
            style("▶").cyan().bold(),
            wave,
            wave_resources.len()
        );
    }

    /// Print resource update
    fn print_resource_update(&self, key: &str) {
        if let Some(resource) = self.resources.get(key) {
            let styled_symbol = resource.status.styled_symbol();

            let readiness = match (resource.ready, resource.desired) {
                (Some(r), Some(d)) => format!(" ({}/{})", r, d),
                _ => String::new(),
            };

            let message = resource
                .message
                .as_ref()
                .map(|m| format!(" - {}", style(m).dim()))
                .unwrap_or_default();

            let line = format!(
                "  {} {}/{}{}{}",
                styled_symbol, resource.kind, resource.name, readiness, message
            );

            let _ = writeln!(io::stderr(), "{}", line);
        }
    }

    /// Print hook execution start
    pub fn hook_start(&self, phase: &str, name: &str) {
        let _ = writeln!(
            io::stderr(),
            "  {} Hook [{}] {}",
            style("⟳").cyan(),
            phase,
            name
        );
    }

    /// Print hook execution result
    pub fn hook_result(&self, name: &str, success: bool, duration: Duration, error: Option<&str>) {
        let symbol = if success {
            style("✓").green()
        } else {
            style("✗").red()
        };

        let duration_str = format!("{:.1}s", duration.as_secs_f64());

        let error_msg = error
            .map(|e| format!(" - {}", style(e).red()))
            .unwrap_or_default();

        let _ = writeln!(
            io::stderr(),
            "  {} Hook {} ({}){}",
            symbol,
            name,
            duration_str,
            error_msg
        );
    }

    /// Print overall progress summary
    pub fn print_summary(&self) {
        let total = self.resources.len();
        let ready = self
            .resources
            .values()
            .filter(|r| r.status == ResourceStatus::Ready)
            .count();
        let failed = self
            .resources
            .values()
            .filter(|r| r.status == ResourceStatus::Failed)
            .count();

        let elapsed = self.start_time.elapsed();

        let _ = writeln!(io::stderr());

        if failed > 0 {
            let _ = writeln!(
                io::stderr(),
                "{} {}/{} resources ready, {} failed ({:.1}s)",
                style("✗").red().bold(),
                ready,
                total,
                failed,
                elapsed.as_secs_f64()
            );
        } else if ready == total {
            let _ = writeln!(
                io::stderr(),
                "{} All {} resources ready ({:.1}s)",
                style("✓").green().bold(),
                total,
                elapsed.as_secs_f64()
            );
        } else {
            let _ = writeln!(
                io::stderr(),
                "{} {}/{} resources ready ({:.1}s)",
                style("○").yellow(),
                ready,
                total,
                elapsed.as_secs_f64()
            );
        }
    }

    /// Print a simple message
    pub fn message(&self, msg: &str) {
        let _ = writeln!(io::stderr(), "  {}", msg);
    }

    /// Print an info message
    pub fn info(&self, msg: &str) {
        let _ = writeln!(io::stderr(), "  {} {}", style("ℹ").blue(), msg);
    }

    /// Print a warning message
    pub fn warn(&self, msg: &str) {
        let _ = writeln!(io::stderr(), "  {} {}", style("⚠").yellow(), msg);
    }

    /// Print an error message
    pub fn error(&self, msg: &str) {
        let _ = writeln!(io::stderr(), "  {} {}", style("✗").red(), msg);
    }

    /// Print success message
    pub fn success(&self, msg: &str) {
        let _ = writeln!(io::stderr(), "  {} {}", style("✓").green(), msg);
    }

    /// Get elapsed time
    pub fn elapsed(&self) -> Duration {
        self.start_time.elapsed()
    }

    /// Clear the screen (for interactive mode)
    pub fn clear(&self) {
        let _ = self.term.clear_screen();
    }

    /// Check if all resources are ready
    pub fn all_ready(&self) -> bool {
        self.resources
            .values()
            .all(|r| r.status == ResourceStatus::Ready || r.status == ResourceStatus::Skipped)
    }

    /// Check if any resource failed
    pub fn any_failed(&self) -> bool {
        self.resources
            .values()
            .any(|r| r.status == ResourceStatus::Failed)
    }

    /// Get failed resources
    pub fn failed_resources(&self) -> Vec<&ResourceState> {
        self.resources
            .values()
            .filter(|r| r.status == ResourceStatus::Failed)
            .collect()
    }
}

impl Default for ProgressReporter {
    fn default() -> Self {
        Self::new()
    }
}

/// Quiet progress reporter that only logs errors
pub struct QuietProgressReporter;

impl QuietProgressReporter {
    pub fn new() -> Self {
        Self
    }

    pub fn error(&self, msg: &str) {
        eprintln!("Error: {}", msg);
    }

    pub fn warn(&self, msg: &str) {
        eprintln!("Warning: {}", msg);
    }
}

impl Default for QuietProgressReporter {
    fn default() -> Self {
        Self::new()
    }
}

/// JSON progress reporter for CI/CD integration
pub struct JsonProgressReporter {
    resources: HashMap<String, ResourceState>,
}

impl JsonProgressReporter {
    pub fn new() -> Self {
        Self {
            resources: HashMap::new(),
        }
    }

    pub fn add_resource(&mut self, kind: &str, name: &str) {
        let key = format!("{}/{}", kind, name);
        self.resources.insert(
            key,
            ResourceState {
                kind: kind.to_string(),
                name: name.to_string(),
                status: ResourceStatus::Pending,
                ready: None,
                desired: None,
                message: None,
                last_update: Instant::now(),
            },
        );
    }

    pub fn update_status(&mut self, key: &str, status: ResourceStatus) {
        if let Some(resource) = self.resources.get_mut(key) {
            resource.status = status;
            self.emit_event(key, "status_changed");
        }
    }

    pub fn update_readiness(&mut self, key: &str, ready: i32, desired: i32) {
        if let Some(resource) = self.resources.get_mut(key) {
            resource.ready = Some(ready);
            resource.desired = Some(desired);
            if ready == desired {
                resource.status = ResourceStatus::Ready;
            }
            self.emit_event(key, "readiness_changed");
        }
    }

    fn emit_event(&self, key: &str, event_type: &str) {
        if let Some(resource) = self.resources.get(key) {
            let event = serde_json::json!({
                "type": event_type,
                "resource": {
                    "kind": resource.kind,
                    "name": resource.name,
                    "status": format!("{:?}", resource.status),
                    "ready": resource.ready,
                    "desired": resource.desired,
                    "message": resource.message,
                }
            });
            println!("{}", event);
        }
    }

    pub fn print_summary(&self) {
        let summary: Vec<_> = self
            .resources
            .values()
            .map(|r| {
                serde_json::json!({
                    "kind": r.kind,
                    "name": r.name,
                    "status": format!("{:?}", r.status),
                    "ready": r.ready,
                    "desired": r.desired,
                })
            })
            .collect();

        let output = serde_json::json!({
            "type": "summary",
            "resources": summary,
        });
        println!("{}", output);
    }
}

impl Default for JsonProgressReporter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resource_status_symbols() {
        assert_eq!(ResourceStatus::Pending.symbol(), "○");
        assert_eq!(ResourceStatus::Ready.symbol(), "●");
        assert_eq!(ResourceStatus::Failed.symbol(), "✗");
    }

    #[test]
    fn test_progress_reporter_add_resource() {
        let mut reporter = ProgressReporter::new();
        reporter.add_resource("Deployment", "my-app");

        assert!(reporter.resources.contains_key("Deployment/my-app"));
        assert_eq!(
            reporter.resources["Deployment/my-app"].status,
            ResourceStatus::Pending
        );
    }

    #[test]
    fn test_progress_reporter_update_status() {
        let mut reporter = ProgressReporter::new();
        reporter.add_resource("Deployment", "my-app");
        reporter.update_status("Deployment/my-app", ResourceStatus::Applied);

        assert_eq!(
            reporter.resources["Deployment/my-app"].status,
            ResourceStatus::Applied
        );
    }

    #[test]
    fn test_progress_reporter_update_readiness() {
        let mut reporter = ProgressReporter::new();
        reporter.add_resource("Deployment", "my-app");
        reporter.update_readiness("Deployment/my-app", 2, 3, Some("Waiting for pods"));

        let resource = &reporter.resources["Deployment/my-app"];
        assert_eq!(resource.ready, Some(2));
        assert_eq!(resource.desired, Some(3));
        assert_eq!(resource.message, Some("Waiting for pods".to_string()));
    }

    #[test]
    fn test_progress_reporter_readiness_triggers_ready() {
        let mut reporter = ProgressReporter::new();
        reporter.add_resource("Deployment", "my-app");
        reporter.update_readiness("Deployment/my-app", 3, 3, None);

        assert_eq!(
            reporter.resources["Deployment/my-app"].status,
            ResourceStatus::Ready
        );
    }

    #[test]
    fn test_progress_reporter_all_ready() {
        let mut reporter = ProgressReporter::new();
        reporter.add_resource("Deployment", "app1");
        reporter.add_resource("Deployment", "app2");

        assert!(!reporter.all_ready());

        reporter.update_status("Deployment/app1", ResourceStatus::Ready);
        assert!(!reporter.all_ready());

        reporter.update_status("Deployment/app2", ResourceStatus::Ready);
        assert!(reporter.all_ready());
    }

    #[test]
    fn test_progress_reporter_any_failed() {
        let mut reporter = ProgressReporter::new();
        reporter.add_resource("Deployment", "app1");
        reporter.add_resource("Deployment", "app2");

        assert!(!reporter.any_failed());

        reporter.fail("Deployment/app1", "ImagePullBackOff");
        assert!(reporter.any_failed());
    }
}
