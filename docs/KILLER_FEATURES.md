# Sherpack Killer Features Roadmap

Based on extensive research of [Helm GitHub issues](https://github.com/helm/helm/issues), [community discussions](https://helm.sh/community/hips/hip-0012/), and [user pain points](https://medium.com/archetypical-software/6-awesome-alternatives-to-helm-for-managing-your-kubernetes-applications-084b1ff6ccfe), here are killer features that would differentiate Sherpack.

---

## 1. Smart CRD Management

### The Problem
Helm has [notoriously poor CRD support](https://helm.sh/docs/chart_best_practices/custom_resource_definitions/):
- CRDs in `crds/` directory are **never upgraded** after initial install
- No templating support in `crds/` directory
- Migration from `crds/` to `templates/` requires manual annotation hacks
- [Upgrading CRDs is explicitly unsupported](https://github.com/helm/helm/issues/7735)

### Sherpack Solution: `CRDPolicy`

```yaml
# Pack.yaml
crdPolicy:
  mode: managed          # managed | ignore | external
  upgradeStrategy: safe  # safe | force | interactive
  backupBeforeUpgrade: true
  validateBeforeApply: true
```

**Features:**
- **Safe CRD upgrades**: Detect breaking changes (removed fields, type changes)
- **Automatic backup**: Create backup CRs before CRD schema changes
- **Version migration**: Auto-migrate CRs when CRD version changes
- **Validation**: Dry-run CRD changes against existing CRs
- **Rollback**: Automatic CRD rollback on upgrade failure

```bash
# Smart CRD operations
sherpack crd diff mypack           # Show CRD changes
sherpack crd migrate mypack        # Migrate CRs to new version
sherpack crd backup myrelease      # Backup all CRs
sherpack crd validate mypack       # Validate CRD changes
```

**Priority**: HIGH - This is one of Helm's most requested features

---

## 2. Chunked Release Storage (Break the 1MB Limit)

### The Problem
[Helm stores releases in Kubernetes Secrets limited to 1MB](https://azure.github.io/azure-service-operator/design/adr-2023-02-helm-chart-size-limitations/):
- Large charts with many resources fail to install
- Umbrella charts hit this limit quickly
- Error: `Secret is invalid: data: Too long: must have at most 1048576 bytes`
- [Proposed chunked secrets feature](https://github.com/helm/helm/issues/10986) never merged

### Sherpack Solution: Automatic Chunking

```yaml
# Pack.yaml
storage:
  driver: secrets        # secrets | configmap | file | sql
  compression: zstd      # gzip | zstd | none
  chunking:
    enabled: auto        # auto | true | false
    maxChunkSize: 900KB  # Per-chunk limit
```

**Features:**
- **Automatic chunking**: Split large releases across multiple secrets
- **ZSTD compression**: 40-60% smaller than gzip (Helm's default)
- **Transparent reassembly**: `sherpack history` works seamlessly
- **Fallback**: Graceful degradation if chunking fails
- **Migration**: Convert existing releases to chunked format

```bash
# Already implemented in storage/chunked.rs
sherpack install myrelease ./large-pack  # Auto-chunks if needed
sherpack migrate myrelease --chunked     # Convert existing release
```

**Priority**: HIGH - Already partially implemented, needs polish

---

## 3. Native Drift Detection

### The Problem
[Drift detection](https://komodor.com/blog/drift-detection-in-kubernetes/) is critical for GitOps but Helm doesn't support it:
- No way to detect manual cluster changes
- ArgoCD/Flux add this on top of Helm
- [Flux drift detection is experimental](https://github.com/fluxcd/helm-controller/issues/643)

### Sherpack Solution: Built-in Drift Detection

```yaml
# Pack.yaml
drift:
  detection: enabled     # enabled | disabled
  interval: 5m           # Check interval
  ignore:
    - metadata.annotations["kubectl.kubernetes.io/last-applied-configuration"]
    - status
  notify:
    webhook: https://slack.example.com/webhook
    email: ops@example.com
```

**Features:**
- **Real-time drift detection**: Watch for unauthorized changes
- **Semantic diff**: Ignore expected changes (status, annotations)
- **Auto-remediation**: Optional auto-sync to restore desired state
- **Audit log**: Who changed what and when
- **Webhooks**: Notify on drift via Slack, PagerDuty, etc.

```bash
sherpack drift status myrelease          # Check current drift
sherpack drift watch myrelease           # Continuous monitoring
sherpack drift sync myrelease            # Restore to pack state
sherpack drift history myrelease         # View drift events
```

**Priority**: HIGH - Critical for GitOps workflows

---

## 4. Smart Dependency Resolution

### The Problem
[Helm dependency resolution is fragile](https://github.com/helm/helm/issues/30875):
- Same chart name from different repos causes failures
- No diamond dependency detection
- Version ranges lock incorrectly
- [Conflicting subcharts produce no warning](https://github.com/helm/helm/issues/30710)

### Sherpack Solution: SAT-based Dependency Solver

```yaml
# Pack.yaml
dependencies:
  - name: redis
    version: ">=7.0.0 <8.0.0"
    repository: https://charts.bitnami.com
    optional: true
    condition: redis.enabled

  - name: postgresql
    version: "^15.0.0"
    repository: oci://registry.example.com/charts
    conflict:
      - mysql  # Cannot be installed with mysql

resolution:
  strategy: newest-compatible  # newest-compatible | locked | minimal
  allowPrerelease: false
  conflictResolution: fail     # fail | warn | prefer-first
```

**Features:**
- **SAT solver**: Proper constraint satisfaction for versions
- **Diamond detection**: Warn when A→B and A→C both need different D versions
- **Conflict declaration**: Explicitly declare incompatible deps
- **Conditional deps**: Only resolve if condition is true
- **Resolution strategies**: Choose newest, locked, or minimal versions

```bash
sherpack dependency resolve ./mypack     # Show resolution plan
sherpack dependency graph ./mypack       # Visual dependency tree
sherpack dependency audit ./mypack       # Security + version audit
sherpack dependency why redis            # Why is redis included?
```

**Priority**: MEDIUM - Already have basic resolution, needs SAT solver

---

## 5. Template Debugging & Profiling

### The Problem
Helm template debugging is painful:
- No breakpoints or step-through
- `--debug` output is overwhelming
- Performance issues hard to diagnose
- [Template delimiter customization requested](https://github.com/helm/helm/issues/31642)

### Sherpack Solution: Interactive Template Debugger

```bash
# Interactive debugging
sherpack debug ./mypack

> break templates/deployment.yaml:15     # Set breakpoint
> run myrelease                          # Start rendering
> print values.app                       # Inspect variable
> step                                   # Next line
> continue                               # Continue to next breakpoint
> eval "values.replicas * 2"             # Evaluate expression
> trace                                  # Show call stack
```

**Features:**
- **Breakpoints**: Stop at specific template lines
- **Variable inspection**: See values at any point
- **Expression evaluation**: Test filters/functions interactively
- **Call tracing**: Track macro/include calls
- **Performance profiling**: Find slow templates
- **Hot reload**: Edit templates, re-render instantly

```bash
sherpack template myrelease ./mypack --profile    # Performance report
sherpack template myrelease ./mypack --trace      # Full execution trace
sherpack template myrelease ./mypack --explain    # Show variable sources
```

**Priority**: MEDIUM - Great DX improvement

---

## 6. Resource Ordering & Wave Deployment

### The Problem
Helm applies resources in a fixed order that doesn't always work:
- CRDs must exist before CRs
- Secrets must exist before Deployments reference them
- No way to wait between resource groups

### Sherpack Solution: Wave-based Deployment

```yaml
# templates/database.yaml
apiVersion: apps/v1
kind: StatefulSet
metadata:
  name: {{ release.name }}-db
  annotations:
    sherpack.io/wave: "1"
    sherpack.io/wait-ready: "true"
    sherpack.io/timeout: "5m"
---
# templates/app.yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: {{ release.name }}-app
  annotations:
    sherpack.io/wave: "2"
    sherpack.io/depends-on: "{{ release.name }}-db"
```

**Features:**
- **Wave ordering**: Deploy resources in numbered waves
- **Explicit dependencies**: `depends-on` for fine-grained control
- **Wait conditions**: Wait for specific conditions between waves
- **Parallel within waves**: Resources in same wave deploy in parallel
- **Failure handling**: Stop, continue, or rollback on wave failure

```bash
sherpack install myrelease ./mypack --waves      # Show wave plan
sherpack install myrelease ./mypack --wave=1     # Deploy only wave 1
```

**Priority**: MEDIUM - Already have basic ordering, needs waves

---

## 7. Multi-Environment Templating

### The Problem
Managing multiple environments with Helm is clunky:
- Separate values files per environment
- No environment inheritance
- Easy to forget environment-specific overrides

### Sherpack Solution: Native Environment Support

```
mypack/
├── Pack.yaml
├── values.yaml              # Base values
├── environments/
│   ├── _base.yaml           # Shared across all envs
│   ├── development.yaml
│   ├── staging.yaml
│   └── production.yaml
└── templates/
```

```yaml
# environments/production.yaml
_inherit: staging           # Inherit from staging

app:
  replicas: 10

resources:
  limits:
    memory: 4Gi

# Environment-specific templates
_templates:
  include:
    - templates/monitoring/*   # Only in production
  exclude:
    - templates/debug/*
```

```bash
sherpack template myrelease ./mypack -e production
sherpack diff staging production ./mypack   # Compare environments
sherpack promote staging production ./mypack # Promote config
```

**Priority**: MEDIUM - Common request

---

## 8. Built-in Secret Management

### The Problem
Helm has no native secret management:
- Secrets stored in plain text in values
- External tools (SOPS, Vault) required
- No encryption at rest

### Sherpack Solution: Encrypted Values

```yaml
# values.secrets.yaml (encrypted)
database:
  password: ENC[AES256_GCM,data:abc123...]

apiKeys:
  stripe: ENC[AES256_GCM,data:def456...]
```

```bash
# Key management
sherpack secrets init                     # Generate encryption key
sherpack secrets encrypt values.yaml      # Encrypt sensitive values
sherpack secrets decrypt values.yaml      # Decrypt for editing
sherpack secrets rotate                   # Rotate encryption key

# Provider integration
sherpack secrets pull vault://secret/myapp  # Pull from Vault
sherpack secrets pull aws://ssm/myapp       # Pull from AWS SSM
sherpack secrets pull gcp://sm/myapp        # Pull from GCP SM
```

**Features:**
- **Age/SOPS encryption**: Industry-standard encryption
- **Provider integration**: Vault, AWS SSM, GCP Secret Manager, Azure Key Vault
- **Git-safe**: Encrypted values safe to commit
- **Automatic injection**: Decrypt at deploy time
- **Audit trail**: Track secret access

**Priority**: HIGH - Security is critical

---

## 9. Intelligent Rollback

### The Problem
Helm rollback is all-or-nothing:
- Rolls back entire release
- No partial rollback
- No rollback preview
- [Issues with `--wait` during rollback](https://github.com/helm/helm/issues/31651)

### Sherpack Solution: Smart Rollback

```bash
# Preview rollback
sherpack rollback myrelease 2 --dry-run --diff

# Partial rollback
sherpack rollback myrelease 2 --only deployment/myapp

# Selective rollback
sherpack rollback myrelease 2 --except secrets/*

# Auto-rollback on failure
sherpack upgrade myrelease ./mypack --auto-rollback \
  --rollback-on="CrashLoopBackOff,ImagePullBackOff"
```

**Features:**
- **Partial rollback**: Rollback specific resources
- **Rollback preview**: See exactly what will change
- **Auto-rollback triggers**: Rollback on specific error conditions
- **Rollback to any revision**: Not just previous
- **Canary rollback**: Gradual rollback with traffic shifting

**Priority**: MEDIUM - Improves safety

---

## 10. Live Template Preview (IDE Integration)

### The Problem
No real-time feedback when editing templates:
- Must run `helm template` after every change
- No autocomplete for values
- No inline error highlighting

### Sherpack Solution: LSP Server

```bash
# Start language server
sherpack lsp

# VS Code / Neovim will connect automatically
```

**Features:**
- **Real-time preview**: See rendered output as you type
- **Autocomplete**: `values.` triggers value suggestions
- **Inline errors**: Template errors shown inline
- **Go to definition**: Jump to value definitions
- **Hover documentation**: Show schema descriptions
- **Format on save**: Auto-format templates

```json
// .vscode/settings.json
{
  "sherpack.lsp.enabled": true,
  "sherpack.lsp.previewOnSave": true
}
```

**Priority**: LOW - Nice to have, high effort

---

## 11. Test Framework

### The Problem
No built-in way to test Helm charts:
- `helm test` is limited to running pods
- No unit testing for templates
- No policy validation

### Sherpack Solution: Native Test Framework

```yaml
# tests/deployment_test.yaml
suite: Deployment Tests

tests:
  - name: should set correct replicas
    template: templates/deployment.yaml
    set:
      app.replicas: 5
    asserts:
      - equal:
          path: spec.replicas
          value: 5

  - name: should require image tag
    template: templates/deployment.yaml
    set:
      image.tag: null
    asserts:
      - failedTemplate:
          contains: "image.tag is required"

  - name: should pass security policy
    template: templates/deployment.yaml
    asserts:
      - matchPolicy: pod-security-restricted
```

```bash
sherpack test ./mypack                    # Run all tests
sherpack test ./mypack --filter "replica" # Filter tests
sherpack test ./mypack --coverage         # Template coverage
sherpack test ./mypack --snapshot         # Snapshot testing
```

**Priority**: HIGH - Testing is essential

---

## 12. GitOps-Native Mode

### The Problem
Helm requires external tools for GitOps:
- ArgoCD or Flux needed for reconciliation
- No native Git integration
- Pull-based deployment requires additional setup

### Sherpack Solution: Built-in GitOps Controller

```yaml
# sherpack-controller.yaml
apiVersion: sherpack.io/v1
kind: PackRelease
metadata:
  name: myapp
  namespace: production
spec:
  source:
    git:
      url: https://github.com/org/packs
      path: ./myapp
      branch: main
  interval: 5m
  values:
    app:
      replicas: 3
  autoSync: true
  pruneOrphans: true
```

```bash
# Deploy controller to cluster
sherpack controller install

# Manage GitOps releases
sherpack gitops sync myapp              # Force sync
sherpack gitops suspend myapp           # Pause reconciliation
sherpack gitops resume myapp            # Resume reconciliation
sherpack gitops logs myapp              # View sync logs
```

**Priority**: LOW - Competes with ArgoCD/Flux, high effort

---

## Implementation Priority

### Phase 1 (Q1) - Critical
1. **Chunked Release Storage** - Already partially implemented
2. **Smart CRD Management** - Helm's biggest gap
3. **Built-in Secret Management** - Security essential

### Phase 2 (Q2) - High Value
4. **Native Drift Detection** - GitOps critical
5. **Test Framework** - Quality essential
6. **Intelligent Rollback** - Safety improvement

### Phase 3 (Q3) - Differentiation
7. **Smart Dependency Resolution** - SAT solver
8. **Template Debugging** - Developer experience
9. **Wave Deployment** - Complex apps

### Phase 4 (Q4) - Nice to Have
10. **Multi-Environment** - Common pattern
11. **LSP Server** - IDE integration
12. **GitOps Controller** - Full GitOps

---

## Sources

- [Helm GitHub Issues](https://github.com/helm/helm/issues)
- [Helm 4 Development Process](https://helm.sh/community/hips/hip-0012/)
- [Helm Chart Size Limitations](https://azure.github.io/azure-service-operator/design/adr-2023-02-helm-chart-size-limitations/)
- [CRD Upgrade Issues](https://github.com/helm/helm/issues/7735)
- [Server-Side Apply HIP](https://helm.sh/community/hips/hip-0023/)
- [Drift Detection in Kubernetes](https://komodor.com/blog/drift-detection-in-kubernetes/)
- [Helm Alternatives](https://northflank.com/blog/7-helm-alternatives-to-simplify-kubernetes-deployments)
- [Werf/Nelm SSA](https://blog.werf.io/server-side-apply-instead-of-3-way-merge-how-werf-2-0-solves-helm-3-challenges-4d7996354ebe)
