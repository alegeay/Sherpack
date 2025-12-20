# Phase 5: Helm Frustrations Analysis & Sherpack Solutions

This document analyzes user frustrations with Helm's repository, OCI, and dependency management features, then proposes improved alternatives for Sherpack.

---

## Table of Contents
1. [Repository Management Frustrations](#1-repository-management-frustrations)
2. [OCI Registry Frustrations](#2-oci-registry-frustrations)
3. [Dependency Management Frustrations](#3-dependency-management-frustrations)
4. [Authentication Frustrations](#4-authentication-frustrations)
5. [Search & Discovery Frustrations](#5-search--discovery-frustrations)
6. [Sherpack Solutions Summary](#6-sherpack-solutions-summary)

---

## 1. Repository Management Frustrations

### Problems Identified

| Issue | Source | Severity |
|-------|--------|----------|
| **Giant index.yaml files** | With 100k charts, `helm repo add` consumes 1GB+ RAM | Critical |
| **Slow `helm dep update`** | 5MB index = 5+ minutes to resolve deps | High |
| **Index parsed multiple times** | No caching during dependency resolution | High |
| **No incremental updates** | Full index rebuild required | Medium |
| **Single file doesn't scale** | YAML parsing bottleneck | High |

**Source**: [Helm Issue #3557 - Repository v2 Proposal](https://github.com/helm/helm/issues/3557), [Helm Issue #9865 - Slow dep update](https://github.com/helm/helm/issues/9865)

### Sherpack Solutions

#### 1.1 Sharded Index Architecture

```rust
// Instead of one giant index.yaml, use sharded indices
pub struct ShardedIndex {
    /// Metadata about the index
    pub meta: IndexMeta,
    /// Shard files: a-d.yaml, e-h.yaml, etc. or by first letter
    pub shards: HashMap<char, PathBuf>,
}

impl ShardedIndex {
    /// Only load the shard needed for a specific pack
    pub async fn get_pack(&self, name: &str) -> Result<PackVersion> {
        let first_char = name.chars().next().unwrap_or('_');
        let shard = self.load_shard(first_char).await?;
        shard.get(name).cloned().ok_or(NotFound)
    }

    /// Lazy loading - never load the full index into memory
    pub async fn search(&self, query: &str) -> Result<Vec<PackVersion>> {
        // Only load shards that might match
        let relevant_shards = self.filter_shards_by_query(query);
        // Stream results instead of collecting all
        todo!()
    }
}
```

**CLI Behavior**:
```bash
# Sherpack downloads only metadata on repo add (fast)
sherpack repo add bitnami https://charts.bitnami.com/bitnami
# Adding repository 'bitnami'... done (downloaded 12KB metadata)

# Full index only downloaded on first search/install
sherpack search bitnami/nginx
# Downloading index shard 'n'... done
```

#### 1.2 Lazy Loading with Streaming Parser

```rust
/// Stream-based YAML parser for large indices
pub struct StreamingIndexParser {
    reader: BufReader<File>,
}

impl StreamingIndexParser {
    /// Parse entries one by one without loading full file
    pub fn entries(&mut self) -> impl Iterator<Item = Result<PackVersion>> + '_ {
        // Use serde_yaml streaming API
        // Yield entries as they're parsed
        todo!()
    }

    /// Find specific pack without full parse
    pub fn find(&mut self, name: &str) -> Result<Option<PackVersion>> {
        for entry in self.entries() {
            let entry = entry?;
            if entry.name == name {
                return Ok(Some(entry));
            }
        }
        Ok(None)
    }
}
```

#### 1.3 Smart Caching with Incremental Updates

```rust
pub struct IndexCache {
    /// SQLite database for fast queries
    db: rusqlite::Connection,
    /// ETags for incremental updates
    etags: HashMap<String, String>,
}

impl IndexCache {
    /// Update only changed entries using ETag/Last-Modified
    pub async fn update(&mut self, repo: &Repository) -> Result<UpdateStats> {
        let response = self.client
            .get(&repo.index_url())
            .header("If-None-Match", self.etags.get(&repo.name))
            .send()
            .await?;

        match response.status() {
            StatusCode::NOT_MODIFIED => Ok(UpdateStats::unchanged()),
            StatusCode::OK => {
                // Parse and update only changed entries
                self.apply_delta(response).await
            }
            _ => Err(...)
        }
    }
}
```

**CLI Behavior**:
```bash
sherpack repo update
# bitnami: unchanged (cached)
# myrepo: 3 packs updated, 1 removed (delta sync)
```

---

## 2. OCI Registry Frustrations

### Problems Identified

| Issue | Source | Severity |
|-------|--------|----------|
| **`helm repo add` doesn't work with OCI** | Major UX confusion | Critical |
| **No search in OCI registries** | OCI spec limitation | High |
| **Auth errors (401/403)** | Token refresh, rate limiting | High |
| **Different UX for OCI vs HTTP** | Two mental models | High |
| **Rate limiting varies by provider** | Docker Hub vs ECR vs GCR | Medium |

**Sources**: [Helm Issue #10565](https://github.com/helm/helm/issues/10565), [Docker Hub Forum](https://forums.docker.com/t/docker-hub-oci-registry-for-helm-charts-is-unstable-and-returns-401-errors/138043)

### Sherpack Solutions

#### 2.1 Unified Repository Interface

```rust
/// Single interface for both HTTP repos and OCI registries
pub enum RepositoryBackend {
    /// Traditional HTTP repository with index.yaml
    Http(HttpRepository),
    /// OCI-compliant registry
    Oci(OciRegistry),
    /// Local filesystem
    File(PathBuf),
    /// Git repository (future)
    Git(GitRepository),
}

impl RepositoryBackend {
    /// Same commands work for all backends!
    pub async fn add(url: &str) -> Result<Self> {
        // Auto-detect backend type from URL
        if url.starts_with("oci://") {
            Self::Oci(OciRegistry::connect(url).await?)
        } else if url.starts_with("file://") {
            Self::File(PathBuf::from(&url[7..]))
        } else {
            Self::Http(HttpRepository::connect(url).await?)
        }
    }

    /// Search works everywhere (including OCI!)
    pub async fn search(&self, query: &str) -> Result<Vec<PackVersion>>;

    /// List works everywhere
    pub async fn list(&self) -> Result<Vec<PackVersion>>;
}
```

**CLI Behavior** (UNIFIED - unlike Helm!):
```bash
# Same command works for both HTTP and OCI
sherpack repo add bitnami https://charts.bitnami.com/bitnami
sherpack repo add myregistry oci://ghcr.io/myorg/charts

# Search works on OCI too!
sherpack search myregistry/
# myregistry/nginx     1.0.0   Nginx web server
# myregistry/redis     7.0.0   Redis cache

# Install from either with same syntax
sherpack install myapp bitnami/nginx
sherpack install myapp myregistry/nginx
```

#### 2.2 OCI Registry Catalog Support

```rust
/// OCI registry with catalog API support
pub struct OciRegistry {
    client: OciClient,
    /// Cached catalog for search
    catalog_cache: Option<RegistryCatalog>,
}

impl OciRegistry {
    /// List all repositories (for search)
    pub async fn catalog(&self) -> Result<Vec<String>> {
        // Use OCI catalog API: GET /v2/_catalog
        let response = self.client.get("/_catalog").await?;
        Ok(response.repositories)
    }

    /// List tags for a repository
    pub async fn tags(&self, repo: &str) -> Result<Vec<String>> {
        // GET /v2/{repo}/tags/list
        let response = self.client.get(&format!("/{}/tags/list", repo)).await?;
        Ok(response.tags)
    }

    /// Search by iterating catalog (with caching)
    pub async fn search(&self, query: &str) -> Result<Vec<PackVersion>> {
        let catalog = self.catalog().await?;
        let matches: Vec<_> = catalog
            .iter()
            .filter(|name| name.contains(query))
            .collect();

        // Fetch metadata for matches in parallel
        let versions = futures::future::join_all(
            matches.iter().map(|name| self.get_latest(name))
        ).await;

        Ok(versions.into_iter().filter_map(|r| r.ok()).collect())
    }
}
```

#### 2.3 Automatic Token Refresh & Retry

```rust
/// Smart authentication with auto-refresh
pub struct SmartAuth {
    credentials: Credentials,
    /// Cached token with expiry
    token_cache: RwLock<Option<TokenInfo>>,
}

struct TokenInfo {
    token: String,
    expires_at: DateTime<Utc>,
}

impl SmartAuth {
    /// Get valid token, refreshing if needed
    pub async fn get_token(&self) -> Result<String> {
        // Check cache
        if let Some(info) = self.token_cache.read().await.as_ref() {
            if info.expires_at > Utc::now() + Duration::minutes(5) {
                return Ok(info.token.clone());
            }
        }

        // Refresh token
        let new_token = self.refresh_token().await?;
        *self.token_cache.write().await = Some(new_token.clone());
        Ok(new_token.token)
    }

    /// Auto-retry with exponential backoff for rate limiting
    pub async fn request_with_retry<T>(&self, req: Request) -> Result<T> {
        let mut attempts = 0;
        loop {
            let response = self.client.execute(req.clone()).await?;

            match response.status() {
                StatusCode::TOO_MANY_REQUESTS => {
                    attempts += 1;
                    if attempts > 5 {
                        return Err(RateLimited);
                    }
                    let retry_after = response
                        .headers()
                        .get("Retry-After")
                        .and_then(|v| v.to_str().ok())
                        .and_then(|v| v.parse().ok())
                        .unwrap_or(2u64.pow(attempts));
                    tokio::time::sleep(Duration::from_secs(retry_after)).await;
                }
                StatusCode::UNAUTHORIZED => {
                    // Force token refresh and retry once
                    self.token_cache.write().await.take();
                    if attempts == 0 {
                        attempts += 1;
                        continue;
                    }
                    return Err(Unauthorized);
                }
                _ => return response.json().await,
            }
        }
    }
}
```

---

## 3. Dependency Management Frustrations

### Problems Identified

| Issue | Source | Severity |
|-------|--------|----------|
| **Diamond dependency problem** | Chart C at 0.1 and 0.2 both installed | Critical |
| **Template function conflicts** | Subcharts override each other | Critical |
| **Chart.lock doesn't lock versions** | Stores range instead of resolved version | Critical |
| **No duplicate detection** | Silent download of duplicate charts | High |
| **Same chart, different repos** | Can't use both | High |
| **Slow resolution** | Index loaded multiple times | High |

**Sources**: [Helm Issue #11933](https://github.com/helm/helm/issues/11933), [Helm Issue #30710](https://github.com/helm/helm/issues/30710), [Helm Issue #2759](https://github.com/helm/helm/issues/2759)

### Sherpack Solutions

#### 3.1 True Version Locking with Integrity

```rust
/// Lock file that ACTUALLY locks versions
#[derive(Debug, Serialize, Deserialize)]
pub struct LockFile {
    /// Schema version
    pub version: u32,

    /// When generated
    pub generated: DateTime<Utc>,

    /// SHA256 of Pack.yaml (detect if deps changed)
    pub pack_yaml_digest: String,

    /// Locked dependencies with EXACT versions
    pub dependencies: Vec<LockedDependency>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LockedDependency {
    pub name: String,
    /// EXACT version, never a range!
    pub version: semver::Version,  // Not String!
    pub repository: String,
    /// SHA256 of the pack archive for integrity
    pub digest: String,
    /// Transitive dependencies
    pub dependencies: Vec<String>,
}
```

**CLI Behavior**:
```bash
sherpack dependency update
# Resolving dependencies...
#   nginx 1.2.3 from bitnami (sha256:abc123...)
#   redis 7.0.1 from bitnami (sha256:def456...)
# Wrote Pack.lock with 2 dependencies

# Later, integrity check on install
sherpack dependency build
# Verifying integrity...
#   nginx: OK (sha256:abc123...)
#   redis: OK (sha256:def456...)
# Downloaded 2 dependencies

# If tampered:
sherpack dependency build
# ERROR: Integrity check failed for 'nginx'
#   Expected: sha256:abc123...
#   Got:      sha256:xyz789...
# Use --force to ignore (NOT RECOMMENDED)
```

#### 3.2 Diamond Dependency Resolution

```rust
/// Dependency resolver with conflict detection
pub struct DependencyResolver {
    repositories: Vec<Repository>,
    /// Resolution strategy
    strategy: ResolutionStrategy,
}

#[derive(Debug, Clone)]
pub enum ResolutionStrategy {
    /// Error on any conflict (safest)
    Strict,
    /// Use highest version that satisfies all constraints
    Highest,
    /// Use lowest version (more stable)
    Lowest,
    /// Use version from root Pack.yaml if specified
    RootWins,
}

impl DependencyResolver {
    pub async fn resolve(&self, pack: &LoadedPack) -> Result<ResolvedDeps> {
        let mut graph = DependencyGraph::new();
        let mut queue = VecDeque::from(pack.dependencies.clone());
        let mut seen = HashSet::new();

        while let Some(dep) = queue.pop_front() {
            if seen.contains(&dep.name) {
                // Already processed - check for conflicts
                let existing = graph.get(&dep.name).unwrap();
                self.check_conflict(&dep, existing)?;
                continue;
            }
            seen.insert(dep.name.clone());

            // Resolve version
            let resolved = self.resolve_version(&dep).await?;
            graph.insert(dep.name.clone(), resolved.clone());

            // Add transitive dependencies
            for transitive in &resolved.dependencies {
                queue.push_back(transitive.clone());
            }
        }

        // Detect diamond dependencies
        self.detect_diamonds(&graph)?;

        Ok(ResolvedDeps { graph })
    }

    fn detect_diamonds(&self, graph: &DependencyGraph) -> Result<()> {
        // Find packages required at multiple versions
        let mut versions: HashMap<String, HashSet<Version>> = HashMap::new();

        for (name, dep) in graph {
            versions
                .entry(name.clone())
                .or_default()
                .insert(dep.version.clone());
        }

        let conflicts: Vec<_> = versions
            .iter()
            .filter(|(_, v)| v.len() > 1)
            .collect();

        if !conflicts.is_empty() {
            return Err(KubeError::DependencyConflict {
                conflicts: conflicts
                    .iter()
                    .map(|(name, versions)| {
                        format!("{}: {}", name, versions.iter().join(", "))
                    })
                    .collect(),
            });
        }

        Ok(())
    }
}
```

**CLI Behavior**:
```bash
sherpack dependency update
# Resolving dependencies...
# ERROR: Diamond dependency conflict detected!
#
#   postgresql required at multiple versions:
#     - 12.2.1 (required by: keycloak)
#     - 10.9.3 (required by: airflow)
#
# Resolution options:
#   1. Pin version in Pack.yaml: postgresql: "^12.0.0"
#   2. Use --strategy=highest to auto-select 12.2.1
#   3. Use aliases to install both versions

# Using strategy
sherpack dependency update --strategy=highest
# Resolved: postgresql 12.2.1 (satisfies both keycloak and airflow)
```

#### 3.3 Template Isolation for Subcharts

```rust
/// Isolated template environments per subchart
pub struct IsolatedEngine {
    /// Root pack's engine
    root: Engine,
    /// Subchart engines with isolated namespaces
    subcharts: HashMap<String, Engine>,
}

impl IsolatedEngine {
    /// Each subchart gets its own environment
    fn create_subchart_engine(&self, name: &str, pack: &LoadedPack) -> Engine {
        let mut env = minijinja::Environment::new();

        // Prefix all template functions with subchart name
        // This prevents conflicts like:
        //   redis's fullname() vs postgresql's fullname()
        for (fn_name, func) in pack.helpers() {
            let prefixed = format!("{}_{}", name, fn_name);
            env.add_function(&prefixed, func);
        }

        // Also provide unprefixed within subchart's own scope
        for (fn_name, func) in pack.helpers() {
            env.add_function(fn_name, func);
        }

        Engine::from_env(env)
    }

    /// Render with isolation
    pub fn render_all(&self, context: &TemplateContext) -> Result<Vec<String>> {
        let mut manifests = Vec::new();

        // Render root pack
        manifests.extend(self.root.render_pack(context)?);

        // Render each subchart with isolated helpers
        for (name, engine) in &self.subcharts {
            let subchart_context = context.for_subchart(name);
            manifests.extend(engine.render_pack(&subchart_context)?);
        }

        Ok(manifests)
    }
}
```

#### 3.4 Dependency Aliases for Same-Chart Different-Versions

```yaml
# Pack.yaml - use aliases to have both versions
dependencies:
  - name: postgresql
    version: "12.2.1"
    repository: https://charts.bitnami.com/bitnami
    alias: keycloak-db  # Aliased!

  - name: postgresql
    version: "10.9.3"
    repository: https://charts.bitnami.com/bitnami
    alias: airflow-db  # Different alias!
```

```rust
/// Alias support for dependency resolution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Dependency {
    pub name: String,
    pub version: VersionReq,
    pub repository: String,
    /// Optional alias - allows same chart multiple times
    pub alias: Option<String>,
}

impl Dependency {
    /// Effective name (alias or original)
    pub fn effective_name(&self) -> &str {
        self.alias.as_deref().unwrap_or(&self.name)
    }
}
```

---

## 4. Authentication Frustrations

### Problems Identified

| Issue | Source | Severity |
|-------|--------|----------|
| **No bearer token support** | Only basic auth for HTTP repos | High |
| **Credentials in plaintext config** | Security risk | High |
| **Mixed public/private confusion** | Which creds for which repo? | Medium |
| **Short-lived tokens expire** | GCP tokens valid only 1 hour | Medium |
| **Credential leak risk** | Creds sent to wrong repo | High |

**Sources**: [Helm Issue #7451](https://github.com/helm/helm/issues/7451), [Helm Issue #8392](https://github.com/helm/helm/issues/8392)

### Sherpack Solutions

#### 4.1 Credential Store with Encryption

```rust
/// Secure credential storage
pub struct CredentialStore {
    /// Encrypted storage path
    path: PathBuf,
    /// Encryption key from keyring or env
    key: EncryptionKey,
}

impl CredentialStore {
    /// Store credentials encrypted
    pub fn store(&self, repo: &str, creds: Credentials) -> Result<()> {
        let encrypted = self.key.encrypt(&serde_json::to_vec(&creds)?)?;
        let entry = keyring::Entry::new("sherpack", repo)?;
        entry.set_password(&base64::encode(&encrypted))?;
        Ok(())
    }

    /// Retrieve and decrypt
    pub fn get(&self, repo: &str) -> Result<Option<Credentials>> {
        let entry = keyring::Entry::new("sherpack", repo)?;
        match entry.get_password() {
            Ok(encrypted) => {
                let decrypted = self.key.decrypt(&base64::decode(&encrypted)?)?;
                Ok(Some(serde_json::from_slice(&decrypted)?))
            }
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }
}

/// Credential types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Credentials {
    /// Basic auth (username/password)
    Basic { username: String, password: String },

    /// Bearer token (for private repos)
    Bearer { token: String },

    /// OAuth2 with refresh (for GCP, Azure, etc.)
    OAuth2 {
        access_token: String,
        refresh_token: Option<String>,
        expires_at: Option<DateTime<Utc>>,
        token_url: String,
    },

    /// Docker config.json reference
    DockerConfig { path: PathBuf },

    /// Environment variable reference (no storage)
    EnvVar { username_var: String, password_var: String },
}
```

**CLI Behavior**:
```bash
# Store credentials securely (uses system keyring)
sherpack repo add private https://charts.private.com \
  --username admin \
  --password-stdin
# Enter password: ********
# Credentials stored in system keychain

# Or use bearer token
sherpack repo add private https://charts.private.com \
  --token-stdin
# Enter token: ********

# Or use environment variables (CI/CD friendly)
sherpack repo add private https://charts.private.com \
  --username-env PRIVATE_USER \
  --password-env PRIVATE_PASS
# Credentials will be read from environment at runtime

# Check credential status
sherpack repo auth status
# bitnami: public (no auth)
# private: bearer token (expires in 45 minutes)
# gcr: oauth2 (auto-refresh enabled)
```

#### 4.2 Scoped Credentials (No Leak Risk)

```rust
/// Credentials are scoped to specific repositories
pub struct ScopedCredentials {
    /// Map of repo URL prefix -> credentials
    scopes: HashMap<String, Credentials>,
}

impl ScopedCredentials {
    /// Get credentials for a URL, matching by longest prefix
    pub fn for_url(&self, url: &str) -> Option<&Credentials> {
        self.scopes
            .iter()
            .filter(|(prefix, _)| url.starts_with(*prefix))
            .max_by_key(|(prefix, _)| prefix.len())
            .map(|(_, creds)| creds)
    }
}

impl HttpClient {
    /// Only add auth headers if credentials match URL scope
    async fn fetch(&self, url: &str) -> Result<Response> {
        let mut request = Request::new(Method::GET, url.parse()?);

        // Only add auth if we have matching credentials
        if let Some(creds) = self.credentials.for_url(url) {
            match creds {
                Credentials::Basic { username, password } => {
                    request.headers_mut().insert(
                        AUTHORIZATION,
                        format!("Basic {}", base64::encode(format!("{}:{}", username, password))).parse()?,
                    );
                }
                Credentials::Bearer { token } => {
                    request.headers_mut().insert(
                        AUTHORIZATION,
                        format!("Bearer {}", token).parse()?,
                    );
                }
                // ... other types
            }
        }
        // No credentials = public repo, no auth header sent

        self.client.execute(request).await
    }
}
```

---

## 5. Search & Discovery Frustrations

### Problems Identified

| Issue | Source | Severity |
|-------|--------|----------|
| **Artifact Hub is external** | Not integrated in Helm | Medium |
| **OCI registries not searchable** | No standard API | High |
| **Monocular legacy API** | Limited query support | Medium |
| **China accessibility** | gcr.io blocked | Medium |

**Source**: [Helm Blog - Hub Moving to Artifact Hub](https://helm.sh/blog/helm-hub-moving-to-artifact-hub/)

### Sherpack Solutions

#### 5.1 Unified Local Search

```rust
/// Unified search across all configured repositories
pub struct UnifiedSearch {
    repos: Vec<Repository>,
    /// Local search index (SQLite FTS)
    index: SearchIndex,
}

impl UnifiedSearch {
    /// Search all repos with ranking
    pub async fn search(&self, query: &str) -> Result<Vec<SearchResult>> {
        let mut results = Vec::new();

        // Search in parallel across all repos
        let searches = self.repos
            .iter()
            .map(|repo| self.search_repo(repo, query));

        let repo_results = futures::future::join_all(searches).await;

        for (repo, result) in self.repos.iter().zip(repo_results) {
            if let Ok(packs) = result {
                for pack in packs {
                    results.push(SearchResult {
                        pack,
                        repo: repo.name.clone(),
                        score: self.calculate_score(&pack, query),
                    });
                }
            }
        }

        // Sort by relevance score
        results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());

        Ok(results)
    }

    fn calculate_score(&self, pack: &PackVersion, query: &str) -> f64 {
        let mut score = 0.0;

        // Exact name match
        if pack.name == query {
            score += 100.0;
        } else if pack.name.contains(query) {
            score += 50.0;
        }

        // Keyword matches
        for keyword in &pack.keywords {
            if keyword.contains(query) {
                score += 10.0;
            }
        }

        // Description match
        if let Some(desc) = &pack.description {
            if desc.to_lowercase().contains(&query.to_lowercase()) {
                score += 5.0;
            }
        }

        // Popularity boost (download count if available)
        if let Some(downloads) = pack.downloads {
            score += (downloads as f64).log10();
        }

        score
    }
}
```

**CLI Behavior**:
```bash
sherpack search nginx
# NAME                REPO      VERSION   DESCRIPTION
# bitnami/nginx       bitnami   15.0.0    NGINX Open Source (score: 150)
# myrepo/nginx-proxy  myrepo    2.1.0     Nginx reverse proxy (score: 85)
# stable/nginx-ing... stable    4.0.0     NGINX Ingress Controller (score: 72)

# Search with filters
sherpack search nginx --repo bitnami --min-version 14.0

# Search in OCI registries too!
sherpack search --repo myoci nginx
# myoci/nginx         1.0.0     Nginx from OCI registry
```

#### 5.2 Offline Search Index

```rust
/// Maintain local search index for offline use
pub struct LocalSearchIndex {
    db: rusqlite::Connection,
}

impl LocalSearchIndex {
    /// Create FTS5 virtual table for fast searching
    pub fn init(&self) -> Result<()> {
        self.db.execute_batch(r#"
            CREATE VIRTUAL TABLE IF NOT EXISTS packs_fts USING fts5(
                name,
                description,
                keywords,
                repo,
                content='packs',
                content_rowid='id'
            );

            CREATE TABLE IF NOT EXISTS packs (
                id INTEGER PRIMARY KEY,
                name TEXT NOT NULL,
                version TEXT NOT NULL,
                description TEXT,
                keywords TEXT,
                repo TEXT NOT NULL,
                updated_at INTEGER,
                UNIQUE(name, version, repo)
            );
        "#)?;
        Ok(())
    }

    /// Full-text search
    pub fn search(&self, query: &str) -> Result<Vec<PackInfo>> {
        let mut stmt = self.db.prepare(r#"
            SELECT p.* FROM packs p
            JOIN packs_fts fts ON p.id = fts.rowid
            WHERE packs_fts MATCH ?
            ORDER BY rank
            LIMIT 50
        "#)?;

        stmt.query_map([query], |row| {
            Ok(PackInfo { /* ... */ })
        })?.collect()
    }
}
```

---

## 6. Sherpack Solutions Summary

### Comparison Table

| Problem | Helm Behavior | Sherpack Solution |
|---------|---------------|-------------------|
| Giant index.yaml | Load entire file (1GB+ RAM) | **Sharded index + lazy loading** |
| Slow dep update | Parse index multiple times | **SQLite cache + incremental updates** |
| OCI `repo add` | Doesn't work, confusing | **Unified interface for all backends** |
| OCI search | Not possible | **Catalog API + local index** |
| Auth errors | Manual retry | **Auto token refresh + exponential backoff** |
| Diamond deps | Silent conflict | **Detection + resolution strategies** |
| Template conflicts | Subcharts override | **Isolated template environments** |
| Lock file drift | Stores range, not version | **Exact version + SHA256 integrity** |
| Credential leak | Creds sent everywhere | **Scoped credentials by URL prefix** |
| Plaintext creds | In config file | **System keyring + encryption** |

### New CLI Commands

```bash
# Repository management (works for HTTP AND OCI)
sherpack repo add NAME URL [--username X] [--password-stdin] [--token-stdin]
sherpack repo list [--show-auth]
sherpack repo update [NAME] [--parallel]
sherpack repo remove NAME

# Search (unified, works everywhere)
sherpack search QUERY [--repo NAME] [--version RANGE] [--json]

# Dependencies (with conflict detection)
sherpack dependency list
sherpack dependency update [--strategy strict|highest|lowest]
sherpack dependency build [--verify]
sherpack dependency tree

# Authentication management
sherpack repo auth status
sherpack repo auth refresh NAME
sherpack repo auth logout NAME
```

### Error Messages (User-Friendly)

```
ERROR: Diamond dependency conflict detected!

  postgresql required at multiple versions:
    - 12.2.1 (required by: keycloak 18.0.0)
    - 10.9.3 (required by: airflow 2.5.0)

  Dependency graph:
    myapp
    ├── keycloak 18.0.0
    │   └── postgresql 12.2.1 ←─┐ CONFLICT
    └── airflow 2.5.0           │
        └── postgresql 10.9.3 ←─┘

  Solutions:
    1. Add to Pack.yaml:  postgresql: "^12.0.0"  (pin to 12.x)
    2. Use:  sherpack dependency update --strategy=highest
    3. Use aliases to install both versions

  See: https://sherpack.io/docs/dependencies#conflicts
```

---

## Implementation Priority

### Phase 5.1 - Core Repository (2-3 weeks)
1. ✅ Unified `RepositoryBackend` trait
2. ✅ HTTP repository with sharded index support
3. ✅ SQLite cache for fast queries
4. ✅ Basic `repo add/list/update/remove` commands

### Phase 5.2 - OCI Support (2-3 weeks)
1. ✅ OCI client with catalog API
2. ✅ Unified auth (auto-refresh, retry)
3. ✅ `push`/`pull` commands
4. ✅ Search in OCI registries

### Phase 5.3 - Dependencies (3-4 weeks)
1. ✅ True lock file with integrity
2. ✅ Diamond detection + resolution strategies
3. ✅ Template isolation for subcharts
4. ✅ Alias support

### Phase 5.4 - Polish (1-2 weeks)
1. ✅ Credential store with encryption
2. ✅ Offline search index
3. ✅ User-friendly error messages
4. ✅ Documentation

---

## References

- [Helm Issue #3557 - Repository v2 Proposal](https://github.com/helm/helm/issues/3557)
- [Helm Issue #9865 - Slow dep update](https://github.com/helm/helm/issues/9865)
- [Helm Issue #10565 - OCI repo add](https://github.com/helm/helm/issues/10565)
- [Helm Issue #11933 - Template conflicts](https://github.com/helm/helm/issues/11933)
- [Helm Issue #30710 - Conflicting subcharts](https://github.com/helm/helm/issues/30710)
- [Helm Issue #7451 - Bearer token auth](https://github.com/helm/helm/issues/7451)
- [Docker Hub OCI Issues](https://forums.docker.com/t/docker-hub-oci-registry-for-helm-charts-is-unstable-and-returns-401-errors/138043)
- [Helm Blog - Artifact Hub Migration](https://helm.sh/blog/helm-hub-moving-to-artifact-hub/)
