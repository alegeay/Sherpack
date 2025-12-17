<div align="center">

# Sherpack

**A blazingly fast Kubernetes package manager with Jinja2 templating**

[![Rust](https://img.shields.io/badge/rust-1.85%2B-orange.svg?style=flat-square&logo=rust)](https://www.rust-lang.org/)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg?style=flat-square)](LICENSE)
[![Build](https://img.shields.io/badge/build-passing-brightgreen.svg?style=flat-square)]()
[![Tests](https://img.shields.io/badge/tests-21%20passed-brightgreen.svg?style=flat-square)]()
[![Binary Size](https://img.shields.io/badge/binary-3.1MB-purple.svg?style=flat-square)]()

*A modern Helm alternative written in Rust, featuring familiar Jinja2 templating syntax*

[Features](#-features) •
[Installation](#-installation) •
[Quick Start](#-quick-start) •
[Templating](#-templating-reference) •
[CLI](#-cli-reference)

</div>

---

## Why Sherpack?

| Feature | Sherpack | Helm |
|---------|----------|------|
| **Templating** | Jinja2 (familiar syntax) | Go templates (complex) |
| **Performance** | Native Rust binary | Go runtime |
| **Binary Size** | ~3 MB | ~50 MB |
| **Learning Curve** | Minimal (if you know Jinja2) | Steep |
| **Dependencies** | None | None |

## Features

- **Jinja2 Templating** - Familiar Python-like syntax with `{{ }}` and `{% %}`
- **Helm-Compatible Filters** - `toyaml`, `tojson`, `b64encode`, `indent`, `nindent`, `quote`, and more
- **Rich Function Library** - `get()`, `ternary()`, `now()`, `uuidv4()`, `tostring()`
- **Strict Mode** - Catch undefined variables before deployment
- **Fast** - Written in Rust with zero runtime dependencies
- **Small** - Single 3MB binary

---

## Installation

### From Source

```bash
# Clone the repository
git clone https://github.com/sherpack/sherpack.git
cd sherpack

# Build release binary
cargo build --release

# Install to your PATH
cp target/release/sherpack ~/.local/bin/
```

### Requirements

- Rust 1.85+ (Edition 2024)

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

### 4. Render Templates

```bash
# Basic rendering
sherpack template myrelease ./myapp

# With namespace
sherpack template myrelease ./myapp -n production

# With value overrides
sherpack template myrelease ./myapp --set app.replicas=5

# With external values file
sherpack template myrelease ./myapp -f production-values.yaml

# Output to files
sherpack template myrelease ./myapp -o ./manifests/
```

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

---

### Filters

#### Serialization

| Filter | Description | Example |
|--------|-------------|---------|
| `toyaml` | Object → YAML string | `{{ values.config \| toyaml }}` |
| `tojson` | Object → JSON (compact) | `{{ values.env \| tojson }}` |
| `tojson_pretty` | Object → JSON (formatted) | `{{ values.config \| tojson_pretty }}` |

<details>
<summary><b>Example</b></summary>

```yaml
# Input: values.config = {server: {port: 8080}}
config.yaml: |
{{ values.config | toyaml | indent(2) }}

# Output:
config.yaml: |
  server:
    port: 8080
```
</details>

#### Encoding

| Filter | Description | Example |
|--------|-------------|---------|
| `b64encode` | Base64 encode | `{{ secret \| b64encode }}` |
| `b64decode` | Base64 decode | `{{ encoded \| b64decode }}` |
| `sha256` | SHA256 hash | `{{ data \| sha256 }}` |

<details>
<summary><b>Example</b></summary>

```yaml
# Kubernetes Secret
apiVersion: v1
kind: Secret
data:
  password: {{ values.secrets.password | b64encode }}
  # Output: cGFzc3dvcmQxMjM=
```
</details>

#### Strings

| Filter | Description | Example |
|--------|-------------|---------|
| `quote` | Wrap in double quotes | `{{ name \| quote }}` → `"myapp"` |
| `squote` | Wrap in single quotes | `{{ name \| squote }}` → `'myapp'` |
| `upper` | UPPERCASE | `{{ name \| upper }}` |
| `lower` | lowercase | `{{ name \| lower }}` |
| `snakecase` | Convert to snake_case | `{{ "myApp" \| snakecase }}` → `my_app` |
| `kebabcase` | Convert to kebab-case | `{{ "myApp" \| kebabcase }}` → `my-app` |
| `trunc(n)` | Truncate to n chars | `{{ hash \| trunc(8) }}` |
| `trimprefix(s)` | Remove prefix | `{{ "/api" \| trimprefix("/") }}` → `api` |
| `trimsuffix(s)` | Remove suffix | `{{ "app.yaml" \| trimsuffix(".yaml") }}` → `app` |

#### Indentation

| Filter | Description | Example |
|--------|-------------|---------|
| `indent(n)` | Add n spaces to each line | `{{ yaml \| indent(4) }}` |
| `nindent(n)` | Newline + indent | `{{ yaml \| nindent(8) }}` |

<details>
<summary><b>Example</b></summary>

```yaml
# Using nindent for nested YAML
spec:
  containers:
    - name: app
      resources:
        {{ values.resources | toyaml | nindent(8) }}
```
</details>

#### Objects & Collections

| Filter | Description | Example |
|--------|-------------|---------|
| `keys` | Get object keys | `{{ env \| keys }}` → `["LOG_LEVEL", "DEBUG"]` |
| `haskey(k)` | Check key exists | `{% if obj \| haskey("tls") %}` |
| `merge(obj)` | Merge two objects | `{{ defaults \| merge(overrides) }}` |
| `dictsort` | Sort for iteration | `{% for k,v in obj \| dictsort %}` |

<details>
<summary><b>Example</b></summary>

```yaml
# Iterate over object (dictsort required)
labels:
  {% for key, value in values.labels | dictsort %}
  {{ key }}: {{ value | quote }}
  {% endfor %}
```
</details>

#### Validation

| Filter | Description | Example |
|--------|-------------|---------|
| `required` | Fail if undefined/empty | `{{ values.name \| required }}` |
| `empty` | Check if empty | `{% if values.list \| empty %}` |

---

### Functions

#### Data Access

| Function | Description | Example |
|----------|-------------|---------|
| `get(obj, key, default)` | Safe access with default | `{{ get(values, "opt", "fallback") }}` |
| `ternary(true, false, cond)` | Conditional value | `{{ ternary("prod", "dev", is_prod) }}` |

<details>
<summary><b>Example</b></summary>

```yaml
# Safe access with default
timeout: {{ get(values, "timeout", 30) }}

# Conditional value
environment: {{ ternary("production", "development", release.namespace == "production") }}
```
</details>

#### Type Conversion

| Function | Description | Example |
|----------|-------------|---------|
| `tostring(v)` | Convert to string | `{{ tostring(8080) }}` → `"8080"` |
| `toint(v)` | Convert to integer | `{{ toint("42") }}` → `42` |
| `tofloat(v)` | Convert to float | `{{ tofloat("3.14") }}` → `3.14` |

#### Generation

| Function | Description | Example |
|----------|-------------|---------|
| `now()` | Current ISO timestamp | `{{ now() }}` → `2024-01-15T10:30:00Z` |
| `uuidv4()` | Random UUID | `{{ uuidv4() }}` → `550e8400-e29b-41d4-...` |

#### Error Handling

| Function | Description | Example |
|----------|-------------|---------|
| `fail(msg)` | Fail with message | `{{ fail("Missing required field") }}` |

---

### Control Structures

#### Conditionals

```jinja
{% if values.ingress.enabled %}
apiVersion: networking.k8s.io/v1
kind: Ingress
metadata:
  name: {{ release.name }}
  {% if values.ingress.tls.enabled %}
  annotations:
    cert-manager.io/cluster-issuer: letsencrypt
  {% endif %}
{% endif %}
```

#### Loops

```jinja
{# Loop over a list #}
{% for port in values.ports %}
- name: {{ port.name }}
  port: {{ port.port }}
{% endfor %}

{# Loop over an object (use dictsort) #}
{% for key, value in values.labels | dictsort %}
{{ key }}: {{ value | quote }}
{% endfor %}

{# Loop with index #}
{% for item in items %}
{{ loop.index }}: {{ item }}
{% endfor %}
```

#### Variables

```jinja
{# Set a variable #}
{% set fullName = release.name ~ "-" ~ pack.version %}

{# String concatenation with ~ #}
{{ release.name ~ "-v" ~ pack.version ~ "-" ~ release.namespace }}
```

#### Whitespace Control

```jinja
{#- Comment that trims whitespace before -#}
{%- if condition -%}   {# Trims both sides #}
{% endif %}
```

---

## Pack Structure

```
mypack/
├── Pack.yaml           # Required: Pack metadata
├── values.yaml         # Required: Default values
├── values.schema.yaml  # Optional: Values JSON schema
└── templates/          # Required: Template directory
    ├── deployment.yaml
    ├── service.yaml
    ├── configmap.yaml
    └── _helpers.tpl    # Optional: Template helpers
```

### Pack.yaml

```yaml
apiVersion: sherpack/v1
kind: application
metadata:
  name: myapp
  version: 1.0.0
  description: My awesome application
  appVersion: "2.0.0"

# Optional: Template engine settings
engine:
  strict: true  # Fail on undefined variables
```

---

## CLI Reference

```
sherpack - A Helm-like Kubernetes package manager

Usage: sherpack <COMMAND>

Commands:
  create    Create a new pack
  template  Render templates locally
  lint      Validate a pack
  show      Show pack information
  help      Print help information

Options:
  -h, --help     Print help
  -V, --version  Print version
```

### `template`

Render templates to stdout or files.

```bash
sherpack template <NAME> <PACK> [OPTIONS]

Arguments:
  <NAME>  Release name
  <PACK>  Path to pack directory

Options:
  -n, --namespace <NS>     Target namespace [default: default]
  -f, --values <FILE>      Values file (can be repeated)
      --set <KEY=VALUE>    Override values (can be repeated)
  -o, --output <DIR>       Output directory
  -s, --show-only <NAME>   Only render specified template
      --show-values        Display computed values
      --debug              Show debug information
```

### `lint`

Validate pack structure and templates.

```bash
sherpack lint <PACK> [OPTIONS]

Options:
  --strict  Fail on undefined variables
```

### `show`

Display pack information.

```bash
sherpack show pack <PACK>    # Show Pack.yaml metadata
sherpack show values <PACK>  # Show default values
```

### `create`

Scaffold a new pack.

```bash
sherpack create <NAME>
```

---

## Examples

### Complete Deployment

```yaml
# templates/deployment.yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: {{ release.name }}
  namespace: {{ release.namespace }}
  labels:
    app.kubernetes.io/name: {{ release.name }}
    app.kubernetes.io/version: {{ pack.version }}
    {% for key, value in values.labels | dictsort %}
    {{ key }}: {{ value | quote }}
    {% endfor %}
  annotations:
    checksum/config: {{ values.config | tojson | sha256 | trunc(16) }}
spec:
  replicas: {{ values.app.replicas }}
  selector:
    matchLabels:
      app.kubernetes.io/name: {{ release.name }}
  template:
    metadata:
      labels:
        app.kubernetes.io/name: {{ release.name }}
    spec:
      containers:
        - name: {{ values.app.name | kebabcase }}
          image: {{ values.image.repository }}:{{ values.image.tag }}
          ports:
            {% for p in values.ports %}
            - name: {{ p.name }}
              containerPort: {{ p.targetPort }}
            {% endfor %}
          env:
            {% for key, value in values.env | dictsort %}
            - name: {{ key }}
              value: {{ value | quote }}
            {% endfor %}
          resources:
            {{ values.resources | toyaml | nindent(12) }}
```

### Kubernetes Secret with Base64

```yaml
# templates/secret.yaml
apiVersion: v1
kind: Secret
metadata:
  name: {{ release.name }}-secrets
  namespace: {{ release.namespace }}
type: Opaque
data:
  api-key: {{ values.secrets.apiKey | b64encode }}
  {% if values.secrets.dbPassword %}
  db-password: {{ values.secrets.dbPassword | b64encode }}
  {% endif %}
```

### Conditional Ingress

```yaml
# templates/ingress.yaml
{% if values.ingress.enabled %}
apiVersion: networking.k8s.io/v1
kind: Ingress
metadata:
  name: {{ release.name }}
  annotations:
    kubernetes.io/ingress.class: nginx
    {% if values.ingress.tls.enabled %}
    cert-manager.io/cluster-issuer: letsencrypt-prod
    {% endif %}
spec:
  {% if values.ingress.tls.enabled %}
  tls:
    - hosts:
        {% for host in values.ingress.hosts %}
        - {{ host }}
        {% endfor %}
      secretName: {{ values.ingress.tls.secretName }}
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

## Try the Demo Pack

```bash
# Clone and build
git clone https://github.com/sherpack/sherpack.git
cd sherpack
cargo build --release

# Run the demo
./target/release/sherpack template my-release fixtures/demo-pack

# Try different namespaces
./target/release/sherpack template my-release fixtures/demo-pack -n production

# Override values
./target/release/sherpack template my-release fixtures/demo-pack --set app.replicas=5
```

---

## Project Status

### Phase 1 - MVP Templating

| Feature | Status |
|---------|--------|
| Pack structure (Pack.yaml, values.yaml, templates/) | Complete |
| Jinja2 templating engine (MiniJinja) | Complete |
| Helm-compatible filters | Complete |
| Custom functions | Complete |
| CLI `template` command | Complete |
| CLI `lint` command | Complete |
| CLI `show` command | Complete |
| CLI `create` command | Complete |
| 21 unit tests | Complete |

### Roadmap

- **Phase 2**: Schema validation, improved error messages
- **Phase 3**: `package` command, signatures
- **Phase 4**: `install`, `upgrade`, `uninstall` commands
- **Phase 5**: OCI registry support, dependencies

---

## Architecture

```
sherpack/
├── crates/
│   ├── sherpack-core/     # Core types: Pack, Values, Context
│   ├── sherpack-engine/   # Template engine, filters, functions
│   └── sherpack-cli/      # CLI commands
├── fixtures/
│   ├── simple-pack/       # Basic test fixture
│   └── demo-pack/         # Comprehensive demo
└── tests/                 # Integration tests
```

### Tech Stack

- **Rust 2024** - Latest edition with rust-version 1.85
- **MiniJinja** - Fast Jinja2 template engine
- **Clap** - CLI argument parsing
- **Serde** - Serialization (YAML, JSON)
- **Miette** - Beautiful error reporting

---

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

```bash
# Run tests
cargo test --workspace

# Run lints
cargo clippy --workspace

# Format code
cargo fmt --all
```

---

## License

Apache-2.0

---

<div align="center">

**[Back to top](#sherpack)**

Made with Rust

</div>
