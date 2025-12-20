---
id: schema-validation
title: Schema Validation
sidebar_position: 4
---

# Schema Validation

Sherpack supports JSON Schema validation for values, helping catch configuration errors before deployment.

## Creating a Schema

Create `values.schema.yaml` in your pack:

```yaml title="values.schema.yaml"
$schema: http://json-schema.org/draft-07/schema#
type: object
properties:
  app:
    type: object
    properties:
      name:
        type: string
        minLength: 1
        description: Application name
      replicas:
        type: integer
        minimum: 1
        maximum: 10
        default: 3
        description: Number of replicas
    required:
      - name

  image:
    type: object
    properties:
      repository:
        type: string
        pattern: "^[a-z0-9.-/]+$"
      tag:
        type: string
        default: latest
    required:
      - repository

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

required:
  - app
  - image
```

## Schema Features

### Types

```yaml
properties:
  count:
    type: integer
  enabled:
    type: boolean
  name:
    type: string
  tags:
    type: array
    items:
      type: string
  config:
    type: object
```

### Constraints

```yaml
properties:
  replicas:
    type: integer
    minimum: 1
    maximum: 100
  name:
    type: string
    minLength: 1
    maxLength: 63
    pattern: "^[a-z][a-z0-9-]*$"
  ports:
    type: array
    minItems: 1
    maxItems: 10
```

### Defaults

```yaml
properties:
  replicas:
    type: integer
    default: 3  # Used if not provided
  tag:
    type: string
    default: latest
```

### Enums

```yaml
properties:
  pullPolicy:
    type: string
    enum:
      - Always
      - IfNotPresent
      - Never
    default: IfNotPresent
```

## Validation Commands

### Validate

```bash
# Validate with schema
sherpack validate ./mypack

# With overrides
sherpack validate ./mypack --set app.replicas=100

# JSON output
sherpack validate ./mypack --json
```

### Lint

```bash
# Lint includes schema validation
sherpack lint ./mypack

# Skip schema validation
sherpack lint ./mypack --skip-schema
```

### Template

```bash
# Template validates by default
sherpack template myapp ./mypack

# Skip validation
sherpack template myapp ./mypack --skip-schema
```

## Error Messages

Sherpack provides helpful error messages:

```
sherpack::cli::validation

  × Validation failed: app.replicas: 100 is greater than the maximum of 10
  help: Check your values against the schema constraints
```

With suggestions for typos:

```
sherpack::cli::template

  × Template error: undefined variable 'value'
  help: Did you mean 'values'? Available: values, release, pack, capabilities
```

## Best Practices

1. **Document with descriptions**
   ```yaml
   properties:
     replicas:
       type: integer
       description: Number of pod replicas to run
   ```

2. **Set sensible defaults**
   ```yaml
   properties:
     tag:
       type: string
       default: latest
   ```

3. **Use patterns for Kubernetes names**
   ```yaml
   properties:
     name:
       type: string
       pattern: "^[a-z][a-z0-9-]{0,62}$"
   ```

4. **Mark required fields**
   ```yaml
   required:
     - name
     - image
   ```
