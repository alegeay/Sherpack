# Sherpack Tutorial

A complete guide to packaging and deploying Kubernetes applications with Sherpack.

## Table of Contents

1. [Introduction](#introduction)
2. [Installation](#installation)
3. [Your First Pack](#your-first-pack)
4. [Understanding Jinja2 Templates](#understanding-jinja2-templates)
5. [Values and Configuration](#values-and-configuration)
6. [Schema Validation](#schema-validation)
7. [Deploying to Kubernetes](#deploying-to-kubernetes)
8. [Lifecycle Management](#lifecycle-management)
9. [Hooks](#hooks)
10. [Shared Helpers](#shared-helpers)
11. [Packaging and Distribution](#packaging-and-distribution)
12. [Migrating from Helm](#migrating-from-helm)
13. [Best Practices](#best-practices)

---

## Introduction

### What is Sherpack?

Sherpack is a Kubernetes package manager written in Rust that uses **Jinja2 templating** instead of Go templates. If you've used Helm, Sherpack will feel familiar - but with a much simpler and more readable template syntax.

### Why Sherpack over Helm?

| Aspect | Sherpack | Helm |
|--------|----------|------|
| **Template Syntax** | `{{ values.name }}` | `{{ .Values.name }}` |
| **Conditionals** | `{% if enabled %}` | `{{- if .enabled }}` |
| **Loops** | `{% for item in list %}` | `{{ range .list }}` |
| **Filters** | `{{ name \| upper }}` | `{{ upper .name }}` |
| **Learning Curve** | Minimal (Python-like) | Steep (Go templates) |

### Key Concepts

- **Pack**: A package containing templates, values, and metadata (like a Helm chart)
- **Release**: A deployed instance of a pack in a Kubernetes cluster
- **Values**: Configuration that customizes the pack for deployment
- **Templates**: Jinja2 files that generate Kubernetes manifests

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
sudo cp target/release/sherpack /usr/local/bin/

# Verify installation
sherpack --version
```

### Verify Installation

```bash
$ sherpack --version
sherpack 0.1.0

$ sherpack --help
The Kubernetes package manager with Jinja2 templates

Usage: sherpack [OPTIONS] <COMMAND>

Commands:
  template, create, lint, validate, show, convert,
  package, inspect, keygen, sign, verify,
  install, upgrade, uninstall, rollback, list, history, status,
  repo, search, pull, push, dependency
```

---

## Your First Pack

Let's create a simple web application pack step by step.

### Step 1: Create the Pack Structure

```bash
sherpack create mywebapp
cd mywebapp
```

This creates:

```
mywebapp/
├── Pack.yaml           # Pack metadata
├── values.yaml         # Default configuration
└── templates/
    └── deployment.yaml # Template file
```

### Step 2: Define Pack Metadata

Edit `Pack.yaml`:

```yaml
apiVersion: sherpack/v1
kind: application
metadata:
  name: mywebapp
  version: 1.0.0
  description: My first web application
  appVersion: "1.0.0"
```

### Step 3: Configure Default Values

Edit `values.yaml`:

```yaml
# Application settings
app:
  name: mywebapp
  replicas: 2

# Container image
image:
  repository: nginx
  tag: "1.25-alpine"
  pullPolicy: IfNotPresent

# Service configuration
service:
  type: ClusterIP
  port: 80

# Resource limits
resources:
  limits:
    cpu: "200m"
    memory: "128Mi"
  requests:
    cpu: "100m"
    memory: "64Mi"

# Feature flags
ingress:
  enabled: false
  host: ""
```

### Step 4: Create Templates

#### Deployment Template

Create `templates/deployment.yaml`:

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: {{ release.name }}
  namespace: {{ release.namespace }}
  labels:
    app.kubernetes.io/name: {{ values.app.name }}
    app.kubernetes.io/instance: {{ release.name }}
    app.kubernetes.io/version: {{ pack.version }}
spec:
  replicas: {{ values.app.replicas }}
  selector:
    matchLabels:
      app.kubernetes.io/name: {{ values.app.name }}
      app.kubernetes.io/instance: {{ release.name }}
  template:
    metadata:
      labels:
        app.kubernetes.io/name: {{ values.app.name }}
        app.kubernetes.io/instance: {{ release.name }}
    spec:
      containers:
        - name: {{ values.app.name }}
          image: {{ values.image.repository }}:{{ values.image.tag }}
          imagePullPolicy: {{ values.image.pullPolicy }}
          ports:
            - name: http
              containerPort: 80
              protocol: TCP
          resources:
            {{ values.resources | toyaml | nindent(12) }}
          livenessProbe:
            httpGet:
              path: /
              port: http
            initialDelaySeconds: 10
          readinessProbe:
            httpGet:
              path: /
              port: http
            initialDelaySeconds: 5
```

#### Service Template

Create `templates/service.yaml`:

```yaml
apiVersion: v1
kind: Service
metadata:
  name: {{ release.name }}
  namespace: {{ release.namespace }}
  labels:
    app.kubernetes.io/name: {{ values.app.name }}
    app.kubernetes.io/instance: {{ release.name }}
spec:
  type: {{ values.service.type }}
  ports:
    - port: {{ values.service.port }}
      targetPort: http
      protocol: TCP
      name: http
  selector:
    app.kubernetes.io/name: {{ values.app.name }}
    app.kubernetes.io/instance: {{ release.name }}
```

### Step 5: Test Your Pack

```bash
# Validate pack structure
sherpack lint .

# Render templates to see the output
sherpack template myapp .

# Render with custom values
sherpack template myapp . --set app.replicas=5
```

---

## Understanding Jinja2 Templates

### Variable Access

```yaml
# Access values
{{ values.app.name }}

# Access release info
{{ release.name }}
{{ release.namespace }}

# Access pack metadata
{{ pack.name }}
{{ pack.version }}
```

### Filters

Filters transform values using the pipe (`|`) syntax:

```yaml
# String transformations
{{ values.name | upper }}          # MYAPP
{{ values.name | lower }}          # myapp
{{ values.name | kebabcase }}      # my-app
{{ values.name | snakecase }}      # my_app
{{ values.name | camelcase }}      # myApp

# Quoting
{{ values.name | quote }}          # "myapp"
{{ values.name | squote }}         # 'myapp'

# Encoding
{{ values.secret | b64encode }}    # base64 encoded
{{ values.data | sha256 }}         # SHA256 hash

# YAML/JSON conversion
{{ values.config | toyaml }}       # Convert to YAML
{{ values.config | tojson }}       # Convert to compact JSON
{{ values.config | tojson_pretty }} # Convert to formatted JSON

# Indentation (crucial for YAML!)
{{ values.config | toyaml | indent(4) }}   # Add 4 spaces to each line
{{ values.config | toyaml | nindent(4) }}  # Newline + indent

# Truncation
{{ values.name | trunc(10) }}      # Truncate to 10 chars

# Default values
{{ values.optional | default("fallback") }}

# Type conversion
{{ values.count | int }}           # Convert to integer
{{ values.ratio | float }}         # Convert to float
```

### Control Structures

#### Conditionals

```yaml
{% if values.ingress.enabled %}
apiVersion: networking.k8s.io/v1
kind: Ingress
metadata:
  name: {{ release.name }}
spec:
  rules:
    - host: {{ values.ingress.host }}
{% endif %}
```

#### If-Else

```yaml
{% if values.service.type == "LoadBalancer" %}
  loadBalancerIP: {{ values.service.loadBalancerIP }}
{% elif values.service.type == "NodePort" %}
  nodePort: {{ values.service.nodePort }}
{% else %}
  # ClusterIP - no additional config
{% endif %}
```

#### Loops

```yaml
# Simple loop
{% for port in values.service.ports %}
- port: {{ port.port }}
  targetPort: {{ port.targetPort }}
  name: {{ port.name }}
{% endfor %}

# Loop with index
{% for item in values.items %}
- name: item-{{ loop.index }}  # 1, 2, 3...
  value: {{ item }}
{% endfor %}

# Dictionary iteration
{% for key, value in values.labels | dictsort %}
{{ key }}: {{ value | quote }}
{% endfor %}
```

#### With Statement

```yaml
{% with db = values.database %}
- name: DB_HOST
  value: {{ db.host }}
- name: DB_PORT
  value: {{ db.port | string }}
{% endwith %}
```

### Functions

```yaml
# Safe access with default
{{ get(values, "optional.nested.key", "default") }}

# Ternary operator
{{ ternary("yes", "no", values.enabled) }}

# Type conversion
{{ tostring(values.port) }}
{{ toint(values.count) }}
{{ tofloat(values.ratio) }}

# Timestamps and UUIDs
{{ now() }}      # Current ISO timestamp
{{ uuidv4() }}   # Random UUID

# Fail with message
{{ fail("Missing required value: database.host") }}
```

### Whitespace Control

Use `-` to trim whitespace:

```yaml
# Without whitespace control
{% if values.enabled %}
enabled: true
{% endif %}

# With whitespace control (removes empty lines)
{%- if values.enabled %}
enabled: true
{%- endif %}
```

---

## Values and Configuration

### Values Hierarchy

Values are merged in this order (later overrides earlier):

```
1. Schema defaults (values.schema.yaml)
2. Pack defaults (values.yaml)
3. User values files (-f values-prod.yaml)
4. Command-line overrides (--set key=value)
```

### Using Values Files

```bash
# Single values file
sherpack template myapp . -f production.yaml

# Multiple values files (merged left to right)
sherpack template myapp . -f base.yaml -f production.yaml -f secrets.yaml
```

Example `production.yaml`:

```yaml
app:
  replicas: 5

image:
  tag: "1.25.3"

resources:
  limits:
    cpu: "1000m"
    memory: "512Mi"
```

### Command-Line Overrides

```bash
# Simple values
sherpack template myapp . --set app.replicas=10

# Nested values
sherpack template myapp . --set image.repository=myregistry/myapp

# Multiple sets
sherpack template myapp . \
  --set app.replicas=5 \
  --set image.tag=v2.0.0 \
  --set service.type=LoadBalancer
```

---

## Schema Validation

### Creating a Schema

Create `values.schema.yaml`:

```yaml
$schema: http://json-schema.org/draft-07/schema#
type: object
required:
  - app
  - image

properties:
  app:
    type: object
    required:
      - name
    properties:
      name:
        type: string
        description: Application name
        pattern: "^[a-z][a-z0-9-]*$"
        default: mywebapp
      replicas:
        type: integer
        minimum: 1
        maximum: 100
        default: 2

  image:
    type: object
    required:
      - repository
      - tag
    properties:
      repository:
        type: string
        description: Container image repository
      tag:
        type: string
        description: Container image tag
        default: "latest"
      pullPolicy:
        type: string
        enum: [Always, IfNotPresent, Never]
        default: IfNotPresent

  service:
    type: object
    properties:
      type:
        type: string
        enum: [ClusterIP, NodePort, LoadBalancer]
        default: ClusterIP
      port:
        type: integer
        minimum: 1
        maximum: 65535
        default: 80

  resources:
    type: object
    properties:
      limits:
        type: object
        properties:
          cpu:
            type: string
            pattern: "^[0-9]+m?$"
          memory:
            type: string
            pattern: "^[0-9]+(Mi|Gi)$"
```

### Validating Values

```bash
# Validate against schema
sherpack validate .

# Validate with custom values
sherpack validate . -f production.yaml

# Get JSON output (for CI/CD)
sherpack validate . --json
```

### Schema Benefits

1. **Automatic Defaults**: Values from `default` are applied automatically
2. **Type Checking**: Ensures values have correct types
3. **Constraints**: Validates ranges, patterns, enums
4. **Documentation**: Schema serves as documentation
5. **IDE Support**: JSON Schema enables autocomplete in editors

---

## Deploying to Kubernetes

### Prerequisites

Ensure you have:
- `kubectl` configured with cluster access
- Appropriate RBAC permissions

### Install a Release

```bash
# Basic install
sherpack install myrelease ./mywebapp

# With namespace
sherpack install myrelease ./mywebapp -n production

# With custom values
sherpack install myrelease ./mywebapp \
  -n production \
  -f production.yaml \
  --set app.replicas=5

# Create namespace if needed
sherpack install myrelease ./mywebapp \
  -n production \
  --create-namespace

# Wait for resources to be ready
sherpack install myrelease ./mywebapp --wait --timeout 300

# Atomic install (rollback on failure)
sherpack install myrelease ./mywebapp --atomic

# Dry-run (preview without applying)
sherpack install myrelease ./mywebapp --dry-run
```

### List Releases

```bash
# List in current namespace
sherpack list

# List in all namespaces
sherpack list -A

# List in specific namespace
sherpack list -n production
```

### Check Release Status

```bash
# Basic status
sherpack status myrelease

# With resource details
sherpack status myrelease --show-resources
```

---

## Lifecycle Management

### Upgrade a Release

```bash
# Basic upgrade
sherpack upgrade myrelease ./mywebapp

# With new values
sherpack upgrade myrelease ./mywebapp \
  -f production-v2.yaml \
  --set image.tag=v2.0.0

# Show diff before applying
sherpack upgrade myrelease ./mywebapp --diff

# Reuse values from previous release
sherpack upgrade myrelease ./mywebapp --reuse-values

# Reset to default values
sherpack upgrade myrelease ./mywebapp --reset-values

# Install if not exists (upsert)
sherpack upgrade myrelease ./mywebapp --install
```

### Rollback

```bash
# View release history
sherpack history myrelease

# Rollback to previous revision
sherpack rollback myrelease 1

# Rollback with wait
sherpack rollback myrelease 1 --wait
```

### Uninstall

```bash
# Basic uninstall
sherpack uninstall myrelease

# Keep release history
sherpack uninstall myrelease --keep-history

# Wait for deletion
sherpack uninstall myrelease --wait

# Dry-run
sherpack uninstall myrelease --dry-run
```

---

## Hooks

Hooks run at specific points in the release lifecycle.

### Hook Types

| Hook | When |
|------|------|
| `pre-install` | Before resources are created |
| `post-install` | After resources are created |
| `pre-upgrade` | Before upgrade |
| `post-upgrade` | After upgrade |
| `pre-rollback` | Before rollback |
| `post-rollback` | After rollback |
| `pre-delete` | Before uninstall |
| `post-delete` | After uninstall |
| `test` | On `sherpack test` |

### Creating a Hook

Create `templates/pre-install-job.yaml`:

```yaml
apiVersion: batch/v1
kind: Job
metadata:
  name: {{ release.name }}-db-migrate
  annotations:
    sherpack.io/hook: pre-install,pre-upgrade
    sherpack.io/hook-weight: "0"
    sherpack.io/hook-delete-policy: hook-succeeded
spec:
  template:
    spec:
      containers:
        - name: migrate
          image: {{ values.image.repository }}:{{ values.image.tag }}
          command: ["./migrate.sh"]
          env:
            - name: DATABASE_URL
              valueFrom:
                secretKeyRef:
                  name: {{ release.name }}-db
                  key: url
      restartPolicy: Never
  backoffLimit: 3
```

### Hook Annotations

```yaml
annotations:
  # Hook phase(s) - comma-separated
  sherpack.io/hook: pre-install,pre-upgrade

  # Execution order (lower = earlier)
  sherpack.io/hook-weight: "0"

  # Cleanup policy
  sherpack.io/hook-delete-policy: hook-succeeded
  # Options: hook-succeeded, hook-failed, before-hook-creation
```

---

## Shared Helpers

### Creating Helper Macros

Create `templates/_helpers.tpl`:

```jinja
{# Generate standard labels #}
{% macro labels() %}
app.kubernetes.io/name: {{ values.app.name }}
app.kubernetes.io/instance: {{ release.name }}
app.kubernetes.io/version: {{ pack.version }}
app.kubernetes.io/managed-by: sherpack
{% endmacro %}

{# Generate selector labels #}
{% macro selectorLabels() %}
app.kubernetes.io/name: {{ values.app.name }}
app.kubernetes.io/instance: {{ release.name }}
{% endmacro %}

{# Full name with truncation #}
{% macro fullname() %}
{{- release.name | trunc(63) -}}
{% endmacro %}

{# Generate image string #}
{% macro image() %}
{{- values.image.repository }}:{{ values.image.tag -}}
{% endmacro %}
```

### Using Helpers

In `templates/deployment.yaml`:

```yaml
{% from "_helpers.tpl" import labels, selectorLabels, fullname, image %}

apiVersion: apps/v1
kind: Deployment
metadata:
  name: {{ fullname() }}
  labels:
    {{ labels() | indent(4) }}
spec:
  selector:
    matchLabels:
      {{ selectorLabels() | indent(6) }}
  template:
    metadata:
      labels:
        {{ selectorLabels() | indent(8) }}
    spec:
      containers:
        - name: app
          image: {{ image() }}
```

---

## Packaging and Distribution

### Create an Archive

```bash
# Package the pack
sherpack package ./mywebapp

# Custom output name
sherpack package ./mywebapp -o mywebapp-1.0.0.tar.gz
```

### Inspect an Archive

```bash
# Show contents
sherpack inspect mywebapp-1.0.0.tar.gz

# Show checksums
sherpack inspect mywebapp-1.0.0.tar.gz --checksums
```

### Sign Archives

```bash
# Generate signing keys
sherpack keygen

# Sign the archive
sherpack sign mywebapp-1.0.0.tar.gz -k ~/.sherpack/keys/sherpack.key

# Verify signature
sherpack verify mywebapp-1.0.0.tar.gz -k ~/.sherpack/keys/sherpack.pub
```

### Push to OCI Registry

```bash
# Push to registry
sherpack push mywebapp-1.0.0.tar.gz oci://registry.example.com/packs/mywebapp:1.0.0
```

### Pull from Repository

```bash
# Add repository
sherpack repo add stable https://packs.example.com

# Search for packs
sherpack search nginx

# Pull a pack
sherpack pull stable/nginx:1.0.0
```

---

## Migrating from Helm

### Convert a Helm Chart

```bash
# Convert Helm chart to Sherpack
sherpack convert ./my-helm-chart

# Specify output directory
sherpack convert ./my-helm-chart -o ./my-sherpack-pack

# Preview without writing
sherpack convert ./my-helm-chart --dry-run

# Verbose output
sherpack convert ./my-helm-chart -v
```

### Conversion Examples

| Helm (Go Template) | Sherpack (Jinja2) |
|-------------------|-------------------|
| `{{ .Values.name }}` | `{{ values.name }}` |
| `{{ .Release.Name }}` | `{{ release.name }}` |
| `{{ .Release.Namespace }}` | `{{ release.namespace }}` |
| `{{ .Chart.Name }}` | `{{ pack.name }}` |
| `{{ .Chart.Version }}` | `{{ pack.version }}` |
| `{{- if .Values.enabled }}` | `{% if values.enabled %}` |
| `{{- else }}` | `{% else %}` |
| `{{- end }}` | `{% endif %}` |
| `{{ range .Values.items }}` | `{% for item in values.items %}` |
| `{{ . }}` (in range) | `{{ item }}` |
| `{{- end }}` | `{% endfor %}` |
| `{{ include "helper" . }}` | `{{ helper() }}` |
| `{{- define "helper" }}` | `{% macro helper() %}` |
| `{{ toYaml .Values \| nindent 2 }}` | `{{ values \| toyaml \| nindent(2) }}` |
| `{{ default "foo" .Values.x }}` | `{{ values.x \| default("foo") }}` |
| `{{ .Values.x \| quote }}` | `{{ values.x \| quote }}` |

### What Gets Converted

- `Chart.yaml` → `Pack.yaml`
- `values.yaml` → `values.yaml` (unchanged)
- `templates/*.yaml` → `templates/*.yaml` (converted)
- `templates/_helpers.tpl` → `templates/_helpers.tpl` (macros)
- `templates/NOTES.txt` → `templates/NOTES.txt` (converted)

---

## Best Practices

### 1. Always Use Schema Validation

```yaml
# values.schema.yaml provides:
# - Documentation
# - Default values
# - Type safety
# - IDE autocomplete
```

### 2. Use Meaningful Labels

```yaml
labels:
  app.kubernetes.io/name: {{ values.app.name }}
  app.kubernetes.io/instance: {{ release.name }}
  app.kubernetes.io/version: {{ pack.version }}
  app.kubernetes.io/component: backend
  app.kubernetes.io/part-of: myplatform
  app.kubernetes.io/managed-by: sherpack
```

### 3. Use Helper Macros

```jinja
{# _helpers.tpl #}
{% macro labels() %}
app.kubernetes.io/name: {{ values.app.name }}
...
{% endmacro %}
```

### 4. Handle Optional Values Safely

```yaml
# Use default filter
{{ values.optional | default("fallback") }}

# Use conditionals
{% if values.optional is defined and values.optional %}
optional: {{ values.optional }}
{% endif %}

# Use get function
{{ get(values, "deep.nested.optional", "default") }}
```

### 5. Use `required` for Mandatory Values

```yaml
image: {{ values.image.repository | required }}:{{ values.image.tag | required }}
```

### 6. Document Your Values

```yaml
# values.yaml
app:
  # Name of the application (used in labels and resource names)
  name: mywebapp

  # Number of pod replicas
  # Minimum: 1, Maximum: 100
  replicas: 2
```

### 7. Use Consistent Naming

```yaml
# Good: consistent naming
{{ release.name }}-deployment
{{ release.name }}-service
{{ release.name }}-configmap

# Bad: inconsistent
{{ values.app.name }}-deploy
{{ release.name }}-svc
my-configmap
```

### 8. Test Before Deploying

```bash
# Always validate and lint
sherpack lint ./mypack
sherpack validate ./mypack

# Preview output
sherpack template myrelease ./mypack

# Dry-run install
sherpack install myrelease ./mypack --dry-run
```

### 9. Use Atomic Installs in CI/CD

```bash
sherpack install myrelease ./mypack \
  --atomic \
  --wait \
  --timeout 300
```

### 10. Version Your Packs

```yaml
# Pack.yaml
metadata:
  name: mywebapp
  version: 1.2.3    # Semantic versioning
  appVersion: "2.0.0"
```

---

## Quick Reference

### Common Commands

```bash
# Development
sherpack create mypack          # Create new pack
sherpack lint ./mypack          # Validate structure
sherpack validate ./mypack      # Validate values
sherpack template rel ./mypack  # Render templates

# Deployment
sherpack install rel ./mypack   # Install
sherpack upgrade rel ./mypack   # Upgrade
sherpack rollback rel 1         # Rollback
sherpack uninstall rel          # Uninstall

# Inspection
sherpack list                   # List releases
sherpack status rel             # Show status
sherpack history rel            # Show history

# Packaging
sherpack package ./mypack       # Create archive
sherpack inspect archive.tar.gz # Show contents

# Conversion
sherpack convert ./helm-chart   # Convert from Helm
```

### Template Cheatsheet

```yaml
# Variables
{{ values.key }}
{{ release.name }}
{{ release.namespace }}
{{ pack.name }}
{{ pack.version }}

# Filters
{{ value | upper }}
{{ value | quote }}
{{ value | b64encode }}
{{ obj | toyaml | nindent(4) }}
{{ value | default("fallback") }}
{{ value | required }}

# Control
{% if condition %}...{% endif %}
{% for item in list %}...{% endfor %}
{% with var = value %}...{% endwith %}

# Macros
{% macro name(args) %}...{% endmacro %}
{% from "_helpers.tpl" import name %}
{{ name(args) }}
```

---

## Next Steps

- Read the [CLI Reference](/docs/cli-reference) for all commands
- Explore the [Architecture](/docs/architecture) to understand internals
- Check [examples](https://github.com/alegeay/sherpack/tree/main/fixtures) for more pack samples

---

*Happy packaging!*
