# Helm Features Roadmap for Sherpack

This document identifies Helm features not yet implemented in Sherpack and proposes implementation strategies for Phase 5 and beyond.

## Feature Comparison Matrix

| Feature | Helm | Sherpack | Priority | Phase |
|---------|------|----------|----------|-------|
| Chart Repositories | ✅ | ❌ | High | 5 |
| OCI Registry Support | ✅ | ❌ | High | 5 |
| Dependency Management | ✅ | ❌ Partial | High | 5 |
| Chart Museum | ✅ | ❌ | Medium | 5 |
| helm search | ✅ | ❌ | High | 5 |
| helm pull | ✅ | ❌ | High | 5 |
| helm push | ✅ | ❌ | High | 5 |
| helm repo add/list/update | ✅ | ❌ | High | 5 |
| Subcharts/Dependencies | ✅ | ❌ | High | 5 |
| Library Charts | ✅ | ❌ | Medium | 6 |
| Post-renderer | ✅ | ❌ | Low | 6 |
| Plugins | ✅ | ❌ | Low | 7 |
| Lua scripting | ❌ | Possible | Low | 7 |
| Test Framework | ✅ | ❌ Partial | Medium | 5 |
| Chart Signing (Provenance) | ✅ PGP | ✅ Minisign | Done | 3 |

---

## Phase 5: Repository & OCI Support

### 1. Repository Management

**Goal**: Support Helm-compatible chart repositories and OCI registries.

#### 1.1 Repository Configuration

```rust
// New file: crates/sherpack-repo/src/config.rs

/// Repository configuration stored in ~/.config/sherpack/repositories.yaml
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepositoryConfig {
    pub api_version: String,  // "sherpack.io/v1"
    pub repositories: Vec<Repository>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Repository {
    /// Unique name for this repository
    pub name: String,

    /// Repository URL (HTTP(S) or OCI)
    pub url: String,

    /// Repository type
    pub repo_type: RepositoryType,

    /// Authentication credentials (optional)
    pub auth: Option<RepoAuth>,

    /// CA bundle for TLS verification
    pub ca_bundle: Option<PathBuf>,

    /// Skip TLS verification (insecure)
    pub insecure_skip_tls: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RepositoryType {
    /// Traditional Helm HTTP repository (index.yaml)
    Helm,
    /// OCI-compliant registry
    Oci,
    /// Git repository (future)
    Git,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RepoAuth {
    Basic { username: String, password: String },
    Bearer { token: String },
    /// Docker config.json style
    DockerConfig { path: PathBuf },
}
```

#### 1.2 Repository Index

```rust
// New file: crates/sherpack-repo/src/index.rs

/// Helm-compatible repository index
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepositoryIndex {
    #[serde(rename = "apiVersion")]
    pub api_version: String,

    /// When this index was generated
    pub generated: DateTime<Utc>,

    /// Packs indexed by name -> versions
    pub entries: HashMap<String, Vec<PackVersion>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackVersion {
    pub name: String,
    pub version: String,
    pub app_version: Option<String>,
    pub description: Option<String>,
    pub home: Option<String>,
    pub sources: Vec<String>,
    pub keywords: Vec<String>,
    pub maintainers: Vec<Maintainer>,
    /// URL to download the pack archive
    pub urls: Vec<String>,
    /// SHA256 digest of the archive
    pub digest: String,
    /// Creation timestamp
    pub created: DateTime<Utc>,
    /// Deprecated flag
    pub deprecated: bool,
}

impl RepositoryIndex {
    /// Fetch and parse index from URL
    pub async fn fetch(url: &str) -> Result<Self>;

    /// Search packs by keyword
    pub fn search(&self, query: &str) -> Vec<&PackVersion>;

    /// Get latest version of a pack
    pub fn get_latest(&self, name: &str) -> Option<&PackVersion>;

    /// Get specific version
    pub fn get_version(&self, name: &str, version: &str) -> Option<&PackVersion>;
}
```

#### 1.3 CLI Commands

```bash
# Repository management
sherpack repo add bitnami https://charts.bitnami.com/bitnami
sherpack repo add myregistry oci://registry.example.com/charts
sherpack repo list
sherpack repo update [NAME]
sherpack repo remove NAME

# Search
sherpack search hub nginx          # Search Artifact Hub
sherpack search repo nginx         # Search configured repos
sherpack search repo nginx --versions

# Pull/Push
sherpack pull nginx --repo bitnami --version 15.0.0
sherpack pull oci://registry.example.com/charts/nginx:15.0.0
sherpack push mypack.tar.gz oci://registry.example.com/charts
sherpack push mypack.tar.gz --repo myregistry
```

### 2. OCI Registry Support

**Goal**: Push/pull packs from OCI-compliant registries (Docker Hub, ECR, GCR, Harbor).

#### 2.1 OCI Client

```rust
// New file: crates/sherpack-repo/src/oci.rs

/// OCI registry client
pub struct OciClient {
    registry: String,
    auth: Option<RepoAuth>,
    client: reqwest::Client,
}

impl OciClient {
    /// Pull a pack from OCI registry
    pub async fn pull(&self, reference: &OciReference) -> Result<Vec<u8>>;

    /// Push a pack to OCI registry
    pub async fn push(&self, pack: &[u8], reference: &OciReference) -> Result<()>;

    /// List tags for a repository
    pub async fn list_tags(&self, repository: &str) -> Result<Vec<String>>;

    /// Check if reference exists
    pub async fn exists(&self, reference: &OciReference) -> Result<bool>;
}

/// OCI reference (e.g., registry.example.com/charts/nginx:15.0.0)
#[derive(Debug, Clone)]
pub struct OciReference {
    pub registry: String,
    pub repository: String,
    pub tag: Option<String>,
    pub digest: Option<String>,
}

impl OciReference {
    pub fn parse(s: &str) -> Result<Self>;

    pub fn to_string(&self) -> String;
}
```

#### 2.2 OCI Artifact Format

Sherpack packs stored as OCI artifacts follow the Helm OCI format:
- Config: `application/vnd.cncf.helm.config.v1+json`
- Content layer: `application/vnd.cncf.helm.chart.content.v1.tar+gzip`
- Provenance layer: `application/vnd.cncf.helm.chart.provenance.v1.prov`

```rust
/// OCI manifest for a Sherpack pack
#[derive(Debug, Serialize, Deserialize)]
pub struct PackManifest {
    pub schema_version: u32,
    pub config: OciDescriptor,
    pub layers: Vec<OciDescriptor>,
    pub annotations: HashMap<String, String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct OciDescriptor {
    pub media_type: String,
    pub digest: String,
    pub size: u64,
    pub annotations: HashMap<String, String>,
}
```

### 3. Dependency Management

**Goal**: Support pack dependencies with automatic resolution.

#### 3.1 Pack.yaml Extensions

```yaml
# Pack.yaml
apiVersion: sherpack.io/v1
kind: Pack
metadata:
  name: myapp
  version: 1.0.0

# Dependencies section
dependencies:
  - name: redis
    version: "^17.0.0"
    repository: https://charts.bitnami.com/bitnami
    condition: redis.enabled
    tags:
      - cache

  - name: postgresql
    version: "12.x"
    repository: oci://registry.example.com/charts
    alias: db
    import-values:
      - child: primary.persistence
        parent: database.persistence

# Alternative: local dependencies
  - name: common
    version: "*"
    repository: file://../common
```

#### 3.2 Dependency Resolution

```rust
// New file: crates/sherpack-repo/src/dependency.rs

/// Dependency resolver
pub struct DependencyResolver {
    repositories: Vec<Repository>,
    cache: DependencyCache,
}

impl DependencyResolver {
    /// Resolve all dependencies for a pack
    pub async fn resolve(&self, pack: &LoadedPack) -> Result<ResolvedDependencies>;

    /// Update lock file
    pub async fn update(&self, pack: &LoadedPack) -> Result<LockFile>;

    /// Build dependency tree
    pub fn build_tree(&self, deps: &ResolvedDependencies) -> DependencyTree;
}

/// Lock file (Pack.lock.yaml)
#[derive(Debug, Serialize, Deserialize)]
pub struct LockFile {
    pub generated: DateTime<Utc>,
    pub digest: String,
    pub dependencies: Vec<LockedDependency>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LockedDependency {
    pub name: String,
    pub version: String,
    pub repository: String,
    pub digest: String,
}
```

#### 3.3 CLI Commands

```bash
sherpack dependency list PACK
sherpack dependency update PACK
sherpack dependency build PACK    # Download and extract to charts/
```

---

## Phase 6: Advanced Features

### 4. Library Packs

**Goal**: Support reusable template libraries (like Helm library charts).

```yaml
# Pack.yaml for library pack
apiVersion: sherpack.io/v1
kind: Pack
metadata:
  name: common-templates
  version: 1.0.0
type: library  # New type field

# Using a library pack
dependencies:
  - name: common-templates
    version: "1.x"
    type: library
```

### 5. Post-Renderer Support

**Goal**: Allow external post-processing of rendered manifests.

```rust
/// Post-renderer interface
pub trait PostRenderer: Send + Sync {
    /// Transform rendered manifest
    fn render(&self, manifest: &str) -> Result<String>;
}

/// Kustomize post-renderer
pub struct KustomizePostRenderer {
    kustomization_path: PathBuf,
}

/// External command post-renderer
pub struct CommandPostRenderer {
    command: String,
    args: Vec<String>,
}
```

```bash
sherpack template myrelease mypack --post-renderer kustomize
sherpack template myrelease mypack --post-renderer "yq eval '.metadata.labels.env = \"prod\"'"
```

### 6. Test Framework Enhancement

**Goal**: Comprehensive testing framework for packs.

```yaml
# templates/tests/test-connection.yaml
apiVersion: v1
kind: Pod
metadata:
  name: "{{ .Release.Name }}-test-connection"
  annotations:
    sherpack.io/hook: test
    sherpack.io/hook-delete-policy: hook-succeeded
spec:
  containers:
    - name: wget
      image: busybox
      command: ['wget']
      args: ['{{ .Release.Name }}-service:{{ .Values.service.port }}']
  restartPolicy: Never
```

```bash
sherpack test myrelease --timeout 5m
sherpack test myrelease --logs              # Show test pod logs
sherpack test myrelease --cleanup           # Always cleanup, even on failure
```

---

## Phase 7: Ecosystem

### 7. Plugin System

**Goal**: Extensible plugin architecture.

```rust
/// Plugin interface
pub trait Plugin: Send + Sync {
    fn name(&self) -> &str;
    fn version(&self) -> &str;

    /// Hook called before template rendering
    fn pre_render(&self, context: &mut TemplateContext) -> Result<()> { Ok(()) }

    /// Hook called after template rendering
    fn post_render(&self, manifests: &mut Vec<String>) -> Result<()> { Ok(()) }

    /// Register custom template functions
    fn register_functions(&self, env: &mut minijinja::Environment) -> Result<()> { Ok(()) }
}
```

```bash
sherpack plugin install sherpack-secrets
sherpack plugin list
sherpack plugin update
```

### 8. Secrets Management Integration

**Goal**: Native integration with secrets managers.

```rust
/// Secrets provider interface
pub trait SecretsProvider: Send + Sync {
    /// Resolve a secret reference
    fn get_secret(&self, reference: &str) -> Result<String>;
}

// Implementations
pub struct VaultSecretsProvider { /* ... */ }
pub struct AwsSecretsManagerProvider { /* ... */ }
pub struct SopsProvider { /* ... */ }
```

```yaml
# values.yaml with secret references
database:
  password: vault:secret/data/myapp#password

tls:
  cert: aws-sm:myapp/tls-cert
  key: sops:secrets/tls.enc.yaml#key
```

---

## Implementation Priorities

### High Priority (Phase 5)
1. **Repository management** - Essential for pack distribution
2. **OCI support** - Modern registry standard
3. **Dependency resolution** - Required for complex applications
4. **sherpack search** - Discovery is critical for adoption

### Medium Priority (Phase 6)
5. **Library packs** - Reduces duplication
6. **Enhanced testing** - Quality assurance
7. **Post-renderer** - Kustomize integration

### Low Priority (Phase 7)
8. **Plugin system** - Extensibility
9. **Secrets integration** - Enterprise feature

---

## Proposed Crate Structure

```
crates/
├── sherpack-core/           # Existing
├── sherpack-engine/         # Existing
├── sherpack-kube/           # Existing
├── sherpack-repo/           # NEW - Phase 5
│   ├── src/
│   │   ├── lib.rs
│   │   ├── config.rs        # Repository configuration
│   │   ├── index.rs         # Repository index
│   │   ├── oci.rs           # OCI client
│   │   ├── dependency.rs    # Dependency resolution
│   │   ├── cache.rs         # Local cache management
│   │   └── search.rs        # Search implementation
│   └── Cargo.toml
├── sherpack-plugin/         # NEW - Phase 7
└── sherpack-cli/            # Existing, updated
```

---

## Dependencies to Add (Phase 5)

```toml
# Cargo.toml workspace additions
oci-distribution = "0.10"     # OCI registry client
semver = "1.0"                # Already in workspace - for dependency resolution
sha2 = "0.10"                 # Already in workspace - for digest verification
```

---

## Migration Path from Helm

Sherpack should support migrating existing Helm charts:

```bash
# Convert Helm chart to Sherpack pack
sherpack migrate ./my-helm-chart --output ./my-sherpack-pack

# Import Helm release into Sherpack management
sherpack adopt myrelease --from-helm --namespace default
```

This involves:
1. Converting Chart.yaml → Pack.yaml
2. Converting Go templates → Jinja2 templates (best effort)
3. Migrating release secrets to Sherpack format
