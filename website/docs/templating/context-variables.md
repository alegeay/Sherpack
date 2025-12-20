---
id: context-variables
title: Context Variables
sidebar_position: 1
---

# Context Variables

Templates have access to several built-in context variables.

## Available Variables

### values

Configuration values from all sources (schema defaults, values.yaml, -f files, --set flags):

```yaml
name: {{ values.app.name }}
replicas: {{ values.app.replicas }}
image: {{ values.image.repository }}:{{ values.image.tag }}
```

### release

Information about the current release:

| Variable | Description | Example |
|----------|-------------|---------|
| `release.name` | Release name from CLI | `myapp` |
| `release.namespace` | Target namespace | `production` |
| `release.revision` | Revision number | `1` |
| `release.isUpgrade` | True if upgrade | `false` |
| `release.isInstall` | True if install | `true` |

```yaml
metadata:
  name: {{ release.name }}
  namespace: {{ release.namespace }}
  labels:
    app.kubernetes.io/instance: {{ release.name }}
```

### pack

Pack metadata from Pack.yaml:

| Variable | Description | Example |
|----------|-------------|---------|
| `pack.name` | Pack name | `nginx` |
| `pack.version` | Pack version | `1.0.0` |
| `pack.appVersion` | Application version | `1.25.0` |
| `pack.description` | Pack description | `Web server` |

```yaml
labels:
  app.kubernetes.io/name: {{ pack.name }}
  app.kubernetes.io/version: {{ pack.version }}
  helm.sh/chart: {{ pack.name }}-{{ pack.version }}
```

### capabilities

Kubernetes cluster capabilities:

| Variable | Description | Example |
|----------|-------------|---------|
| `capabilities.kubeVersion` | Kubernetes version | `1.28.0` |
| `capabilities.apiVersions` | Available API versions | `["v1", "apps/v1", ...]` |

```yaml
{% if "networking.k8s.io/v1" in capabilities.apiVersions %}
apiVersion: networking.k8s.io/v1
{% else %}
apiVersion: extensions/v1beta1
{% endif %}
kind: Ingress
```

## Usage Examples

### Full Metadata Block

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: {{ release.name }}
  namespace: {{ release.namespace }}
  labels:
    app.kubernetes.io/name: {{ pack.name }}
    app.kubernetes.io/instance: {{ release.name }}
    app.kubernetes.io/version: {{ pack.appVersion | default(pack.version) }}
    app.kubernetes.io/managed-by: sherpack
  annotations:
    meta.helm.sh/release-name: {{ release.name }}
    meta.helm.sh/release-namespace: {{ release.namespace }}
```

### Conditional Based on Install/Upgrade

```yaml
{% if release.isInstall %}
# First-time installation
annotations:
  sherpack.io/first-deployed: {{ now() }}
{% endif %}

{% if release.isUpgrade %}
# Upgrade from previous version
annotations:
  sherpack.io/upgraded-at: {{ now() }}
{% endif %}
```

### Version-Based API Selection

```yaml
{% set kubeVersion = capabilities.kubeVersion | replace("v", "") %}
{% if kubeVersion >= "1.19" %}
apiVersion: networking.k8s.io/v1
{% else %}
apiVersion: networking.k8s.io/v1beta1
{% endif %}
kind: Ingress
```
