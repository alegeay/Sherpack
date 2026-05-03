# `lookup()` — Reading Existing Cluster State at Render Time

Sherpack's `lookup()` reads existing Kubernetes resources during template
rendering. It's the same idea as Helm's `lookup`, with the same trade-offs:
useful for migration of existing charts, but **non-deterministic by design**
— the same Pack rendered against different clusters produces different
manifests.

If you're starting a new Pack from scratch, prefer the alternatives in
[§ When *not* to use `lookup()`](#when-not-to-use-lookup). If you're
migrating a Helm chart that already uses `lookup`, the converter preserves
the call and this doc explains how it behaves.

---

## Signature

```jinja
{{ lookup(api_version, kind, namespace, name) }}
```

All four arguments are strings.

| Argument | Description |
|---|---|
| `api_version` | API version, e.g. `"v1"`, `"apps/v1"`, `"cert-manager.io/v1"` |
| `kind`        | Kind, e.g. `"Secret"`, `"ConfigMap"`, `"Deployment"`, `"Issuer"` |
| `namespace`   | Namespace; `""` for cluster-scoped resources or "all namespaces" |
| `name`        | Resource name; `""` to list resources of this kind |

## Behavior

### When does `lookup` query the cluster?

| Command | Behavior |
|---|---|
| `sherpack template <release> <pack>` | Always returns `{}` (no cluster access). Matches `helm template`. |
| `sherpack install <release> <pack>` | Queries the cluster live. |
| `sherpack upgrade <release> <pack>` | Queries the cluster live. |
| `sherpack lint <pack>` / `sherpack validate <pack>` | No cluster access; treat as `template` mode. |

### Return shape

| Call | Returned by `lookup()` |
|---|---|
| `lookup("v1", "Secret", "default", "tls")`, **resource exists** | The full object as a dict (`metadata`, `spec`, `data`, `status`, …) |
| `lookup("v1", "Secret", "default", "tls")`, **not found** | `{}` (empty dict) |
| `lookup("v1", "ConfigMap", "default", "")`, **list mode** | `{items: [...]}` (always wrapped in `items` key, like Helm) |
| `lookup(...)`, kind unknown | `{}` |
| `lookup(...)`, RBAC 403 / network 5xx / timeout | `{}` (silent — see [§ Error handling](#error-handling)) |

### Caching within a single render

Each install/upgrade builds a **fresh cache**. Within one render, two
identical `lookup(...)` calls hit the API server **once**:

```jinja
{# Both reads share a single cluster round-trip. #}
{% set s1 = lookup("v1", "Secret", "ns", "tls") %}
{% set s2 = lookup("v1", "Secret", "ns", "tls") %}
```

The cache is dropped after the render completes — a follow-up upgrade
re-reads everything fresh.

### Timeout

Each individual lookup has a timeout; on expiry it resolves to `{}`
(matching all other error paths). Default: **5 seconds**.

Override via environment variable for slow clusters or large list operations:

```bash
SHERPACK_LOOKUP_TIMEOUT_SECS=15 sherpack install myapp ./pack
```

### Warnings emitted

When a `lookup()` call returns a **non-empty** result during install/upgrade,
Sherpack emits a `tracing::warn!` log entry:

```
WARN lookup() returned cluster state for Secret/default/tls — render is non-deterministic
```

The warning is deduplicated per `(kind, name)` so you get one per resource,
not one per call. Set `RUST_LOG=warn` (or higher) to see them on stderr.
Empty results don't generate warnings — only when the render actually
*used* live cluster state.

## Common patterns

### Reuse an existing secret if present (the #1 use case)

```jinja
{# templates/secret.yaml #}
{%- set existing = lookup("v1", "Secret", release.namespace, release.name ~ "-tls") %}
apiVersion: v1
kind: Secret
metadata:
  name: {{ release.name }}-tls
type: kubernetes.io/tls
data:
{%- if existing and existing.data %}
  # Preserve the existing certificate so we don't rotate on every upgrade
  tls.crt: {{ existing.data["tls.crt"] }}
  tls.key: {{ existing.data["tls.key"] }}
{%- else %}
  # First install — emit placeholders to be filled by cert-manager / external-secrets
  tls.crt: ""
  tls.key: ""
{%- endif %}
```

> **Better alternative for new Packs**: use Sherpack's
> [`generate_secret()`](#alternatives) — it persists the generated value in
> the release state and is fully deterministic across renders.

### Conditional install if a CRD is already present

```jinja
{%- if lookup("apiextensions.k8s.io/v1", "CustomResourceDefinition", "", "issuers.cert-manager.io") %}
apiVersion: cert-manager.io/v1
kind: Issuer
metadata:
  name: {{ release.name }}
spec:
  selfSigned: {}
{%- endif %}
```

This pattern is the cleanest legitimate use of `lookup` — it gates a piece
of the chart on a precondition that's hard to express otherwise.

### Read a ConfigMap that another component owns

```jinja
{%- set info = lookup("v1", "ConfigMap", "kube-system", "cluster-info") %}
config:
  clusterDomain: {{ info.data.clusterDomain | default("cluster.local") }}
```

### List all Pods in a namespace

```jinja
{%- set pods = lookup("v1", "Pod", "production", "") %}
podCount: {{ pods["items"] | length }}
```

## Error handling

`lookup` is **non-fatal** by design. The following all resolve to `{}` (or
`{items: []}` for list mode), without aborting the render:

- Resource not found (404)
- RBAC denied (403) — the service account running `sherpack install` lacks
  `get`/`list` on the kind
- Unknown `apiVersion` / `kind` (typo, CRD not installed yet)
- Timeout (default 5s, configurable)
- Network error / TLS failure / kube-apiserver unreachable

This mirrors Helm exactly: a `lookup` should never make your template fail.
**The cost** is that errors stay invisible if you don't watch the logs —
always run with `RUST_LOG=warn` (or higher) when debugging template
behavior that depends on `lookup`.

## When *not* to use `lookup`

`lookup` undermines reproducibility. If you can avoid it, do.

| Goal | Better than `lookup` |
|---|---|
| Generate a stable random secret | `generate_secret("name", 32)` — Sherpack persists the value, idempotent across renders |
| Reuse a secret managed by a different controller | [external-secrets-operator](https://external-secrets.io/) or [sealed-secrets](https://github.com/bitnami-labs/sealed-secrets) — the value lives in Git, declaratively |
| Conditionally install based on cluster state | Pass a values flag (`values.cert_manager.enabled: true`) — explicit and reproducible |
| Look up a Service IP / DNS name | Use the Kubernetes Service DNS name (`<service>.<ns>.svc.cluster.local`) — no lookup needed |
| Migrate a `lookup`-using Helm chart | The converter preserves the call. Add a TODO to swap it for the patterns above when feasible. |

### Alternatives, summarized

```jinja
{# Bad — non-deterministic #}
{%- set s = lookup("v1", "Secret", release.namespace, "db-password") %}
password: {{ s.data.password | default(b64encode("changeme")) }}

{# Good — Sherpack-native, deterministic, GitOps-friendly #}
password: {{ generate_secret("db-password", 32) }}
```

## GitOps and ArgoCD/Flux

If you use ArgoCD or Flux to apply Packs:

- The output of `sherpack template` (or whatever your operator uses to
  render) **never** sees `lookup` results — `lookup` always returns `{}` in
  template mode. Your GitOps manifests are reproducible from Git alone.
- The output of `sherpack install/upgrade` *does* see live cluster state.
  If you run install/upgrade from a CI pipeline that's *not* the GitOps
  controller, you may produce manifests that drift from what GitOps then
  reconciles. **Don't mix GitOps reconciliation with imperative `sherpack
  install`** unless you understand the consequences.

The pragmatic rule: if your delivery pipeline is GitOps, prefer
`sherpack template` + commit the rendered manifests, or migrate `lookup`
calls to the alternatives above.

## Implementation reference

For curious readers / contributors:

- Trait: `crates/sherpack-engine/src/cluster_reader.rs::ClusterReader`
- Engine integration: `EngineBuilder::with_cluster_reader`
- Kubernetes impl: `crates/sherpack-kube/src/lookup.rs::KubeClusterReader`
- Wiring: `crates/sherpack-kube/src/client.rs::engine_with_lookup`
- Helm chart converter: preserves `lookup(av, k, ns, n)` calls
  (`crates/sherpack-convert/src/transformer.rs`)

The trait is sync (because MiniJinja functions are sync); the impl bridges
to async via `tokio::task::block_in_place` + `Handle::block_on`. This
requires being inside a multi-threaded tokio runtime, which is what the
Sherpack CLI provides.
