# Sherpack Template Functions: Beyond Helm

This document analyzes how Sherpack can improve upon Helm's `toStrings`, `lookup`, and `tpl` functions.

## Executive Summary

| Function | Helm Limitation | Sherpack Improvement |
|----------|-----------------|----------------------|
| `lookup` | Returns `{}` in template mode, breaks GitOps | **Lookup Providers** + **Declared Lookups** |
| `tpl` | Security risks, no sandboxing, poor errors | **Safe Mode** + **Pre-compilation** + **Scoped Context** |
| `toStrings` | Basic functionality only | **Format strings** + **Null handling** |

---

## 1. Lookup: The GitOps Problem

### Helm's Fundamental Flaw

Helm's `lookup` creates an **irreconcilable tension**:

```
Template-Time (offline, deterministic) ←→ Runtime (online, dynamic)
```

**Impact on GitOps ([ArgoCD #5202](https://github.com/argoproj/argo-cd/issues/5202), [#21745](https://github.com/argoproj/argo-cd/issues/21745)):**
- ArgoCD uses `helm template` → lookup returns `{}`
- Charts using lookup **cannot be diffed** properly
- FluxCD uses native Helm library but still has limitations
- Multi-cluster/multi-tenant setups cannot use workarounds

**Current Workarounds (all inadequate):**
1. Custom wrapper scripts with `--validate` flag
2. Config Management Plugins with kubectl pre-fetch
3. Abandoning lookup entirely

### Sherpack Solution: Lookup Providers

#### 1.1 Provider Trait System

```rust
// crates/sherpack-engine/src/lookup.rs

/// Trait for lookup providers - can be injected into the engine
pub trait LookupProvider: Send + Sync {
    /// Perform a lookup against some data source
    fn lookup(
        &self,
        api_version: &str,
        kind: &str,
        namespace: &str,
        name: &str,
    ) -> LookupResult;

    /// Provider name for error messages
    fn name(&self) -> &str;
}

/// Rich result type with provenance
pub enum LookupResult {
    /// Resource found
    Found(Value),
    /// Resource doesn't exist
    NotFound,
    /// No permission to access (RBAC)
    Forbidden(String),
    /// Cluster not reachable
    Unavailable(String),
    /// From mock/test provider
    Mocked(Value),
    /// From cache with age
    Cached { value: Value, age: Duration },
}
```

**Available Providers:**

| Provider | Use Case | Cluster Required |
|----------|----------|------------------|
| `EmptyProvider` | Pure templates (current) | No |
| `MockProvider` | Testing, CI/CD | No |
| `FileProvider` | Local development | No |
| `CachedProvider` | Offline GitOps | No (uses snapshot) |
| `ClusterProvider` | Install/upgrade | Yes |

#### 1.2 Mock Provider for Testing

```yaml
# test/lookup-mocks.yaml
mocks:
  - apiVersion: v1
    kind: Secret
    namespace: default
    name: existing-secret
    data:
      password: dGVzdC1wYXNzd29yZA==  # test-password

  - apiVersion: v1
    kind: ConfigMap
    namespace: kube-system
    name: cluster-info
    data:
      cluster-name: test-cluster
```

```bash
# Use mock provider for testing
sherpack template myrelease ./mypack --lookup-mocks=test/lookup-mocks.yaml
```

#### 1.3 Cached Provider for GitOps

```bash
# Capture cluster state for GitOps diffing
sherpack cluster-snapshot --output=cluster-state.yaml \
  --resources="Secret,ConfigMap,Service" \
  --namespaces="default,kube-system"

# Use cached state for template rendering
sherpack template myrelease ./mypack --lookup-cache=cluster-state.yaml
```

**GitOps Integration:**
```yaml
# ArgoCD Application
apiVersion: argoproj.io/v1alpha1
kind: Application
spec:
  source:
    plugin:
      name: sherpack
      env:
        - name: SHERPACK_LOOKUP_CACHE
          value: /config/cluster-state.yaml
```

#### 1.4 Declared Lookups (Best Practice)

Instead of runtime lookups, **declare dependencies upfront**:

```yaml
# Pack.yaml
apiVersion: sherpack/v1
kind: pack
name: myapp

# Declare what the pack needs to lookup
lookups:
  existingSecret:
    apiVersion: v1
    kind: Secret
    namespace: "{{ release.namespace }}"
    name: "{{ values.existingSecretName }}"
    optional: true  # Don't fail if not found

  clusterIssuer:
    apiVersion: cert-manager.io/v1
    kind: ClusterIssuer
    name: "{{ values.certIssuer }}"
    optional: false  # Required - fail if not found
```

**Benefits:**
1. **Validated at pack load time** - catch typos early
2. **Can be mocked/overridden** via values
3. **GitOps tools understand dependencies**
4. **Cacheable** - snapshot only declared resources

**Template usage:**
```jinja
{# Declared lookups available as 'lookups' context #}
{% if lookups.existingSecret %}
secretName: {{ lookups.existingSecret.metadata.name }}
{% else %}
secretName: {{ release.name }}-generated-secret
{% endif %}
```

#### 1.5 Lookup with Detailed Results

```jinja
{# New: lookup_detailed returns LookupResult with status #}
{% set result = lookup_detailed("v1", "Secret", ns, name) %}

{% if result.found %}
  password: {{ result.value.data.password }}
{% elif result.not_found %}
  {# Generate new secret #}
  password: {{ uuidv4() | b64encode }}
{% elif result.forbidden %}
  {{ fail("RBAC error: " ~ result.message) }}
{% elif result.unavailable %}
  {# Cluster offline - use default #}
  password: {{ values.defaultPassword | b64encode }}
{% endif %}
```

---

## 2. tpl: The Security Problem

### Helm's Security Issues

**[CVE-2025-53547](https://security.snyk.io/vuln/SNYK-GOLANG-GITHUBCOMHELMHELMPKGDOWNLOADER-10664612)** (CVSS 8.5) demonstrates Helm's template security weaknesses.

**tpl-specific risks:**
1. **Arbitrary code execution** - values.yaml can contain malicious templates
2. **No recursion limit** - `tpl` calling `tpl` infinitely
3. **No sandboxing** - full context access including secrets
4. **Poor error messages** - `<tpl>` as source, no trace
5. **Performance** - recompiles template on every call

### Sherpack Solution: Safe tpl

#### 2.1 Safe Mode Configuration

```yaml
# Pack.yaml
tplOptions:
  # Recursion protection
  maxDepth: 3

  # Timeout protection (DoS prevention)
  timeout: 100ms

  # Function whitelist (if set, only these allowed)
  allowedFunctions:
    - default
    - quote
    - b64encode
    - upper
    - lower
    - trim

  # Function blacklist (always denied in tpl)
  deniedFunctions:
    - lookup        # No cluster access from values
    - lookup_detailed
    - fail          # Don't allow fail from values

  # Context restrictions
  allowedContext:
    - values        # Only values accessible
    - release.name
    - release.namespace
    # pack.* and capabilities.* NOT accessible
```

#### 2.2 Pre-compilation for Performance

```rust
// Detect and pre-compile tpl-able values at pack load time
pub struct CompiledValues {
    /// Raw values
    raw: Value,
    /// Pre-compiled templates for values containing {{ or {%
    compiled: HashMap<String, Template>,
}

impl CompiledValues {
    pub fn load(values: Value) -> Result<Self, Error> {
        let mut compiled = HashMap::new();

        // Walk values tree, compile any string with template markers
        Self::compile_recursive(&values, "", &mut compiled)?;

        Ok(Self { raw: values, compiled })
    }

    fn compile_recursive(
        value: &Value,
        path: &str,
        compiled: &mut HashMap<String, Template>,
    ) -> Result<(), Error> {
        if let Some(s) = value.as_str() {
            if s.contains("{{") || s.contains("{%") {
                // Pre-compile and store
                let template = compile_template(s)
                    .map_err(|e| Error::new(
                        format!("Invalid template in values at '{}': {}", path, e)
                    ))?;
                compiled.insert(path.to_string(), template);
            }
        }
        // ... recurse into objects/arrays
        Ok(())
    }
}
```

**Benefits:**
- Syntax errors caught at pack load time
- Each template compiled once, not on every render
- Can analyze variable usage statically

#### 2.3 Explicit tpl Values (YAML Tag)

```yaml
# values.yaml

# Option 1: Explicit _tpl key
host:
  _tpl: "{{ release.name }}.{{ values.domain }}"

# Option 2: YAML tag (requires custom deserializer)
host: !tpl "{{ release.name }}.{{ values.domain }}"

# Option 3: Template file reference
configFile:
  _tpl_file: "templates/_config.tpl"
```

**Validation:**
```bash
$ sherpack lint ./mypack

✓ Pack.yaml valid
✓ values.yaml valid
  ├─ Found 3 tpl values:
  │  ├─ ingress.host: "{{ release.name }}.{{ values.domain }}"
  │  ├─ labels.app: "{{ pack.name }}"
  │  └─ config.url: "https://{{ values.host }}:{{ values.port }}"
  └─ All tpl values compile successfully
```

#### 2.4 Scoped Context

```jinja
{# Limit what's accessible in tpl evaluation #}
{{ tpl_scoped(values.template, {
    "name": release.name,
    "ns": release.namespace
}) }}

{# values.template can only access 'name' and 'ns' #}
{# NOT values.secretPassword, NOT pack.*, etc. #}
```

#### 2.5 Recursion Detection

```rust
fn tpl_with_guard(
    state: &State,
    template: String,
    context: Value,
) -> Result<String, Error> {
    // Get or initialize recursion counter
    let depth = state.get_temp::<AtomicUsize>("tpl_depth")
        .map(|d| d.fetch_add(1, Ordering::SeqCst) + 1)
        .unwrap_or(1);

    if depth > MAX_TPL_DEPTH {
        return Err(Error::new(
            ErrorKind::InvalidOperation,
            format!(
                "tpl recursion depth {} exceeded maximum {}. \
                 Possible infinite loop in values.",
                depth, MAX_TPL_DEPTH
            )
        ));
    }

    // Render with timeout
    let result = tokio::time::timeout(
        TPL_TIMEOUT,
        state.env().render_str(&template, context)
    ).await;

    match result {
        Ok(Ok(s)) => Ok(s),
        Ok(Err(e)) => Err(e),
        Err(_) => Err(Error::new(
            ErrorKind::InvalidOperation,
            "tpl evaluation timed out (possible infinite loop)"
        )),
    }
}
```

#### 2.6 Source Tracking for Errors

```
Error in tpl evaluation:

  Source chain:
    1. values.yaml:15 → ingress.hosts[0]
    2. tpl evaluation of: "{{ release.name }}.{{ values.domian }}"

  Error at position 34:
    "{{ release.name }}.{{ values.domian }}"
                                  ^^^^^^
    undefined variable 'domian'

  Did you mean: values.domain?

  Available in values:
    - domain: "example.com"
    - port: 443
```

---

## 3. toStrings: Minor Improvements

### 3.1 Format String Support

```jinja
{# Basic conversion #}
{{ list(1, 2, 3) | tostrings }}
→ ["1", "2", "3"]

{# With format string (new) #}
{{ list(1, 2, 3) | tostrings(format="%03d") }}
→ ["001", "002", "003"]

{# With prefix/suffix #}
{{ list(80, 443) | tostrings(prefix="port-", suffix="/TCP") }}
→ ["port-80/TCP", "port-443/TCP"]
```

### 3.2 Null Handling Options

```jinja
{# Default: null becomes "null" string #}
{{ list(1, none, 3) | tostrings }}
→ ["1", "null", "3"]

{# Replace nulls #}
{{ list(1, none, 3) | tostrings(null="N/A") }}
→ ["1", "N/A", "3"]

{# Skip nulls #}
{{ list(1, none, 3) | tostrings(skip_null=true) }}
→ ["1", "3"]

{# Fail on nulls #}
{{ list(1, none, 3) | tostrings(null_error=true) }}
→ Error: null value at index 1
```

### 3.3 Object Key/Value Conversion

```jinja
{# Convert dict to list of "key=value" strings #}
{{ {"a": 1, "b": 2} | tostrings(format="{key}={value}") }}
→ ["a=1", "b=2"]

{# Useful for environment variables #}
{{ values.env | tostrings(format="{key}={value}") | join(",") }}
→ "FOO=bar,BAZ=qux"
```

---

## 4. Implementation Roadmap

### Phase 1: Foundation (Current)
- [x] Basic `tostrings` filter
- [x] Basic `tpl` / `tpl_ctx` functions
- [x] Basic `lookup` (returns empty)

### Phase 2: Safety & Errors
- [ ] tpl recursion depth limit
- [ ] tpl timeout
- [ ] Better tpl error messages with source chain
- [ ] Pre-compilation of values templates

### Phase 3: Lookup Providers
- [ ] `LookupProvider` trait
- [ ] `MockProvider` implementation
- [ ] `FileProvider` implementation
- [ ] `--lookup-mocks` CLI flag

### Phase 4: Advanced Features
- [ ] Declared lookups in Pack.yaml
- [ ] `CachedProvider` for GitOps
- [ ] `lookup_detailed` with rich results
- [ ] tpl sandboxing (allowed/denied functions)

### Phase 5: GitOps Integration
- [ ] `sherpack cluster-snapshot` command
- [ ] ArgoCD plugin with lookup cache
- [ ] FluxCD HelmRelease integration docs

---

## 5. Comparison Matrix

| Feature | Helm | Sherpack (Current) | Sherpack (Planned) |
|---------|------|-------------------|-------------------|
| **lookup in template mode** | Returns `{}` | Returns `{}` | Mock/File/Cache providers |
| **lookup in GitOps** | Broken | Broken | Full support via cache |
| **lookup testing** | Manual mocks | Manual mocks | `--lookup-mocks` flag |
| **tpl security** | None | None | Sandbox, whitelist |
| **tpl recursion** | Unlimited | Unlimited | Max depth limit |
| **tpl timeout** | None | None | Configurable |
| **tpl errors** | `<tpl>` source | `<tpl>` source | Full source chain |
| **tpl pre-compile** | No | No | Yes |
| **tostrings format** | No | No | Yes |
| **tostrings null handling** | Basic | Basic | Configurable |

---

## Sources

- [ArgoCD Issue #5202: Helm lookup function support](https://github.com/argoproj/argo-cd/issues/5202)
- [ArgoCD Issue #21745: Helm lookup enhancement proposal](https://github.com/argoproj/argo-cd/issues/21745)
- [CVE-2025-53547: Helm Code Execution Vulnerability](https://security.snyk.io/vuln/SNYK-GOLANG-GITHUBCOMHELMHELMPKGDOWNLOADER-10664612)
- [Helm Chart Injection: Security Risks](https://www.startupdefense.io/cyberattacks/helm-chart-injection)
- [Helm tpl Function Documentation](https://helm.sh/docs/howto/charts_tips_and_tricks/)
- [MiniJinja State Documentation](https://docs.rs/minijinja/latest/minijinja/struct.State.html)
