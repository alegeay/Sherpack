---
id: storage-drivers
title: Storage Drivers
sidebar_position: 5
---

# Storage Drivers

Sherpack stores release information using configurable storage backends.

## Available Drivers

| Driver | Storage Location | Use Case |
|--------|------------------|----------|
| `secrets` | Kubernetes Secrets | Default, production |
| `configmap` | Kubernetes ConfigMaps | Debugging, no RBAC for secrets |
| `file` | Local filesystem | Development, CI testing |

## Secrets Driver (Default)

Stores releases as Kubernetes Secrets:

```bash
# Default behavior
sherpack install myapp ./mypack

# Explicitly specify
sherpack install myapp ./mypack --storage secrets
```

### Secret Format

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
  release: <zstd-compressed-json>
```

### Advantages

- Secure (requires RBAC for secrets)
- Works across teams
- Survives pod restarts

## ConfigMap Driver

Stores releases as ConfigMaps:

```bash
sherpack install myapp ./mypack --storage configmap
```

### ConfigMap Format

```yaml
apiVersion: v1
kind: ConfigMap
metadata:
  name: sh.sherpack.release.v1.myapp.v1
  labels:
    owner: sherpack
    name: myapp
data:
  release: <zstd-compressed-json>
```

### When to Use

- Debugging release data
- Clusters without secret access
- Development environments

## File Driver

Stores releases on local filesystem:

```bash
sherpack install myapp ./mypack --storage file --storage-path ~/.sherpack/releases
```

### File Structure

```
~/.sherpack/releases/
└── default/
    └── myapp/
        ├── v1.json
        ├── v2.json
        └── v3.json
```

### When to Use

- Local development
- CI/CD testing without cluster
- Offline scenarios

## Configuration

### Environment Variables

```bash
# Set default driver
export SHERPACK_STORAGE=secrets

# Set file storage path
export SHERPACK_STORAGE_PATH=~/.sherpack/releases
```

### Per-Command

```bash
sherpack install myapp ./mypack --storage configmap
sherpack list --storage file --storage-path /tmp/releases
```

## Release Data

Stored release contains:

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

Release data is compressed with Zstd for efficiency:

- ~30% better compression than gzip
- Fast decompression
- Reduces Secret/ConfigMap size

## Migration

Move releases between storage backends:

```bash
# Export from secrets
sherpack get-release myapp --storage secrets > release.json

# Import to configmap
sherpack import-release release.json --storage configmap
```

## RBAC Requirements

### Secrets Driver

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

### ConfigMap Driver

```yaml
rules:
  - apiGroups: [""]
    resources: ["configmaps"]
    verbs: ["get", "list", "create", "update", "delete"]
```
