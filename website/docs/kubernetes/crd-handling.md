---
id: crd-handling
title: CRD Handling
sidebar_position: 6
---

# CRD Handling

Sherpack provides sophisticated CustomResourceDefinition (CRD) handling that addresses Helm's major limitations with an intent-based policy system.

## The Problem with Helm's Approach

Helm's CRD handling has several well-documented issues:

1. **CRDs never update** - Once installed, CRDs are never upgraded ([#7735](https://github.com/helm/helm/issues/7735))
2. **Wrong patch strategy** - Strategic Merge Patch fails for CRDs ([#5853](https://github.com/helm/helm/issues/5853))
3. **Dependency timing** - CRDs aren't ready before CRs are applied ([#10585](https://github.com/helm/helm/issues/10585))
4. **No templating** - CRDs in `crds/` cannot use template syntax
5. **Broken dry-run** - `--dry-run` doesn't work with CRDs
6. **Deletion cascades** - Deleting a CRD deletes ALL custom resources

## Sherpack's Solution: Intent-Based Policies

Instead of determining behavior by file location (`crds/` vs `templates/`), Sherpack uses **intent-based policies** that explicitly declare how each CRD should be managed.

### Three Policies

| Policy | Behavior | Use Case |
|--------|----------|----------|
| `managed` | Full lifecycle - install, update, protect on uninstall | CRDs owned by your pack |
| `shared` | Install/update, never delete | CRDs used by multiple releases |
| `external` | Don't touch | Pre-existing cluster CRDs |

### Setting Policies

Add an annotation to your CRD:

```yaml
apiVersion: apiextensions.k8s.io/v1
kind: CustomResourceDefinition
metadata:
  name: certificates.cert-manager.io
  annotations:
    sherpack.io/crd-policy: shared
```

Or use the Helm-compatible annotation:

```yaml
annotations:
  helm.sh/resource-policy: keep  # Translates to "shared"
```

## CRD Installation

### Automatic Ordering

Sherpack guarantees CRDs are installed and ready before custom resources:

```
1. CRDs from crds/ directory
2. CRDs from templates/
3. Wait for all CRDs to be Established
4. Regular resources (Services, Deployments, etc.)
5. Custom Resources (after their CRD is ready)
```

### Skip CRDs

If CRDs are already installed externally:

```bash
sherpack install myrelease ./mypack --skip-crds
```

## CRD Updates

### Safe Update Analysis

Sherpack analyzes CRD changes with **24 different change types** and classifies them by severity:

| Severity | Action | Examples |
|----------|--------|----------|
| **Safe** | Auto-apply | Add optional field, add version, add printer column |
| **Warning** | Show warning | Validation changes, conversion changes |
| **Dangerous** | Block (unless forced) | Remove version, change scope, remove required field |

### View Changes Before Applying

```bash
sherpack upgrade myrelease ./mypack --show-crd-diff
```

Example output:

```
CRD Changes for certificates.cert-manager.io:

  + spec.versions[0].schema.openAPIV3Schema.properties.newField:
      type: string
      description: "New optional field"

  ~ spec.versions[0].schema.openAPIV3Schema.properties.config.maxLength:
      - 256
      + 512

  ⚠ Validation change detected. Existing CRs may be affected.

Proceed with upgrade? [y/N]
```

### Force Updates

To apply dangerous changes (use with caution):

```bash
sherpack upgrade myrelease ./mypack --force-crd-update
```

### Skip Updates

To never update CRDs:

```bash
sherpack upgrade myrelease ./mypack --skip-crd-update
```

## CRD Uninstall

### Default Behavior

By default, CRDs are **kept** when uninstalling a release. This prevents accidental data loss.

### Delete CRDs

To delete CRDs (with safety checks):

```bash
sherpack uninstall myrelease --delete-crds
```

If CRDs have existing custom resources, confirmation is required:

```
This will delete all CustomResources of these types:
  - certificates.cert-manager.io (15 resources in production)
  - issuers.cert-manager.io (3 resources in production)

Use --confirm-crd-deletion to proceed.
```

```bash
sherpack uninstall myrelease --delete-crds --confirm-crd-deletion
```

### Policy Protection

CRDs with `shared` or `external` policy are **never deleted**, even with `--delete-crds`:

```
Blocked by policy:
  - certificates.cert-manager.io (policy: shared)

These CRDs will not be deleted. To override, change the policy annotation.
```

## Templated CRDs

Unlike Helm, Sherpack supports templated CRDs:

```yaml
# templates/crd.yaml
apiVersion: apiextensions.k8s.io/v1
kind: CustomResourceDefinition
metadata:
  name: {{ values.crdName }}.{{ values.group }}
  labels:
    {{- values.labels | toyaml | indent(4) }}
```

### Lint Warnings

Templated CRDs generate lint warnings for awareness:

```bash
sherpack lint ./mypack
```

```
⚠ Warning: Templated CRD in templates/crd.yaml
  Consider placing in crds/ directory for:
  - Predictable installation order
  - Protection from accidental deletion
  - Clearer upgrade semantics
```

### Static CRDs in crds/

The `crds/` directory should contain **static** YAML only. If Jinja syntax is detected:

```
✗ Error: Jinja syntax detected in crds/mycrd.yaml
  Files in crds/ are NOT templated by Sherpack.
  Move to templates/ if templating is needed.
```

## Dependency CRDs

When depending on a pack that provides CRDs:

```yaml
# Pack.yaml
dependencies:
  - name: cert-manager
    version: "1.x"
    repository: https://charts.jetstack.io
```

Sherpack builds a dependency graph ensuring correct order:

```
1. cert-manager CRDs
2. Wait for cert-manager CRDs ready
3. cert-manager templates
4. Your CRDs (if any)
5. Wait for your CRDs ready
6. Your templates (can now use Certificate, Issuer, etc.)
```

## Pack.yaml Configuration

Configure CRD behavior in your Pack.yaml:

```yaml
apiVersion: sherpack/v1
kind: application
metadata:
  name: my-operator
  version: 1.0.0

crds:
  # Installation behavior
  install: true              # Install CRDs (default: true)

  # Upgrade behavior
  upgrade:
    enabled: true            # Allow CRD updates (default: true)
    strategy: safe           # safe | force | skip

  # Uninstall behavior
  uninstall:
    keep: true               # Keep CRDs on uninstall (default: true)

  # Wait for CRD registration
  waitReady: true            # Wait for Established condition
  waitTimeout: 60s           # Timeout for readiness
```

## CLI Reference

| Command | Description |
|---------|-------------|
| `--skip-crds` | Don't install CRDs |
| `--skip-crd-update` | Don't update existing CRDs |
| `--force-crd-update` | Apply dangerous CRD changes |
| `--show-crd-diff` | Show CRD changes before applying |
| `--delete-crds` | Delete CRDs on uninstall |
| `--confirm-crd-deletion` | Confirm CRD deletion with data loss |

## Comparison with Helm

| Feature | Helm | Sherpack |
|---------|------|----------|
| Policy model | Location-based | Intent-based annotations |
| CRD updates | Never | Safe by default, configurable |
| Patch strategy | Strategic Merge (broken) | Server-Side Apply |
| Templating in crds/ | No | No (with lint error) |
| Templating in templates/ | Yes (but deleted on uninstall) | Yes (with lint warning) |
| Dependency ordering | None | Automatic |
| Wait for ready | No | Yes (configurable) |
| Dry-run | Broken | Full support |
| Deletion | Always blocked | Configurable with confirmation |
| Diff output | None | Rich diff with impact analysis |
| Safe update detection | None | 24 change type analysis |

## Best Practices

1. **Use `managed` policy** for CRDs your pack owns exclusively
2. **Use `shared` policy** for CRDs that might be used by other releases
3. **Use `external` policy** for cluster-wide CRDs like cert-manager
4. **Place static CRDs in `crds/`** for predictable behavior
5. **Review `--show-crd-diff`** before upgrades in production
6. **Never use `--force-crd-update`** without understanding the impact
7. **Test CRD changes** with `--dry-run` first
