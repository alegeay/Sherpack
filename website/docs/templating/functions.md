---
id: functions
title: Functions
sidebar_position: 3
---

# Functions

Functions are called with parentheses and can take arguments.

## Data Access

### get

Safe access with default value:

```yaml
# Simple access
timeout: {{ get(values, "timeout", 30) }}

# Nested access with dot notation
port: {{ get(values, "service.port", 80) }}

# Object access
host: {{ get(values.ingress, "host", "localhost") }}
```

### ternary

Conditional value selection:

```yaml
# ternary(true_value, false_value, condition)
env: {{ ternary("production", "development", release.namespace == "prod") }}

# Common use cases
replicas: {{ ternary(3, 1, values.highAvailability) }}
pullPolicy: {{ ternary("Always", "IfNotPresent", values.image.tag == "latest") }}
```

## Type Conversion

### tostring

Convert to string:

```yaml
port: {{ tostring(values.port) }}  # "8080"
```

### toint

Convert to integer:

```yaml
replicas: {{ toint(values.replicas) }}  # 3
```

### tofloat

Convert to float:

```yaml
ratio: {{ tofloat(values.ratio) }}  # 0.5
```

## Generation

### now

Current ISO timestamp:

```yaml
annotations:
  deployed-at: {{ now() }}
  # Output: 2024-01-15T10:30:00Z
```

### uuidv4

Generate random UUID:

```yaml
metadata:
  annotations:
    deployment-id: {{ uuidv4() }}
    # Output: 550e8400-e29b-41d4-a716-446655440000
```

### generate_secret

Generate idempotent secrets with various charsets:

```yaml
apiVersion: v1
kind: Secret
metadata:
  name: {{ release.name }}-secrets
type: Opaque
data:
  # Alphanumeric (default) - 24 characters
  db-password: {{ generate_secret("db-password", 24) | b64encode }}

  # Hexadecimal - 32 characters
  api-key: {{ generate_secret("api-key", 32, "hex") | b64encode }}

  # Numeric only - 6 digits
  pin-code: {{ generate_secret("pin-code", 6, "numeric") | b64encode }}

  # Letters only
  token: {{ generate_secret("token", 16, "alpha") | b64encode }}
```

**Signature:** `generate_secret(name, length, charset?)`

| Charset | Characters | Example |
|---------|------------|---------|
| `alphanumeric` (default) | `a-zA-Z0-9` | `ZyitwTXQeYUNX5tC` |
| `hex` | `0-9a-f` | `3b56ff6fe00929f0` |
| `numeric` | `0-9` | `529607` |
| `alpha` | `a-zA-Z` | `QeYUNXtCuvmTB` |
| `base64` | Base64 alphabet | `+/aB3xZ=` |
| `urlsafe` | URL-safe Base64 | `_-aB3xZ` |

**Key feature: Idempotent** - The same name always returns the same value within a render session:

```yaml
# All three calls return the SAME value
first: {{ generate_secret("shared-key", 16) }}
second: {{ generate_secret("shared-key", 16) }}
third: {{ generate_secret("shared-key", 16) }}
```

:::tip GitOps Compatible
Unlike Helm's `randAlphaNum`, `generate_secret` is designed for GitOps workflows.
The state can be persisted between renders, ensuring secrets don't change on every upgrade.
:::

## Parsing

### fromjson

Parse a JSON string into a value (Helm-compatible `fromJson`):

```yaml
# Parse a JSON string from values
{% set parsed = fromjson(values.json_blob) %}
host: {{ parsed.database.host }}

# Inline literal
{{ fromjson('{"name":"test","port":8080}').port }}
# Output: 8080
```

Available as both a filter (`values.json_blob | fromjson`) and a function.
Errors out if the input is not valid JSON.

### fromyaml

Parse a YAML string into a value (Helm-compatible `fromYaml`):

```yaml
{% set config = fromyaml(values.raw_yaml) %}
{{ config | toyaml }}

# Inline literal
{{ fromyaml('a:\n  b: deep').a.b }}
# Output: deep
```

Available as both a filter and a function.

## Cluster Lookup

### lookup

Read existing cluster resources at render time (Helm-compatible). See
[the dedicated guide](https://github.com/alegeay/Sherpack/blob/main/docs/LOOKUP.md)
for the full contract, gotchas, and alternatives.

**Signature:** `lookup(api_version, kind, namespace, name)`

```yaml
# Reuse an existing TLS secret if it already exists
{%- set existing = lookup("v1", "Secret", release.namespace, release.name ~ "-tls") %}
data:
  tls.crt: {{ existing.data["tls.crt"] if existing else "" }}
  tls.key: {{ existing.data["tls.key"] if existing else "" }}

# Conditional install if a CRD is present
{%- if lookup("apiextensions.k8s.io/v1", "CustomResourceDefinition", "", "issuers.cert-manager.io") %}
apiVersion: cert-manager.io/v1
kind: Issuer
metadata:
  name: {{ release.name }}
{%- endif %}

# List mode (empty name argument)
{%- set pods = lookup("v1", "Pod", "production", "") %}
podCount: {{ pods["items"] | length }}
```

| Mode | Behavior |
|---|---|
| `sherpack template` | Always returns `{}` (no cluster access). |
| `sherpack install/upgrade` | Queries the cluster live. |
| Resource not found / 403 / 404 / timeout | Returns `{}` silently. |
| Empty `name` argument | Returns `{items: [...]}` (list mode). |

**Configurable timeout** (default 5s):

```bash
SHERPACK_LOOKUP_TIMEOUT_SECS=15 sherpack install myapp ./pack
```

:::warning Non-deterministic by design
Templates that use `lookup` produce different manifests against different
clusters. Prefer `generate_secret` for random values, and explicit values
for cluster-state-aware logic. See [LOOKUP.md](https://github.com/alegeay/Sherpack/blob/main/docs/LOOKUP.md)
for migration patterns.
:::

## Error Handling

### fail

Fail with custom error message:

```yaml
{% if not values.required.field %}
{{ fail("required.field must be set") }}
{% endif %}

# With condition
{{ fail("Database password required") if not values.db.password }}
```

## Usage Examples

### Safe Nested Access

```yaml
# Instead of crashing on missing keys
apiVersion: {{ get(values, "apiVersion", "apps/v1") }}
kind: {{ get(values, "kind", "Deployment") }}

# Deeply nested
tlsSecret: {{ get(values, "ingress.tls.secretName", release.name ~ "-tls") }}
```

### Dynamic Configuration

```yaml
# Environment-based settings
{% set isProd = release.namespace == "production" %}

spec:
  replicas: {{ ternary(3, 1, isProd) }}
  template:
    spec:
      containers:
        - resources:
            limits:
              cpu: {{ ternary("1000m", "100m", isProd) }}
              memory: {{ ternary("1Gi", "256Mi", isProd) }}
```

### Validation with fail

```yaml
# Require certain values
{% if not values.image.repository %}
{{ fail("image.repository is required") }}
{% endif %}

{% if values.replicas > 10 %}
{{ fail("replicas cannot exceed 10") }}
{% endif %}

# Validate combinations
{% if values.ingress.enabled and not values.ingress.host %}
{{ fail("ingress.host is required when ingress is enabled") }}
{% endif %}
```

### Type Coercion

```yaml
# Ensure string for annotations
annotations:
  replicas: {{ tostring(values.replicas) }}

# Ensure integer for spec
spec:
  replicas: {{ toint(values.replicas) }}
```

### Deployment Tracking

```yaml
metadata:
  annotations:
    # Unique deployment identifier
    deployment.kubernetes.io/revision: {{ uuidv4() | trunc(8) }}

    # Timestamp for tracking
    deployed-at: {{ now() }}
```
