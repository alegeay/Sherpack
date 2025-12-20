# sherpack

The Kubernetes package manager with Jinja2 templates.

## Overview

Sherpack is a modern Kubernetes package manager that uses Jinja2 templating instead of Go templates. It provides a simpler, more readable syntax while maintaining full lifecycle management capabilities including install, upgrade, rollback, and dependency management.

## Installation

### From Binary

```bash
# Download latest release
curl -LO https://github.com/myorg/sherpack/releases/latest/download/sherpack-linux-amd64
chmod +x sherpack-linux-amd64
sudo mv sherpack-linux-amd64 /usr/local/bin/sherpack
```

### From Source

```bash
git clone https://github.com/myorg/sherpack
cd sherpack
cargo build --release
sudo cp target/release/sherpack /usr/local/bin/
```

## Quick Start

```bash
# Create a new pack
sherpack create my-app

# Edit templates and values
cd my-app
vim values.yaml
vim templates/deployment.yaml

# Render templates (dry-run)
sherpack template my-release .

# Install to cluster
sherpack install my-release . --namespace production

# Upgrade
sherpack upgrade my-release . --set image.tag=v2.0

# Rollback
sherpack rollback my-release --revision 1

# Uninstall
sherpack uninstall my-release --namespace production
```

## Commands

### Templating Commands

#### `sherpack template`

Render templates without installing.

```bash
# Render to stdout
sherpack template my-release ./my-pack

# With custom values
sherpack template my-release ./my-pack \
  --values custom-values.yaml \
  --set image.tag=v2.0 \
  --set replicas=3

# Output to directory
sherpack template my-release ./my-pack --output-dir ./manifests

# Show only specific template
sherpack template my-release ./my-pack --show-only deployment.yaml
```

#### `sherpack lint`

Validate pack structure and templates.

```bash
sherpack lint ./my-pack

# With schema validation
sherpack lint ./my-pack --strict

# Skip schema validation
sherpack lint ./my-pack --skip-schema
```

#### `sherpack validate`

Validate values against schema.

```bash
# Validate with default values
sherpack validate ./my-pack

# With custom values file
sherpack validate ./my-pack --values production.yaml

# JSON output (for CI/CD)
sherpack validate ./my-pack --json
```

#### `sherpack show`

Display pack information.

```bash
# Show pack metadata
sherpack show pack ./my-pack

# Show values
sherpack show values ./my-pack

# Show computed values (with defaults)
sherpack show computed-values ./my-pack --values override.yaml
```

#### `sherpack create`

Create a new pack from template.

```bash
# Basic pack
sherpack create my-app

# With options
sherpack create my-app --starter web-service
```

#### `sherpack convert`

Convert Helm chart to Sherpack pack.

```bash
# Basic conversion
sherpack convert ./helm-chart ./sherpack-pack

# Force overwrite
sherpack convert ./helm-chart ./sherpack-pack --force

# Dry run (show what would change)
sherpack convert ./helm-chart ./sherpack-pack --dry-run

# Verbose output
sherpack convert ./helm-chart ./sherpack-pack --verbose
```

### Packaging Commands

#### `sherpack package`

Create a distributable archive.

```bash
# Create archive
sherpack package ./my-pack

# Output: my-pack-1.0.0.tgz

# Custom destination
sherpack package ./my-pack --destination ./dist/

# Include dependencies
sherpack package ./my-pack --include-deps
```

#### `sherpack inspect`

Show archive contents.

```bash
sherpack inspect my-pack-1.0.0.tgz

# Output:
# my-pack-1.0.0/
# ├── Pack.yaml
# ├── values.yaml
# ├── MANIFEST
# └── templates/
#     ├── deployment.yaml
#     └── service.yaml
```

#### `sherpack verify`

Verify archive integrity.

```bash
# Verify manifest checksums
sherpack verify my-pack-1.0.0.tgz

# Verify signature
sherpack verify my-pack-1.0.0.tgz --signature my-pack-1.0.0.tgz.sig
```

### Signing Commands

#### `sherpack keygen`

Generate signing keypair.

```bash
sherpack keygen

# Output:
# Generated keypair:
#   Private key: ~/.config/sherpack/keys/sherpack.key
#   Public key:  ~/.config/sherpack/keys/sherpack.pub

# Custom path
sherpack keygen --output ./my-keys/
```

#### `sherpack sign`

Sign a package archive.

```bash
# Sign with default key
sherpack sign my-pack-1.0.0.tgz

# Output: my-pack-1.0.0.tgz.sig

# With specific key
sherpack sign my-pack-1.0.0.tgz --key ./my-key.key
```

### Kubernetes Commands

#### `sherpack install`

Install a pack to the cluster.

```bash
# Basic install
sherpack install my-release ./my-pack

# With namespace
sherpack install my-release ./my-pack --namespace production

# Create namespace if missing
sherpack install my-release ./my-pack --namespace production --create-namespace

# With values
sherpack install my-release ./my-pack \
  --values production.yaml \
  --set image.tag=v2.0

# Wait for resources
sherpack install my-release ./my-pack --wait --timeout 5m

# Dry run (show manifests)
sherpack install my-release ./my-pack --dry-run
```

#### `sherpack upgrade`

Upgrade an existing release.

```bash
# Upgrade with new values
sherpack upgrade my-release ./my-pack --set image.tag=v3.0

# Reuse previous values
sherpack upgrade my-release ./my-pack --reuse-values

# Reset to pack defaults
sherpack upgrade my-release ./my-pack --reset-values

# Force upgrade (recreate resources)
sherpack upgrade my-release ./my-pack --force

# Install if not exists
sherpack upgrade my-release ./my-pack --install
```

#### `sherpack uninstall`

Remove a release.

```bash
# Basic uninstall
sherpack uninstall my-release

# With namespace
sherpack uninstall my-release --namespace production

# Keep history
sherpack uninstall my-release --keep-history

# Dry run
sherpack uninstall my-release --dry-run
```

#### `sherpack rollback`

Rollback to a previous revision.

```bash
# Rollback to previous
sherpack rollback my-release

# Rollback to specific revision
sherpack rollback my-release --revision 3

# With wait
sherpack rollback my-release --wait
```

#### `sherpack list`

List installed releases.

```bash
# All namespaces
sherpack list --all-namespaces

# Specific namespace
sherpack list --namespace production

# Filter by status
sherpack list --status deployed

# Output formats
sherpack list --output json
sherpack list --output yaml
```

#### `sherpack history`

Show release history.

```bash
sherpack history my-release --namespace production

# Output:
# REVISION  STATUS      DESCRIPTION          DATE
# 1         superseded  Install complete     2024-01-15 10:00:00
# 2         superseded  Upgrade to v2.0      2024-01-16 11:00:00
# 3         deployed    Upgrade to v3.0      2024-01-17 12:00:00
```

#### `sherpack status`

Show release status.

```bash
sherpack status my-release --namespace production

# Output:
# NAME: my-release
# NAMESPACE: production
# STATUS: deployed
# REVISION: 3
#
# RESOURCES:
#   Deployment/my-app: 3/3 ready
#   Service/my-app: ClusterIP
#   Ingress/my-app: my-app.example.com
```

#### `sherpack recover`

Recover a stale release.

```bash
# Find stale releases
sherpack list --status pending

# Recover (mark as failed)
sherpack recover my-release --namespace production
```

### Repository Commands

#### `sherpack repo add`

Add a repository.

```bash
# HTTP repository
sherpack repo add bitnami https://charts.bitnami.com/bitnami

# OCI registry
sherpack repo add myorg oci://ghcr.io/myorg/charts

# With credentials
sherpack repo add private https://charts.example.com \
  --username user \
  --password pass
```

#### `sherpack repo list`

List configured repositories.

```bash
sherpack repo list

# Output:
# NAME      URL                                    TYPE
# bitnami   https://charts.bitnami.com/bitnami    http
# myorg     oci://ghcr.io/myorg/charts            oci
```

#### `sherpack repo update`

Update repository indexes.

```bash
# Update all
sherpack repo update

# Update specific
sherpack repo update bitnami
```

#### `sherpack repo remove`

Remove a repository.

```bash
sherpack repo remove bitnami
```

#### `sherpack search`

Search for packs.

```bash
# Search in all repos
sherpack search nginx

# Search in specific repo
sherpack search nginx --repo bitnami

# Show all versions
sherpack search nginx --versions
```

#### `sherpack pull`

Download a pack.

```bash
# Pull latest
sherpack pull bitnami/nginx

# Pull specific version
sherpack pull bitnami/nginx --version 15.0.0

# Extract to directory
sherpack pull bitnami/nginx --untar --untardir ./nginx
```

#### `sherpack push`

Push to OCI registry.

```bash
sherpack push my-pack-1.0.0.tgz oci://ghcr.io/myorg/charts
```

### Dependency Commands

#### `sherpack dependency list`

List dependencies.

```bash
sherpack dependency list ./my-pack

# Output:
# NAME        VERSION   REPOSITORY                              STATUS
# redis       ^17.0.0   https://charts.bitnami.com/bitnami     [condition: true]
# postgresql  ^12.0.0   https://charts.bitnami.com/bitnami     [disabled]
```

#### `sherpack dependency update`

Resolve and lock dependencies.

```bash
sherpack dependency update ./my-pack

# Output:
# Resolving dependencies for my-app...
#
# Skipping 1 dependencies:
#   postgresql (enabled: false)
#
# Resolved 2 dependencies:
#   redis @ 17.0.0
#   common @ 2.0.0
#
# Dependency tree:
# └── redis@17.0.0
#     └── common@2.0.0
#
# Wrote Pack.lock.yaml with 2 locked dependencies
```

#### `sherpack dependency build`

Download locked dependencies.

```bash
sherpack dependency build ./my-pack

# With integrity verification
sherpack dependency build ./my-pack --verify
```

#### `sherpack dependency tree`

Show dependency tree.

```bash
sherpack dependency tree ./my-pack

# Output:
# my-app@1.0.0
# ├── redis@17.0.0
# │   └── common@2.0.0
# └── nginx@15.0.0
```

## Pack Structure

```
my-pack/
├── Pack.yaml              # Package metadata (required)
├── values.yaml            # Default values (required)
├── values.schema.yaml     # JSON Schema for validation (optional)
├── Pack.lock.yaml         # Locked dependencies (generated)
├── packs/                 # Downloaded dependencies
│   ├── redis/
│   └── postgresql/
└── templates/             # Jinja2 templates (required)
    ├── deployment.yaml
    ├── service.yaml
    ├── ingress.yaml
    └── _helpers.tpl       # Shared macros
```

### Pack.yaml

```yaml
apiVersion: sherpack/v1
kind: application

metadata:
  name: my-app
  version: 1.0.0
  description: My awesome application
  appVersion: "2.0"
  keywords:
    - web
    - api
  maintainers:
    - name: John Doe
      email: john@example.com
  home: https://myapp.example.com
  sources:
    - https://github.com/myorg/myapp

dependencies:
  - name: redis
    version: "^17.0.0"
    repository: https://charts.bitnami.com/bitnami
    condition: redis.enabled

  - name: postgresql
    version: "^12.0.0"
    repository: https://charts.bitnami.com/bitnami
    enabled: false
    resolve: never
```

### values.yaml

```yaml
# Application settings
name: my-app
replicas: 3

image:
  repository: myorg/myapp
  tag: latest
  pullPolicy: IfNotPresent

service:
  type: ClusterIP
  port: 80

ingress:
  enabled: true
  host: myapp.example.com

redis:
  enabled: true

postgresql:
  enabled: false
```

### Template Example

```jinja2
{# templates/deployment.yaml #}
apiVersion: apps/v1
kind: Deployment
metadata:
  name: {{ release.name }}
  labels:
    app.kubernetes.io/name: {{ values.name }}
    app.kubernetes.io/instance: {{ release.name }}
spec:
  replicas: {{ values.replicas | default(1) }}
  selector:
    matchLabels:
      app.kubernetes.io/name: {{ values.name }}
      app.kubernetes.io/instance: {{ release.name }}
  template:
    metadata:
      labels:
        app.kubernetes.io/name: {{ values.name }}
        app.kubernetes.io/instance: {{ release.name }}
    spec:
      containers:
        - name: {{ values.name }}
          image: {{ values.image.repository }}:{{ values.image.tag }}
          imagePullPolicy: {{ values.image.pullPolicy }}
          ports:
            - containerPort: {{ values.service.port }}
          {%- if values.env %}
          env:
            {{ values.env | toyaml | indent(12) }}
          {%- endif %}
```

## Configuration

### Config File

Located at `~/.config/sherpack/config.yaml`:

```yaml
# Default namespace
namespace: default

# Storage driver: secrets, configmap, file
storage:
  driver: secrets
  namespace: sherpack-system  # Optional override

# Default timeout
timeout: 5m

# Output format: table, json, yaml
output: table
```

### Environment Variables

| Variable | Description |
|----------|-------------|
| `KUBECONFIG` | Kubernetes config path |
| `SHERPACK_NAMESPACE` | Default namespace |
| `SHERPACK_DEBUG` | Enable debug output |
| `SHERPACK_NO_COLOR` | Disable colored output |

## Exit Codes

| Code | Description |
|------|-------------|
| 0 | Success |
| 1 | General error |
| 2 | Invalid arguments |
| 3 | Pack not found |
| 4 | Template error |
| 5 | Validation error |
| 6 | Kubernetes error |
| 7 | Repository error |
| 8 | Dependency error |

## Comparison with Helm

| Feature | Helm | Sherpack |
|---------|------|----------|
| Template syntax | Go templates | Jinja2 |
| Learning curve | Steep | Gentle |
| Error messages | Cryptic | Contextual with suggestions |
| Schema validation | JSON Schema | JSON Schema + simplified |
| Dependencies | Auto-resolve | Explicit resolution |
| Conflict handling | Silent | Error with solutions |
| Signature format | PGP | Minisign |

## Dependencies

- `clap` - CLI argument parsing
- `sherpack-core` - Core types
- `sherpack-engine` - Template rendering
- `sherpack-kube` - Kubernetes operations
- `sherpack-repo` - Repository management
- `sherpack-convert` - Helm conversion
- `miette` - Error reporting
- `console` / `indicatif` - Terminal UI
- `minisign` - Signatures
- `tokio` - Async runtime

## License

MIT OR Apache-2.0
