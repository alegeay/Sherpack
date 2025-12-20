# sherpack-repo

Repository management for Sherpack - HTTP repos, OCI registries, and dependency resolution.

## Overview

`sherpack-repo` provides complete repository management for Sherpack packages. It supports HTTP repositories (Helm-compatible), OCI registries, and local file repositories with a unified interface. It also includes dependency resolution with diamond conflict detection and lock file management for reproducible builds.

## Features

- **Multiple Backends** - HTTP, OCI, and File repositories
- **Unified Interface** - Same API for all repository types
- **SQLite Cache** - Fast local search with FTS5
- **Secure Credentials** - Scoped credentials with redirect protection
- **Lock Files** - Reproducible builds with integrity verification
- **Dependency Resolution** - Diamond conflict detection
- **Conditional Dependencies** - Filter based on values

## Quick Start

```rust
use sherpack_repo::{Repository, RepositoryConfig, create_backend};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Add a repository
    let repo = Repository::new("bitnami", "https://charts.bitnami.com/bitnami")?;

    // Create backend (auto-detects type)
    let mut backend = create_backend(repo, None).await?;

    // Search for packs
    let results = backend.search("nginx").await?;
    for pack in results {
        println!("{} v{}", pack.name, pack.version);
    }

    // Download a pack
    let data = backend.download("nginx", "15.0.0").await?;
    std::fs::write("nginx-15.0.0.tgz", &data)?;

    Ok(())
}
```

## Repository Backends

### HTTP Repository

Traditional Helm-style repository with `index.yaml`:

```rust
use sherpack_repo::{HttpRepository, Repository};

let repo = Repository::new("bitnami", "https://charts.bitnami.com/bitnami")?;
let mut backend = HttpRepository::new(repo, None).await?;

// Update index
backend.refresh().await?;

// Search
let results = backend.search("redis").await?;

// Get specific version
let entry = backend.find_best_match("redis", "^17.0.0").await?;

// Download
let data = backend.download("redis", "17.0.0").await?;
```

**Features:**
- ETag caching for efficient updates
- Automatic gzip decompression
- Credential support (Basic, Bearer)

### OCI Registry

Push and pull from OCI-compliant registries (Docker Hub, GHCR, ECR, etc.):

```rust
use sherpack_repo::{OciRegistry, OciReference};

// Parse OCI reference
let reference = OciReference::parse("ghcr.io/myorg/mypack:1.0.0")?;

let registry = OciRegistry::new();

// Pull
let data = registry.pull(&reference, Some(&credentials)).await?;

// Push
registry.push(&reference, &pack_data, Some(&credentials)).await?;
```

**OCI Reference Formats:**
```
ghcr.io/myorg/mypack:1.0.0           # With tag
ghcr.io/myorg/mypack@sha256:abc123   # With digest
docker.io/library/nginx:latest       # Docker Hub
```

### File Repository

Local file-based repository for development:

```rust
use sherpack_repo::backend::FileRepository;

let repo = Repository::new("local", "file:///path/to/repo")?;
let backend = FileRepository::new(repo)?;

// Works like other backends
let results = backend.search("mypack").await?;
```

### Backend Factory

Automatically create the right backend based on URL:

```rust
use sherpack_repo::{create_backend, create_backend_by_name, RepositoryConfig};

// From repository object
let backend = create_backend(repo, credentials).await?;

// By name from config
let config = RepositoryConfig::load()?;
let backend = create_backend_by_name(&config, "bitnami", credentials).await?;
```

## Repository Configuration

### Config File

Located at `~/.config/sherpack/repositories.yaml`:

```yaml
repositories:
  - name: bitnami
    url: https://charts.bitnami.com/bitnami
    type: http

  - name: myorg
    url: oci://ghcr.io/myorg/charts
    type: oci

  - name: local
    url: file:///home/user/charts
    type: file
```

### API

```rust
use sherpack_repo::{RepositoryConfig, Repository, RepositoryType};

// Load configuration
let mut config = RepositoryConfig::load()?;

// Add repository
config.add(Repository {
    name: "custom".to_string(),
    url: "https://charts.example.com".to_string(),
    repo_type: RepositoryType::Http,
})?;

// Remove repository
config.remove("custom")?;

// Save
config.save()?;

// List all
for repo in &config.repositories {
    println!("{}: {} ({})", repo.name, repo.url, repo.repo_type);
}
```

## Credentials

### Secure Credential Store

```rust
use sherpack_repo::{CredentialStore, Credentials, ScopedCredentials};

let mut store = CredentialStore::load()?;

// Add credentials
store.add("ghcr", Credentials::Basic {
    username: "user".to_string(),
    password: "token".to_string(),
});

// Or bearer token
store.add("myrepo", Credentials::Bearer {
    token: "my-token".to_string(),
});

store.save()?;

// Get credentials for a repository
if let Some(creds) = store.get("ghcr") {
    let resolved = creds.resolve()?;
}
```

### Scoped Credentials

Credentials are scoped to prevent leakage on redirects:

```rust
use sherpack_repo::{SecureHttpClient, ScopedCredentials};

let client = SecureHttpClient::new();

// Credentials only sent to matching host
let scoped = ScopedCredentials::new(
    "ghcr.io",
    Credentials::Bearer { token: "...".to_string() }
);

// If server redirects to different host, credentials are NOT sent
let response = client.get_with_credentials(url, &scoped).await?;
```

## Index Cache

### SQLite with FTS5

Fast local search using full-text indexing:

```rust
use sherpack_repo::{IndexCache, CacheStats};

let cache = IndexCache::open("~/.cache/sherpack/index.db")?;

// Add repository index to cache
cache.add_repository("bitnami", &index).await?;

// Search across all repositories
let results = cache.search("nginx web server").await?;

// Get latest versions
let latest = cache.list_latest("bitnami").await?;

// Cache statistics
let stats = cache.stats()?;
println!("Repositories: {}", stats.repository_count);
println!("Packages: {}", stats.pack_count);
println!("Cache size: {} KB", stats.size_kb);
```

### Cache Management

```rust
// Update single repository
cache.update_repository("bitnami", &new_index).await?;

// Remove repository from cache
cache.remove_repository("bitnami").await?;

// Clear entire cache
cache.clear().await?;
```

## Dependency Resolution

### Basic Resolution

```rust
use sherpack_repo::{DependencyResolver, DependencySpec, DependencyGraph};

// Define dependencies
let deps = vec![
    DependencySpec {
        name: "redis".to_string(),
        version: "^17.0.0".to_string(),
        repository: "https://charts.bitnami.com/bitnami".to_string(),
        condition: None,
        tags: vec![],
        alias: None,
    },
    DependencySpec {
        name: "postgresql".to_string(),
        version: "^12.0.0".to_string(),
        repository: "https://charts.bitnami.com/bitnami".to_string(),
        condition: Some("postgresql.enabled".to_string()),
        tags: vec![],
        alias: Some("db".to_string()),
    },
];

// Create resolver with fetch function
let resolver = DependencyResolver::new(|repo_url, name, version| {
    // Fetch pack entry from repository
    fetch_from_repo(repo_url, name, version)
});

// Resolve all dependencies (including transitive)
let graph = resolver.resolve(&deps)?;

println!("Resolved {} dependencies:", graph.len());
for dep in graph.iter() {
    println!("  {} @ {}", dep.name, dep.version);
}
```

### Diamond Conflict Detection

Sherpack does NOT silently resolve version conflicts:

```rust
// If app1 requires redis@17.0.0 and app2 requires redis@16.0.0
let result = resolver.resolve(&deps);

match result {
    Err(RepoError::DiamondConflict { conflicts }) => {
        println!("{}", conflicts);
        // Diamond dependency conflict for 'redis':
        //
        //   Version 17.0.0 required by: app1
        //   Version 16.0.0 required by: app2
        //
        // Solutions:
        //   1. Pin a specific version in your Pack.yaml
        //   2. Use aliases to install both versions
        //   3. Update the conflicting dependency
    }
    Ok(graph) => { /* Success */ }
    Err(e) => { /* Other error */ }
}
```

### Dependency Filtering

Filter dependencies before resolution (for air-gapped environments):

```rust
use sherpack_repo::{filter_dependencies, FilterResult, SkipReason};
use sherpack_core::Dependency;

// Dependencies from Pack.yaml
let deps: Vec<Dependency> = pack.dependencies.clone();

// Values from values.yaml
let values = serde_json::json!({
    "redis": { "enabled": true },
    "postgresql": { "enabled": false }
});

// Filter based on enabled/resolve/condition
let result = filter_dependencies(&deps, &values);

// Dependencies to actually resolve
for spec in &result.to_resolve {
    println!("Will resolve: {}", spec.name);
}

// Skipped dependencies (won't be downloaded)
for skipped in &result.skipped {
    match &skipped.reason {
        SkipReason::StaticDisabled => {
            println!("{}: disabled in Pack.yaml", skipped.dependency.name);
        }
        SkipReason::PolicyNever => {
            println!("{}: resolve: never", skipped.dependency.name);
        }
        SkipReason::ConditionFalse { condition } => {
            println!("{}: {} is false", skipped.dependency.name, condition);
        }
    }
}
```

### Resolve Policies

```yaml
# Pack.yaml
dependencies:
  - name: redis
    version: "^17.0.0"
    repository: https://charts.bitnami.com/bitnami
    resolve: always      # Always resolve, ignore condition

  - name: postgresql
    version: "^12.0.0"
    repository: https://charts.bitnami.com/bitnami
    condition: db.enabled
    resolve: when-enabled  # (default) Respect condition

  - name: monitoring
    version: "^1.0.0"
    repository: https://example.com
    enabled: false        # Static disable
    resolve: never        # Never resolve (manual install)
```

### Dependency Graph

```rust
let graph = resolver.resolve(&deps)?;

// Topological sort for install order
let order = graph.install_order();
for dep in order {
    println!("Install: {} (required by: {})",
        dep.name,
        dep.required_by.join(", ")
    );
}

// Render as tree
println!("{}", graph.render_tree());
// └── my-app@1.0.0
//     ├── redis@17.0.0
//     └── postgresql@12.0.0
//         └── common@2.0.0
```

## Lock Files

### Overview

Lock files ensure reproducible builds by pinning exact versions:

```yaml
# Pack.lock.yaml
sherpack-lock-version: "1"
pack-yaml-hash: sha256:abc123...
policy: strict  # or: version, semver-patch, semver-minor

dependencies:
  - name: redis
    version: 17.0.0
    repository: https://charts.bitnami.com/bitnami
    constraint: "^17.0.0"
    digest: sha256:def456...
    alias: null
    dependencies:
      - common

  - name: common
    version: 2.0.0
    repository: https://charts.bitnami.com/bitnami
    constraint: "^2.0.0"
    digest: sha256:789abc...
```

### Lock Policies

| Policy | Description |
|--------|-------------|
| `strict` | Exact version and digest must match |
| `version` | Version must match, digest can differ |
| `semver-patch` | Allow patch updates (1.0.x) |
| `semver-minor` | Allow minor updates (1.x.x) |

### API

```rust
use sherpack_repo::{LockFile, LockedDependency, LockPolicy, VerifyResult};

// Create from resolved graph
let lock = graph.to_lock_file(&pack_yaml_content);

// Save
lock.save("Pack.lock.yaml")?;

// Load
let lock = LockFile::load("Pack.lock.yaml")?;

// Check if outdated
if lock.is_outdated(&current_pack_yaml) {
    println!("Lock file is outdated, run 'sherpack dependency update'");
}

// Verify downloaded package
match lock.verify("redis", &downloaded_data) {
    Ok(VerifyResult::Match) => println!("Integrity verified"),
    Ok(VerifyResult::DigestChanged { expected, actual }) => {
        println!("WARNING: Digest changed!");
        println!("  Expected: {}", expected);
        println!("  Actual: {}", actual);
    }
    Err(e) => println!("Verification failed: {}", e),
}
```

## Repository Index

### Index Format

HTTP repositories use `index.yaml`:

```yaml
apiVersion: v1
entries:
  nginx:
    - name: nginx
      version: 15.0.0
      appVersion: "1.25.0"
      description: NGINX web server
      home: https://nginx.org
      urls:
        - https://charts.bitnami.com/bitnami/nginx-15.0.0.tgz
      digest: sha256:abc123...
      created: "2024-01-15T10:00:00Z"
      deprecated: false
      dependencies:
        - name: common
          version: "^2.0.0"
          condition: common.enabled
```

### API

```rust
use sherpack_repo::{RepositoryIndex, PackEntry};

// Parse index
let index = RepositoryIndex::from_yaml(&yaml_content)?;

// Get all versions of a pack
let versions = index.get_all_versions("nginx");

// Get latest version
let latest = index.get_latest("nginx");

// Search by keyword
let results = index.search("web server");

// Semver matching
let entry = index.find_best_match("nginx", "^15.0.0")?;
```

## Error Handling

```rust
use sherpack_repo::{RepoError, Result};

match operation() {
    Err(RepoError::PackNotFound { name, repo }) => {
        println!("Pack '{}' not found in '{}'", name, repo);
    }
    Err(RepoError::VersionNotFound { name, version, available }) => {
        println!("Version {} not found for {}", version, name);
        println!("Available versions: {}", available.join(", "));
    }
    Err(RepoError::DiamondConflict { conflicts }) => {
        println!("{}", conflicts);
    }
    Err(RepoError::NetworkError(e)) => {
        println!("Network error: {}", e);
    }
    Err(RepoError::RegistryError(msg)) => {
        println!("OCI registry error: {}", msg);
    }
    Err(e) => println!("Error: {}", e),
    Ok(_) => {}
}
```

## Complete Example

```rust
use sherpack_repo::{
    RepositoryConfig, CredentialStore, create_backend,
    DependencyResolver, LockFile, filter_dependencies,
};
use sherpack_core::LoadedPack;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Load pack
    let pack = LoadedPack::load("./my-pack")?;
    let values = sherpack_core::Values::from_file(&pack.values_path)?;

    // Filter dependencies
    let filter_result = filter_dependencies(
        &pack.pack.dependencies,
        &values.into_inner()
    );

    if filter_result.has_skipped() {
        println!("Skipping:");
        println!("{}", filter_result.skipped_summary());
    }

    // Load repository config and credentials
    let config = RepositoryConfig::load()?;
    let creds = CredentialStore::load()?;

    // Create resolver
    let resolver = DependencyResolver::new(|repo_url, name, version| {
        // Async block for fetching
        tokio::runtime::Handle::current().block_on(async {
            let repo = config.find_by_url(repo_url)?;
            let credentials = creds.get(&repo.name);
            let mut backend = create_backend(repo.clone(), credentials).await?;
            backend.find_best_match(name, version).await
        })
    });

    // Resolve
    let graph = resolver.resolve(&filter_result.to_resolve)?;

    println!("Dependency tree:");
    println!("{}", graph.render_tree());

    // Create lock file
    let pack_yaml = std::fs::read_to_string("Pack.yaml")?;
    let lock = graph.to_lock_file(&pack_yaml);
    lock.save("Pack.lock.yaml")?;

    // Download dependencies
    for dep in graph.install_order() {
        let repo = config.find_by_url(&dep.repository)?;
        let credentials = creds.get(&repo.name);
        let backend = create_backend(repo.clone(), credentials).await?;

        let data = backend.download(&dep.name, &dep.version.to_string()).await?;

        // Verify integrity
        if let Ok(VerifyResult::Match) = lock.verify(&dep.name, &data) {
            std::fs::write(format!("packs/{}.tgz", dep.name), &data)?;
            println!("Downloaded: {} @ {}", dep.name, dep.version);
        }
    }

    Ok(())
}
```

## Dependencies

- `reqwest` - HTTP client with TLS
- `oci-distribution` - OCI registry client
- `rusqlite` - SQLite with FTS5
- `sherpack-core` - Core types
- `semver` - Version parsing/matching
- `sha2` - SHA256 verification
- `tokio` - Async runtime

## License

MIT OR Apache-2.0
