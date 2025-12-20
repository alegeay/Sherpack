# Technical Design: Killer Features Implementation

Deep technical analysis of how to implement each killer feature in Sherpack, leveraging the existing architecture.

---

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────────┐
│                        sherpack-cli                              │
├─────────────────────────────────────────────────────────────────┤
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────────┐  │
│  │   Engine    │  │    Kube     │  │         Repo            │  │
│  │  (Jinja2)   │  │  (K8s ops)  │  │  (Registry/Deps)        │  │
│  └──────┬──────┘  └──────┬──────┘  └────────────┬────────────┘  │
│         │                │                      │               │
│  ┌──────┴────────────────┴──────────────────────┴────────────┐  │
│  │                      sherpack-core                         │  │
│  │  Pack | Values | Release | Context | Schema | Archive     │  │
│  └────────────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────────┘
```

**Key Extension Points:**
1. `StorageDriver` trait → New storage backends
2. `KubeClient<S>` → New operations
3. `DiffEngine` → Drift detection
4. `HookExecutor` → New hook phases
5. `Engine` → New filters/functions

---

## Feature 1: Smart CRD Management

### Problem Analysis
- CRDs are cluster-scoped, releases are namespace-scoped
- CRD changes can break existing CRs
- CRD deletion orphans CRs
- Version migration (v1beta1 → v1) is complex

### Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                      CrdManager                              │
├─────────────────────────────────────────────────────────────┤
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────┐  │
│  │  Analyzer   │  │  Migrator   │  │     BackupStore     │  │
│  │ (schema diff)│  │ (CR update) │  │  (CR snapshots)     │  │
│  └──────┬──────┘  └──────┬──────┘  └──────────┬──────────┘  │
│         │                │                     │             │
│  ┌──────┴────────────────┴─────────────────────┴──────────┐  │
│  │                   ResourceManager                       │  │
│  │              (existing K8s operations)                  │  │
│  └─────────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────────┘
```

### Implementation

```rust
// crates/sherpack-kube/src/crd/mod.rs

/// CRD management policy
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrdPolicy {
    pub mode: CrdMode,
    pub upgrade_strategy: CrdUpgradeStrategy,
    pub backup_before_upgrade: bool,
    pub validate_before_apply: bool,
    pub cleanup_orphans: OrphanPolicy,
}

#[derive(Debug, Clone)]
pub enum CrdMode {
    /// Sherpack manages CRD lifecycle
    Managed,
    /// CRDs exist externally, just validate
    External,
    /// Ignore CRDs completely
    Ignore,
}

#[derive(Debug, Clone)]
pub enum CrdUpgradeStrategy {
    /// Fail if breaking changes detected
    Safe,
    /// Force upgrade, may break CRs
    Force,
    /// Ask user interactively
    Interactive,
    /// Auto-migrate CRs to new schema
    AutoMigrate,
}

/// Analyzes CRD schema changes
pub struct CrdAnalyzer {
    client: kube::Client,
}

impl CrdAnalyzer {
    /// Compare two CRD versions for breaking changes
    pub async fn diff(&self, old: &Crd, new: &Crd) -> CrdDiff {
        CrdDiff {
            added_fields: self.find_added_fields(old, new),
            removed_fields: self.find_removed_fields(old, new),
            type_changes: self.find_type_changes(old, new),
            validation_changes: self.find_validation_changes(old, new),
            is_breaking: self.is_breaking_change(old, new),
        }
    }

    /// Check if existing CRs are compatible with new CRD
    pub async fn validate_compatibility(
        &self,
        crd: &Crd,
        new_schema: &JSONSchema,
    ) -> Vec<IncompatibleCR> {
        let crs = self.list_crs_for_crd(crd).await?;
        crs.iter()
            .filter_map(|cr| {
                match jsonschema::validate(new_schema, cr) {
                    Ok(_) => None,
                    Err(errors) => Some(IncompatibleCR {
                        name: cr.name(),
                        namespace: cr.namespace(),
                        errors: errors.collect(),
                    }),
                }
            })
            .collect()
    }
}

/// Migrates CRs between CRD versions
pub struct CrdMigrator {
    client: kube::Client,
    backup_store: BackupStore,
}

impl CrdMigrator {
    /// Migrate all CRs to new CRD version
    pub async fn migrate(
        &self,
        crd: &Crd,
        from_version: &str,
        to_version: &str,
        transform: Option<MigrationTransform>,
    ) -> Result<MigrationResult> {
        // 1. Backup all CRs
        let backup_id = self.backup_store.backup_crs(crd).await?;

        // 2. Apply new CRD (with both versions)
        self.apply_crd_with_conversion(crd, from_version, to_version).await?;

        // 3. Migrate each CR
        let mut results = Vec::new();
        for cr in self.list_crs(crd).await? {
            match self.migrate_cr(&cr, to_version, &transform).await {
                Ok(_) => results.push(MigrationStatus::Success(cr.name())),
                Err(e) => results.push(MigrationStatus::Failed(cr.name(), e)),
            }
        }

        // 4. Remove old version from CRD
        if results.iter().all(|r| r.is_success()) {
            self.remove_old_version(crd, from_version).await?;
        }

        Ok(MigrationResult { backup_id, results })
    }
}

/// Stores CR backups for recovery
pub struct BackupStore {
    storage: Box<dyn StorageDriver>,
}

impl BackupStore {
    /// Backup all CRs for a CRD
    pub async fn backup_crs(&self, crd: &Crd) -> Result<BackupId> {
        let crs = self.list_all_crs(crd).await?;
        let backup = CrBackup {
            id: BackupId::new(),
            crd_name: crd.name(),
            timestamp: Utc::now(),
            crs: crs.iter().map(|cr| cr.to_yaml()).collect(),
        };

        // Store as a special release-like object
        self.storage.store_backup(&backup).await?;
        Ok(backup.id)
    }

    /// Restore CRs from backup
    pub async fn restore(&self, backup_id: &BackupId) -> Result<()> {
        let backup = self.storage.get_backup(backup_id).await?;
        for cr_yaml in &backup.crs {
            self.apply_cr(cr_yaml).await?;
        }
        Ok(())
    }
}
```

### CLI Integration

```rust
// crates/sherpack-cli/src/commands/crd.rs

#[derive(Subcommand)]
pub enum CrdCommand {
    /// Show CRD diff between pack versions
    Diff {
        pack: PathBuf,
        #[arg(long)]
        revision: Option<u32>,
    },

    /// Validate CRD changes against existing CRs
    Validate {
        pack: PathBuf,
    },

    /// Backup all CRs for a CRD
    Backup {
        #[arg(long)]
        crd: String,
        #[arg(long)]
        output: Option<PathBuf>,
    },

    /// Restore CRs from backup
    Restore {
        backup_id: String,
    },

    /// Migrate CRs to new CRD version
    Migrate {
        #[arg(long)]
        crd: String,
        #[arg(long)]
        from: String,
        #[arg(long)]
        to: String,
    },
}
```

### Integration with Install/Upgrade

```rust
// In KubeClient::install/upgrade

async fn install(&self, pack: &LoadedPack, options: &InstallOptions) -> Result<()> {
    let crd_policy = pack.crd_policy();

    if crd_policy.mode == CrdMode::Managed {
        let crds = self.extract_crds(&pack.manifests)?;

        for crd in &crds {
            // Check for existing CRD
            if let Some(existing) = self.get_crd(&crd.name()).await? {
                let diff = self.crd_analyzer.diff(&existing, crd).await;

                if diff.is_breaking && crd_policy.upgrade_strategy == CrdUpgradeStrategy::Safe {
                    return Err(KubeError::BreakingCrdChange {
                        crd: crd.name(),
                        changes: diff,
                    });
                }

                if crd_policy.backup_before_upgrade {
                    self.backup_store.backup_crs(crd).await?;
                }

                if crd_policy.validate_before_apply {
                    let incompatible = self.crd_analyzer
                        .validate_compatibility(crd, &crd.schema())
                        .await?;
                    if !incompatible.is_empty() {
                        return Err(KubeError::IncompatibleCRs { crs: incompatible });
                    }
                }
            }

            // Apply CRD with Server-Side Apply
            self.resource_manager.apply_crd(crd).await?;

            // Wait for CRD to be established
            self.wait_for_crd_ready(&crd.name()).await?;
        }
    }

    // Continue with normal install...
}
```

### Rust Dependencies
```toml
# Already have kube, add:
json-patch = "1.0"  # For CR migration transforms
```

---

## Feature 2: Chunked Release Storage

### Current State
Already implemented in `storage/chunked.rs`! Key components:
- `ChunkedIndex`: Metadata about chunks
- `ChunkedStorage`: Read/write chunked data
- `LargeReleaseStrategy`: How to handle large releases

### Improvements Needed

```rust
// crates/sherpack-kube/src/storage/chunked.rs

/// Enhanced chunking with external storage support
pub enum LargeReleaseStrategy {
    /// Fail if release > 1MB
    Fail,
    /// Split across multiple secrets (current)
    ChunkedSecrets,
    /// Store manifest separately
    SeparateManifest,
    /// Store in external S3-compatible storage
    ExternalStorage {
        endpoint: String,
        bucket: String,
        credentials: CredentialSource,
    },
    /// Store in PostgreSQL
    SqlBackend {
        connection_string: String,
    },
}

/// Compression improvements
pub enum CompressionMethod {
    None,
    Gzip,
    Zstd,
    /// New: Dictionary-based compression for similar releases
    ZstdWithDictionary {
        dictionary: Vec<u8>,
    },
}

impl ChunkedStorage {
    /// Optimize storage by deduplicating common content
    pub async fn store_with_dedup(
        &self,
        release: &StoredRelease,
    ) -> Result<StorageRef> {
        // 1. Extract templates (often unchanged between versions)
        let templates_hash = self.hash_templates(&release.manifest)?;

        // 2. Check if templates already stored
        if let Some(existing) = self.get_by_hash(&templates_hash).await? {
            // Store only delta
            return self.store_delta(release, &existing).await;
        }

        // 3. Full storage with chunking
        self.store_full(release).await
    }
}
```

### CLI Integration

```rust
// Already functional, add migration command:

/// Migrate existing releases to chunked storage
#[derive(Parser)]
pub struct MigrateStorage {
    /// Release to migrate
    name: String,

    /// Target storage strategy
    #[arg(long, default_value = "chunked")]
    strategy: String,

    /// Namespace
    #[arg(short, long, default_value = "default")]
    namespace: String,
}

async fn execute(&self, client: &KubeClient<impl StorageDriver>) -> Result<()> {
    let release = client.get_release(&self.name, &self.namespace).await?;

    // Re-encode with new strategy
    let new_storage = match self.strategy.as_str() {
        "chunked" => StorageConfig::chunked(),
        "zstd" => StorageConfig::zstd_compressed(),
        _ => return Err(anyhow!("Unknown strategy")),
    };

    client.migrate_release_storage(&release, new_storage).await?;
    println!("Migrated {} to {} storage", self.name, self.strategy);
    Ok(())
}
```

---

## Feature 3: Native Drift Detection

### Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                      DriftDetector                               │
├─────────────────────────────────────────────────────────────────┤
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────────┐  │
│  │  Watcher    │  │ Reconciler  │  │      Notifier           │  │
│  │ (K8s watch) │  │ (auto-sync) │  │ (webhook/email)         │  │
│  └──────┬──────┘  └──────┬──────┘  └──────────┬──────────────┘  │
│         │                │                     │                 │
│  ┌──────┴────────────────┴─────────────────────┴──────────────┐  │
│  │                      DiffEngine                             │  │
│  │              (existing diff infrastructure)                 │  │
│  └─────────────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────────────┘
```

### Implementation

```rust
// crates/sherpack-kube/src/drift/mod.rs

use tokio::sync::broadcast;
use kube::runtime::watcher;

/// Configuration for drift detection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriftConfig {
    /// Enable drift detection
    pub enabled: bool,
    /// Check interval for polling mode
    pub interval: Duration,
    /// Use watch API instead of polling
    pub use_watch: bool,
    /// Paths to ignore in diff
    pub ignore_paths: Vec<String>,
    /// Auto-remediate drift
    pub auto_sync: bool,
    /// Notification configuration
    pub notifications: Vec<NotificationConfig>,
}

#[derive(Debug, Clone)]
pub enum NotificationConfig {
    Webhook { url: String, secret: Option<String> },
    Slack { webhook_url: String, channel: String },
    Email { smtp: SmtpConfig, recipients: Vec<String> },
    Stdout,
}

/// Drift detection result
#[derive(Debug, Clone)]
pub struct DriftReport {
    pub release_name: String,
    pub namespace: String,
    pub detected_at: DateTime<Utc>,
    pub drifted_resources: Vec<DriftedResource>,
    pub severity: DriftSeverity,
}

#[derive(Debug, Clone)]
pub struct DriftedResource {
    pub kind: String,
    pub name: String,
    pub namespace: Option<String>,
    pub diff: String,
    pub drift_type: DriftType,
}

#[derive(Debug, Clone)]
pub enum DriftType {
    /// Resource modified from desired state
    Modified { fields: Vec<String> },
    /// Resource deleted from cluster
    Deleted,
    /// Resource added outside of Sherpack
    Unexpected,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum DriftSeverity {
    None,
    Low,      // Cosmetic changes (labels, annotations)
    Medium,   // Config changes (env vars, mounts)
    High,     // Critical changes (image, replicas)
    Critical, // Resource deleted
}

/// Main drift detector
pub struct DriftDetector {
    client: kube::Client,
    diff_engine: DiffEngine,
    config: DriftConfig,
    event_tx: broadcast::Sender<DriftEvent>,
}

impl DriftDetector {
    /// Start continuous drift monitoring
    pub async fn watch(
        &self,
        release: &StoredRelease,
    ) -> Result<impl Stream<Item = DriftReport>> {
        let resources = self.parse_manifest(&release.manifest)?;

        // Create unified watch stream for all resources
        let watches = resources.iter().map(|r| {
            self.watch_resource(r)
        });

        let merged = futures::stream::select_all(watches);

        merged.filter_map(|event| async move {
            match event {
                WatchEvent::Modified(obj) => {
                    self.check_drift(&obj, release).await.ok()
                }
                WatchEvent::Deleted(obj) => {
                    Some(DriftReport::deleted(&obj))
                }
                _ => None,
            }
        })
    }

    /// One-time drift check
    pub async fn check(&self, release: &StoredRelease) -> Result<DriftReport> {
        let desired = self.parse_manifest(&release.manifest)?;
        let mut drifted = Vec::new();

        for resource in &desired {
            let live = self.get_live_resource(resource).await?;

            match live {
                Some(live_obj) => {
                    let diff = self.diff_engine.diff_resource(resource, &live_obj)?;
                    if !diff.is_empty() {
                        drifted.push(DriftedResource {
                            kind: resource.kind.clone(),
                            name: resource.name.clone(),
                            namespace: resource.namespace.clone(),
                            diff,
                            drift_type: DriftType::Modified {
                                fields: self.extract_changed_fields(&diff),
                            },
                        });
                    }
                }
                None => {
                    drifted.push(DriftedResource {
                        kind: resource.kind.clone(),
                        name: resource.name.clone(),
                        namespace: resource.namespace.clone(),
                        diff: String::new(),
                        drift_type: DriftType::Deleted,
                    });
                }
            }
        }

        Ok(DriftReport {
            release_name: release.name.clone(),
            namespace: release.namespace.clone(),
            detected_at: Utc::now(),
            severity: self.calculate_severity(&drifted),
            drifted_resources: drifted,
        })
    }

    /// Auto-sync drifted resources back to desired state
    pub async fn sync(&self, release: &StoredRelease) -> Result<SyncResult> {
        let drift = self.check(release).await?;

        if drift.drifted_resources.is_empty() {
            return Ok(SyncResult::NoChanges);
        }

        let mut synced = Vec::new();
        let mut failed = Vec::new();

        for drifted in &drift.drifted_resources {
            let resource = self.find_resource_in_manifest(
                &release.manifest,
                &drifted.kind,
                &drifted.name,
            )?;

            match self.apply_resource(&resource).await {
                Ok(_) => synced.push(drifted.name.clone()),
                Err(e) => failed.push((drifted.name.clone(), e.to_string())),
            }
        }

        Ok(SyncResult::Synced { synced, failed })
    }

    /// Calculate severity based on what changed
    fn calculate_severity(&self, drifted: &[DriftedResource]) -> DriftSeverity {
        drifted.iter()
            .map(|d| match &d.drift_type {
                DriftType::Deleted => DriftSeverity::Critical,
                DriftType::Unexpected => DriftSeverity::Medium,
                DriftType::Modified { fields } => {
                    if fields.iter().any(|f| f.contains("spec.template.spec.containers")) {
                        DriftSeverity::High
                    } else if fields.iter().any(|f| f.contains("spec.replicas")) {
                        DriftSeverity::High
                    } else if fields.iter().any(|f| f.contains("metadata.labels")) {
                        DriftSeverity::Low
                    } else {
                        DriftSeverity::Medium
                    }
                }
            })
            .max()
            .unwrap_or(DriftSeverity::None)
    }
}

/// Notification sender
pub struct DriftNotifier {
    config: Vec<NotificationConfig>,
    http_client: reqwest::Client,
}

impl DriftNotifier {
    pub async fn notify(&self, report: &DriftReport) -> Result<()> {
        for config in &self.config {
            match config {
                NotificationConfig::Webhook { url, secret } => {
                    self.send_webhook(url, secret.as_deref(), report).await?;
                }
                NotificationConfig::Slack { webhook_url, channel } => {
                    self.send_slack(webhook_url, channel, report).await?;
                }
                NotificationConfig::Email { smtp, recipients } => {
                    self.send_email(smtp, recipients, report).await?;
                }
                NotificationConfig::Stdout => {
                    println!("{}", self.format_report(report));
                }
            }
        }
        Ok(())
    }

    async fn send_slack(&self, url: &str, channel: &str, report: &DriftReport) -> Result<()> {
        let message = json!({
            "channel": channel,
            "text": format!(
                ":warning: Drift detected in release `{}/{}` ({} resources)",
                report.namespace,
                report.release_name,
                report.drifted_resources.len()
            ),
            "attachments": [{
                "color": match report.severity {
                    DriftSeverity::Critical => "danger",
                    DriftSeverity::High => "danger",
                    DriftSeverity::Medium => "warning",
                    _ => "good",
                },
                "fields": report.drifted_resources.iter().map(|d| {
                    json!({
                        "title": format!("{}/{}", d.kind, d.name),
                        "value": format!("{:?}", d.drift_type),
                        "short": true
                    })
                }).collect::<Vec<_>>()
            }]
        });

        self.http_client.post(url).json(&message).send().await?;
        Ok(())
    }
}
```

### CLI Integration

```rust
// crates/sherpack-cli/src/commands/drift.rs

#[derive(Subcommand)]
pub enum DriftCommand {
    /// Check for drift in a release
    Status {
        name: String,
        #[arg(short, long, default_value = "default")]
        namespace: String,
        #[arg(long)]
        json: bool,
    },

    /// Watch for drift continuously
    Watch {
        name: String,
        #[arg(short, long, default_value = "default")]
        namespace: String,
        #[arg(long)]
        interval: Option<humantime::Duration>,
    },

    /// Sync drifted resources back to desired state
    Sync {
        name: String,
        #[arg(short, long, default_value = "default")]
        namespace: String,
        #[arg(long)]
        dry_run: bool,
    },

    /// Show drift history
    History {
        name: String,
        #[arg(short, long, default_value = "default")]
        namespace: String,
        #[arg(long, default_value = "10")]
        limit: usize,
    },
}
```

### Background Daemon Mode

```rust
// For running as a controller in-cluster

pub struct DriftController {
    detector: DriftDetector,
    notifier: DriftNotifier,
    reconciler: Option<Reconciler>,
}

impl DriftController {
    /// Run as a Kubernetes controller
    pub async fn run(&self) -> Result<()> {
        // Watch all Sherpack releases
        let releases = self.watch_releases().await?;

        releases.for_each_concurrent(None, |release| async {
            match release {
                Ok(release) => {
                    // Set up drift watch for this release
                    let config = self.get_drift_config(&release);
                    if config.enabled {
                        self.detector.watch(&release).await
                            .for_each(|drift| async {
                                self.notifier.notify(&drift).await.ok();
                                if config.auto_sync {
                                    self.detector.sync(&release).await.ok();
                                }
                            })
                            .await;
                    }
                }
                Err(e) => eprintln!("Watch error: {}", e),
            }
        }).await;

        Ok(())
    }
}
```

---

## Feature 4: Built-in Secret Management

### Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                      SecretManager                               │
├─────────────────────────────────────────────────────────────────┤
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────────┐  │
│  │  Encryptor  │  │  Provider   │  │      KeyManager         │  │
│  │ (Age/SOPS)  │  │  (Vault/..) │  │   (rotation/audit)      │  │
│  └──────┬──────┘  └──────┬──────┘  └──────────┬──────────────┘  │
│         │                │                     │                 │
│  ┌──────┴────────────────┴─────────────────────┴──────────────┐  │
│  │                    ValuesProcessor                          │  │
│  │            (intercepts values before templating)            │  │
│  └─────────────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────────────┘
```

### Implementation

```rust
// crates/sherpack-core/src/secrets/mod.rs

use age::{Encryptor, Decryptor, Recipient};
use serde_yaml::Value;

/// Encrypted value marker
const ENCRYPTED_PREFIX: &str = "ENC[";
const ENCRYPTED_SUFFIX: &str = "]";

/// Secret encryption configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecretConfig {
    /// Encryption method
    pub method: EncryptionMethod,
    /// Key source
    pub key_source: KeySource,
    /// Fields to encrypt (glob patterns)
    pub encrypted_fields: Vec<String>,
    /// External secret providers
    pub providers: Vec<ProviderConfig>,
}

#[derive(Debug, Clone)]
pub enum EncryptionMethod {
    /// Age encryption (modern, simple)
    Age,
    /// SOPS-compatible (supports multiple backends)
    Sops {
        kms: Option<Vec<String>>,
        pgp: Option<Vec<String>>,
        age: Option<Vec<String>>,
    },
    /// AWS KMS
    AwsKms { key_id: String },
    /// GCP KMS
    GcpKms { key_name: String },
    /// Azure Key Vault
    AzureKeyVault { vault_url: String, key_name: String },
}

#[derive(Debug, Clone)]
pub enum KeySource {
    /// From environment variable
    Environment(String),
    /// From file
    File(PathBuf),
    /// From Kubernetes secret
    KubernetesSecret { name: String, namespace: String, key: String },
    /// Interactive (prompt user)
    Interactive,
}

#[derive(Debug, Clone)]
pub enum ProviderConfig {
    /// HashiCorp Vault
    Vault {
        address: String,
        path: String,
        auth: VaultAuth,
    },
    /// AWS Secrets Manager
    AwsSecretsManager {
        secret_id: String,
        region: Option<String>,
    },
    /// AWS SSM Parameter Store
    AwsSsm {
        path: String,
        region: Option<String>,
        recursive: bool,
    },
    /// GCP Secret Manager
    GcpSecretManager {
        project: String,
        secret_id: String,
    },
    /// Azure Key Vault
    AzureKeyVault {
        vault_url: String,
        secret_name: String,
    },
    /// Kubernetes Secret
    KubernetesSecret {
        name: String,
        namespace: String,
    },
}

/// Main secret manager
pub struct SecretManager {
    encryptor: Box<dyn SecretEncryptor>,
    providers: Vec<Box<dyn SecretProvider>>,
    key_manager: KeyManager,
}

impl SecretManager {
    /// Encrypt sensitive values in a YAML document
    pub fn encrypt_values(&self, values: &Value, patterns: &[String]) -> Result<Value> {
        let mut result = values.clone();

        for pattern in patterns {
            for path in self.find_matching_paths(&result, pattern) {
                if let Some(value) = self.get_value_at_path(&result, &path) {
                    if let Value::String(s) = value {
                        let encrypted = self.encryptor.encrypt(s.as_bytes())?;
                        let marker = format!("{}AGE,{}{}",
                            ENCRYPTED_PREFIX,
                            base64::encode(&encrypted),
                            ENCRYPTED_SUFFIX
                        );
                        self.set_value_at_path(&mut result, &path, Value::String(marker));
                    }
                }
            }
        }

        Ok(result)
    }

    /// Decrypt all encrypted values
    pub fn decrypt_values(&self, values: &Value) -> Result<Value> {
        self.transform_values(values, |v| {
            if let Value::String(s) = v {
                if s.starts_with(ENCRYPTED_PREFIX) && s.ends_with(ENCRYPTED_SUFFIX) {
                    let encrypted = self.parse_encrypted_value(s)?;
                    let decrypted = self.encryptor.decrypt(&encrypted)?;
                    return Ok(Value::String(String::from_utf8(decrypted)?));
                }
            }
            Ok(v.clone())
        })
    }

    /// Pull secrets from external providers and merge into values
    pub async fn pull_external_secrets(&self, values: &mut Value) -> Result<()> {
        for provider in &self.providers {
            let secrets = provider.fetch().await?;
            self.merge_secrets(values, secrets)?;
        }
        Ok(())
    }

    /// Rotate encryption key
    pub async fn rotate_key(&self, values_path: &Path) -> Result<()> {
        // 1. Generate new key
        let new_key = self.key_manager.generate_key()?;

        // 2. Decrypt with old key
        let content = fs::read_to_string(values_path)?;
        let values: Value = serde_yaml::from_str(&content)?;
        let decrypted = self.decrypt_values(&values)?;

        // 3. Re-encrypt with new key
        self.key_manager.set_active_key(new_key)?;
        let re_encrypted = self.encrypt_values(&decrypted, &["**"])?;

        // 4. Write back
        let output = serde_yaml::to_string(&re_encrypted)?;
        fs::write(values_path, output)?;

        // 5. Archive old key
        self.key_manager.archive_old_key()?;

        Ok(())
    }
}

/// Age encryption implementation
pub struct AgeEncryptor {
    identity: age::x25519::Identity,
    recipients: Vec<Box<dyn age::Recipient>>,
}

impl SecretEncryptor for AgeEncryptor {
    fn encrypt(&self, data: &[u8]) -> Result<Vec<u8>> {
        let encryptor = age::Encryptor::with_recipients(self.recipients.clone())?;
        let mut encrypted = Vec::new();
        let mut writer = encryptor.wrap_output(&mut encrypted)?;
        writer.write_all(data)?;
        writer.finish()?;
        Ok(encrypted)
    }

    fn decrypt(&self, data: &[u8]) -> Result<Vec<u8>> {
        let decryptor = age::Decryptor::new(data)?;
        let mut decrypted = Vec::new();
        let mut reader = decryptor.decrypt(std::iter::once(&self.identity as _))?;
        reader.read_to_end(&mut decrypted)?;
        Ok(decrypted)
    }
}

/// HashiCorp Vault provider
pub struct VaultProvider {
    client: VaultClient,
    path: String,
}

#[async_trait]
impl SecretProvider for VaultProvider {
    async fn fetch(&self) -> Result<HashMap<String, String>> {
        let secret = self.client.read_secret(&self.path).await?;
        Ok(secret.data)
    }

    async fn write(&self, secrets: &HashMap<String, String>) -> Result<()> {
        self.client.write_secret(&self.path, secrets).await
    }
}

/// AWS Secrets Manager provider
pub struct AwsSecretsManagerProvider {
    client: aws_sdk_secretsmanager::Client,
    secret_id: String,
}

#[async_trait]
impl SecretProvider for AwsSecretsManagerProvider {
    async fn fetch(&self) -> Result<HashMap<String, String>> {
        let response = self.client
            .get_secret_value()
            .secret_id(&self.secret_id)
            .send()
            .await?;

        let secret_string = response.secret_string()
            .ok_or_else(|| anyhow!("Binary secrets not supported"))?;

        let secrets: HashMap<String, String> = serde_json::from_str(secret_string)?;
        Ok(secrets)
    }
}
```

### CLI Integration

```rust
// crates/sherpack-cli/src/commands/secrets.rs

#[derive(Subcommand)]
pub enum SecretsCommand {
    /// Initialize secret encryption for a pack
    Init {
        #[arg(long, default_value = "age")]
        method: String,
        #[arg(long)]
        output: Option<PathBuf>,
    },

    /// Encrypt values file
    Encrypt {
        file: PathBuf,
        #[arg(long)]
        in_place: bool,
        #[arg(long)]
        patterns: Vec<String>,
    },

    /// Decrypt values file
    Decrypt {
        file: PathBuf,
        #[arg(long)]
        output: Option<PathBuf>,
    },

    /// Edit encrypted file (decrypt, edit, re-encrypt)
    Edit {
        file: PathBuf,
        #[arg(long, env = "EDITOR")]
        editor: String,
    },

    /// Rotate encryption key
    Rotate {
        file: PathBuf,
    },

    /// Pull secrets from external provider
    Pull {
        #[arg(long)]
        provider: String,
        #[arg(long)]
        source: String,
        #[arg(long)]
        output: Option<PathBuf>,
    },

    /// Push secrets to external provider
    Push {
        file: PathBuf,
        #[arg(long)]
        provider: String,
        #[arg(long)]
        destination: String,
    },
}
```

### Integration with Template Rendering

```rust
// In sherpack-engine, modify values processing

impl Engine {
    pub fn render_with_secrets(
        &self,
        templates: &[Template],
        values: &Value,
        secret_manager: &SecretManager,
    ) -> Result<Vec<RenderedTemplate>> {
        // Decrypt values just-in-time
        let decrypted_values = secret_manager.decrypt_values(values)?;

        // Render templates
        self.render(templates, &decrypted_values)

        // Note: decrypted values are dropped here, never persisted
    }
}
```

### Rust Dependencies
```toml
# New dependencies
age = "0.9"
aws-sdk-secretsmanager = "1.0"
aws-sdk-ssm = "1.0"
google-cloud-secretmanager = "0.3"
azure_security_keyvault = "0.10"
vaultrs = "0.7"  # HashiCorp Vault client
```

---

## Feature 5: Test Framework

### Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                      TestRunner                                  │
├─────────────────────────────────────────────────────────────────┤
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────────┐  │
│  │  TestParser │  │ Assertions  │  │     Reporter            │  │
│  │   (YAML)    │  │  (matchers) │  │  (output/coverage)      │  │
│  └──────┬──────┘  └──────┬──────┘  └──────────┬──────────────┘  │
│         │                │                     │                 │
│  ┌──────┴────────────────┴─────────────────────┴──────────────┐  │
│  │                       Engine                                │  │
│  │                 (template rendering)                        │  │
│  └─────────────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────────────┘
```

### Implementation

```rust
// crates/sherpack-test/src/lib.rs (new crate)

use sherpack_engine::Engine;
use jsonpath_rust::JsonPath;

/// Test suite definition
#[derive(Debug, Deserialize)]
pub struct TestSuite {
    pub suite: String,
    pub templates: Option<Vec<String>>,
    pub values: Option<serde_yaml::Value>,
    pub tests: Vec<TestCase>,
}

#[derive(Debug, Deserialize)]
pub struct TestCase {
    pub name: String,
    #[serde(default)]
    pub template: Option<String>,
    #[serde(default)]
    pub templates: Option<Vec<String>>,
    #[serde(default)]
    pub set: HashMap<String, serde_yaml::Value>,
    #[serde(default)]
    pub values_files: Vec<PathBuf>,
    pub asserts: Vec<Assertion>,
    #[serde(default)]
    pub release: Option<ReleaseConfig>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Assertion {
    /// Assert value at path equals expected
    Equal {
        path: String,
        value: serde_yaml::Value,
    },

    /// Assert value at path matches pattern
    MatchRegex {
        path: String,
        pattern: String,
    },

    /// Assert path exists
    Exists {
        path: String,
    },

    /// Assert path does not exist
    NotExists {
        path: String,
    },

    /// Assert value is one of
    IsOneOf {
        path: String,
        values: Vec<serde_yaml::Value>,
    },

    /// Assert array length
    HasLength {
        path: String,
        count: usize,
    },

    /// Assert contains substring
    Contains {
        path: String,
        content: String,
    },

    /// Assert template fails
    FailedTemplate {
        #[serde(default)]
        contains: Option<String>,
    },

    /// Assert matches snapshot
    MatchSnapshot {
        #[serde(default)]
        name: Option<String>,
    },

    /// Assert passes Kubernetes validation
    IsValidKubernetes,

    /// Assert matches OPA/Rego policy
    MatchPolicy {
        policy: String,
    },

    /// Assert rendered YAML contains document
    ContainsDocument {
        kind: String,
        name: String,
        #[serde(default)]
        namespace: Option<String>,
    },

    /// Custom assertion with Jinja expression
    Expression {
        expr: String,
        message: Option<String>,
    },
}

/// Test runner
pub struct TestRunner {
    engine: Engine,
    pack_path: PathBuf,
    snapshot_dir: PathBuf,
}

impl TestRunner {
    /// Run all tests in a suite
    pub fn run_suite(&self, suite: &TestSuite) -> TestSuiteResult {
        let mut results = Vec::new();

        for test in &suite.tests {
            let result = self.run_test(test, &suite.values);
            results.push(result);
        }

        TestSuiteResult {
            suite: suite.suite.clone(),
            total: results.len(),
            passed: results.iter().filter(|r| r.passed).count(),
            failed: results.iter().filter(|r| !r.passed).count(),
            results,
        }
    }

    /// Run a single test case
    fn run_test(&self, test: &TestCase, base_values: &Option<Value>) -> TestResult {
        // Build values
        let mut values = base_values.clone().unwrap_or_default();
        for (k, v) in &test.set {
            self.set_value_at_path(&mut values, k, v.clone());
        }

        // Render template(s)
        let templates = test.template.as_ref()
            .map(|t| vec![t.clone()])
            .or_else(|| test.templates.clone())
            .unwrap_or_else(|| vec!["**/*.yaml".to_string()]);

        let render_result = self.engine.render_templates(&templates, &values);

        // Run assertions
        let mut assertion_results = Vec::new();

        for assertion in &test.asserts {
            let result = match assertion {
                Assertion::FailedTemplate { contains } => {
                    match &render_result {
                        Err(e) => {
                            if let Some(expected) = contains {
                                if e.to_string().contains(expected) {
                                    AssertionResult::Passed
                                } else {
                                    AssertionResult::Failed {
                                        expected: format!("error containing '{}'", expected),
                                        actual: e.to_string(),
                                    }
                                }
                            } else {
                                AssertionResult::Passed
                            }
                        }
                        Ok(_) => AssertionResult::Failed {
                            expected: "template to fail".to_string(),
                            actual: "template succeeded".to_string(),
                        },
                    }
                }
                _ => {
                    match &render_result {
                        Ok(rendered) => self.run_assertion(assertion, rendered),
                        Err(e) => AssertionResult::Error(e.to_string()),
                    }
                }
            };

            assertion_results.push((assertion.clone(), result));
        }

        TestResult {
            name: test.name.clone(),
            passed: assertion_results.iter().all(|(_, r)| matches!(r, AssertionResult::Passed)),
            assertions: assertion_results,
            duration: std::time::Duration::ZERO, // TODO: measure
        }
    }

    fn run_assertion(&self, assertion: &Assertion, rendered: &str) -> AssertionResult {
        // Parse rendered YAML
        let docs: Vec<Value> = serde_yaml::Deserializer::from_str(rendered)
            .map(|d| Value::deserialize(d))
            .collect::<Result<_, _>>()
            .unwrap_or_default();

        match assertion {
            Assertion::Equal { path, value } => {
                let actual = self.query_path(&docs, path);
                if actual == Some(value.clone()) {
                    AssertionResult::Passed
                } else {
                    AssertionResult::Failed {
                        expected: format!("{:?}", value),
                        actual: format!("{:?}", actual),
                    }
                }
            }

            Assertion::Exists { path } => {
                if self.query_path(&docs, path).is_some() {
                    AssertionResult::Passed
                } else {
                    AssertionResult::Failed {
                        expected: format!("path '{}' to exist", path),
                        actual: "path not found".to_string(),
                    }
                }
            }

            Assertion::MatchSnapshot { name } => {
                let snapshot_name = name.clone()
                    .unwrap_or_else(|| self.generate_snapshot_name());
                self.compare_snapshot(&snapshot_name, rendered)
            }

            Assertion::IsValidKubernetes => {
                self.validate_kubernetes(&docs)
            }

            Assertion::MatchPolicy { policy } => {
                self.evaluate_opa_policy(policy, &docs)
            }

            // ... other assertions
            _ => AssertionResult::Error("Not implemented".to_string()),
        }
    }

    /// Query path using JSONPath
    fn query_path(&self, docs: &[Value], path: &str) -> Option<Value> {
        // Support both JSONPath and simple dot notation
        if path.starts_with('$') {
            // JSONPath
            let jsonpath = JsonPath::parse(path).ok()?;
            let json_docs: Vec<serde_json::Value> = docs.iter()
                .filter_map(|d| serde_json::to_value(d).ok())
                .collect();

            for doc in &json_docs {
                if let Some(result) = jsonpath.find(doc).first() {
                    return serde_yaml::to_value(result).ok();
                }
            }
            None
        } else {
            // Simple dot notation: kind/name.path.to.field
            self.simple_path_query(docs, path)
        }
    }
}

/// Coverage tracking
pub struct CoverageTracker {
    templates: HashMap<String, TemplateCoverage>,
}

#[derive(Default)]
pub struct TemplateCoverage {
    pub lines: HashSet<usize>,
    pub branches: HashMap<usize, BranchCoverage>,
    pub values_accessed: HashSet<String>,
}

impl CoverageTracker {
    /// Track which templates and paths were exercised
    pub fn track(&mut self, template: &str, context: &TrackingContext) {
        let coverage = self.templates.entry(template.to_string())
            .or_default();

        coverage.lines.extend(&context.lines_executed);
        coverage.values_accessed.extend(context.values_accessed.clone());
    }

    /// Generate coverage report
    pub fn report(&self) -> CoverageReport {
        let mut total_lines = 0;
        let mut covered_lines = 0;

        for (_, coverage) in &self.templates {
            total_lines += coverage.total_lines;
            covered_lines += coverage.lines.len();
        }

        CoverageReport {
            line_coverage: covered_lines as f64 / total_lines as f64 * 100.0,
            templates: self.templates.clone(),
        }
    }
}
```

### Test File Format

```yaml
# tests/deployment_test.yaml
suite: Deployment Tests

# Base values for all tests
values:
  app:
    name: myapp
    replicas: 1
  image:
    repository: nginx
    tag: latest

tests:
  - name: should set correct replicas
    template: templates/deployment.yaml
    set:
      app.replicas: 5
    asserts:
      - equal:
          path: spec.replicas
          value: 5

  - name: should use image from values
    template: templates/deployment.yaml
    asserts:
      - equal:
          path: spec.template.spec.containers[0].image
          value: nginx:latest

  - name: should create service when enabled
    templates:
      - templates/deployment.yaml
      - templates/service.yaml
    set:
      service.enabled: true
    asserts:
      - containsDocument:
          kind: Service
          name: myapp

  - name: should fail when name is empty
    template: templates/deployment.yaml
    set:
      app.name: ""
    asserts:
      - failedTemplate:
          contains: "name is required"

  - name: should match snapshot
    template: templates/deployment.yaml
    set:
      app.replicas: 3
    asserts:
      - matchSnapshot:
          name: deployment-3-replicas

  - name: should pass pod security policy
    template: templates/deployment.yaml
    asserts:
      - matchPolicy: pod-security-restricted
      - isValidKubernetes
```

### CLI Integration

```rust
// crates/sherpack-cli/src/commands/test.rs

#[derive(Parser)]
pub struct TestCommand {
    /// Pack path
    #[arg(default_value = ".")]
    pack: PathBuf,

    /// Filter tests by name
    #[arg(long)]
    filter: Option<String>,

    /// Update snapshots
    #[arg(long)]
    update_snapshots: bool,

    /// Show coverage report
    #[arg(long)]
    coverage: bool,

    /// Output format
    #[arg(long, default_value = "pretty")]
    output: OutputFormat,

    /// Fail on first error
    #[arg(long)]
    fail_fast: bool,
}

async fn execute(&self) -> Result<()> {
    let runner = TestRunner::new(&self.pack)?;
    let suites = runner.discover_test_files()?;

    let mut total_passed = 0;
    let mut total_failed = 0;

    for suite_path in suites {
        let suite: TestSuite = serde_yaml::from_reader(File::open(&suite_path)?)?;

        println!("  {} {}", "SUITE".blue(), suite.suite);

        let result = runner.run_suite(&suite);

        for test_result in &result.results {
            if test_result.passed {
                println!("    {} {}", "✓".green(), test_result.name);
                total_passed += 1;
            } else {
                println!("    {} {}", "✗".red(), test_result.name);
                for (assertion, result) in &test_result.assertions {
                    if let AssertionResult::Failed { expected, actual } = result {
                        println!("      Expected: {}", expected);
                        println!("      Actual:   {}", actual);
                    }
                }
                total_failed += 1;

                if self.fail_fast {
                    return Err(anyhow!("Test failed"));
                }
            }
        }
    }

    println!();
    println!("Tests: {} passed, {} failed", total_passed, total_failed);

    if self.coverage {
        let report = runner.coverage_report();
        println!("Coverage: {:.1}%", report.line_coverage);
    }

    if total_failed > 0 {
        std::process::exit(1);
    }

    Ok(())
}
```

---

## Feature 6: Wave Deployment

### Current State
Already implemented in `waves.rs`! Key components:
- `Resource` with wave number
- `SyncWaveExecutor` for wave-based execution
- Dependency tracking via annotations

### Enhancements

```rust
// crates/sherpack-kube/src/waves.rs

/// Enhanced wave configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WaveConfig {
    /// Wait strategy between waves
    pub wait_strategy: WaveWaitStrategy,
    /// Parallel deployment within wave
    pub parallel_within_wave: bool,
    /// Continue on failure
    pub continue_on_failure: bool,
    /// Wave timeouts
    pub timeouts: HashMap<i32, Duration>,
}

#[derive(Debug, Clone)]
pub enum WaveWaitStrategy {
    /// Wait for all resources in wave to be ready
    AllReady,
    /// Wait for any resource to be ready
    AnyReady,
    /// Custom health checks
    Custom(Vec<WaveHealthCheck>),
    /// No waiting
    None,
}

#[derive(Debug, Clone)]
pub struct WaveHealthCheck {
    pub wave: i32,
    pub checks: Vec<HealthCheckConfig>,
}

/// Wave execution with progress reporting
pub struct WaveExecutor {
    resource_manager: ResourceManager,
    health_checker: HealthChecker,
    config: WaveConfig,
}

impl WaveExecutor {
    /// Execute resources in waves with progress callback
    pub async fn execute<F>(
        &self,
        resources: Vec<Resource>,
        mut on_progress: F,
    ) -> Result<WaveExecutionResult>
    where
        F: FnMut(WaveProgress),
    {
        // Group by wave
        let waves = self.group_by_wave(resources);
        let wave_numbers: Vec<i32> = waves.keys().copied().sorted().collect();

        let mut results = Vec::new();

        for (idx, wave_num) in wave_numbers.iter().enumerate() {
            on_progress(WaveProgress::WaveStarted {
                wave: *wave_num,
                total_waves: wave_numbers.len(),
                resources: waves[wave_num].len(),
            });

            let wave_resources = &waves[wave_num];

            // Apply resources (parallel or sequential)
            let apply_results = if self.config.parallel_within_wave {
                self.apply_parallel(wave_resources).await?
            } else {
                self.apply_sequential(wave_resources).await?
            };

            // Wait for wave health
            if self.config.wait_strategy != WaveWaitStrategy::None {
                let timeout = self.config.timeouts
                    .get(wave_num)
                    .copied()
                    .unwrap_or(Duration::from_secs(300));

                self.wait_for_wave_health(wave_resources, timeout).await?;
            }

            on_progress(WaveProgress::WaveCompleted {
                wave: *wave_num,
                applied: apply_results.len(),
                failed: apply_results.iter().filter(|r| r.is_err()).count(),
            });

            results.extend(apply_results);
        }

        Ok(WaveExecutionResult { results })
    }

    /// Visualize wave plan
    pub fn plan(&self, resources: &[Resource]) -> WavePlan {
        let waves = self.group_by_wave(resources.to_vec());

        WavePlan {
            waves: waves.into_iter().map(|(num, resources)| {
                WavePlanEntry {
                    wave: num,
                    resources: resources.into_iter().map(|r| {
                        ResourcePlan {
                            kind: r.kind,
                            name: r.name,
                            dependencies: r.dependencies,
                        }
                    }).collect(),
                }
            }).collect(),
        }
    }
}
```

### CLI Integration

```rust
#[derive(Parser)]
pub struct InstallCommand {
    // ... existing fields ...

    /// Show wave execution plan
    #[arg(long)]
    show_waves: bool,

    /// Execute only specific wave
    #[arg(long)]
    wave: Option<i32>,

    /// Pause between waves
    #[arg(long)]
    pause_between_waves: bool,
}
```

---

## Implementation Phases

### Phase 1: Foundation (Week 1-2)
1. **Chunked Storage Polish**
   - Add ZSTD dictionary compression
   - Add storage migration CLI
   - Improve error messages

2. **CRD Manager Core**
   - CRD diff analyzer
   - Backup/restore for CRs
   - Basic validation

### Phase 2: Security (Week 3-4)
3. **Secret Management**
   - Age encryption
   - Values encryption/decryption
   - Edit command

4. **Secret Providers**
   - Vault integration
   - AWS Secrets Manager
   - Environment variables

### Phase 3: Observability (Week 5-6)
5. **Drift Detection**
   - One-time check
   - Watch mode
   - Notification webhooks

6. **Auto-remediation**
   - Sync command
   - Severity calculation
   - Audit logging

### Phase 4: Testing (Week 7-8)
7. **Test Framework**
   - Test parser
   - Core assertions
   - Snapshot testing

8. **Coverage & Policies**
   - Coverage tracking
   - OPA integration
   - Kubernetes validation

### Phase 5: Advanced (Week 9-10)
9. **CRD Migration**
   - Version migration
   - CR transformation
   - Rollback support

10. **Wave Enhancements**
    - Progress reporting
    - Pause/resume
    - Interactive mode

---

## Dependencies Summary

```toml
# Cargo.toml additions

[workspace.dependencies]
# Encryption
age = "0.9"

# Secret providers
vaultrs = "0.7"
aws-sdk-secretsmanager = "1.0"
aws-sdk-ssm = "1.0"

# Testing
jsonpath-rust = "0.5"
insta = "1.34"  # Already have this

# Policy
opa-wasm = "0.1"  # For OPA policy evaluation

# Notifications
lettre = "0.11"  # Email
```

---

## File Structure

```
crates/
├── sherpack-core/
│   └── src/
│       └── secrets/
│           ├── mod.rs
│           ├── encryptor.rs
│           └── providers/
│               ├── vault.rs
│               ├── aws.rs
│               └── gcp.rs
│
├── sherpack-kube/
│   └── src/
│       ├── crd/
│       │   ├── mod.rs
│       │   ├── analyzer.rs
│       │   ├── migrator.rs
│       │   └── backup.rs
│       │
│       └── drift/
│           ├── mod.rs
│           ├── detector.rs
│           ├── reconciler.rs
│           └── notifier.rs
│
└── sherpack-test/  (new crate)
    └── src/
        ├── lib.rs
        ├── runner.rs
        ├── assertions.rs
        ├── coverage.rs
        └── snapshot.rs
```
