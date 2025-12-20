# Sherpack CRD Handling Design

## Executive Summary

This document proposes a CRD handling approach for Sherpack that addresses the major frustrations with Helm's implementation while maintaining safety guarantees.

---

## Helm's CRD Problems (From GitHub Issues)

### 1. CRDs Never Update ([#7735](https://github.com/helm/helm/issues/7735), [#5853](https://github.com/helm/helm/issues/5853))
> "CRDs will not be upgraded any more if changes are made in a later release."

Helm explicitly refuses to update CRDs to prevent data loss. While well-intentioned, this forces users into manual `kubectl apply` workflows, breaking automation.

### 2. Wrong Patch Strategy ([#5853](https://github.com/helm/helm/issues/5853))
> "failed to create patch: merging an object in json but data type is not struct"

Helm uses Strategic Merge Patch for CRDs, but CRDs are unstructured and require JSON Merge Patch. This causes upgrade failures even for safe, additive changes.

### 3. Dependency Timing ([#10585](https://github.com/helm/helm/issues/10585), [#3632](https://github.com/helm/helm/issues/3632))
> "Helm does not wait until the dependency has been installed so the main chart tries to use a CRD that was not installed yet."

Charts with CRD dependencies fail because Helm doesn't ensure CRDs are registered before CRs are applied.

### 4. No Templating
CRDs in `crds/` directory cannot be templated, forcing users to either:
- Put CRDs in `templates/` (but then they're deleted on uninstall)
- Maintain separate CRD charts

### 5. Dry-Run Broken ([Helm docs](https://github.com/helm/helm-www/blob/d688dd300bc483be7410d9d0aeae7f402ec22560/content/en/docs/chart_best_practices/custom_resource_definitions.md))
> "The --dry-run flag of helm install and helm upgrade is not currently supported for CRDs."

### 6. Deletion Cascades
Deleting a CRD deletes ALL custom resources of that type. Helm prevents this but offers no granular control.

---

## Sherpack Design Principles

1. **Safe by default, powerful when needed** - Block dangerous operations but allow override
2. **Deterministic ordering** - CRDs always install before CRs
3. **Proper patching** - Use JSON Merge Patch for CRDs
4. **Full templating** - Allow templated CRDs with safety warnings
5. **Clear feedback** - Show exactly what will change before applying

---

## Proposed Implementation

### Pack Structure

```
mypack/
├── Pack.yaml
├── values.yaml
├── crds/                    # Static CRDs (not templated)
│   └── mycrd.yaml
└── templates/
    ├── deployment.yaml
    ├── _crd.yaml            # Templated CRD (optional)
    └── mycr.yaml            # Custom Resource using the CRD
```

### Pack.yaml CRD Configuration

```yaml
apiVersion: sherpack/v1
kind: application
metadata:
  name: my-operator
  version: 1.0.0

# CRD-specific configuration
crds:
  # Installation behavior
  install: true              # Install CRDs (default: true)

  # Upgrade behavior
  upgrade:
    enabled: true            # Allow CRD updates (default: true)
    strategy: safe           # safe | force | skip
    # safe: Only additive changes (new fields, new versions)
    # force: Apply all changes (may break existing CRs)
    # skip: Never update CRDs

  # Uninstall behavior
  uninstall:
    keep: true               # Keep CRDs on uninstall (default: true)
    # If false, requires --confirm-crd-deletion flag

  # Wait for CRD registration
  waitReady: true            # Wait for CRD to be Established (default: true)
  waitTimeout: 60s           # Timeout for CRD readiness
```

### Installation Order

Sherpack guarantees this order:

```
1. CRDs from crds/ directory (sorted by filename)
2. CRDs from templates/ (detected by kind: CustomResourceDefinition)
3. Wait for all CRDs to be Established
4. Regular resources (using existing hook ordering)
5. Custom Resources (after their CRD is ready)
```

#### Detection Algorithm

```rust
pub enum ResourceCategory {
    Crd,           // CustomResourceDefinition
    Namespace,     // Namespace
    ClusterRole,   // ClusterRole, ClusterRoleBinding
    ServiceAccount,
    ConfigMap,
    Secret,
    Service,
    Deployment,
    CustomResource, // Uses a CRD (detected by apiVersion not in core APIs)
    Other,
}

fn categorize_resource(manifest: &Manifest) -> ResourceCategory {
    match manifest.kind.as_str() {
        "CustomResourceDefinition" => ResourceCategory::Crd,
        "Namespace" => ResourceCategory::Namespace,
        // ... etc
        _ => {
            // Check if apiVersion indicates a custom resource
            if is_custom_api_version(&manifest.api_version) {
                ResourceCategory::CustomResource
            } else {
                ResourceCategory::Other
            }
        }
    }
}
```

### Safe Update Detection

```rust
pub enum CrdChangeType {
    /// Safe: Adding new optional fields
    AddOptionalField,
    /// Safe: Adding new API version
    AddVersion,
    /// Safe: Adding new printer columns
    AddPrinterColumn,
    /// Warning: Changing validation (may reject existing CRs)
    ValidationChange,
    /// Dangerous: Removing required field
    RemoveRequiredField,
    /// Dangerous: Changing field type
    FieldTypeChange,
    /// Dangerous: Removing API version
    RemoveVersion,
    /// Dangerous: Changing scope (Namespaced <-> Cluster)
    ScopeChange,
}

fn analyze_crd_changes(old: &Crd, new: &Crd) -> Vec<CrdChange> {
    let mut changes = vec![];

    // Compare versions
    for old_version in &old.spec.versions {
        if !new.spec.versions.iter().any(|v| v.name == old_version.name) {
            changes.push(CrdChange {
                change_type: CrdChangeType::RemoveVersion,
                severity: Severity::Dangerous,
                message: format!("Removing API version {}", old_version.name),
            });
        }
    }

    // Compare schema fields
    // ... detailed field-by-field comparison

    changes
}
```

### CLI Behavior

#### Install
```bash
# Normal install (CRDs installed first, wait for ready)
sherpack install myrelease ./mypack

# Skip CRDs (if already installed externally)
sherpack install myrelease ./mypack --skip-crds

# Show what CRDs would be installed
sherpack install myrelease ./mypack --dry-run
```

#### Upgrade
```bash
# Normal upgrade (safe CRD updates only)
sherpack upgrade myrelease ./mypack

# Force CRD updates (dangerous changes allowed)
sherpack upgrade myrelease ./mypack --force-crd-update

# Skip CRD updates entirely
sherpack upgrade myrelease ./mypack --skip-crd-update

# Show CRD diff before applying
sherpack upgrade myrelease ./mypack --show-crd-diff
```

#### Uninstall
```bash
# Normal uninstall (keeps CRDs)
sherpack uninstall myrelease

# Delete CRDs too (requires confirmation)
sherpack uninstall myrelease --delete-crds
# Error: This will delete all CustomResources of these types:
#   - myresources.example.com (15 resources in cluster)
# Use --confirm-crd-deletion to proceed.

sherpack uninstall myrelease --delete-crds --confirm-crd-deletion
```

### Templated CRDs

Unlike Helm, Sherpack allows templated CRDs in `templates/`:

```yaml
# templates/_crd.yaml
apiVersion: apiextensions.k8s.io/v1
kind: CustomResourceDefinition
metadata:
  name: {{ values.crdName }}.{{ values.group }}
  labels:
    {{- values.labels | toyaml | nindent 4 }}
spec:
  group: {{ values.group }}
  names:
    kind: {{ values.kind }}
    # ...
```

#### Safety Warnings

When linting templated CRDs:

```
⚠ Warning: Templated CRD detected (templates/_crd.yaml)
  Consider placing static CRDs in crds/ directory for:
  - Predictable installation order
  - Protection from accidental deletion
  - Clearer upgrade semantics
```

### Dependency CRD Handling

When a pack depends on another pack that provides CRDs:

```yaml
# Pack.yaml
dependencies:
  - name: cert-manager
    version: "1.x"
    repository: https://charts.jetstack.io
    # CRD options for dependency
    crds:
      import: true           # Use CRDs from this dependency
      waitReady: true        # Wait for CRDs before our templates
```

Sherpack builds a dependency graph:

```
1. cert-manager CRDs
2. Wait for cert-manager CRDs ready
3. cert-manager templates
4. Our CRDs (if any)
5. Wait for our CRDs ready
6. Our templates (can now use Certificate, Issuer, etc.)
```

### Diff Output

```bash
$ sherpack upgrade myrelease ./mypack --show-crd-diff

CRD Changes for myresources.example.com:

  + spec.versions[0].schema.openAPIV3Schema.properties.newField:
      type: string
      description: "New optional field"

  ~ spec.versions[0].schema.openAPIV3Schema.properties.config.maxLength:
      - 256
      + 512

  ⚠ Validation change detected. Existing CRs may be affected.

Proceed with upgrade? [y/N]
```

### Error Handling

```rust
#[derive(Debug, thiserror::Error)]
pub enum CrdError {
    #[error("CRD {name} not ready after {timeout:?}")]
    NotReady { name: String, timeout: Duration },

    #[error("Breaking change detected in CRD {name}: {change}")]
    BreakingChange { name: String, change: String },

    #[error("CRD {name} is owned by release {owner}, not {current}")]
    OwnershipConflict { name: String, owner: String, current: String },

    #[error("Cannot delete CRD {name}: {count} CustomResources exist")]
    DeletionBlocked { name: String, count: usize },
}
```

---

## Implementation Plan

### Phase 1: Core CRD Support ✅ COMPLETE
1. ✅ Add `crds/` directory scanning in pack loader (`LoadedPack.crds_dir`, `crd_files()`, `load_crds()`)
2. ✅ Implement resource categorization and ordering (`ResourceCategory` enum in `crd.rs`)
3. ✅ Add CRD-first installation logic (`ResourceManager` uses `ResourceCategory` for sorting)
4. ✅ Implement wait-for-ready with timeout (`CrdManager.wait_for_crd()`)
5. ✅ Add Pack.yaml CRD configuration (`CrdConfig`, `CrdUpgradeConfig`, `CrdUninstallConfig`)
6. ✅ Add CLI flags (`--skip-crds`, `--force-crd-update`, `--show-crd-diff`, `--delete-crds`)

### Phase 2: Safe Updates ✅ COMPLETE
1. ✅ Server-Side Apply for CRDs (`CrdManager.apply_crd()` uses `Patch::Apply`)
2. ✅ CRD schema representation (`crd/schema.rs`: `CrdSchema`, `CrdVersionSchema`, `OpenApiSchema`)
3. ✅ CRD YAML parser (`crd/parser.rs`: `CrdParser::parse()`)
4. ✅ CRD diff analysis (`crd/analyzer.rs`: `CrdAnalyzer::analyze()` with 24 change types)
5. ✅ Change severity classification (`ChangeSeverity`: Safe, Warning, Dangerous)
6. ✅ Safe/Force/Skip strategies (`crd/strategy.rs`: `SafeStrategy`, `ForceStrategy`, `SkipStrategy`)
7. ✅ Terminal diff renderer (`display.rs`: `CrdDiffRenderer`)
8. ✅ Integration with CLI (`--show-crd-diff` flag functional)

### Phase 3: Templated CRDs ✅ COMPLETE
1. ✅ Intent-based CRD policies (`crd/policy.rs`: `CrdPolicy` enum - managed/shared/external)
2. ✅ CRD location tracking (`CrdLocation` enum - crds dir, templates, dependencies)
3. ✅ CRD ownership model (`CrdOwnership` struct with release tracking)
4. ✅ Policy extraction from annotations (`sherpack.io/crd-policy`, `helm.sh/resource-policy`)
5. ✅ CRD detection in templates (`crd/detection.rs`: `detect_crds_in_manifests()`)
6. ✅ Templating detection in crds/ (`contains_jinja_syntax()`, `TemplatedCrdFile`)
7. ✅ Lint warnings for CRD placement (`lint_crds()`, `CrdLintWarning`, `CrdLintCode`)
8. ✅ Pack.rs enhancement (`is_templated` field, `static_crds()`, `templated_crds()`)
9. ✅ Deletion protection (`crd/protection.rs`: `CrdProtection`, `CrdDeletionImpact`)
10. ✅ Impact analysis display (`display.rs`: `display_deletion_impact()`)
11. ✅ CLI lint integration (`lint.rs`: `lint_crds_in_pack()`)

### Phase 4: Dependency CRDs
1. Extend dependency resolver for CRD ordering
2. Implement cross-chart CRD waiting
3. Add CRD ownership tracking

---

## Comparison with Helm

| Feature | Helm | Sherpack |
|---------|------|----------|
| CRD location | `crds/` only | `crds/` or `templates/` |
| CRD updates | Never | Safe by default, configurable |
| Patch strategy | Strategic (broken) | Server-Side Apply |
| Templating | No | Yes (with warnings) |
| Dependency ordering | None | Automatic |
| Wait for ready | No | Yes (configurable) |
| Dry-run | Broken | Full support |
| Deletion | Blocked | Configurable with confirmation |
| Diff output | None | Rich diff with impact analysis |

---

## References

- [Helm CRD Best Practices](https://github.com/helm/helm-www/blob/main/content/en/docs/chart_best_practices/custom_resource_definitions.md)
- [Helm Issue #7735: Allow patching/updating CRD resources](https://github.com/helm/helm/issues/7735)
- [Helm Issue #5853: Can't upgrade charts with CRD changes](https://github.com/helm/helm/issues/5853)
- [Helm Issue #10585: Managing CRDs & chart dependencies](https://github.com/helm/helm/issues/10585)
- [ArgoCD Sync Waves](https://argo-cd.readthedocs.io/en/stable/user-guide/sync-waves/)
- [banzaicloud/crd-updater](https://github.com/banzaicloud/crd-updater)
