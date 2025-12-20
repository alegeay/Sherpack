---
id: quick-start
title: Démarrage rapide
sidebar_position: 2
---

# Démarrage rapide

Déployez votre première application avec Sherpack.

## Créer un pack

```bash
sherpack create myapp
cd myapp
```

Structure générée :

```
myapp/
├── Pack.yaml
├── values.yaml
└── templates/
    ├── deployment.yaml
    ├── service.yaml
    └── _helpers.tpl
```

## Examiner les fichiers

### Pack.yaml

```yaml
apiVersion: sherpack/v1
kind: application
metadata:
  name: myapp
  version: 1.0.0
  description: Mon application
```

### values.yaml

```yaml
replicaCount: 1

image:
  repository: nginx
  tag: latest
  pullPolicy: IfNotPresent

service:
  type: ClusterIP
  port: 80
```

### templates/deployment.yaml

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: {{ release.name }}-{{ pack.name }}
spec:
  replicas: {{ values.replicaCount }}
  selector:
    matchLabels:
      app: {{ release.name }}
  template:
    spec:
      containers:
        - name: {{ pack.name }}
          image: {{ values.image.repository }}:{{ values.image.tag }}
          ports:
            - containerPort: {{ values.service.port }}
```

## Prévisualiser les manifestes

```bash
sherpack template my-release .
```

Sortie :

```yaml
---
# Source: myapp/templates/deployment.yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: my-release-myapp
spec:
  replicas: 1
  ...
```

## Personnaliser les valeurs

```bash
# Via fichier
sherpack template my-release . -f custom-values.yaml

# Via ligne de commande
sherpack template my-release . --set replicaCount=3
```

## Installer sur Kubernetes

```bash
# Installation simple
sherpack install my-release . -n default

# Avec attente de déploiement
sherpack install my-release . -n default --wait

# Installation atomique (rollback automatique en cas d'échec)
sherpack install my-release . -n default --atomic
```

## Gérer le release

```bash
# Lister les releases
sherpack list

# Voir le statut
sherpack status my-release

# Mettre à jour
sherpack upgrade my-release . --set replicaCount=5

# Voir l'historique
sherpack history my-release

# Rollback
sherpack rollback my-release 1

# Désinstaller
sherpack uninstall my-release
```

## Prochaines étapes

- [Créer un pack](/docs/getting-started/create-pack) - Guide complet
- [Templating](/docs/concepts/templating) - Syntaxe Jinja2
- [Référence CLI](/docs/cli-reference) - Toutes les commandes
