---
id: intro
title: Introduction
sidebar_position: 1
slug: /
---

# Sherpack

**A blazingly fast Kubernetes package manager with Jinja2 templating**

Sherpack is a modern alternative to Helm written in Rust, featuring familiar Jinja2 templating syntax instead of Go templates.

## Why Sherpack?

| Feature | Sherpack | Helm |
|---------|----------|------|
| **Templating** | Jinja2 (familiar syntax) | Go templates (complex) |
| **Performance** | Native Rust binary | Go runtime |
| **Binary Size** | ~5 MB | ~50 MB |
| **Learning Curve** | Minimal (if you know Jinja2) | Steep |
| **Schema Validation** | Built-in JSON Schema | External tools |
| **Error Messages** | Contextual suggestions | Generic errors |

## Features

### Core Templating
- **Jinja2 Templating** - Familiar Python-like syntax with `{{ }}` and `{% %}`
- **Helm-Compatible Filters** - `toyaml`, `tojson`, `b64encode`, `indent`, `nindent`, `quote`, and 20+ more
- **Rich Function Library** - `get()`, `ternary()`, `now()`, `uuidv4()`, `tostring()`, `fail()`
- **Strict Mode** - Catch undefined variables before deployment

### Schema Validation
- **JSON Schema Support** - Validate values against schema before rendering
- **Default Extraction** - Automatic default values from schema
- **Helpful Error Messages** - Contextual suggestions for typos and missing keys

### Packaging & Signing
- **Archive Format** - Reproducible tar.gz with SHA256 manifest
- **Cryptographic Signatures** - Minisign-based signing for supply chain security
- **Integrity Verification** - Verify archives before deployment

### Kubernetes Integration
- **Full Lifecycle Management** - Install, upgrade, rollback, uninstall
- **Server-Side Apply** - Modern Kubernetes apply with conflict detection
- **Hook Support** - Pre/post install, upgrade, rollback, delete hooks
- **Health Checks** - Wait for deployments, custom HTTP/command probes
- **Release Storage** - Secrets, ConfigMap, or file-based storage
- **Diff Preview** - See changes before applying

### Repository & Dependencies
- **Repository Management** - HTTP, OCI, and file-based repositories
- **Dependency Resolution** - Lock file with version policies
- **SQLite Search** - Fast local search with FTS5
- **OCI Registry Support** - Push/pull to container registries

## Quick Example

```yaml title="templates/deployment.yaml"
apiVersion: apps/v1
kind: Deployment
metadata:
  name: {{ release.name }}
  namespace: {{ release.namespace }}
spec:
  replicas: {{ values.app.replicas }}
  template:
    spec:
      containers:
        - name: {{ values.app.name | kebabcase }}
          image: {{ values.image.repository }}:{{ values.image.tag }}
          resources:
            {{ values.resources | toyaml | nindent(12) }}
```

```bash
# Render templates
sherpack template myapp ./mypack

# Install to cluster
sherpack install myapp ./mypack -n production --wait
```

## Architecture

Sherpack is built as a Cargo workspace with 5 crates:

| Crate | Purpose | Tests |
|-------|---------|-------|
| `sherpack-core` | Pack, Values, Archive, Manifest | 19 |
| `sherpack-engine` | MiniJinja templating, filters, functions | 43 |
| `sherpack-kube` | Kubernetes operations, storage, hooks | 107 |
| `sherpack-repo` | Repository backends, dependencies, search | 42 |
| `sherpack-cli` | CLI application | 71 |
| **Total** | | **282** |

## Getting Started

Ready to get started? Head to the [Installation](/docs/getting-started/installation) guide.
