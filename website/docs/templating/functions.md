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
