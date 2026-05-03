# Project Index: Sherpack

Generated: 2026-05-03 · Version: 0.3.0 · License: Apache-2.0

A Kubernetes package manager written in Rust with Jinja2 templating (MiniJinja).
Modern Helm alternative: faster binary, familiar syntax, smarter CRD handling.

## Project Structure

```
Sherpack/
├── Cargo.toml              # Workspace manifest (6 members)
├── crates/                 # 6 Rust crates (~78k LOC, 685 tests)
│   ├── sherpack-core/      # Pack, Values, Release, Schema, Manifest
│   ├── sherpack-engine/    # MiniJinja templating, filters, functions
│   ├── sherpack-kube/      # K8s client, hooks, health, diff, CRD, storage
│   ├── sherpack-repo/      # HTTP/OCI/File backends, deps, lock, cache
│   ├── sherpack-convert/   # Helm chart → Sherpack (pest PEG parser)
│   └── sherpack-cli/       # Clap-based CLI binary (sherpack)
├── docs/                   # 22 design + reference markdown files
├── fixtures/               # Test packs (simple, demo, helm-nginx, subcharts)
├── website/                # Docusaurus site (architecture, CLI ref, tutorials)
└── .github/workflows/      # ci.yml, release.yml, deploy-docs.yml, security.yml
```

## Entry Points

- **CLI binary**: `crates/sherpack-cli/src/main.rs` — Clap subcommand dispatcher
- **Workspace**: `Cargo.toml` — Edition 2024, Rust 1.88+, resolver 2
- **Library roots**: `crates/sherpack-{core,engine,kube,repo,convert}/src/lib.rs`
- **Integration tests**: `crates/sherpack-cli/tests/{integration_tests,snapshot_tests}.rs`

## Core Modules

### sherpack-core (`crates/sherpack-core/src/`)
Foundational types. Re-exported from `lib.rs`.
- `pack.rs` — `Pack`, `LoadedPack`, `Dependency`, `CrdConfig`, `ResolvePolicy`
- `values.rs` — `Values` with deep-merge
- `release.rs` — `Release`, `ReleaseInfo`, `ReleaseStatus`
- `schema.rs` — `Schema`, `SchemaValidator`, `SherpSchema`, `ValidationResult`
- `manifest.rs` — SHA256 manifests, `VerificationResult`
- `archive.rs` — tar.gz create/extract, `verify_archive`
- `secrets.rs` — `SecretGenerator`, `SecretCharset`, `SecretState` (idempotent)
- `files.rs` — `FileProvider`, `SandboxedFileProvider`, `MockFileProvider`
- `context.rs` — `TemplateContext` (values + release + pack + capabilities)

### sherpack-engine (`crates/sherpack-engine/src/`)
MiniJinja-based template rendering.
- `engine.rs` — `Engine`, `EngineBuilder`
- `filters.rs` — 25+ Helm-compatible filters (toyaml, b64encode, nindent, sha256, …)
- `functions.rs` — `get`, `ternary`, `now`, `uuidv4`, `fail`, `tostring`, `generate_secret`
- `secrets.rs` — Idempotent secret generation
- `pack_renderer.rs` — End-to-end pack rendering pipeline
- `subchart.rs` — Subchart rendering with global values
- `files_object.rs` — `.Files.Get`, `.Files.Glob` template object
- `suggestions.rs` — Fuzzy-match contextual error hints

### sherpack-kube (`crates/sherpack-kube/src/`)
Kubernetes lifecycle management (async).
- `client.rs` — `KubeClient<S>` install/upgrade/uninstall/rollback/list/history/status/recover
- `resources.rs` — `ResourceManager`, Server-Side Apply, discovery, ordering
- `storage/` — `StorageDriver` trait + `secrets`, `configmap`, `file`, `mock`, `chunked` backends
- `hooks.rs` — 11 phases (pre/post-install/upgrade/rollback/delete, test)
- `health.rs` — Deployment/StatefulSet readiness, HTTP/command probes
- `diff.rs` — Three-way merge diff visualization
- `crd/` — `analyzer`, `apply`, `detection`, `parser`, `policy`, `protection`, `schema`, `strategy`
- `actions.rs` — Builder-pattern options structs
- `release.rs` — `StoredRelease`, `ReleaseState`, auto-recovery
- `annotations.rs`, `progress.rs`, `waves.rs`

### sherpack-repo (`crates/sherpack-repo/src/`)
Repository, dependency, and lock management.
- `backend.rs` — `RepositoryBackend` trait
- `http.rs` — HTTP repo with ETag caching
- `oci.rs` — OCI registry via `oci-distribution`
- `cache.rs` — SQLite FTS5 search index (WAL mode)
- `dependency.rs` — Resolver + diamond conflict detection
- `lock.rs` — `Pack.lock.yaml`, version policies (Strict, Version, SemverPatch, SemverMinor)
- `index.rs` — Index parsing + semver matching
- `credentials.rs` — Cross-origin-protected credential handling
- `config.rs` — Repository configuration

### sherpack-convert (`crates/sherpack-convert/src/`)
Helm chart → Sherpack pack converter.
- `parser.rs` + `go_template.pest` — Pest PEG parser for Go templates
- `ast.rs` — Go template AST nodes
- `transformer.rs` — AST → Jinja2 transformer
- `macro_processor.rs` — Three-pass `define`/`include` handling
- `converter.rs` — Full chart conversion orchestrator
- `chart.rs` — Helm Chart.yaml ingestion
- `type_inference.rs` — Type hints for converted values

### sherpack-cli (`crates/sherpack-cli/src/`)
Clap CLI. 26 subcommands in `commands/`:
- **Templating**: `template`, `lint`, `validate`, `show`, `create`, `convert`
- **Packaging**: `package`, `inspect`, `keygen`, `sign`, `verify`, `signing`
- **Kubernetes**: `install`, `upgrade`, `uninstall`, `rollback`, `list`, `history`, `status`, `recover`
- **Repository**: `repo`, `search`, `pull`, `push`
- **Dependencies**: `dep` (list/update/build/tree)

## Pack Structure

```
mypack/
├── Pack.yaml           # Required: name, version, description
├── values.yaml         # Required: default config
├── values.schema.yaml  # Optional: JSON Schema validation
├── Pack.lock.yaml      # Generated: locked dependencies
├── packs/              # Downloaded dependencies
└── templates/          # Required: Jinja2 templates
    ├── deployment.yaml
    ├── service.yaml
    └── _helpers.tpl    # Optional: shared helpers
```

Template context: `values.*`, `release.{name,namespace}`, `pack.{name,version}`, `capabilities.kubeVersion`.

## Configuration

- `Cargo.toml` — Workspace manifest, shared dependency versions
- `.cargo/` — Cargo config overrides
- `.github/workflows/{ci,release,deploy-docs,security}.yml` — CI pipelines
- `rust-toolchain` — pinned via `rust-version = "1.88"`

## Documentation

Top-level:
- `README.md` — User-facing overview, install, quick start
- `CLAUDE.md` — Claude Code working notes (this index references it)
- `RELEASE_NOTES_v0.1.0.md` — Initial release notes

`docs/` (design + analysis):
- `ARCHITECTURE.md`, `TECHNICAL_DESIGN.md`, `TUTORIAL.md`, `CLI_REFERENCE.md`
- `CONVERSION.md`, `HELM_CONVERTER_DESIGN.md`, `CONVERTER_FIX_PLAN.md`
- `DESIGN_CRD_HANDLING.md`, `DESIGN_CRD_PHASE2.md` — CRD strategy
- `DESIGN_HELM_DEPENDENCIES.md`, `SUBCHART_RENDERER_DESIGN.md`
- `HELM_COMPARISON.md`, `HELM_FEATURE_GAP_ANALYSIS.md`, `HELM_FEATURES_ROADMAP.md`
- `HELM_COMMUNITY_FRUSTRATIONS.md`, `PHASE5_FRUSTRATIONS_AND_SOLUTIONS.md`
- `KILLER_FEATURES.md`, `IMPROVED_TEMPLATE_FUNCTIONS.md`, `PHASE1_FUNCTIONS_DESIGN.md`
- `CONSOLIDATION.md`, `FEATURE_IMPLEMENTATIONS_DESIGN.md`
- `ANALYSIS_HELM_DEPENDENCY_RISKS.md`, `PHASE5_CRITICAL_ANALYSIS.md`

`website/` — Docusaurus site (architecture, cli-reference, intro, concepts/, getting-started/, kubernetes/, packaging/, repositories/, templating/).

Per-crate `README.md` in each `crates/sherpack-*/`.

## Test Coverage

- **Total tests**: 685 passing (core 68, engine 151, convert 96, kube 225, repo 54, cli 91)
- **Source size**: ~78,000 lines of Rust
- **Unit tests**: Inline `#[cfg(test)] mod tests` in every crate
- **Integration tests**: `crates/sherpack-cli/tests/integration_tests.rs`
- **Snapshot tests**: `crates/sherpack-cli/tests/snapshot_tests.rs` (insta)
- **Fixtures**: `fixtures/{simple-pack, demo-pack, helm-nginx, pack-with-subcharts}`
- **Mocks**: `MockStorageDriver` (sherpack-kube), `MockFileProvider` (sherpack-core), `wiremock` (HTTP)

## Key Dependencies

| Crate | Version | Purpose |
|-------|---------|---------|
| `minijinja` | 2 | Jinja2 templating |
| `pest` / `pest_derive` | 2.7 | PEG parser for Go template conversion |
| `kube` | 2.0 | Kubernetes client (runtime, derive) |
| `k8s-openapi` | 0.26 (v1_32) | K8s type definitions |
| `oci-distribution` | 0.11 | OCI registry client |
| `rusqlite` | 0.38 (bundled, FTS5) | Search index |
| `minisign` | 0.8 | Cryptographic signatures |
| `clap` | 4 | CLI parsing |
| `tokio` | 1 | Async runtime |
| `reqwest` | 0.13 (rustls) | HTTP client |
| `jsonschema` | 0.38 | Schema validation |
| `serde` / `serde_yaml` / `serde_json` | — | Serialization |
| `thiserror` / `miette` | 2 / 7 | Error types + fancy reporting |
| `insta` | 1 | Snapshot testing |

Release profile: `lto = true`, `codegen-units = 1`, `strip = true`.

## Quick Start

```bash
# Build
cargo build --workspace               # debug
cargo build --release                  # ~19 MB binary

# Test
cargo test --workspace                 # all 685 tests (core 68, engine 151, convert 96, kube 225, repo 54, cli 91)
cargo test -p sherpack-kube            # single crate
cargo test --workspace -- --nocapture  # with output

# Lint & format
cargo clippy --workspace
cargo fmt --all

# Run CLI
cargo run -p sherpack -- template my-release fixtures/demo-pack
cargo run -p sherpack -- lint fixtures/demo-pack
cargo run -p sherpack -- package fixtures/demo-pack
```
