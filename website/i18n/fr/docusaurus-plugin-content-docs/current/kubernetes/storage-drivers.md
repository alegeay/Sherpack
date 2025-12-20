---
id: storage-drivers
title: Drivers de stockage
sidebar_position: 5
---

# Drivers de stockage

Sherpack stocke les informations de release en utilisant des backends de stockage configurables.

## Drivers disponibles

| Driver | Emplacement de stockage | Cas d'usage |
|--------|-------------------------|-------------|
| `secrets` | Kubernetes Secrets | Par défaut, production |
| `configmap` | Kubernetes ConfigMaps | Débogage, pas de RBAC pour les secrets |
| `file` | Système de fichiers local | Développement, tests CI |

## Driver Secrets (par défaut)

Stocke les releases comme Kubernetes Secrets :

```bash
# Comportement par défaut
sherpack install myapp ./mypack

# Spécifier explicitement
sherpack install myapp ./mypack --storage secrets
```

### Format Secret

```yaml
apiVersion: v1
kind: Secret
metadata:
  name: sh.sherpack.release.v1.myapp.v1
  namespace: default
  labels:
    owner: sherpack
    name: myapp
    version: "1"
    status: deployed
type: sherpack.io/release.v1
data:
  release: <json-compressé-zstd>
```

### Avantages

- Sécurisé (nécessite RBAC pour les secrets)
- Fonctionne entre équipes
- Survit aux redémarrages de pods

## Driver ConfigMap

Stocke les releases comme ConfigMaps :

```bash
sherpack install myapp ./mypack --storage configmap
```

### Format ConfigMap

```yaml
apiVersion: v1
kind: ConfigMap
metadata:
  name: sh.sherpack.release.v1.myapp.v1
  labels:
    owner: sherpack
    name: myapp
data:
  release: <json-compressé-zstd>
```

### Quand utiliser

- Débogage des données de release
- Clusters sans accès aux secrets
- Environnements de développement

## Driver File

Stocke les releases sur le système de fichiers local :

```bash
sherpack install myapp ./mypack --storage file --storage-path ~/.sherpack/releases
```

### Structure de fichiers

```
~/.sherpack/releases/
└── default/
    └── myapp/
        ├── v1.json
        ├── v2.json
        └── v3.json
```

### Quand utiliser

- Développement local
- Tests CI/CD sans cluster
- Scénarios hors ligne

## Configuration

### Variables d'environnement

```bash
# Définir le driver par défaut
export SHERPACK_STORAGE=secrets

# Définir le chemin de stockage fichier
export SHERPACK_STORAGE_PATH=~/.sherpack/releases
```

### Par commande

```bash
sherpack install myapp ./mypack --storage configmap
sherpack list --storage file --storage-path /tmp/releases
```

## Données de release

La release stockée contient :

```json
{
  "name": "myapp",
  "namespace": "default",
  "revision": 1,
  "state": "deployed",
  "manifest": "---\napiVersion: apps/v1\n...",
  "values": { "app": { "replicas": 3 } },
  "values_provenance": {
    "pack_defaults": { "app.replicas": 1 },
    "user_values": { "app.replicas": 3 }
  },
  "pack_metadata": {
    "name": "mypack",
    "version": "1.0.0"
  },
  "created_at": "2024-01-15T10:00:00Z",
  "updated_at": "2024-01-15T10:00:00Z"
}
```

## Compression

Les données de release sont compressées avec Zstd pour l'efficacité :

- ~30% meilleure compression que gzip
- Décompression rapide
- Réduit la taille des Secret/ConfigMap

## Migration

Déplacer les releases entre backends de stockage :

```bash
# Exporter depuis secrets
sherpack get-release myapp --storage secrets > release.json

# Importer vers configmap
sherpack import-release release.json --storage configmap
```

## Exigences RBAC

### Driver Secrets

```yaml
apiVersion: rbac.authorization.k8s.io/v1
kind: Role
metadata:
  name: sherpack-release-manager
rules:
  - apiGroups: [""]
    resources: ["secrets"]
    verbs: ["get", "list", "create", "update", "delete"]
    resourceNames: []
```

### Driver ConfigMap

```yaml
rules:
  - apiGroups: [""]
    resources: ["configmaps"]
    verbs: ["get", "list", "create", "update", "delete"]
```
