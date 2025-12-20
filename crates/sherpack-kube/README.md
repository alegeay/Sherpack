# sherpack-kube

Kubernetes integration for Sherpack - storage drivers, release management, and cluster operations.

## Overview

`sherpack-kube` provides the complete Kubernetes lifecycle management layer for Sherpack. It handles storing release state, applying resources to clusters, executing hooks, tracking health, and managing rollbacks.

## Features

- **Storage Drivers** - Persist release state in Secrets, ConfigMaps, or files
- **Release Management** - Full lifecycle with state machine and auto-recovery
- **Server-Side Apply** - Modern Kubernetes apply with conflict detection
- **CRD Handling** - Intent-based policies, smart updates with safety analysis (24 change types)
- **Hooks System** - Pre/post install/upgrade/rollback/delete with policies
- **Health Checks** - Deployment readiness, HTTP probes, custom commands
- **Diff Engine** - Three-way merge visualization
- **Sync Waves** - Resource ordering with dependencies
- **Progress Reporting** - Real-time feedback during operations

## CRD Handling

Sherpack provides sophisticated CRD (CustomResourceDefinition) handling that addresses Helm's major limitations.

### Intent-Based Policies

Unlike Helm's location-based approach (`crds/` vs `templates/`), Sherpack uses **intent-based policies**:

```rust
use sherpack_kube::crd::{CrdPolicy, DetectedCrd};

// Three policies determine CRD behavior
CrdPolicy::Managed   // Owned by release - installed, updated, protected on uninstall
CrdPolicy::Shared    // Multiple releases use it - never deleted
CrdPolicy::External  // Pre-existing CRD - don't touch at all
```

### Policy Annotations

Set CRD policy via annotations:

```yaml
apiVersion: apiextensions.k8s.io/v1
kind: CustomResourceDefinition
metadata:
  name: certificates.cert-manager.io
  annotations:
    # Explicit Sherpack policy
    sherpack.io/crd-policy: shared

    # Or use Helm-compatible annotation
    helm.sh/resource-policy: keep   # Translates to "shared"
```

### Safe Update Analysis

CRD updates are analyzed for safety with **24 change types**:

```rust
use sherpack_kube::crd::{CrdAnalyzer, ChangeSeverity};

let analyzer = CrdAnalyzer::new();
let changes = analyzer.analyze(&old_crd, &new_crd)?;

for change in &changes {
    match change.severity {
        ChangeSeverity::Safe => {
            // Auto-apply: new fields, new versions, new printer columns
        }
        ChangeSeverity::Warning => {
            // Show warning: validation changes, conversion changes
        }
        ChangeSeverity::Dangerous => {
            // Block by default: removing versions, scope changes, field type changes
        }
    }
}
```

### Detected Change Types

| Category | Safe Changes | Dangerous Changes |
|----------|--------------|-------------------|
| **Versions** | Add new version | Remove version, change storage version |
| **Schema** | Add optional field | Remove required field, change field type |
| **Validation** | Relax constraints | Tighten constraints |
| **Scope** | - | Change Namespaced ↔ Cluster |
| **Names** | - | Change plural/singular/kind |
| **Printer** | Add column | Remove column |

### Deletion Protection

```rust
use sherpack_kube::crd::{CrdProtection, CrdDeletionImpact, DeletionConfirmation};

let protection = CrdProtection::new(kube_client);

// Analyze impact before deletion
let impact = protection.analyze_deletion_impact(
    "certificates.cert-manager.io",
    CrdPolicy::Managed,
).await?;

println!("CRD: {}", impact.crd_name);
println!("Total CRs that would be deleted: {}", impact.total_resources);
for (namespace, count) in impact.sorted_namespaces() {
    println!("  {}: {} resources", namespace, count);
}

// Check if confirmation is needed
let confirmation = DeletionConfirmation::from_impact(&summary);
if confirmation.required {
    println!("Required flags: {:?}", confirmation.required_flags);
}
```

### CRD Location Tracking

Track where CRDs come from for debugging:

```rust
use sherpack_kube::crd::CrdLocation;

match crd.location {
    CrdLocation::CrdsDirectory { path, templated } => {
        if templated {
            // Warning: crds/ should not contain Jinja syntax
        }
    }
    CrdLocation::Templates { path } => {
        // CRD in templates/ - will be deleted on uninstall
    }
    CrdLocation::Dependency { dependency_name, .. } => {
        // CRD from a subchart
    }
}
```

### Lint Warnings

```rust
use sherpack_kube::crd::{lint_crds, CrdLintWarning, CrdLintCode};

let warnings = lint_crds(&crds_dir_crds, &template_crds, &templated_files);

for warning in &warnings {
    match warning.code {
        CrdLintCode::TemplatedInCrdsDir => {
            // Error: Jinja syntax in crds/ won't be rendered
        }
        CrdLintCode::StaticInTemplates => {
            // Warning: Static CRD in templates/ will be deleted on uninstall
        }
        CrdLintCode::MissingPolicy => {
            // Info: Consider adding sherpack.io/crd-policy annotation
        }
        CrdLintCode::ExternalInPack => {
            // Warning: External policy CRD shouldn't be in pack
        }
    }
}
```

### CLI Flags

```bash
# Skip CRD installation
sherpack install myrelease ./mypack --skip-crds

# Force CRD updates (bypass safety checks)
sherpack upgrade myrelease ./mypack --force-crd-update

# Show CRD changes before applying
sherpack upgrade myrelease ./mypack --show-crd-diff

# Delete CRDs on uninstall (requires confirmation)
sherpack uninstall myrelease --delete-crds --confirm-crd-deletion
```

### Comparison with Helm

| Feature | Helm | Sherpack |
|---------|------|----------|
| Policy model | Location-based | Intent-based |
| CRD updates | Never (blocked) | Safe by default, configurable |
| Patch strategy | Strategic (broken) | Server-Side Apply |
| Templating | No (in crds/) | Yes (with lint warnings) |
| Dependency ordering | None | Automatic |
| Wait for ready | No | Yes (configurable) |
| Dry-run | Broken | Full support |
| Deletion | Blocked | Configurable with confirmation |
| Diff output | None | Rich diff with impact analysis |

## Quick Start

```rust
use sherpack_kube::{KubeClient, InstallOptions, StorageConfig};
use kube::Client;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create Kubernetes client
    let client = Client::try_default().await?;

    // Create Sherpack client with storage configuration
    let storage_config = StorageConfig::default(); // Uses Secrets
    let sherpack = KubeClient::new(client, storage_config).await?;

    // Install a release
    let options = InstallOptions::builder()
        .name("my-app")
        .namespace("production")
        .values(values)
        .wait(true)
        .timeout(Duration::from_secs(300))
        .build();

    let release = sherpack.install(&pack, options).await?;
    println!("Installed {} revision {}", release.name, release.revision);

    Ok(())
}
```

## Storage Drivers

### Overview

Sherpack stores release metadata (values, manifests, status) using pluggable storage drivers.

```rust
use sherpack_kube::{StorageDriver, StorageConfig, CompressionMethod, LargeReleaseStrategy};

// Configure storage
let config = StorageConfig {
    // Driver type: Secrets (default), ConfigMap, or File
    driver: "secrets".to_string(),

    // Compression for stored data
    compression: CompressionMethod::Zstd,

    // How to handle releases > 1MB
    large_release_strategy: LargeReleaseStrategy::Chunked,

    // Namespace for storage (None = release namespace)
    storage_namespace: None,
};
```

### Available Drivers

#### Secrets Driver (Default)

```rust
use sherpack_kube::storage::SecretsDriver;

let driver = SecretsDriver::new(kube_client.clone(), namespace);

// Stores as: Secret/sh.sherpack.release.v1.{name}.v{revision}
```

**Pros:**
- Data encrypted at rest (if cluster encryption enabled)
- Standard Kubernetes RBAC
- Familiar pattern (same as Helm)

#### ConfigMap Driver

```rust
use sherpack_kube::storage::ConfigMapDriver;

let driver = ConfigMapDriver::new(kube_client.clone(), namespace);

// Stores as: ConfigMap/sh.sherpack.release.v1.{name}.v{revision}
```

**Pros:**
- Visible in standard ConfigMap tools
- No Secret access required

#### File Driver

```rust
use sherpack_kube::storage::FileDriver;

let driver = FileDriver::new("/var/lib/sherpack/releases");

// Stores as: /var/lib/sherpack/releases/{namespace}/{name}/v{revision}.yaml
```

**Pros:**
- Works without Kubernetes
- Easy backup/restore
- Good for testing

#### Mock Driver (Testing)

```rust
use sherpack_kube::{MockStorageDriver, OperationCounts};

let mock = MockStorageDriver::new();

// Perform operations...

// Check what happened
let counts = mock.operation_counts();
assert_eq!(counts.creates, 1);
assert_eq!(counts.updates, 0);
```

### Large Release Handling

Kubernetes Secrets/ConfigMaps have a ~1MB limit. Sherpack handles large releases automatically:

```rust
use sherpack_kube::LargeReleaseStrategy;

// Split into multiple objects
let config = StorageConfig {
    large_release_strategy: LargeReleaseStrategy::Chunked,
    ..Default::default()
};

// Or store manifests separately
let config = StorageConfig {
    large_release_strategy: LargeReleaseStrategy::SeparateManifests,
    ..Default::default()
};
```

## Release Management

### Release State Machine

```
                    ┌─────────────────────────────┐
                    │                             │
                    ▼                             │
┌─────────┐   ┌──────────┐   ┌──────────┐   ┌────────────┐
│ Pending │──▶│ Deployed │──▶│ Superseded│   │   Failed   │
└─────────┘   └──────────┘   └──────────┘   └────────────┘
     │              │              ▲               ▲
     │              │              │               │
     │              └──────────────┘               │
     │                  (upgrade)                  │
     │                                             │
     └─────────────────────────────────────────────┘
                        (error)
```

### StoredRelease

```rust
use sherpack_kube::{StoredRelease, ReleaseState, ValuesProvenance};

let release = StoredRelease {
    name: "my-app".to_string(),
    namespace: "production".to_string(),
    revision: 5,
    state: ReleaseState::Deployed,

    // Track where values came from
    values_provenance: ValuesProvenance {
        sources: vec![
            ValueSource::File("values.yaml".into()),
            ValueSource::Set("image.tag=v2.0".into()),
        ],
    },

    // Rendered manifests
    manifests: vec![...],

    // Timestamps
    created_at: Utc::now(),
    updated_at: Utc::now(),

    // Optional description
    description: Some("Upgrade to v2.0".into()),
};
```

### Auto-Recovery

Detect and recover stale releases:

```rust
use sherpack_kube::KubeClient;

// Find releases stuck in pending state
let stale = sherpack.find_stale_releases("production").await?;

for release in stale {
    println!("Stale: {} (pending since {})",
        release.name, release.created_at);

    // Recover by marking as failed
    sherpack.recover(&release.name, &release.namespace).await?;
}
```

## Resource Management

### Server-Side Apply

Sherpack uses Kubernetes Server-Side Apply for all resource operations:

```rust
use sherpack_kube::{ResourceManager, ApplyResult};

let manager = ResourceManager::new(kube_client);

// Apply with field manager
let result = manager.apply(
    &manifest,
    "sherpack",        // field manager
    true,              // force conflicts
).await?;

match result {
    ApplyResult::Created(resource) => println!("Created: {}", resource.name),
    ApplyResult::Updated(resource) => println!("Updated: {}", resource.name),
    ApplyResult::Unchanged => println!("No changes"),
}
```

### Resource Discovery

Automatically discover API resources:

```rust
// ResourceManager discovers available resources
let manager = ResourceManager::new(client).await?;

// Handles both core and custom resources
manager.apply(&deployment).await?;  // apps/v1 Deployment
manager.apply(&certificate).await?; // cert-manager.io/v1 Certificate
```

### Ordered Application

Resources are applied in the correct order:

1. Namespaces
2. CRDs
3. RBAC (ServiceAccount, Role, RoleBinding, ClusterRole, ClusterRoleBinding)
4. ConfigMaps, Secrets
5. Services
6. Deployments, StatefulSets, DaemonSets
7. Ingress
8. Custom resources

## Hooks System

### Hook Phases

| Phase | When |
|-------|------|
| `pre-install` | Before any resources are created |
| `post-install` | After all resources are created |
| `pre-upgrade` | Before upgrade starts |
| `post-upgrade` | After upgrade completes |
| `pre-rollback` | Before rollback starts |
| `post-rollback` | After rollback completes |
| `pre-delete` | Before uninstall starts |
| `post-delete` | After all resources deleted |
| `test` | Manual test execution |

### Defining Hooks

```yaml
apiVersion: batch/v1
kind: Job
metadata:
  name: db-migrate
  annotations:
    sherpack.io/hook: pre-upgrade
    sherpack.io/hook-weight: "5"
    sherpack.io/hook-delete-policy: hook-succeeded
spec:
  template:
    spec:
      containers:
        - name: migrate
          image: myapp:migrate
          command: ["./migrate.sh"]
      restartPolicy: Never
```

### Hook Policies

```rust
use sherpack_kube::{HookFailurePolicy, HookCleanupPolicy};

// Failure policies
HookFailurePolicy::Fail        // Abort operation (default)
HookFailurePolicy::Continue    // Log and continue
HookFailurePolicy::Rollback    // Trigger rollback

// Cleanup policies
HookCleanupPolicy::BeforeHookCreation  // Delete before creating new
HookCleanupPolicy::HookSucceeded       // Delete on success
HookCleanupPolicy::HookFailed          // Delete on failure
```

### Hook Execution

```rust
use sherpack_kube::{HookExecutor, HookPhase};

let executor = HookExecutor::new(kube_client, namespace);

// Execute hooks for a phase
let results = executor.execute(
    &manifests,
    HookPhase::PreUpgrade,
    Duration::from_secs(300), // timeout
).await?;

for result in results {
    match result {
        Ok(hook) => println!("Hook {} succeeded", hook.name),
        Err(e) => println!("Hook failed: {}", e),
    }
}
```

## Health Checks

### Built-in Checks

```rust
use sherpack_kube::{HealthChecker, HealthCheckConfig, HealthStatus};

let checker = HealthChecker::new(kube_client);

let config = HealthCheckConfig {
    timeout: Duration::from_secs(300),
    poll_interval: Duration::from_secs(5),
    deployment_ready: true,
    statefulset_ready: true,
    custom_checks: vec![],
};

let status = checker.check(&manifests, &config).await?;

match status {
    HealthStatus::Healthy => println!("All resources healthy"),
    HealthStatus::Progressing(msg) => println!("In progress: {}", msg),
    HealthStatus::Degraded(resources) => {
        for r in resources {
            println!("Degraded: {}/{}", r.kind, r.name);
        }
    }
    HealthStatus::Failed(msg) => println!("Failed: {}", msg),
}
```

### Custom Health Checks

```yaml
apiVersion: v1
kind: Pod
metadata:
  name: health-check
  annotations:
    sherpack.io/health-check: "true"
    sherpack.io/health-check-type: http
    sherpack.io/health-check-path: /healthz
    sherpack.io/health-check-port: "8080"
```

Or command-based:

```yaml
annotations:
  sherpack.io/health-check: "true"
  sherpack.io/health-check-type: command
  sherpack.io/health-check-command: "/bin/check-ready.sh"
```

### Resource Health States

```rust
use sherpack_kube::{ResourceHealth, ResourceState};

let health = checker.check_resource(&deployment).await?;

match health.state {
    ResourceState::Ready => println!("Ready"),
    ResourceState::Progressing { current, desired } => {
        println!("Progressing: {}/{}", current, desired);
    }
    ResourceState::Degraded { reason } => {
        println!("Degraded: {}", reason);
    }
    ResourceState::Failed { reason } => {
        println!("Failed: {}", reason);
    }
}
```

## Diff Engine

### Compare Releases

```rust
use sherpack_kube::{DiffEngine, ChangeType};

let engine = DiffEngine::new();

// Diff between two releases
let diff = engine.diff(&old_manifests, &new_manifests)?;

for change in &diff.changes {
    match change.change_type {
        ChangeType::Added => println!("+ {}", change.resource),
        ChangeType::Removed => println!("- {}", change.resource),
        ChangeType::Modified => {
            println!("~ {}", change.resource);
            println!("{}", change.diff_text);
        }
        ChangeType::Unchanged => {}
    }
}
```

### Three-Way Merge

Compare live cluster state with desired:

```rust
// Get live state from cluster
let live = manager.get_current_state(&manifests).await?;

// Three-way diff: original → live → new
let diff = engine.three_way_diff(&original, &live, &new)?;

for change in diff.changes {
    if change.has_drift {
        println!("DRIFT: {} was modified in cluster", change.resource);
    }
}
```

## Sync Waves

### Resource Ordering

Control resource application order:

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: database
  annotations:
    sherpack.io/sync-wave: "-1"  # Apply first
---
apiVersion: apps/v1
kind: Deployment
metadata:
  name: backend
  annotations:
    sherpack.io/sync-wave: "0"   # Apply second
---
apiVersion: apps/v1
kind: Deployment
metadata:
  name: frontend
  annotations:
    sherpack.io/sync-wave: "1"   # Apply last
```

### Execution Plan

```rust
use sherpack_kube::{ExecutionPlan, WaveExecutionConfig};

let config = WaveExecutionConfig {
    wait_between_waves: true,
    health_check_timeout: Duration::from_secs(120),
};

let plan = ExecutionPlan::from_manifests(&manifests)?;

println!("Execution plan:");
for wave in &plan.waves {
    println!("Wave {}: {} resources", wave.order, wave.resources.len());
    for resource in &wave.resources {
        println!("  - {}/{}", resource.kind, resource.name);
    }
}

// Execute with waits
plan.execute(&manager, &config).await?;
```

## Progress Reporting

### Real-Time Feedback

```rust
use sherpack_kube::{ProgressReporter, ResourceStatus};

let reporter = ProgressReporter::new();

// During installation
reporter.resource_status("Deployment/backend", ResourceStatus::Creating);
reporter.resource_status("Deployment/backend", ResourceStatus::Waiting {
    ready: 1,
    desired: 3
});
reporter.resource_status("Deployment/backend", ResourceStatus::Ready);

// Terminal output:
// ✓ Deployment/backend  [3/3 ready]
```

### Status Types

```rust
use sherpack_kube::ResourceState;

ResourceState::Pending      // Not yet started
ResourceState::Creating     // Being created
ResourceState::Waiting { ready, desired }  // Waiting for replicas
ResourceState::Ready        // Fully ready
ResourceState::Failed { reason }  // Failed
ResourceState::Deleted      // Successfully deleted
```

## Client Operations

### Install

```rust
let options = InstallOptions::builder()
    .name("my-app")
    .namespace("production")
    .values(values)
    .wait(true)
    .timeout(Duration::from_secs(300))
    .dry_run(false)
    .build();

let release = client.install(&pack, options).await?;
```

### Upgrade

```rust
let options = UpgradeOptions::builder()
    .values(values)
    .wait(true)
    .reuse_values(false)
    .reset_values(false)
    .force(false)
    .build();

let release = client.upgrade("my-app", "production", &pack, options).await?;
```

### Rollback

```rust
let options = RollbackOptions::builder()
    .revision(3)  // Target revision
    .wait(true)
    .build();

let release = client.rollback("my-app", "production", options).await?;
```

### Uninstall

```rust
let options = UninstallOptions::builder()
    .keep_history(false)
    .wait(true)
    .build();

client.uninstall("my-app", "production", options).await?;
```

### List Releases

```rust
let releases = client.list("production").await?;

for release in releases {
    println!("{} v{} ({})",
        release.name,
        release.revision,
        release.state
    );
}
```

### Get History

```rust
let history = client.history("my-app", "production").await?;

for revision in history {
    println!("v{}: {} at {}",
        revision.revision,
        revision.description,
        revision.created_at
    );
}
```

## Annotations Reference

| Annotation | Description | Values |
|------------|-------------|--------|
| `sherpack.io/hook` | Hook phase | `pre-install`, `post-install`, etc. |
| `sherpack.io/hook-weight` | Execution order | Integer (lower = first) |
| `sherpack.io/hook-delete-policy` | Cleanup policy | `before-hook-creation`, `hook-succeeded`, `hook-failed` |
| `sherpack.io/resource-policy` | Resource policy | `keep` (don't delete on uninstall) |
| `sherpack.io/sync-wave` | Wave order | Integer |
| `sherpack.io/health-check` | Enable health check | `true` |
| `sherpack.io/health-check-type` | Check type | `http`, `command` |

## Dependencies

- `kube` / `k8s-openapi` - Kubernetes client
- `sherpack-core` - Core types
- `sherpack-engine` - Template rendering
- `tokio` - Async runtime
- `zstd` / `flate2` - Compression
- `similar` - Diff algorithm
- `reqwest` - HTTP for health checks

## License

MIT OR Apache-2.0
