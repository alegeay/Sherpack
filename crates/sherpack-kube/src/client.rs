//! High-level Kubernetes client for Sherpack operations
//!
//! This module provides a unified interface for all Sherpack Kubernetes operations,
//! combining storage, rendering, hooks, health checks, and resource management.

use sherpack_core::{LoadedPack, ReleaseInfo, TemplateContext, Values};
use sherpack_engine::Engine;

use crate::actions::{InstallOptions, RollbackOptions, UninstallOptions, UpgradeOptions};
use crate::diff::{DiffEngine, DiffResult};
use crate::error::{KubeError, Result};
use crate::health::{HealthCheckConfig, HealthChecker, HealthStatus};
use crate::hooks::{HookExecutor, HookPhase, parse_hooks_from_manifest};
use crate::release::{ReleaseState, StoredRelease};
use crate::resources::ResourceManager;
use crate::storage::StorageDriver;

/// High-level Kubernetes client for Sherpack
pub struct KubeClient<S: StorageDriver> {
    /// Kubernetes client
    client: kube::Client,

    /// Storage driver
    storage: S,

    /// Template engine
    engine: Engine,

    /// Diff engine
    diff_engine: DiffEngine,
}

impl<S: StorageDriver> KubeClient<S> {
    /// Create a new KubeClient with the given storage driver
    pub async fn new(storage: S) -> Result<Self> {
        let client = kube::Client::try_default().await?;
        let engine = Engine::builder().strict(true).build();
        let diff_engine = DiffEngine::new();

        Ok(Self {
            client,
            storage,
            engine,
            diff_engine,
        })
    }

    /// Create with an existing Kubernetes client
    pub fn with_client(client: kube::Client, storage: S) -> Self {
        let engine = Engine::builder().strict(true).build();
        let diff_engine = DiffEngine::new();

        Self {
            client,
            storage,
            engine,
            diff_engine,
        }
    }

    /// Get the underlying Kubernetes client
    pub fn kube_client(&self) -> &kube::Client {
        &self.client
    }

    /// Get the storage driver
    pub fn storage(&self) -> &S {
        &self.storage
    }

    // ========== Install ==========

    /// Install a pack as a new release
    pub async fn install(
        &self,
        pack: &LoadedPack,
        values: Values,
        options: &InstallOptions,
    ) -> Result<StoredRelease> {
        // Check if release already exists
        if self
            .storage
            .exists(&options.namespace, &options.name)
            .await?
        {
            return Err(KubeError::ReleaseAlreadyExists {
                name: options.name.clone(),
                namespace: options.namespace.clone(),
            });
        }

        // Create template context
        let release_info = ReleaseInfo::for_install(&options.name, &options.namespace);
        let context = TemplateContext::new(values.clone(), release_info, &pack.pack.metadata);

        // Render templates
        let render_result = self
            .engine
            .render_pack(pack, &context)
            .map_err(|e| KubeError::Template(e.to_string()))?;

        // Create release
        let mut release = StoredRelease::for_install(
            options.name.clone(),
            options.namespace.clone(),
            pack.pack.metadata.clone(),
            values,
            render_result
                .manifests
                .values()
                .cloned()
                .collect::<Vec<_>>()
                .join("\n---\n"),
        );
        release.notes = render_result.notes;
        release.hooks = parse_hooks_from_manifest(&release.manifest);

        // Add custom labels
        for (k, v) in &options.labels {
            release.labels.insert(k.clone(), v.clone());
        }

        // Dry run - just return what would be created
        if options.dry_run {
            return Ok(release);
        }

        // Show diff if requested
        if options.show_diff {
            // For install, show all resources as additions
            println!("Resources to be created:");
            for manifest in render_result.manifests.keys() {
                println!("  + {}", manifest);
            }
        }

        // Store the pending release
        self.storage.create(&release).await?;

        // Execute pre-install hooks
        let mut hook_executor = HookExecutor::new();
        if let Err(e) = hook_executor
            .execute_phase(
                &release.hooks,
                HookPhase::PreInstall,
                &release.name,
                release.version,
                &self.client,
            )
            .await
        {
            release.mark_failed(e.to_string(), true);
            self.storage.update(&release).await?;
            return Err(e);
        }

        // Apply manifests to cluster
        if let Err(e) = self
            .apply_manifest(&release.namespace, &release.manifest)
            .await
        {
            release.mark_failed(e.to_string(), true);
            self.storage.update(&release).await?;

            if options.atomic {
                // Cleanup on failure
                let _ = self.cleanup_release(&release).await;
            }

            return Err(e);
        }

        // Execute during-install hooks
        let _ = hook_executor
            .execute_phase(
                &release.hooks,
                HookPhase::DuringInstall,
                &release.name,
                release.version,
                &self.client,
            )
            .await;

        // Wait for resources if requested
        if options.wait {
            // TODO: Use timeout to configure health checker
            let _timeout = options.timeout.unwrap_or(chrono::Duration::minutes(5));
            let health_config = options.health_check.clone().unwrap_or_default();
            let checker = HealthChecker::new(health_config);

            let status = checker.check(&release, &self.client).await?;
            if !status.healthy {
                let err_msg = status.summary();
                release.mark_failed(err_msg.clone(), true);
                self.storage.update(&release).await?;

                if options.atomic {
                    let _ = self.cleanup_release(&release).await;
                }

                return Err(KubeError::HealthCheckFailed {
                    name: release.name.clone(),
                    message: err_msg,
                });
            }
        }

        // Execute post-install hooks
        let _ = hook_executor
            .execute_phase(
                &release.hooks,
                HookPhase::PostInstall,
                &release.name,
                release.version,
                &self.client,
            )
            .await;

        // Mark as deployed
        release.mark_deployed();
        self.storage.update(&release).await?;

        Ok(release)
    }

    // ========== Upgrade ==========

    /// Upgrade an existing release
    pub async fn upgrade(
        &self,
        pack: &LoadedPack,
        values: Values,
        options: &UpgradeOptions,
    ) -> Result<StoredRelease> {
        // Get existing release
        let existing = match self
            .storage
            .get_latest(&options.namespace, &options.name)
            .await
        {
            Ok(r) => Some(r),
            Err(KubeError::ReleaseNotFound { .. }) if options.install => None,
            Err(e) => return Err(e),
        };

        // If no existing release and install flag set, do install
        if existing.is_none() {
            let install_opts = InstallOptions {
                name: options.name.clone(),
                namespace: options.namespace.clone(),
                wait: options.wait,
                timeout: options.timeout,
                health_check: options.health_check.clone(),
                atomic: options.atomic,
                dry_run: options.dry_run,
                show_diff: options.show_diff,
                labels: options.labels.clone(),
                description: options.description.clone(),
                ..Default::default()
            };
            return self.install(pack, values, &install_opts).await;
        }

        let existing = existing.unwrap();

        // Check for stuck state
        if existing.state.is_pending() {
            if existing.is_stuck() {
                return Err(KubeError::StuckRelease {
                    name: existing.name.clone(),
                    status: existing.state.status_name().to_string(),
                    elapsed: existing
                        .state
                        .elapsed()
                        .map(|d| format!("{} seconds", d.num_seconds()))
                        .unwrap_or_else(|| "unknown".to_string()),
                });
            } else {
                return Err(KubeError::OperationInProgress {
                    name: existing.name.clone(),
                    status: existing.state.to_string(),
                });
            }
        }

        // Merge values
        let final_values = if options.reset_values {
            values
        } else if options.reuse_values {
            let mut merged = existing.values.clone();
            merged.merge(&values);
            merged
        } else {
            values
        };

        // Create template context
        let release_info =
            ReleaseInfo::for_upgrade(&options.name, &options.namespace, existing.version + 1);
        let context = TemplateContext::new(final_values.clone(), release_info, &pack.pack.metadata);

        // Render templates
        let render_result = self
            .engine
            .render_pack(pack, &context)
            .map_err(|e| KubeError::Template(e.to_string()))?;

        // Create new release
        let manifest = render_result
            .manifests
            .values()
            .cloned()
            .collect::<Vec<_>>()
            .join("\n---\n");
        let mut release = StoredRelease::for_upgrade(&existing, final_values, manifest);
        release.notes = render_result.notes;
        release.hooks = parse_hooks_from_manifest(&release.manifest);

        // Add custom labels
        for (k, v) in &options.labels {
            release.labels.insert(k.clone(), v.clone());
        }

        // Show diff if requested
        if options.show_diff {
            let diff = self.diff_engine.diff_releases(&existing, &release);
            println!("Changes: {}", self.diff_engine.summary(&diff));
            // TODO: Print detailed diff
        }

        // Dry run
        if options.dry_run {
            return Ok(release);
        }

        // Store the pending release
        self.storage.create(&release).await?;

        // Mark previous as superseded
        let mut prev = existing;
        prev.mark_superseded();
        self.storage.update(&prev).await?;

        // Execute pre-upgrade hooks
        let mut hook_executor = HookExecutor::new();
        if !options.no_hooks
            && let Err(e) = hook_executor
                .execute_phase(
                    &release.hooks,
                    HookPhase::PreUpgrade,
                    &release.name,
                    release.version,
                    &self.client,
                )
                .await
        {
            release.mark_failed(e.to_string(), true);
            self.storage.update(&release).await?;

            if options.atomic {
                return self.rollback_to(&release, prev.version).await;
            }
            return Err(e);
        }

        // Apply manifests
        if let Err(e) = self
            .apply_manifest(&release.namespace, &release.manifest)
            .await
        {
            release.mark_failed(e.to_string(), true);
            self.storage.update(&release).await?;

            if options.atomic {
                return self.rollback_to(&release, prev.version).await;
            }
            return Err(e);
        }

        // Execute during-upgrade hooks
        if !options.no_hooks {
            let _ = hook_executor
                .execute_phase(
                    &release.hooks,
                    HookPhase::DuringUpgrade,
                    &release.name,
                    release.version,
                    &self.client,
                )
                .await;
        }

        // Wait for resources
        if options.wait {
            let health_config = options.health_check.clone().unwrap_or_default();
            let checker = HealthChecker::new(health_config);

            let status = checker.check(&release, &self.client).await?;
            if !status.healthy {
                let err_msg = status.summary();
                release.mark_failed(err_msg.clone(), true);
                self.storage.update(&release).await?;

                if options.atomic {
                    return self.rollback_to(&release, prev.version).await;
                }

                return Err(KubeError::HealthCheckFailed {
                    name: release.name.clone(),
                    message: err_msg,
                });
            }
        }

        // Execute post-upgrade hooks
        if !options.no_hooks {
            let _ = hook_executor
                .execute_phase(
                    &release.hooks,
                    HookPhase::PostUpgrade,
                    &release.name,
                    release.version,
                    &self.client,
                )
                .await;
        }

        // Mark as deployed
        release.mark_deployed();
        self.storage.update(&release).await?;

        // Cleanup old releases
        if let Some(max_history) = options.max_history {
            self.cleanup_history(&release.namespace, &release.name, max_history)
                .await?;
        }

        Ok(release)
    }

    // ========== Uninstall ==========

    /// Uninstall a release
    pub async fn uninstall(&self, options: &UninstallOptions) -> Result<StoredRelease> {
        // Get existing release
        let mut release = self
            .storage
            .get_latest(&options.namespace, &options.name)
            .await?;

        // Update state
        release.state = ReleaseState::PendingUninstall {
            started_at: chrono::Utc::now(),
            timeout: options.timeout.unwrap_or(chrono::Duration::minutes(5)),
        };
        self.storage.update(&release).await?;

        // Dry run
        if options.dry_run {
            return Ok(release);
        }

        // Execute pre-delete hooks
        let mut hook_executor = HookExecutor::new();
        if !options.no_hooks {
            let _ = hook_executor
                .execute_phase(
                    &release.hooks,
                    HookPhase::PreDelete,
                    &release.name,
                    release.version,
                    &self.client,
                )
                .await;
        }

        // Delete resources
        if let Err(e) = self
            .delete_manifest(&release.namespace, &release.manifest)
            .await
        {
            release.mark_failed(e.to_string(), true);
            self.storage.update(&release).await?;
            return Err(e);
        }

        // Execute post-delete hooks
        if !options.no_hooks {
            let _ = hook_executor
                .execute_phase(
                    &release.hooks,
                    HookPhase::PostDelete,
                    &release.name,
                    release.version,
                    &self.client,
                )
                .await;
        }

        // Mark as uninstalled
        release.mark_uninstalled();
        self.storage.update(&release).await?;

        // Delete history unless keep_history
        if !options.keep_history {
            self.storage
                .delete_all(&options.namespace, &options.name)
                .await?;
        }

        Ok(release)
    }

    // ========== Rollback ==========

    /// Rollback to a previous revision
    pub async fn rollback(&self, options: &RollbackOptions) -> Result<StoredRelease> {
        // Get history
        let history = self
            .storage
            .history(&options.namespace, &options.name)
            .await?;

        if history.is_empty() {
            return Err(KubeError::ReleaseNotFound {
                name: options.name.clone(),
                namespace: options.namespace.clone(),
            });
        }

        // Determine target revision
        let target_version = if options.revision == 0 {
            // Rollback to previous
            if history.len() < 2 {
                return Err(KubeError::RollbackNotPossible {
                    name: options.name.clone(),
                    reason: "no previous revision available".to_string(),
                });
            }
            history[1].version
        } else {
            options.revision
        };

        // Find target release
        let target = history
            .iter()
            .find(|r| r.version == target_version)
            .ok_or_else(|| KubeError::RollbackNotPossible {
                name: options.name.clone(),
                reason: format!("revision {} not found", target_version),
            })?;

        let current = &history[0];

        // Show diff if requested
        if options.show_diff {
            let diff = self.diff_engine.diff_releases(current, target);
            println!("Rollback changes: {}", self.diff_engine.summary(&diff));
        }

        // Dry run
        if options.dry_run {
            return Ok(target.clone());
        }

        // Create new release based on target
        let mut release =
            StoredRelease::for_upgrade(current, target.values.clone(), target.manifest.clone());
        release.state = ReleaseState::PendingRollback {
            started_at: chrono::Utc::now(),
            timeout: options.timeout.unwrap_or(chrono::Duration::minutes(5)),
            target_version,
        };

        // Store pending release
        self.storage.create(&release).await?;

        // Mark current as superseded
        let mut prev = current.clone();
        prev.mark_superseded();
        self.storage.update(&prev).await?;

        // Execute pre-rollback hooks
        let mut hook_executor = HookExecutor::new();
        if !options.no_hooks
            && let Err(e) = hook_executor
                .execute_phase(
                    &release.hooks,
                    HookPhase::PreRollback,
                    &release.name,
                    release.version,
                    &self.client,
                )
                .await
        {
            release.mark_failed(e.to_string(), true);
            self.storage.update(&release).await?;
            return Err(e);
        }

        // Apply target manifest
        if let Err(e) = self
            .apply_manifest(&release.namespace, &release.manifest)
            .await
        {
            release.mark_failed(e.to_string(), true);
            self.storage.update(&release).await?;
            return Err(e);
        }

        // Wait for resources
        if options.wait {
            let health_config = options.health_check.clone().unwrap_or_default();
            let checker = HealthChecker::new(health_config);

            let status = checker.check(&release, &self.client).await?;
            if !status.healthy {
                let err_msg = status.summary();
                release.mark_failed(err_msg.clone(), true);
                self.storage.update(&release).await?;
                return Err(KubeError::HealthCheckFailed {
                    name: release.name.clone(),
                    message: err_msg,
                });
            }
        }

        // Execute post-rollback hooks
        if !options.no_hooks {
            let _ = hook_executor
                .execute_phase(
                    &release.hooks,
                    HookPhase::PostRollback,
                    &release.name,
                    release.version,
                    &self.client,
                )
                .await;
        }

        // Mark as deployed
        release.mark_deployed();
        self.storage.update(&release).await?;

        // Cleanup old releases
        if let Some(max_history) = options.max_history {
            self.cleanup_history(&release.namespace, &release.name, max_history)
                .await?;
        }

        Ok(release)
    }

    // ========== Query Operations ==========

    /// List releases
    pub async fn list(
        &self,
        namespace: Option<&str>,
        all_namespaces: bool,
    ) -> Result<Vec<StoredRelease>> {
        let ns = if all_namespaces { None } else { namespace };
        self.storage.list(ns, None, false).await
    }

    /// Get release history
    pub async fn history(&self, namespace: &str, name: &str) -> Result<Vec<StoredRelease>> {
        self.storage.history(namespace, name).await
    }

    /// Get release status
    pub async fn status(&self, namespace: &str, name: &str) -> Result<StoredRelease> {
        self.storage.get_latest(namespace, name).await
    }

    /// Get health status
    pub async fn health(
        &self,
        namespace: &str,
        name: &str,
        config: Option<HealthCheckConfig>,
    ) -> Result<HealthStatus> {
        let release = self.storage.get_latest(namespace, name).await?;
        let checker = HealthChecker::new(config.unwrap_or_default());
        checker.check_once(&release, &self.client).await
    }

    /// Diff between two revisions
    pub async fn diff(
        &self,
        namespace: &str,
        name: &str,
        revision1: u32,
        revision2: u32,
    ) -> Result<DiffResult> {
        let r1 = self.storage.get(namespace, name, revision1).await?;
        let r2 = self.storage.get(namespace, name, revision2).await?;
        Ok(self.diff_engine.diff_releases(&r1, &r2))
    }

    /// Recover a stuck release
    pub async fn recover(&self, namespace: &str, name: &str) -> Result<StoredRelease> {
        let mut release = self.storage.get_latest(namespace, name).await?;

        if !release.state.is_pending() {
            return Err(KubeError::InvalidConfig(format!(
                "release '{}' is not in a pending state",
                name
            )));
        }

        release.mark_failed("Manually recovered from stuck state".to_string(), true);
        self.storage.update(&release).await?;

        Ok(release)
    }

    // ========== Internal Helpers ==========

    /// Create a ResourceManager for Kubernetes operations
    async fn resource_manager(&self) -> Result<ResourceManager> {
        ResourceManager::new(self.client.clone()).await
    }

    /// Apply a manifest to the cluster using Server-Side Apply
    async fn apply_manifest(&self, namespace: &str, manifest: &str) -> Result<()> {
        let manager = self.resource_manager().await?;
        let summary = manager.apply_manifest(namespace, manifest, false).await?;

        if !summary.is_success() {
            let errors: Vec<String> = summary
                .failed
                .iter()
                .map(|(name, err)| format!("{}: {}", name, err))
                .collect();
            return Err(KubeError::InvalidConfig(format!(
                "Failed to apply resources: {}",
                errors.join("; ")
            )));
        }

        Ok(())
    }

    /// Apply a manifest in dry-run mode (validate without applying)
    #[allow(dead_code)]
    async fn apply_manifest_dry_run(
        &self,
        namespace: &str,
        manifest: &str,
    ) -> Result<crate::resources::OperationSummary> {
        let manager = self.resource_manager().await?;
        manager.apply_manifest(namespace, manifest, true).await
    }

    /// Delete resources from a manifest
    async fn delete_manifest(&self, namespace: &str, manifest: &str) -> Result<()> {
        let manager = self.resource_manager().await?;
        let summary = manager.delete_manifest(namespace, manifest, false).await?;

        if !summary.is_success() {
            let errors: Vec<String> = summary
                .failed
                .iter()
                .map(|(name, err)| format!("{}: {}", name, err))
                .collect();
            return Err(KubeError::InvalidConfig(format!(
                "Failed to delete resources: {}",
                errors.join("; ")
            )));
        }

        Ok(())
    }

    /// Cleanup failed release resources
    async fn cleanup_release(&self, release: &StoredRelease) -> Result<()> {
        // Delete all resources from the manifest
        self.delete_manifest(&release.namespace, &release.manifest)
            .await
    }

    /// Rollback to a specific version (internal, used for atomic operations)
    async fn rollback_to(
        &self,
        current: &StoredRelease,
        target_version: u32,
    ) -> Result<StoredRelease> {
        // Verify the target release exists
        let _target = self
            .storage
            .get(&current.namespace, &current.name, target_version)
            .await?;

        // Create rollback options
        let options = RollbackOptions {
            name: current.name.clone(),
            namespace: current.namespace.clone(),
            revision: target_version,
            wait: true,
            timeout: Some(chrono::Duration::minutes(5)),
            ..Default::default()
        };

        // Perform the rollback
        self.rollback(&options).await
    }

    /// Cleanup old releases beyond max_history
    async fn cleanup_history(&self, namespace: &str, name: &str, max_history: u32) -> Result<()> {
        let history = self.storage.history(namespace, name).await?;

        if history.len() as u32 <= max_history {
            return Ok(());
        }

        // Delete oldest releases beyond max_history
        for release in history.iter().skip(max_history as usize) {
            self.storage
                .delete(namespace, name, release.version)
                .await?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    // Integration tests would go here
    // They require a running Kubernetes cluster
}
