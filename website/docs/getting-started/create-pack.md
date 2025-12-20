---
id: create-pack
title: Creating a Pack
sidebar_position: 3
---

# Creating a Pack

A **pack** is Sherpack's equivalent of a Helm chart. It contains all the templates and configuration needed to deploy an application.

## Using the Create Command

```bash
sherpack create myapp
```

This scaffolds a new pack with the following structure:

```
myapp/
├── Pack.yaml           # Pack metadata (required)
├── values.yaml         # Default values (required)
├── values.schema.yaml  # JSON Schema for validation (optional)
└── templates/          # Template directory (required)
    ├── deployment.yaml
    └── service.yaml
```

## Pack.yaml

The `Pack.yaml` file defines your pack's metadata:

```yaml title="Pack.yaml"
apiVersion: sherpack/v1
kind: application
metadata:
  name: myapp
  version: 1.0.0
  description: My awesome application
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

### Pack Types

| Type | Description |
|------|-------------|
| `application` | Standard deployable pack |
| `library` | Shared templates, not directly installable |

## values.yaml

Default configuration values:

```yaml title="values.yaml"
app:
  name: myapp
  replicas: 3

image:
  repository: nginx
  tag: latest
  pullPolicy: IfNotPresent

service:
  type: ClusterIP
  port: 80

resources:
  limits:
    cpu: 500m
    memory: 256Mi
  requests:
    cpu: 100m
    memory: 128Mi

ingress:
  enabled: false
  hosts: []
```

## values.schema.yaml

Optional JSON Schema for validation:

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
      replicas:
        type: integer
        minimum: 1
        maximum: 10
        default: 3
    required: [name]
  image:
    type: object
    properties:
      repository:
        type: string
      tag:
        type: string
        default: latest
```

## Template Files

Templates use Jinja2 syntax and have access to several context variables:

```yaml title="templates/deployment.yaml"
apiVersion: apps/v1
kind: Deployment
metadata:
  name: {{ release.name }}
  namespace: {{ release.namespace }}
  labels:
    app.kubernetes.io/name: {{ release.name }}
    app.kubernetes.io/version: {{ pack.version }}
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
          imagePullPolicy: {{ values.image.pullPolicy }}
          ports:
            - containerPort: 80
          resources:
            {{ values.resources | toyaml | nindent(12) }}
```

## Helper Templates

Create reusable template snippets in `_helpers.tpl`:

```jinja title="templates/_helpers.tpl"
{# Common labels #}
{% macro labels() %}
app.kubernetes.io/name: {{ release.name }}
app.kubernetes.io/instance: {{ release.name }}
app.kubernetes.io/version: {{ pack.version }}
app.kubernetes.io/managed-by: sherpack
{% endmacro %}

{# Selector labels #}
{% macro selectorLabels() %}
app.kubernetes.io/name: {{ release.name }}
app.kubernetes.io/instance: {{ release.name }}
{% endmacro %}
```

Use them in other templates:

```yaml
{% from "_helpers.tpl" import labels, selectorLabels %}

metadata:
  labels:
    {{ labels() | indent(4) }}
```
