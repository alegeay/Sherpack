<div align="center">

# Sherpack

**A blazingly fast Kubernetes package manager with Jinja2 templating**

[![Rust](https://img.shields.io/badge/rust-1.85%2B-orange.svg?style=flat-square&logo=rust)](https://www.rust-lang.org/)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg?style=flat-square)](LICENSE)
[![Build](https://img.shields.io/badge/build-passing-brightgreen.svg?style=flat-square)]()
[![Tests](https://img.shields.io/badge/tests-410%20passed-brightgreen.svg?style=flat-square)]()

*A modern Helm alternative written in Rust, featuring familiar Jinja2 templating syntax*

[Features](#-features) •
[Installation](#-installation) •
[Quick Start](#-quick-start) •
[Commands](#-cli-reference) •
[Templating](#-templating-reference)

</div>

---

## Why Sherpack?

| Feature | Sherpack | Helm |
|---------|----------|------|
| **Templating** | Jinja2 (familiar syntax) | Go templates (complex) |
| **Performance** | Native Rust binary | Go runtime |
| **Binary Size** | ~19 MB | ~50 MB |
| **Learning Curve** | Minimal (if you know Jinja2) | Steep |
| **Dependencies** | None | None |
| **Schema Validation** | Built-in JSON Schema | External tools |
| **Error Messages** | Contextual suggestions | Generic errors |
| **Helm Migration** | Automatic chart converter | N/A |

---

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

### Helm Migration
- **Automatic Conversion** - Convert Helm charts to Sherpack packs
- **Template Translation** - Go templates → Jinja2 syntax
- **Helper Function Support** - Converts `include`, `define`, `range`, `with`, etc.
- **Full Chart Compatibility** - Tested with ingress-nginx (43 templates)

### Repository & Dependencies
- **Repository Management** - HTTP, OCI, and file-based repositories
- **Dependency Resolution** - Lock file with version policies
- **SQLite Search** - Fast local search with FTS5
- **OCI Registry Support** - Push/pull to container registries

---

## Installation

### From Source

```bash
# Clone the repository
git clone https://github.com/alegeay/sherpack.git
cd sherpack

# Build release binary
cargo build --release

# Install to your PATH
cp target/release/sherpack ~/.local/bin/
```

### Requirements

- Rust 1.85+ (Edition 2024)
- For Kubernetes operations: `kubectl` configured with cluster access

---

## Quick Start

### 1. Create a Pack

```bash
sherpack create myapp
```

This generates:
```
myapp/
├── Pack.yaml           # Pack metadata
├── values.yaml         # Default values
├── values.schema.yaml  # Optional: JSON Schema
└── templates/
    └── deployment.yaml # Kubernetes template
```

### 2. Define Your Values

```yaml
# values.yaml
app:
  name: mywebapp
  replicas: 3

image:
  repository: nginx
  tag: "1.25"

resources:
  limits:
    cpu: "500m"
    memory: "256Mi"
```

### 3. Create Templates

```yaml
# templates/deployment.yaml
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

### 4. Validate and Render

```bash
# Validate against schema
sherpack validate ./myapp

# Lint the pack structure
sherpack lint ./myapp

# Render templates locally
sherpack template myrelease ./myapp

# Render with overrides
sherpack template myrelease ./myapp --set app.replicas=5 -n production
```

### 5. Deploy to Kubernetes

```bash
# Install to cluster
sherpack install myrelease ./myapp -n production

# Upgrade existing release
sherpack upgrade myrelease ./myapp --set app.replicas=5

# View release status
sherpack status myrelease

# Rollback if needed
sherpack rollback myrelease 1

# Uninstall
sherpack uninstall myrelease
```

---

## CLI Reference

### Templating Commands

| Command | Description |
|---------|-------------|
| `sherpack template <name> <pack>` | Render templates to stdout |
| `sherpack lint <pack>` | Validate pack structure and templates |
| `sherpack validate <pack>` | Validate values against schema |
| `sherpack show <pack>` | Display pack information |
| `sherpack create <name>` | Scaffold a new pack |
| `sherpack convert <chart>` | Convert Helm chart to Sherpack pack |

### Packaging Commands

| Command | Description |
|---------|-------------|
| `sherpack package <pack>` | Create archive from pack directory |
| `sherpack inspect <archive>` | Show archive contents and manifest |
| `sherpack keygen` | Generate signing keypair |
| `sherpack sign <archive>` | Sign archive with private key |
| `sherpack verify <archive>` | Verify archive integrity and signature |

### Kubernetes Commands

| Command | Description |
|---------|-------------|
| `sherpack install <name> <pack>` | Install pack to cluster |
| `sherpack upgrade <name> <pack>` | Upgrade existing release |
| `sherpack uninstall <name>` | Remove release from cluster |
| `sherpack rollback <name> <rev>` | Rollback to previous revision |
| `sherpack list` | List installed releases |
| `sherpack history <name>` | Show release history |
| `sherpack status <name>` | Show release status |
| `sherpack recover <name>` | Recover stale release |

### Repository Commands

| Command | Description |
|---------|-------------|
| `sherpack repo add <name> <url>` | Add repository |
| `sherpack repo list` | List repositories |
| `sherpack repo update [name]` | Update repository index |
| `sherpack repo remove <name>` | Remove repository |
| `sherpack search <query>` | Search for packs |
| `sherpack pull <pack>` | Download pack from repository |
| `sherpack push <archive> <dest>` | Push to OCI registry |

### Dependency Commands

| Command | Description |
|---------|-------------|
| `sherpack dependency list <pack>` | List pack dependencies |
| `sherpack dependency update <pack>` | Resolve and lock dependencies |
| `sherpack dependency build <pack>` | Download locked dependencies |
| `sherpack dependency tree <pack>` | Show dependency tree |

---

## Templating Reference

### Context Variables

| Variable | Description | Example |
|----------|-------------|---------|
| `values.*` | Values from values.yaml | `{{ values.app.name }}` |
| `release.name` | Release name | `{{ release.name }}` |
| `release.namespace` | Target namespace | `{{ release.namespace }}` |
| `pack.name` | Pack name from Pack.yaml | `{{ pack.name }}` |
| `pack.version` | Pack version | `{{ pack.version }}` |
| `capabilities.*` | Cluster capabilities | `{{ capabilities.kubeVersion }}` |

### Filters

#### Serialization
| Filter | Description |
|--------|-------------|
| `toyaml` | Object to YAML string |
| `tojson` | Object to compact JSON |
| `tojson_pretty` | Object to formatted JSON |

#### Encoding
| Filter | Description |
|--------|-------------|
| `b64encode` | Base64 encode |
| `b64decode` | Base64 decode |
| `sha256` | SHA256 hash |

#### Strings
| Filter | Description |
|--------|-------------|
| `quote` / `squote` | Wrap in quotes |
| `upper` / `lower` | Change case |
| `snakecase` / `kebabcase` / `camelcase` | Case conversion |
| `trunc(n)` | Truncate to n characters |
| `trimprefix(s)` / `trimsuffix(s)` | Remove prefix/suffix |
| `replace(old, new)` | Replace substring |

#### Indentation
| Filter | Description |
|--------|-------------|
| `indent(n)` | Add n spaces to each line |
| `nindent(n)` | Newline + indent |

#### Collections
| Filter | Description |
|--------|-------------|
| `keys` | Get object keys |
| `haskey(k)` | Check if key exists |
| `merge(obj)` | Merge objects |
| `dictsort` | Sort for iteration |
| `first` / `last` | First/last element |
| `default(val)` | Default if undefined |

#### Validation
| Filter | Description |
|--------|-------------|
| `required` | Fail if undefined/empty |
| `empty` | Check if empty |

### Type Conversion
| Filter | Description |
|--------|-------------|
| `int` | Convert to integer |
| `float` | Convert to float |
| `string` | Convert to string |

### Functions

| Function | Description |
|----------|-------------|
| `get(obj, key, default)` | Safe access with default |
| `ternary(true, false, cond)` | Conditional value |
| `tostring(v)` / `toint(v)` / `tofloat(v)` | Type conversion |
| `now()` | Current ISO timestamp |
| `uuidv4()` | Random UUID |
| `fail(msg)` | Fail with message |

---

## Pack Structure

```
mypack/
├── Pack.yaml             # Required: Pack metadata
├── values.yaml           # Required: Default values
├── values.schema.yaml    # Optional: JSON Schema for validation
├── Pack.lock.yaml        # Generated: Locked dependencies
├── packs/                # Downloaded dependencies
└── templates/            # Required: Template files
    ├── deployment.yaml
    ├── service.yaml
    └── _helpers.tpl      # Optional: Shared helpers
```

### Pack.yaml

```yaml
apiVersion: sherpack/v1
kind: application
metadata:
  name: myapp
  version: 1.0.0
  description: My application
  appVersion: "2.0.0"

# Optional: Dependencies
dependencies:
  - name: redis
    version: ">=7.0.0"
    repository: https://charts.example.com

# Optional: Engine settings
engine:
  strict: true  # Fail on undefined variables
```

### values.schema.yaml

```yaml
$schema: http://json-schema.org/draft-07/schema#
type: object
properties:
  app:
    type: object
    properties:
      name:
        type: string
        default: myapp
      replicas:
        type: integer
        minimum: 1
        maximum: 10
        default: 3
    required: [name]
```

---

## Kubernetes Hooks

Sherpack supports lifecycle hooks for custom actions:

```yaml
# templates/pre-install-job.yaml
apiVersion: batch/v1
kind: Job
metadata:
  name: {{ release.name }}-pre-install
  annotations:
    sherpack.io/hook: pre-install
    sherpack.io/hook-weight: "0"
    sherpack.io/hook-delete-policy: hook-succeeded
spec:
  template:
    spec:
      containers:
        - name: migrate
          image: myapp:{{ values.image.tag }}
          command: ["./migrate.sh"]
      restartPolicy: Never
```

### Hook Phases

| Phase | When |
|-------|------|
| `pre-install` | Before install |
| `post-install` | After install |
| `pre-upgrade` | Before upgrade |
| `post-upgrade` | After upgrade |
| `pre-rollback` | Before rollback |
| `post-rollback` | After rollback |
| `pre-delete` | Before uninstall |
| `post-delete` | After uninstall |
| `test` | On `sherpack test` |

---

## Helm Chart Conversion

Sherpack can automatically convert Helm charts to Sherpack packs:

```bash
# Convert a Helm chart
sherpack convert ./my-helm-chart

# Specify output directory
sherpack convert ./my-helm-chart -o ./my-sherpack-pack

# Preview without writing
sherpack convert ./my-helm-chart --dry-run

# Force overwrite existing
sherpack convert ./my-helm-chart --force
```

### Conversion Examples

Go templates are automatically translated to Jinja2:

| Go Template | Jinja2 |
|-------------|--------|
| `{{ .Values.name }}` | `{{ values.name }}` |
| `{{ include "helper" . }}` | `{{ helper() }}` |
| `{{- if .Values.enabled }}` | `{% if values.enabled %}` |
| `{{ range .Values.items }}` | `{% for item in values.items %}` |
| `{{ .Release.Name }}` | `{{ release.name }}` |
| `{{ default "foo" .Values.x }}` | `{{ values.x \| default("foo") }}` |
| `{{ .Values.x \| quote }}` | `{{ values.x \| quote }}` |
| `{{ toYaml .Values \| nindent 2 }}` | `{{ values \| toyaml \| nindent(2) }}` |

### Supported Features

- `{{- define "name" }}` → `{% macro name() %}`
- `{{ include "name" . }}` → `{{ name() }}`
- `{{ if }}/{{ else }}/{{ end }}` → `{% if %}/{% else %}/{% endif %}`
- `{{ range }}/{{ end }}` → `{% for %}/{% endfor %}`
- `{{ with }}/{{ end }}` → `{% with %}/{% endwith %}` or inline
- Variable declarations: `$var := value`
- All common Helm functions and pipelines

---

## Repository Configuration

### Add Repositories

```bash
# HTTP repository
sherpack repo add stable https://charts.example.com

# With authentication
sherpack repo add private https://charts.example.com --username user --password pass

# OCI registry
sherpack repo add oci oci://registry.example.com/charts
```

### Search and Pull

```bash
# Search across repositories
sherpack search nginx

# Pull specific version
sherpack pull stable/nginx:1.0.0

# Pull from OCI
sherpack pull oci://registry.example.com/charts/nginx:1.0.0
```

### Push to OCI

```bash
# Package and push
sherpack package ./myapp
sherpack push myapp-1.0.0.tar.gz oci://registry.example.com/charts/myapp:1.0.0
```

---

## Architecture

```
sherpack/
├── crates/
│   ├── sherpack-core/     # Core types: Pack, Values, Context, Archive
│   ├── sherpack-engine/   # Template engine, filters, functions
│   ├── sherpack-convert/  # Helm chart to Sherpack converter
│   ├── sherpack-kube/     # Kubernetes client, storage, hooks, health
│   ├── sherpack-repo/     # Repository, OCI, dependencies, search
│   └── sherpack-cli/      # CLI commands
├── fixtures/
│   ├── simple-pack/       # Basic test fixture
│   ├── demo-pack/         # Comprehensive demo
│   └── helm-nginx/        # Helm conversion test
└── docs/                  # Documentation
```

### Crates

| Crate | Purpose | Tests |
|-------|---------|-------|
| `sherpack-core` | Pack, Values, Archive, Manifest | 19 |
| `sherpack-engine` | MiniJinja templating, filters, functions | 58 |
| `sherpack-convert` | Helm Go templates → Jinja2 converter | 63 |
| `sherpack-kube` | Kubernetes operations, storage, hooks | 151 |
| `sherpack-repo` | Repository backends, dependencies, search | 43 |
| `sherpack-cli` | CLI application | 75 |
| **Total** | ~32k lines of Rust | **410** |

---

## Examples

### Complete Deployment with ConfigMap

```yaml
# templates/deployment.yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: {{ release.name }}
  labels:
    app.kubernetes.io/name: {{ release.name }}
    app.kubernetes.io/version: {{ pack.version }}
  annotations:
    checksum/config: {{ values.config | tojson | sha256 | trunc(16) }}
spec:
  replicas: {{ values.replicas }}
  selector:
    matchLabels:
      app.kubernetes.io/name: {{ release.name }}
  template:
    spec:
      containers:
        - name: app
          image: {{ values.image.repository }}:{{ values.image.tag }}
          envFrom:
            - configMapRef:
                name: {{ release.name }}-config
          resources:
            {{ values.resources | toyaml | nindent(12) }}
---
# templates/configmap.yaml
apiVersion: v1
kind: ConfigMap
metadata:
  name: {{ release.name }}-config
data:
  {% for key, value in values.env | dictsort %}
  {{ key }}: {{ value | quote }}
  {% endfor %}
```

### Conditional Ingress with TLS

```yaml
{% if values.ingress.enabled %}
apiVersion: networking.k8s.io/v1
kind: Ingress
metadata:
  name: {{ release.name }}
  annotations:
    {% if values.ingress.tls %}
    cert-manager.io/cluster-issuer: letsencrypt
    {% endif %}
spec:
  {% if values.ingress.tls %}
  tls:
    - hosts: {{ values.ingress.hosts | tojson }}
      secretName: {{ release.name }}-tls
  {% endif %}
  rules:
    {% for host in values.ingress.hosts %}
    - host: {{ host }}
      http:
        paths:
          - path: /
            pathType: Prefix
            backend:
              service:
                name: {{ release.name }}
                port:
                  number: 80
    {% endfor %}
{% endif %}
```

---

## Development

```bash
# Build
cargo build --workspace

# Test
cargo test --workspace

# Lint
cargo clippy --workspace

# Format
cargo fmt --all

# Run CLI
cargo run -p sherpack -- <command>
```

---

## License

Apache-2.0

---

<div align="center">

**[Back to top](#sherpack)**

Made with Rust

</div>
