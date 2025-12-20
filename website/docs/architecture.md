---
id: architecture
title: Architecture
sidebar_position: 101
---

# Architecture

Sherpack is built as a modular Rust workspace with 6 crates (~35k lines of code).

## Crate Overview

```
sherpack/
├── crates/
│   ├── sherpack-core/     # Core types
│   ├── sherpack-engine/   # Template engine
│   ├── sherpack-convert/  # Helm chart converter
│   ├── sherpack-kube/     # Kubernetes integration
│   ├── sherpack-repo/     # Repository management
│   └── sherpack-cli/      # CLI application
```

### Dependencies

```
sherpack-cli
    ├── sherpack-core
    ├── sherpack-engine ─── sherpack-core
    ├── sherpack-convert ── sherpack-core, sherpack-engine
    ├── sherpack-kube ───── sherpack-core
    └── sherpack-repo ───── sherpack-core
```

## sherpack-core

Core types shared across all crates.

### Key Types

| Type | Description |
|------|-------------|
| `Pack` | Pack metadata from Pack.yaml |
| `LoadedPack` | Pack with loaded files |
| `Values` | Configuration values with merge |
| `Release` | Deployment state |
| `TemplateContext` | Context for templates |
| `Archive` | Tar.gz archive operations |
| `Manifest` | SHA256 checksums |
| `Schema` | JSON Schema validation |

### Values Merging

```
Schema defaults
    └── values.yaml
        └── -f files (in order)
            └── --set flags
```

## sherpack-engine

MiniJinja-based template engine.

### Components

| Component | Description |
|-----------|-------------|
| `Engine` | Template compilation and rendering |
| `filters.rs` | 25+ Helm-compatible filters |
| `functions.rs` | Built-in functions |
| `suggestions.rs` | Error suggestions with fuzzy matching |

### Filter Categories

- **Serialization**: `toyaml`, `tojson`, `tojson_pretty`
- **Encoding**: `b64encode`, `b64decode`, `sha256`
- **Strings**: `quote`, `upper`, `lower`, `kebabcase`, `snakecase`
- **Indentation**: `indent`, `nindent`
- **Collections**: `keys`, `haskey`, `merge`, `dictsort`
- **Validation**: `required`, `empty`, `default`
- **Type Conversion**: `int`, `float`, `string`

## sherpack-convert

Helm chart to Sherpack converter.

### Components

| Component | Description |
|-----------|-------------|
| `GoTemplateParser` | PEG-based Go template parser |
| `Transformer` | Go template → Jinja2 AST transform |
| `HelmConverter` | Full chart conversion orchestration |

### Supported Conversions

| Go Template | Jinja2 |
|-------------|--------|
| `{{ .Values.x }}` | `{{ values.x }}` |
| `{{ include "name" . }}` | `{{ name() }}` |
| `{{- define "name" }}` | `{% macro name() %}` |
| `{{ if }}...{{ end }}` | `{% if %}...{% endif %}` |
| `{{ range }}...{{ end }}` | `{% for %}...{% endfor %}` |
| `{{ with }}...{{ end }}` | Inlined or `{% with %}` |

### Features

- Three-pass conversion for cross-helper imports
- Automatic macro extraction from `_helpers.tpl`
- NOTES.txt template conversion
- Full variable pipeline support

## sherpack-kube

Kubernetes integration.

### Components

| Component | Description |
|-----------|-------------|
| `KubeClient<S>` | Main client with lifecycle operations |
| `ResourceManager` | Server-Side Apply with Discovery |
| `StorageDriver` | Release storage trait |
| `HookExecutor` | Hook lifecycle management |
| `HealthChecker` | Deployment/StatefulSet health |
| `DiffEngine` | Three-way merge diff |
| `CrdManager` | CRD lifecycle with safe updates |
| `CrdAnalyzer` | 24 change type detection |
| `CrdProtection` | Deletion impact analysis |

### Storage Backends

| Backend | Storage |
|---------|---------|
| `SecretsDriver` | Kubernetes Secrets |
| `ConfigMapDriver` | Kubernetes ConfigMaps |
| `FileDriver` | Local filesystem |
| `MockDriver` | In-memory (testing) |

### Release States

```
PendingInstall → Deployed
                    ↓
              PendingUpgrade → Deployed
                    ↓              ↓
              PendingRollback  Failed
                    ↓
                Deployed
                    ↓
              Uninstalling → Uninstalled
```

### Resource Order

Resources are applied in order:

1. Namespace (0)
2. CRDs (5)
3. ClusterRole, ServiceAccount (10-14)
4. ConfigMap, Secret (20-21)
5. Service, Ingress (30-34)
6. Deployment, StatefulSet (40-44)
7. Job, CronJob (50-51)
8. HPA (60+)

### CRD Handling

Intent-based policies instead of location-based:

| Policy | Install | Update | Delete |
|--------|---------|--------|--------|
| `Managed` | Yes | Safe only | Protected |
| `Shared` | Yes | Safe only | Never |
| `External` | No | No | No |

24 change types analyzed for safety:

| Severity | Changes |
|----------|---------|
| **Safe** | Add optional field, add version, add column |
| **Warning** | Validation change, conversion change |
| **Dangerous** | Remove version, change scope, change type |

## sherpack-repo

Repository and dependency management.

### Components

| Component | Description |
|-----------|-------------|
| `RepositoryBackend` | Unified interface |
| `HttpBackend` | HTTP repos with ETag |
| `OciBackend` | OCI registries |
| `FileBackend` | Local directories |
| `IndexCache` | SQLite FTS5 search |
| `DependencyResolver` | Version resolution |
| `LockFile` | Pack.lock.yaml |

### Security

- Cross-origin redirect protection
- Credentials never sent after redirect to different host
- Encrypted credential storage

### Lock Policies

| Policy | Behavior |
|--------|----------|
| `Strict` | Version + SHA |
| `Version` | Version only (default) |
| `SemverPatch` | Allow patch updates |
| `SemverMinor` | Allow minor updates |

## Data Flow

### Template Command

```
1. Load pack (LoadedPack::load)
2. Merge values (schema → yaml → files → set)
3. Validate (Schema::validate)
4. Build context (TemplateContext)
5. Render (Engine::render)
6. Output (stdout or files)
```

### Install Command

```
1. Load pack & merge values
2. Validate schema
3. Render templates
4. Store release (PendingInstall)
5. Execute pre-install hooks
6. Apply resources (Server-Side Apply)
7. Wait for health
8. Execute post-install hooks
9. Update release (Deployed)
```

## Testing

| Crate | Tests | Type |
|-------|-------|------|
| sherpack-core | 35 | Unit |
| sherpack-engine | 70 | Unit |
| sherpack-convert | 85 | Unit + Parser |
| sherpack-kube | 200 | Unit + Mock |
| sherpack-repo | 55 | Unit |
| sherpack-cli | 155 | Integration |
| **Total** | **600** | |

### Test Patterns

- Unit tests inline with `#[cfg(test)]`
- Integration tests in `tests/`
- Snapshot tests with `insta`
- `MockStorageDriver` for K8s tests

## Key Dependencies

| Dependency | Purpose |
|------------|---------|
| `minijinja` | Template engine |
| `kube` | Kubernetes client |
| `oci-distribution` | OCI registry |
| `rusqlite` | SQLite FTS5 |
| `minisign` | Signatures |
| `clap` | CLI parsing |
| `serde` | Serialization |
| `tokio` | Async runtime |
