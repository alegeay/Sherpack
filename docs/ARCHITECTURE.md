# Sherpack Architecture

This document describes the internal architecture of Sherpack, a Kubernetes package manager written in Rust.

## Overview

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                              sherpack-cli                                    │
│  ┌─────────┐ ┌──────────┐ ┌─────────┐ ┌──────────┐ ┌────────┐ ┌──────────┐  │
│  │template │ │ install  │ │ package │ │  search  │ │  repo  │ │dependency│  │
│  │ lint    │ │ upgrade  │ │ inspect │ │  pull    │ │  add   │ │  update  │  │
│  │validate │ │uninstall │ │ keygen  │ │  push    │ │  list  │ │  build   │  │
│  │ show    │ │ rollback │ │  sign   │ │          │ │ update │ │  tree    │  │
│  │ create  │ │  list    │ │ verify  │ │          │ │ remove │ │          │  │
│  │         │ │ history  │ │         │ │          │ │        │ │          │  │
│  │         │ │ status   │ │         │ │          │ │        │ │          │  │
│  │         │ │ recover  │ │         │ │          │ │        │ │          │  │
│  └────┬────┘ └────┬─────┘ └────┬────┘ └────┬─────┘ └───┬────┘ └────┬─────┘  │
└───────┼──────────┼───────────┼──────────┼──────────┼─────────┼──────────────┘
        │          │           │          │          │         │
        ▼          ▼           ▼          ▼          ▼         ▼
┌───────────┐ ┌──────────┐ ┌────────┐ ┌──────────────────────────────────────┐
│  engine   │ │   kube   │ │  core  │ │                repo                  │
│           │ │          │ │        │ │                                      │
│ MiniJinja │ │ KubeClient│ │ Pack   │ │ HTTP │ OCI │ File │ Cache │ Lock   │
│ filters   │ │ Storage  │ │ Values │ │ Backend    Backend  FTS5    Deps    │
│ functions │ │ Hooks    │ │ Archive│ │                                      │
│suggestions│ │ Health   │ │Manifest│ │                                      │
│           │ │ Resources│ │ Schema │ │                                      │
└───────────┘ └──────────┘ └────────┘ └──────────────────────────────────────┘
```

## Crate Dependencies

```
sherpack-cli
    ├── sherpack-core
    ├── sherpack-engine ─── sherpack-core
    ├── sherpack-kube ───── sherpack-core
    └── sherpack-repo ───── sherpack-core
```

## sherpack-core

Core types shared across all crates.

### Pack Loading

```rust
// Pack.yaml structure
pub struct Pack {
    pub api_version: String,        // sherpack/v1
    pub kind: PackKind,             // application | library
    pub metadata: PackMetadata,     // name, version, description
    pub dependencies: Vec<Dependency>,
    pub engine: Option<EngineConfig>,
}

// LoadedPack includes loaded files
pub struct LoadedPack {
    pub pack: Pack,
    pub values: Values,
    pub schema: Option<Schema>,
    pub templates: HashMap<String, String>,
    pub path: PathBuf,
}

// Loading flow
LoadedPack::load(path) -> Result<LoadedPack>
    ├── Read Pack.yaml -> Pack
    ├── Read values.yaml -> Values
    ├── Read values.schema.yaml? -> Schema
    └── Read templates/*.yaml -> HashMap
```

### Values Merging

```rust
// Values are JSON with deep merge support
pub struct Values(pub serde_json::Value);

impl Values {
    // Merge order: schema defaults < values.yaml < -f files < --set flags
    pub fn merge(&self, other: &Values) -> Values;

    // Apply schema defaults first, then merge base values
    pub fn with_schema_defaults(defaults: Value, base: Values) -> Values;
}

// --set flag parsing
"app.replicas=3" -> {"app": {"replicas": 3}}
"tags[0]=v1"     -> {"tags": ["v1"]}
```

### Archive Format

```
archive.tar.gz
├── MANIFEST              # SHA256 checksums
├── Pack.yaml
├── values.yaml
├── values.schema.yaml    # Optional
└── templates/
    ├── deployment.yaml
    └── service.yaml
```

```rust
// MANIFEST format (TOML-like)
sherpack-manifest-version: 1

[files]
Pack.yaml = "sha256:abc123..."
values.yaml = "sha256:def456..."

[digest]
archive = "sha256:789..."
```

### Schema Validation

```rust
pub struct Schema {
    raw: Value,
    compiled: Option<jsonschema::Validator>,
}

impl Schema {
    // Extract default values from schema
    pub fn extract_defaults(&self) -> Value;

    // Validate values against schema
    pub fn validate(&self, values: &Values) -> Result<Vec<ValidationError>>;
}
```

## sherpack-engine

Template engine based on MiniJinja.

### Engine Architecture

```rust
pub struct Engine {
    env: minijinja::Environment<'static>,
    strict_mode: bool,
}

impl Engine {
    pub fn render(&self, context: &TemplateContext) -> Result<RenderedManifest>;
}

// Context passed to templates
pub struct TemplateContext {
    pub values: Values,
    pub release: Release,
    pub pack: PackInfo,
    pub capabilities: Capabilities,
}
```

### Filter Registration

```rust
// filters.rs - 25+ filters
env.add_filter("toyaml", filter_toyaml);
env.add_filter("tojson", filter_tojson);
env.add_filter("b64encode", filter_b64encode);
env.add_filter("indent", filter_indent);
env.add_filter("nindent", filter_nindent);
env.add_filter("quote", filter_quote);
env.add_filter("kebabcase", filter_kebabcase);
env.add_filter("sha256", filter_sha256);
// ... etc
```

### Error Suggestions

```rust
// suggestions.rs - Fuzzy matching for helpful errors
pub fn suggest_variable(undefined: &str, context: &Value) -> Option<String>;
pub fn suggest_filter(unknown: &str) -> Option<String>;

// Example output:
// Error: undefined variable 'value'
// Did you mean 'values'?

// Error: unknown filter 'toyml'
// Did you mean 'toyaml'?
```

## sherpack-kube

Kubernetes integration with full lifecycle management.

### Client Architecture

```rust
pub struct KubeClient<S: StorageDriver> {
    client: kube::Client,
    storage: S,
    namespace: String,
}

impl<S: StorageDriver> KubeClient<S> {
    // Lifecycle operations
    pub async fn install(&self, opts: &InstallOptions) -> Result<InstalledRelease>;
    pub async fn upgrade(&self, opts: &UpgradeOptions) -> Result<UpgradedRelease>;
    pub async fn uninstall(&self, opts: &UninstallOptions) -> Result<()>;
    pub async fn rollback(&self, opts: &RollbackOptions) -> Result<RolledBackRelease>;

    // Query operations
    pub async fn list(&self) -> Result<Vec<StoredRelease>>;
    pub async fn history(&self, name: &str) -> Result<Vec<StoredRelease>>;
    pub async fn status(&self, name: &str) -> Result<ReleaseStatus>;
    pub async fn recover(&self, name: &str) -> Result<()>;
}
```

### Install Flow

```
install(opts) -> Result<InstalledRelease>
    │
    ├── 1. Load pack
    │      LoadedPack::load(path)
    │
    ├── 2. Merge values
    │      schema_defaults < values.yaml < -f files < --set
    │
    ├── 3. Render templates
    │      engine.render(context) -> manifest
    │
    ├── 4. Store pending release
    │      storage.create(release, PendingInstall)
    │
    ├── 5. Run pre-install hooks
    │      execute_hooks(PreInstall, manifest)
    │
    ├── 6. Apply resources
    │      resource_manager.apply_manifest(manifest)
    │
    ├── 7. Wait for health (if --wait)
    │      health_checker.wait_for_ready(manifest)
    │
    ├── 8. Run post-install hooks
    │      execute_hooks(PostInstall, manifest)
    │
    └── 9. Update release state
           storage.update(release, Deployed)
```

### Storage Driver

```rust
#[async_trait]
pub trait StorageDriver: Send + Sync {
    async fn create(&self, release: &StoredRelease) -> Result<()>;
    async fn update(&self, release: &StoredRelease) -> Result<()>;
    async fn get(&self, name: &str, revision: u32) -> Result<StoredRelease>;
    async fn list(&self, filter: &ReleaseFilter) -> Result<Vec<StoredRelease>>;
    async fn delete(&self, name: &str, revision: u32) -> Result<()>;
}

// Implementations
pub struct SecretsDriver { client: Client, namespace: String }
pub struct ConfigMapDriver { client: Client, namespace: String }
pub struct FileDriver { base_path: PathBuf }
pub struct MockDriver { releases: Arc<RwLock<HashMap<String, StoredRelease>>> }
```

### Release Storage Format

```rust
pub struct StoredRelease {
    pub name: String,
    pub namespace: String,
    pub revision: u32,
    pub state: ReleaseState,
    pub manifest: String,           // Rendered YAML
    pub values: Values,             // Applied values
    pub values_provenance: ValuesProvenance,  // Where values came from
    pub pack_metadata: PackMetadata,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

pub enum ReleaseState {
    PendingInstall,
    PendingUpgrade,
    PendingRollback,
    Deployed,
    Failed,
    Superseded,
    Uninstalling,
    Uninstalled,
}
```

### Resource Manager

```rust
pub struct ResourceManager {
    client: Client,
    discovery: Discovery,
}

impl ResourceManager {
    // Server-Side Apply with field manager
    pub async fn apply_manifest(&self, manifest: &str) -> Result<ApplyResult>;

    // Delete with propagation policy
    pub async fn delete_manifest(&self, manifest: &str) -> Result<DeleteResult>;

    // Resource creation order (Namespace first, then CRDs, then workloads)
    fn creation_order(kind: &str) -> u32;
}

// Creation order priorities
Namespace       -> 0
CRD             -> 5
ClusterRole     -> 10
ServiceAccount  -> 12
Role            -> 13
RoleBinding     -> 14
ConfigMap       -> 20
Secret          -> 21
Service         -> 30
Ingress         -> 34
Deployment      -> 40
StatefulSet     -> 42
Job             -> 50
CronJob         -> 51
HPA             -> 60
```

### Hook System

```rust
pub enum HookPhase {
    PreInstall,
    PostInstall,
    PreUpgrade,
    PostUpgrade,
    PreRollback,
    PostRollback,
    PreDelete,
    PostDelete,
    Test,
    // + 2 internal phases
}

// Hook annotations
sherpack.io/hook: pre-install
sherpack.io/hook-weight: "0"        // Execution order
sherpack.io/hook-delete-policy: hook-succeeded,hook-failed

pub enum HookDeletePolicy {
    HookSucceeded,  // Delete after success
    HookFailed,     // Delete after failure
    BeforeHookCreation,  // Delete before recreating
}
```

### Health Checks

```rust
pub struct HealthChecker {
    client: Client,
}

impl HealthChecker {
    // Wait for workloads to be ready
    pub async fn wait_for_ready(&self, manifest: &str, timeout: Duration) -> Result<()>;

    // Check specific resource types
    async fn check_deployment(&self, name: &str, ns: &str) -> Result<HealthStatus>;
    async fn check_statefulset(&self, name: &str, ns: &str) -> Result<HealthStatus>;

    // Custom health checks from annotations
    async fn run_http_check(&self, url: &str) -> Result<()>;
    async fn run_command_check(&self, pod: &str, cmd: &[String]) -> Result<()>;
}
```

## sherpack-repo

Repository and dependency management.

### Repository Backend

```rust
#[async_trait]
pub trait RepositoryBackend: Send + Sync {
    async fn get_index(&mut self) -> Result<RepositoryIndex>;
    async fn search(&mut self, query: &str) -> Result<Vec<PackEntry>>;
    async fn find_best_match(&mut self, name: &str, constraint: &str) -> Result<PackEntry>;
    async fn download(&self, name: &str, version: &str) -> Result<Vec<u8>>;
}

// Implementations
pub struct HttpBackend { ... }   // HTTP repos with ETag caching
pub struct OciBackend { ... }    // OCI registries
pub struct FileBackend { ... }   // Local file repos
```

### HTTP Repository

```rust
pub struct HttpRepository {
    name: String,
    url: String,
    client: SecureHttpClient,
    cached_index: Option<RepositoryIndex>,
    etag: Option<String>,
}

impl HttpRepository {
    // Fetch with ETag caching
    async fn fetch_index(&mut self) -> Result<RepositoryIndex> {
        let response = self.client
            .get(&format!("{}/index.yaml", self.url))
            .header("If-None-Match", &self.etag)
            .send().await?;

        if response.status() == 304 {
            return Ok(self.cached_index.clone());
        }
        // Parse and cache new index
    }
}
```

### Credential Security

```rust
pub struct SecureHttpClient {
    inner: reqwest::Client,
    credentials: Option<Credentials>,
}

impl SecureHttpClient {
    // CRITICAL: Never send credentials after cross-origin redirect
    async fn fetch(&self, url: &str) -> Result<Response> {
        let response = self.inner
            .redirect(Policy::none())  // Manual redirects
            .send().await?;

        if response.status().is_redirection() {
            let redirect_url = response.headers().get(LOCATION)?;
            if !same_origin(url, redirect_url) {
                // Cross-origin: NO credentials
                return self.fetch_without_creds(redirect_url).await;
            }
        }
        Ok(response)
    }
}
```

### SQLite Search Cache

```rust
pub struct IndexCache {
    conn: Connection,  // SQLite with WAL mode
}

impl IndexCache {
    pub fn new(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)?;
        conn.execute_batch("
            PRAGMA journal_mode=WAL;
            CREATE VIRTUAL TABLE IF NOT EXISTS packs
            USING fts5(name, description, repo, version);
        ")?;
        Ok(Self { conn })
    }

    pub fn search(&self, query: &str) -> Result<Vec<PackEntry>> {
        self.conn.prepare("
            SELECT * FROM packs WHERE packs MATCH ?
        ")?.query_map([query], ...)?
    }
}
```

### Dependency Resolution

```rust
pub struct DependencyResolver {
    backends: HashMap<String, Box<dyn RepositoryBackend>>,
}

impl DependencyResolver {
    pub fn resolve(&self, deps: &[DependencySpec]) -> Result<DependencyGraph> {
        let mut resolved = HashMap::new();
        let mut queue = VecDeque::from(deps.to_vec());

        while let Some(dep) = queue.pop_front() {
            let entry = self.find_best_match(&dep)?;

            // Diamond conflict detection
            if let Some(existing) = resolved.get(&dep.name) {
                if !compatible(existing.version, entry.version) {
                    return Err(RepoError::DiamondConflict {
                        name: dep.name,
                        versions: format!("{} vs {}", existing.version, entry.version),
                    });
                }
            }

            resolved.insert(dep.name.clone(), entry.clone());

            // Add transitive dependencies
            for subdep in &entry.dependencies {
                queue.push_back(subdep.clone());
            }
        }

        Ok(DependencyGraph { resolved })
    }
}
```

### Lock File

```rust
pub struct LockFile {
    pub pack_yaml_digest: String,  // Detect Pack.yaml changes
    pub policy: LockPolicy,
    pub dependencies: Vec<LockedDependency>,
}

pub enum LockPolicy {
    Strict,      // Exact version + SHA must match
    Version,     // Default: version only
    SemverPatch, // Allow 1.2.3 -> 1.2.4
    SemverMinor, // Allow 1.2.3 -> 1.3.0
}

pub struct LockedDependency {
    pub name: String,
    pub version: String,
    pub repository: String,
    pub digest: String,  // SHA256 of archive
}
```

## sherpack-cli

Command-line interface using Clap.

### Command Structure

```rust
#[derive(Parser)]
pub enum Commands {
    // Templating
    Template { name: String, pack: PathBuf, ... },
    Lint { pack: PathBuf, ... },
    Validate { pack: PathBuf, ... },
    Show { pack: PathBuf, ... },
    Create { name: String },

    // Packaging
    Package { pack: PathBuf, ... },
    Inspect { archive: PathBuf, ... },
    Keygen { output: PathBuf, ... },
    Sign { archive: PathBuf, key: PathBuf },
    Verify { archive: PathBuf, ... },

    // Kubernetes
    Install { name: String, pack: PathBuf, ... },
    Upgrade { name: String, pack: PathBuf, ... },
    Uninstall { name: String },
    Rollback { name: String, revision: u32 },
    List { ... },
    History { name: String },
    Status { name: String },
    Recover { name: String },

    // Repository
    Repo(RepoCommands),
    Search { query: String, ... },
    Pull { pack: String, ... },
    Push { archive: PathBuf, destination: String },

    // Dependencies
    Dependency(DependencyCommands),
}
```

### Error Handling

```rust
#[derive(Error, Debug, Diagnostic)]
pub enum CliError {
    #[error("Validation failed: {message}")]
    #[diagnostic(code(sherpack::cli::validation))]
    Validation { message: String, help: Option<String> },

    #[error("Template error: {message}")]
    Template { message: String, help: Option<String> },

    // ... other variants
}

impl CliError {
    pub fn exit_code(&self) -> i32 {
        match self {
            Self::Validation { .. } => 2,
            Self::Template { .. } => 3,
            Self::Io { .. } => 4,
            _ => 1,
        }
    }
}
```

## Data Flow

### Template Command

```
User: sherpack template myrelease ./mypack --set app.replicas=5

1. Parse CLI args
   └── name="myrelease", pack="./mypack", set=["app.replicas=5"]

2. Load pack
   └── LoadedPack::load("./mypack")
       ├── Pack.yaml -> Pack
       ├── values.yaml -> Values
       ├── values.schema.yaml -> Schema (optional)
       └── templates/*.yaml -> HashMap

3. Merge values
   └── schema.extract_defaults()
       └── merge(values.yaml)
           └── merge(--set flags)

4. Validate (if schema)
   └── schema.validate(merged_values)

5. Build context
   └── TemplateContext { values, release, pack, capabilities }

6. Render templates
   └── engine.render(context)
       └── for each template: minijinja.render(template, context)

7. Output
   └── stdout or -o directory
```

### Install Command

```
User: sherpack install myrelease ./mypack -n production --wait

1. Load pack & merge values (same as template)

2. Check existing release
   └── storage.get("myrelease") -> None (new install)

3. Render manifest
   └── engine.render(context) -> YAML string

4. Create pending release
   └── storage.create(StoredRelease { state: PendingInstall, ... })

5. Parse hooks from manifest
   └── parse_hooks_from_manifest(manifest) -> Vec<Hook>

6. Execute pre-install hooks
   └── for hook in hooks.filter(phase == PreInstall):
       └── resource_manager.apply(hook)
       └── wait_for_job(hook)

7. Apply manifest (excluding hooks)
   └── resource_manager.apply_manifest(manifest)
       └── for resource in manifest.sorted_by(creation_order):
           └── api.patch(resource, ServerSideApply)

8. Wait for health (--wait)
   └── health_checker.wait_for_ready(manifest, timeout)

9. Execute post-install hooks
   └── for hook in hooks.filter(phase == PostInstall): ...

10. Update release state
    └── storage.update(release { state: Deployed })

11. Cleanup hook resources (based on delete-policy)
```

## Testing Strategy

### Unit Tests

Each module has inline tests:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_values_merge() { ... }

    #[tokio::test]
    async fn test_storage_create() { ... }
}
```

### Integration Tests

`tests/integration_tests.rs`:

```rust
fn sherpack(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_sherpack"))
        .args(args)
        .output()
        .expect("Failed to execute")
}

#[test]
fn test_template_basic() {
    let output = sherpack(&["template", "myrelease", "fixtures/demo-pack"]);
    assert!(output.status.success());
}
```

### Mock Storage

For Kubernetes tests without a cluster:

```rust
pub struct MockDriver {
    releases: Arc<RwLock<HashMap<String, StoredRelease>>>,
}

#[tokio::test]
async fn test_install_flow() {
    let storage = MockDriver::new();
    let client = KubeClient::new(storage);

    client.install(&InstallOptions::new("test", pack)).await?;

    let releases = client.list().await?;
    assert_eq!(releases.len(), 1);
}
```

## Performance Considerations

1. **Template Compilation**: Templates are compiled once and cached in `Engine`
2. **Discovery Caching**: K8s API discovery is expensive; consider caching `ResourceManager`
3. **Index Caching**: HTTP repos use ETag for conditional requests
4. **SQLite WAL**: FTS5 search uses WAL mode for concurrent reads
5. **Parallel Applies**: Resource applies could be parallelized (currently sequential for ordering)

## Security Considerations

1. **Credential Protection**: Never send credentials after cross-origin redirects
2. **Archive Verification**: SHA256 checksums in MANIFEST
3. **Signature Verification**: Minisign signatures for supply chain security
4. **Secret Handling**: Base64 encoding in templates, not encryption
5. **RBAC**: Storage drivers respect Kubernetes RBAC
