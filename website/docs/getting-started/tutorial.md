---
id: tutorial
title: Tutorial
sidebar_position: 3
---

# Sherpack Tutorial

Learn Sherpack by building a complete web application pack from scratch.

## Overview

In this tutorial, you'll learn how to:
- Create a pack structure
- Write Jinja2 templates
- Configure values and schemas
- Deploy to Kubernetes
- Manage releases

**Time**: ~20 minutes

---

## Part 1: Create Your Pack

### Initialize the Pack

```bash
sherpack create webapp
cd webapp
```

This creates the basic structure:

```
webapp/
├── Pack.yaml         # Metadata
├── values.yaml       # Default values
└── templates/
    └── deployment.yaml
```

### Define Metadata

Edit `Pack.yaml`:

```yaml
apiVersion: sherpack/v1
kind: application
metadata:
  name: webapp
  version: 1.0.0
  description: A sample web application
  appVersion: "1.0"
```

---

## Part 2: Configure Values

Edit `values.yaml`:

```yaml
# Application
app:
  name: webapp
  replicas: 2

# Image
image:
  repository: nginx
  tag: "1.25-alpine"
  pullPolicy: IfNotPresent

# Service
service:
  type: ClusterIP
  port: 80

# Resources
resources:
  limits:
    cpu: "200m"
    memory: "128Mi"
  requests:
    cpu: "100m"
    memory: "64Mi"

# Optional features
ingress:
  enabled: false
  host: ""
```

---

## Part 3: Write Templates

### Deployment

Replace `templates/deployment.yaml`:

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: {{ release.name }}
  namespace: {{ release.namespace }}
  labels:
    app: {{ values.app.name }}
spec:
  replicas: {{ values.app.replicas }}
  selector:
    matchLabels:
      app: {{ values.app.name }}
      instance: {{ release.name }}
  template:
    metadata:
      labels:
        app: {{ values.app.name }}
        instance: {{ release.name }}
    spec:
      containers:
        - name: {{ values.app.name }}
          image: {{ values.image.repository }}:{{ values.image.tag }}
          imagePullPolicy: {{ values.image.pullPolicy }}
          ports:
            - containerPort: 80
          resources:
            {{ values.resources | toyaml | nindent(12) }}
```

### Service

Create `templates/service.yaml`:

```yaml
apiVersion: v1
kind: Service
metadata:
  name: {{ release.name }}
  namespace: {{ release.namespace }}
spec:
  type: {{ values.service.type }}
  ports:
    - port: {{ values.service.port }}
      targetPort: 80
  selector:
    app: {{ values.app.name }}
    instance: {{ release.name }}
```

### Optional Ingress

Create `templates/ingress.yaml`:

```yaml
{% if values.ingress.enabled %}
apiVersion: networking.k8s.io/v1
kind: Ingress
metadata:
  name: {{ release.name }}
  namespace: {{ release.namespace }}
spec:
  rules:
    - host: {{ values.ingress.host }}
      http:
        paths:
          - path: /
            pathType: Prefix
            backend:
              service:
                name: {{ release.name }}
                port:
                  number: {{ values.service.port }}
{% endif %}
```

---

## Part 4: Test Your Pack

### Validate Structure

```bash
sherpack lint .
```

Expected output:
```
✓ Pack.yaml is valid
✓ values.yaml is valid
✓ Templates render successfully
```

### Preview Output

```bash
# Render with defaults
sherpack template myapp .

# Render with overrides
sherpack template myapp . --set app.replicas=5
```

### Test with Custom Values

Create `values-prod.yaml`:

```yaml
app:
  replicas: 5

image:
  tag: "1.25.3"

service:
  type: LoadBalancer

ingress:
  enabled: true
  host: webapp.example.com
```

```bash
sherpack template myapp . -f values-prod.yaml
```

---

## Part 5: Deploy to Kubernetes

### Install

```bash
# Basic install
sherpack install myapp . -n default

# With production values
sherpack install myapp . -n production -f values-prod.yaml --create-namespace

# With wait and atomic
sherpack install myapp . --wait --atomic --timeout 120
```

### Check Status

```bash
# List releases
sherpack list

# Check status
sherpack status myapp

# View history
sherpack history myapp
```

### Upgrade

```bash
# Upgrade with new values
sherpack upgrade myapp . --set image.tag=1.26-alpine

# Preview changes first
sherpack upgrade myapp . --set image.tag=1.26-alpine --diff --dry-run
```

### Rollback

```bash
# View history
sherpack history myapp

# Rollback to revision 1
sherpack rollback myapp 1
```

### Uninstall

```bash
sherpack uninstall myapp
```

---

## Part 6: Add Schema Validation

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
    properties:
      name:
        type: string
        default: webapp
      replicas:
        type: integer
        minimum: 1
        maximum: 100
        default: 2

  image:
    type: object
    required:
      - repository
    properties:
      repository:
        type: string
      tag:
        type: string
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
        default: 80
```

Now validate:

```bash
sherpack validate .
```

---

## Part 7: Create Helper Macros

Create `templates/_helpers.tpl`:

```jinja
{% macro labels() %}
app.kubernetes.io/name: {{ values.app.name }}
app.kubernetes.io/instance: {{ release.name }}
app.kubernetes.io/version: {{ pack.version }}
app.kubernetes.io/managed-by: sherpack
{% endmacro %}

{% macro selectorLabels() %}
app.kubernetes.io/name: {{ values.app.name }}
app.kubernetes.io/instance: {{ release.name }}
{% endmacro %}
```

Use in templates:

```yaml
{% from "_helpers.tpl" import labels, selectorLabels %}

apiVersion: apps/v1
kind: Deployment
metadata:
  name: {{ release.name }}
  labels:
    {{ labels() | indent(4) }}
spec:
  selector:
    matchLabels:
      {{ selectorLabels() | indent(6) }}
```

---

## Part 8: Package and Distribute

### Create Archive

```bash
sherpack package .
# Creates: webapp-1.0.0.tar.gz
```

### Sign Archive

```bash
# Generate keys (once)
sherpack keygen

# Sign
sherpack sign webapp-1.0.0.tar.gz -k ~/.sherpack/keys/sherpack.key
```

### Push to Registry

```bash
sherpack push webapp-1.0.0.tar.gz oci://registry.example.com/packs/webapp:1.0.0
```

---

## Summary

You've learned how to:

| Task | Command |
|------|---------|
| Create pack | `sherpack create <name>` |
| Validate | `sherpack lint` / `sherpack validate` |
| Preview | `sherpack template <release> <pack>` |
| Install | `sherpack install <release> <pack>` |
| Upgrade | `sherpack upgrade <release> <pack>` |
| Rollback | `sherpack rollback <release> <rev>` |
| Uninstall | `sherpack uninstall <release>` |
| Package | `sherpack package <pack>` |

## Next Steps

- Explore [Templating Reference](/docs/templating/filters) for all filters and functions
- Learn about [CRD Handling](/docs/kubernetes/crd-handling) for advanced Kubernetes operations
- Check [CLI Reference](/docs/cli-reference) for all commands
- Learn about [Converting from Helm](/docs/cli-reference#convert)
