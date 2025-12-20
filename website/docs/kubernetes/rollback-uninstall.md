---
id: rollback-uninstall
title: Rollback & Uninstall
sidebar_position: 2
---

# Rollback & Uninstall

Revert to previous versions or remove releases entirely.

## Rollback

Revert to a previous revision:

```bash
sherpack rollback <name> <revision>
```

### View History First

```bash
# See available revisions
sherpack history myapp
```

Output:

```
REVISION  STATUS      UPDATED                   DESCRIPTION
1         superseded  2024-01-10T10:00:00Z      Install complete
2         superseded  2024-01-11T14:30:00Z      Upgrade complete
3         deployed    2024-01-12T09:15:00Z      Upgrade complete
```

### Rollback to Revision

```bash
# Rollback to revision 1
sherpack rollback myapp 1

# With wait
sherpack rollback myapp 1 --wait
```

### Rollback Options

| Option | Description |
|--------|-------------|
| `-n, --namespace` | Namespace |
| `--wait` | Wait for rollback to complete |
| `--timeout` | Wait timeout |
| `--dry-run` | Preview without applying |

### Rollback Flow

1. Get target revision from storage
2. Store new release as "pending-rollback"
3. Execute pre-rollback hooks
4. Apply resources from target revision
5. Wait for health (if `--wait`)
6. Execute post-rollback hooks
7. Mark new release as "deployed"

## Uninstall

Remove a release from the cluster:

```bash
sherpack uninstall <name>
```

### Basic Uninstall

```bash
# Uninstall release
sherpack uninstall myapp

# In specific namespace
sherpack uninstall myapp -n production
```

### Keep History

Preserve release history for audit:

```bash
sherpack uninstall myapp --keep-history
```

### Wait for Deletion

Wait until all resources are deleted:

```bash
sherpack uninstall myapp --wait
```

### Dry Run

Preview what will be deleted:

```bash
sherpack uninstall myapp --dry-run
```

### Uninstall Options

| Option | Description |
|--------|-------------|
| `-n, --namespace` | Namespace |
| `--keep-history` | Keep release records |
| `--wait` | Wait for deletion |
| `--timeout` | Wait timeout |
| `--dry-run` | Preview without deleting |

### Uninstall Flow

1. Get current release
2. Update state to "uninstalling"
3. Execute pre-delete hooks
4. Delete all release resources
5. Wait for deletion (if `--wait`)
6. Execute post-delete hooks
7. Delete or mark release as "uninstalled"

## Resource Policy

Resources with the `sherpack.io/resource-policy: keep` annotation are preserved:

```yaml
apiVersion: v1
kind: PersistentVolumeClaim
metadata:
  name: {{ release.name }}-data
  annotations:
    sherpack.io/resource-policy: keep
```

This PVC won't be deleted during uninstall or upgrade.

## Recover Stale Releases

If a release is stuck in a pending state:

```bash
# Check status
sherpack status myapp

# Output shows stuck state
# Status: pending-upgrade (stale)

# Recover
sherpack recover myapp
```

This resets the release to its last known good state.

## List Releases

View all installed releases:

```bash
# Current namespace
sherpack list

# All namespaces
sherpack list -A

# Include uninstalled (with --keep-history)
sherpack list --all
```

Output:

```
NAME    NAMESPACE   REVISION  STATUS    UPDATED
myapp   default     3         deployed  2024-01-12T09:15:00Z
nginx   production  1         deployed  2024-01-10T08:00:00Z
```

## Release Status

Get detailed status:

```bash
sherpack status myapp
```

Output:

```
Name: myapp
Namespace: default
Revision: 3
Status: deployed
Updated: 2024-01-12T09:15:00Z

Resources:
  Deployment/myapp: Ready (3/3 replicas)
  Service/myapp: Active
  ConfigMap/myapp-config: Created
```

With resource details:

```bash
sherpack status myapp --show-resources
```
