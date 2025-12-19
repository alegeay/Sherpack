---
id: quick-start
title: Quick Start
sidebar_position: 2
---

# Quick Start

This guide will walk you through creating your first Sherpack pack and deploying it.

## 1. Create a Pack

```bash
sherpack create myapp
```

This generates:

```
myapp/
├── Pack.yaml           # Pack metadata
├── values.yaml         # Default values
├── values.schema.yaml  # JSON Schema (optional)
└── templates/
    └── deployment.yaml # Kubernetes template
```

## 2. Define Your Values

Edit `values.yaml` with your configuration:

```yaml title="values.yaml"
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

## 3. Create Templates

Edit `templates/deployment.yaml`:

```yaml title="templates/deployment.yaml"
apiVersion: apps/v1
kind: Deployment
metadata:
  name: {{ release.name }}
  namespace: {{ release.namespace }}
spec:
  replicas: {{ values.app.replicas }}
  selector:
    matchLabels:
      app: {{ release.name }}
  template:
    metadata:
      labels:
        app: {{ release.name }}
    spec:
      containers:
        - name: {{ values.app.name | kebabcase }}
          image: {{ values.image.repository }}:{{ values.image.tag }}
          resources:
            {{ values.resources | toyaml | nindent(12) }}
```

## 4. Validate and Render

```bash
# Lint the pack structure
sherpack lint ./myapp

# Validate against schema (if schema exists)
sherpack validate ./myapp

# Render templates locally
sherpack template myrelease ./myapp

# Render with overrides
sherpack template myrelease ./myapp --set app.replicas=5 -n production
```

## 5. Deploy to Kubernetes

```bash
# Install to cluster
sherpack install myrelease ./myapp -n production

# Check status
sherpack status myrelease

# Upgrade with new values
sherpack upgrade myrelease ./myapp --set app.replicas=5

# Rollback if needed
sherpack rollback myrelease 1

# Uninstall
sherpack uninstall myrelease
```

## Next Steps

- Follow the [Complete Tutorial](/docs/getting-started/tutorial) for a hands-on guide
- Learn about [Pack Structure](/docs/concepts/pack-structure)
- Explore [Templating](/docs/templating/context-variables)
- Read the [CLI Reference](/docs/cli-reference)
- [Convert from Helm](/docs/cli-reference#convert) if migrating existing charts
