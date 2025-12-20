---
id: dependencies
title: Dependencies
sidebar_position: 4
---

# Dependencies

Manage pack dependencies with version locking.

## Declaring Dependencies

Add dependencies to `Pack.yaml`:

```yaml title="Pack.yaml"
apiVersion: sherpack/v1
kind: application
metadata:
  name: myapp
  version: 1.0.0

dependencies:
  - name: redis
    version: ">=7.0.0"
    repository: https://charts.bitnami.com/bitnami

  - name: postgresql
    version: "~15.0.0"
    repository: https://charts.bitnami.com/bitnami
    condition: postgresql.enabled

  - name: common
    version: "*"
    repository: https://charts.bitnami.com/bitnami
    alias: helpers
```

## Version Constraints

| Constraint | Meaning |
|------------|---------|
| `1.0.0` | Exact version |
| `>=1.0.0` | Minimum version |
| `<=2.0.0` | Maximum version |
| `>=1.0.0,<2.0.0` | Range |
| `~1.2.0` | Patch updates (allows 1.2.x) |
| `^1.2.0` | Minor updates (allows 1.x.x) |
| `*` | Any version |

## Dependency Commands

### List Dependencies

```bash
sherpack dependency list ./mypack
```

Output:

```
DEPENDENCY   VERSION     REPOSITORY                              STATUS
redis        >=7.0.0     https://charts.bitnami.com/bitnami     not installed
postgresql   ~15.0.0     https://charts.bitnami.com/bitnami     not installed
common       *           https://charts.bitnami.com/bitnami     not installed
```

### Update (Resolve & Lock)

```bash
sherpack dependency update ./mypack
```

Creates/updates `Pack.lock.yaml`:

```yaml
pack_yaml_digest: sha256:a1b2c3d4...
policy: version
dependencies:
  - name: redis
    version: "7.2.4"
    repository: https://charts.bitnami.com/bitnami
    digest: sha256:abc123...

  - name: postgresql
    version: "15.2.0"
    repository: https://charts.bitnami.com/bitnami
    digest: sha256:def456...

  - name: common
    version: "2.4.0"
    repository: https://charts.bitnami.com/bitnami
    digest: sha256:789abc...
```

### Build (Download)

```bash
sherpack dependency build ./mypack
```

Downloads to `packs/` directory:

```
mypack/
├── Pack.yaml
├── Pack.lock.yaml
├── packs/
│   ├── redis-7.2.4.tar.gz
│   ├── postgresql-15.2.0.tar.gz
│   └── common-2.4.0.tar.gz
```

### Show Tree

```bash
sherpack dependency tree ./mypack
```

```
myapp@1.0.0
├── redis@7.2.4
│   └── common@2.4.0
├── postgresql@15.2.0
│   └── common@2.4.0
└── common@2.4.0 (alias: helpers)
```

## Lock Policies

Configure how strictly versions are locked:

```bash
sherpack dependency update ./mypack --policy strict
```

| Policy | Behavior |
|--------|----------|
| `strict` | Exact version + SHA must match |
| `version` | Version must match (default) |
| `semver-patch` | Allow patch updates (1.2.3 → 1.2.4) |
| `semver-minor` | Allow minor updates (1.2.3 → 1.3.0) |

## Conditional Dependencies

Enable/disable dependencies based on values:

```yaml title="Pack.yaml"
dependencies:
  - name: postgresql
    version: "~15.0.0"
    repository: https://charts.bitnami.com
    condition: postgresql.enabled
```

```yaml title="values.yaml"
postgresql:
  enabled: true
```

## Dependency Aliases

Use the same pack multiple times with different names:

```yaml
dependencies:
  - name: redis
    version: "7.0.0"
    repository: https://charts.bitnami.com
    alias: cache

  - name: redis
    version: "7.0.0"
    repository: https://charts.bitnami.com
    alias: session
```

Access in templates:

```yaml
cache: {{ values.cache.host }}
session: {{ values.session.host }}
```

## Diamond Dependencies

When dependencies share a common dependency:

```
myapp
├── redis → common@2.4.0
└── postgresql → common@2.5.0
```

Sherpack detects this conflict:

```
Error: Diamond dependency conflict

  common required at incompatible versions:
    - redis requires common@2.4.0
    - postgresql requires common@2.5.0

Resolution options:
  1. Pin common version in myapp
  2. Use dependency aliases
  3. Update dependencies to compatible versions
```

## Using Dependencies in Templates

Dependencies are available in the `packs/` directory:

```yaml
{% include "packs/common/templates/_helpers.tpl" %}
```

Or import values:

```yaml title="values.yaml"
redis:
  # Values passed to redis dependency
  replica:
    replicaCount: 3
```
