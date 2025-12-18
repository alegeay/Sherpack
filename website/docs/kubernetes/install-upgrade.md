---
id: install-upgrade
title: Install & Upgrade
sidebar_position: 1
---

# Install & Upgrade

Deploy and update your applications on Kubernetes.

## Install

Install a pack to the cluster:

```bash
sherpack install <name> <pack> [options]
```

### Basic Install

```bash
# Install from directory
sherpack install myapp ./mypack

# Install from archive
sherpack install myapp mypack-1.0.0.tar.gz

# With namespace
sherpack install myapp ./mypack -n production
```

### With Values

```bash
# Override values
sherpack install myapp ./mypack --set app.replicas=3

# Use values file
sherpack install myapp ./mypack -f production.yaml

# Combine both
sherpack install myapp ./mypack -f base.yaml --set image.tag=v2.0.0
```

### Wait for Ready

```bash
# Wait for all resources to be ready
sherpack install myapp ./mypack --wait

# With timeout
sherpack install myapp ./mypack --wait --timeout 10m
```

### Atomic Install

Automatically rollback on failure:

```bash
sherpack install myapp ./mypack --atomic
```

### Dry Run

Preview without applying:

```bash
sherpack install myapp ./mypack --dry-run
```

## Upgrade

Upgrade an existing release:

```bash
sherpack upgrade <name> <pack> [options]
```

### Basic Upgrade

```bash
# Upgrade with new pack version
sherpack upgrade myapp ./mypack

# Upgrade with new values
sherpack upgrade myapp ./mypack --set app.replicas=5
```

### Value Handling

```bash
# Reset to pack defaults, then apply new values
sherpack upgrade myapp ./mypack --reset-values --set image.tag=v2

# Reuse previous values, override specific ones
sherpack upgrade myapp ./mypack --reuse-values --set image.tag=v2

# Reset then reuse (reset defaults, keep user values)
sherpack upgrade myapp ./mypack --reset-then-reuse-values
```

### Diff Before Upgrade

Preview changes:

```bash
sherpack upgrade myapp ./mypack --diff
```

Output shows what will change:

```diff
--- deployed
+++ pending
@@ -10,7 +10,7 @@
 spec:
-  replicas: 3
+  replicas: 5
   template:
```

### Install or Upgrade

Install if not exists, otherwise upgrade:

```bash
sherpack upgrade myapp ./mypack --install
```

## Options Reference

### Install Options

| Option | Description |
|--------|-------------|
| `-n, --namespace` | Target namespace |
| `-f, --values` | Values file (repeatable) |
| `--set` | Set value (repeatable) |
| `--wait` | Wait for ready |
| `--timeout` | Wait timeout [default: 5m] |
| `--atomic` | Rollback on failure |
| `--dry-run` | Don't apply |
| `--create-namespace` | Create namespace if missing |

### Upgrade Options

| Option | Description |
|--------|-------------|
| `-n, --namespace` | Target namespace |
| `-f, --values` | Values file (repeatable) |
| `--set` | Set value (repeatable) |
| `--wait` | Wait for ready |
| `--timeout` | Wait timeout |
| `--atomic` | Rollback on failure |
| `--dry-run` | Don't apply |
| `--diff` | Show diff before applying |
| `--reuse-values` | Reuse previous values |
| `--reset-values` | Reset to defaults |
| `--install` | Install if not exists |

## Install Flow

1. Load pack and merge values
2. Validate against schema (if present)
3. Render templates
4. Store release as "pending-install"
5. Execute pre-install hooks
6. Apply resources (Server-Side Apply)
7. Wait for health (if `--wait`)
8. Execute post-install hooks
9. Update release to "deployed"

## Upgrade Flow

1. Get current release
2. Load pack and merge values
3. Render new templates
4. Store release as "pending-upgrade"
5. Execute pre-upgrade hooks
6. Apply resource changes
7. Wait for health (if `--wait`)
8. Execute post-upgrade hooks
9. Mark previous as "superseded", new as "deployed"
