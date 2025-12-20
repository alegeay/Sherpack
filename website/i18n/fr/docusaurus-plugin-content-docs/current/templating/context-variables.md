---
id: context-variables
title: Variables de contexte
sidebar_position: 1
---

# Variables de contexte

Les templates ont accès à plusieurs variables de contexte intégrées.

## Variables disponibles

### values

Valeurs de configuration depuis toutes les sources (valeurs par défaut du schema, values.yaml, fichiers -f, flags --set) :

```yaml
name: {{ values.app.name }}
replicas: {{ values.app.replicas }}
image: {{ values.image.repository }}:{{ values.image.tag }}
```

### release

Informations sur le release actuel :

| Variable | Description | Exemple |
|----------|-------------|---------|
| `release.name` | Nom du release depuis la CLI | `myapp` |
| `release.namespace` | Namespace cible | `production` |
| `release.revision` | Numéro de révision | `1` |
| `release.isUpgrade` | Vrai si mise à jour | `false` |
| `release.isInstall` | Vrai si installation | `true` |

```yaml
metadata:
  name: {{ release.name }}
  namespace: {{ release.namespace }}
  labels:
    app.kubernetes.io/instance: {{ release.name }}
```

### pack

Métadonnées du pack depuis Pack.yaml :

| Variable | Description | Exemple |
|----------|-------------|---------|
| `pack.name` | Nom du pack | `nginx` |
| `pack.version` | Version du pack | `1.0.0` |
| `pack.appVersion` | Version de l'application | `1.25.0` |
| `pack.description` | Description du pack | `Web server` |

```yaml
labels:
  app.kubernetes.io/name: {{ pack.name }}
  app.kubernetes.io/version: {{ pack.version }}
  helm.sh/chart: {{ pack.name }}-{{ pack.version }}
```

### capabilities

Capacités du cluster Kubernetes :

| Variable | Description | Exemple |
|----------|-------------|---------|
| `capabilities.kubeVersion` | Version Kubernetes | `1.28.0` |
| `capabilities.apiVersions` | Versions API disponibles | `["v1", "apps/v1", ...]` |

```yaml
{% if "networking.k8s.io/v1" in capabilities.apiVersions %}
apiVersion: networking.k8s.io/v1
{% else %}
apiVersion: extensions/v1beta1
{% endif %}
kind: Ingress
```

## Exemples d'utilisation

### Bloc de métadonnées complet

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

### Condition basée sur installation/mise à jour

```yaml
{% if release.isInstall %}
# Première installation
annotations:
  sherpack.io/first-deployed: {{ now() }}
{% endif %}

{% if release.isUpgrade %}
# Mise à jour depuis la version précédente
annotations:
  sherpack.io/upgraded-at: {{ now() }}
{% endif %}
```

### Sélection d'API basée sur la version

```yaml
{% set kubeVersion = capabilities.kubeVersion | replace("v", "") %}
{% if kubeVersion >= "1.19" %}
apiVersion: networking.k8s.io/v1
{% else %}
apiVersion: networking.k8s.io/v1beta1
{% endif %}
kind: Ingress
```
