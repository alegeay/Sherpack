# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Sherpack is a Kubernetes package manager written in Rust that uses Jinja2 templating (via MiniJinja) instead of Go templates. It's designed as a simpler, faster alternative to Helm with full lifecycle management.

## Build Commands

```bash
# Build (debug)
cargo build --workspace

# Build (release)
cargo build --release

# Run tests (282 tests)
cargo test --workspace

# Run tests for specific crate
cargo test -p sherpack-core
cargo test -p sherpack-engine
cargo test -p sherpack-kube
cargo test -p sherpack-repo
cargo test -p sherpack

# Run a single test
cargo test --workspace test_name

# Lint
cargo clippy --workspace

# Format
cargo fmt --all

# Run CLI directly
cargo run -p sherpack -- <command>

# Examples
cargo run -p sherpack -- template my-release fixtures/demo-pack
cargo run -p sherpack -- lint fixtures/demo-pack
cargo run -p sherpack -- validate fixtures/demo-pack
cargo run -p sherpack -- package fixtures/demo-pack
```

## Architecture

The project is a Cargo workspace with five crates:

### `sherpack-core`
Core types and data structures (19 tests):
- `Pack` / `LoadedPack` - Package definition loaded from `Pack.yaml`
- `Values` - Configuration values with deep merge support
- `Release` - Deployment state (name, namespace, revision)
- `TemplateContext` - Combined context passed to templates
- `Archive` - Create/extract tar.gz archives with MANIFEST
- `Manifest` - SHA256 checksums for integrity verification
- `Schema` - JSON Schema validation and default extraction

### `sherpack-engine`
MiniJinja-based template engine (43 tests):
- `Engine` / `EngineBuilder` - Template compilation and rendering
- `filters.rs` - 25+ Helm-compatible filters: `toyaml`, `tojson`, `b64encode`, `indent`, `nindent`, `quote`, `kebabcase`, `sha256`, etc.
- `functions.rs` - Template functions: `get()`, `ternary()`, `now()`, `uuidv4()`, `fail()`, `tostring()`, `toint()`, `tofloat()`
- `suggestions.rs` - Contextual error suggestions with fuzzy matching

### `sherpack-kube`
Kubernetes integration (107 tests):
- `client.rs` - `KubeClient<S>` with install/upgrade/uninstall/rollback/list/history/status/recover
- `resources.rs` - `ResourceManager` with Server-Side Apply, Discovery, creation ordering
- `storage/` - `StorageDriver` trait with Secrets, ConfigMap, File, Mock backends
- `hooks.rs` - 11 hook phases (pre/post-install/upgrade/rollback/delete, test)
- `health.rs` - Health checks for Deployments/StatefulSets, HTTP/command probes
- `diff.rs` - Three-way merge diff visualization
- `actions.rs` - Options structs with builder pattern
- `release.rs` - `StoredRelease`, `ReleaseState`, auto-recovery

### `sherpack-repo`
Repository and dependency management (42 tests):
- `backend.rs` - `RepositoryBackend` trait with HTTP, OCI, File backends
- `http.rs` - HTTP repository with ETag caching
- `oci.rs` - OCI registry client using `oci-distribution`
- `cache.rs` - SQLite FTS5 search index with WAL mode
- `config.rs` - Repository configuration management
- `credentials.rs` - Secure credential handling with cross-origin protection
- `dependency.rs` - Dependency resolver with diamond conflict detection
- `lock.rs` - `Pack.lock.yaml` with version policies (Strict, Version, SemverPatch, SemverMinor)
- `index.rs` - Repository index parsing and semver matching

### `sherpack-cli`
CLI application using Clap (71 tests):

**Templating:**
- `template` - Render templates to stdout or files
- `lint` - Validate pack structure and templates
- `validate` - Validate values against JSON Schema
- `show` - Display pack information
- `create` - Scaffold new pack

**Packaging:**
- `package` - Create tar.gz archive with manifest
- `inspect` - Show archive contents
- `keygen` - Generate Minisign keypair
- `sign` - Sign archive with private key
- `verify` - Verify integrity and signature

**Kubernetes:**
- `install` - Install pack to cluster
- `upgrade` - Upgrade existing release
- `uninstall` - Remove release
- `rollback` - Rollback to previous revision
- `list` - List installed releases
- `history` - Show release history
- `status` - Show release status
- `recover` - Recover stale release

**Repository:**
- `repo add/list/update/remove` - Manage repositories
- `search` - Search for packs
- `pull` - Download pack from repository
- `push` - Push to OCI registry

**Dependencies:**
- `dependency list/update/build/tree` - Manage dependencies

## Pack Structure

A pack (package) contains:
```
mypack/
├── Pack.yaml           # Required: metadata (name, version, description)
├── values.yaml         # Required: default configuration values
├── values.schema.yaml  # Optional: JSON Schema for validation
├── Pack.lock.yaml      # Generated: locked dependencies
├── packs/              # Downloaded dependencies
└── templates/          # Required: Jinja2 template files
    ├── deployment.yaml
    ├── service.yaml
    └── _helpers.tpl    # Optional: shared helpers
```

## Template Context

Templates receive these variables:
- `values.*` - Merged values from values.yaml and overrides
- `release.name` - Release name from CLI
- `release.namespace` - Target namespace
- `pack.name` / `pack.version` - From Pack.yaml
- `capabilities.kubeVersion` - Kubernetes version

## Key Patterns

### Error Handling
- All crates use `thiserror` for error types
- CLI uses `miette` for beautiful error reporting
- Errors include contextual help messages

### Async
- Kubernetes operations are async (tokio)
- `StorageDriver` trait uses `async_trait`
- CLI wraps async with `Runtime::new()?.block_on()`

### Testing
- Unit tests inline with `#[cfg(test)] mod tests`
- Integration tests in `tests/integration_tests.rs`
- Snapshot tests using `insta` crate
- `MockStorageDriver` for testing without K8s cluster

## Testing

Test fixtures are in `fixtures/`:
- `fixtures/simple-pack/` - Basic test fixture
- `fixtures/demo-pack/` - Comprehensive demo with schema

```bash
# Run all tests (282 total)
cargo test --workspace

# Run specific crate tests
cargo test -p sherpack-kube

# Run integration tests only
cargo test -p sherpack --test integration_tests

# Run with output
cargo test --workspace -- --nocapture
```

## Dependencies

Key dependencies:
- `minijinja` - Jinja2 template engine
- `kube` / `k8s-openapi` - Kubernetes client
- `oci-distribution` - OCI registry client
- `rusqlite` - SQLite with FTS5 for search
- `minisign` - Cryptographic signatures
- `clap` - CLI parsing
- `serde` / `serde_yaml` / `serde_json` - Serialization
- `tokio` - Async runtime
- `reqwest` - HTTP client
