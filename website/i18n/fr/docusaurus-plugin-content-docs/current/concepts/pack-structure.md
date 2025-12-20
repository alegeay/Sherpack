---
id: pack-structure
title: Structure d'un pack
sidebar_position: 1
---

# Structure d'un pack

Un pack Sherpack est une collection de fichiers organisés pour déployer une application Kubernetes.

## Vue d'ensemble

```
myapp/
├── Pack.yaml           # Métadonnées (obligatoire)
├── values.yaml         # Valeurs par défaut (obligatoire)
├── values.schema.yaml  # Schéma de validation (optionnel)
├── README.md           # Documentation (optionnel)
├── LICENSE             # Licence (optionnel)
├── packs/              # Dépendances (optionnel)
└── templates/          # Templates Jinja2 (obligatoire)
    ├── deployment.yaml
    ├── service.yaml
    ├── _helpers.tpl    # Templates partiels
    └── tests/          # Tests de hook
```

## Fichiers obligatoires

### Pack.yaml

Définit les métadonnées du pack :

```yaml
apiVersion: sherpack/v1
kind: application  # ou 'library'
metadata:
  name: myapp
  version: 1.0.0
  description: Mon application web
  appVersion: "2.1.0"
  keywords:
    - web
    - api
  maintainers:
    - name: John Doe
      email: john@example.com
      url: https://example.com
  home: https://myapp.example.com
  sources:
    - https://github.com/example/myapp
  icon: https://example.com/icon.png

dependencies:
  - name: redis
    version: ">=7.0.0"
    repository: https://charts.bitnami.com/bitnami
    condition: redis.enabled
    alias: cache
```

### values.yaml

Les valeurs de configuration par défaut :

```yaml
replicaCount: 1

image:
  repository: myapp
  tag: latest
  pullPolicy: IfNotPresent

service:
  type: ClusterIP
  port: 80

resources:
  limits:
    cpu: 100m
    memory: 128Mi
```

### templates/

Le répertoire contenant les templates Jinja2 :

- Les fichiers `.yaml` sont rendus en manifestes Kubernetes
- Les fichiers commençant par `_` sont des templates partiels (helpers)
- Le répertoire `tests/` contient les tests de hook

## Fichiers optionnels

### values.schema.yaml

Schéma de validation des valeurs :

```yaml
schemaVersion: "1.0"
title: Configuration MyApp
required:
  - image
properties:
  replicaCount:
    type: integer
    minimum: 1
    default: 1
```

### README.md

Documentation du pack (affichée dans les registres).

### LICENSE

Licence du pack.

### packs/

Répertoire des dépendances téléchargées (géré par `sherpack dependency build`).

## Types de pack

### application

Pack standard déployable :

```yaml
apiVersion: sherpack/v1
kind: application
```

### library

Pack réutilisable (templates partiels uniquement) :

```yaml
apiVersion: sherpack/v1
kind: library
```

Les packs library ne produisent pas de manifestes directement mais fournissent des macros et des templates réutilisables.
