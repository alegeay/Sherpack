---
id: values
title: Values
sidebar_position: 2
---

# Values

Values are the configuration parameters for your pack. They can be provided from multiple sources and are merged together.

## Value Sources

Values are merged in this order (later sources override earlier ones):

1. **Schema defaults** - From `values.schema.yaml`
2. **values.yaml** - Default pack values
3. **Value files** - Via `-f` or `--values` flag
4. **Set flags** - Via `--set` flag

## Providing Values

### Default Values (values.yaml)

```yaml title="values.yaml"
app:
  name: myapp
  replicas: 3

image:
  repository: nginx
  tag: latest
```

### Value Files

```bash
# Single file
sherpack template myapp ./pack -f production.yaml

# Multiple files (merged in order)
sherpack template myapp ./pack -f base.yaml -f production.yaml -f secrets.yaml
```

### Set Flags

```bash
# Simple value
sherpack template myapp ./pack --set app.replicas=5

# Nested value
sherpack template myapp ./pack --set image.tag=v2.0.0

# Multiple values
sherpack template myapp ./pack --set app.replicas=5 --set image.tag=v2

# Array index
sherpack template myapp ./pack --set "hosts[0]=example.com"

# String with special characters
sherpack template myapp ./pack --set 'annotation=key\=value'
```

## Accessing Values in Templates

Values are available via the `values` object:

```yaml
name: {{ values.app.name }}
replicas: {{ values.app.replicas }}
image: {{ values.image.repository }}:{{ values.image.tag }}
```

### Safe Access with Default

Use `get()` for optional values:

```yaml
# Returns "default" if values.optional is undefined
timeout: {{ get(values, "timeout", 30) }}

# Nested access
port: {{ get(values.service, "port", 80) }}
```

### Conditional Values

```yaml
{% if values.ingress.enabled %}
apiVersion: networking.k8s.io/v1
kind: Ingress
# ...
{% endif %}
```

## Schema Defaults

If you have a `values.schema.yaml`, default values are extracted automatically:

```yaml title="values.schema.yaml"
properties:
  app:
    properties:
      replicas:
        type: integer
        default: 3  # This becomes the default
```

Even if `values.yaml` doesn't specify `app.replicas`, it will be `3`.

## Viewing Computed Values

See the final merged values:

```bash
sherpack template myapp ./pack --show-values
```

Output:

```yaml
# Computed Values
# ---------------
app:
  name: myapp
  replicas: 3
image:
  repository: nginx
  tag: latest
```
