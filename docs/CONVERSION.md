# Helm to Sherpack Conversion Guide

This guide explains how Sherpack converts Helm charts to Sherpack packs,
transforming Go templates into **idiomatic Jinja2** syntax.

## Philosophy

Sherpack doesn't replicate Helm's Go template complexity. Instead, it converts
to **elegant, native Jinja2 patterns** that Python developers will find familiar:

```
Helm (verbose, function-based):
{{ ternary "yes" "no" .Values.enabled }}
{{ printf "%s-%s" .Release.Name .Chart.Name }}
{{ index .Values.list 0 }}

Sherpack (elegant, Pythonic):
{{ "yes" if values.enabled else "no" }}
{{ release.name ~ "-" ~ pack.name }}
{{ values.list[0] }}
```

## Quick Start

```bash
# Convert a Helm chart
sherpack convert ./my-helm-chart

# Convert with custom output
sherpack convert ./my-helm-chart -o ./my-sherpack-pack

# Preview without writing files
sherpack convert ./my-helm-chart --dry-run

# Overwrite existing output
sherpack convert ./my-helm-chart --force
```

## Conversion Reference

### Variable Access

| Helm (Go Template) | Sherpack (Jinja2) |
|-------------------|-------------------|
| `{{ .Values.x }}` | `{{ values.x }}` |
| `{{ .Values.nested.value }}` | `{{ values.nested.value }}` |
| `{{ .Release.Name }}` | `{{ release.name }}` |
| `{{ .Release.Namespace }}` | `{{ release.namespace }}` |
| `{{ .Release.Service }}` | `{{ "Sherpack" }}` |
| `{{ .Chart.Name }}` | `{{ pack.name }}` |
| `{{ .Chart.Version }}` | `{{ pack.version }}` |
| `{{ .Chart.AppVersion }}` | `{{ pack.appVersion }}` |
| `{{ .Capabilities.KubeVersion }}` | `{{ capabilities.kubeVersion }}` |

### Control Structures

#### Conditionals

```yaml
# Helm
{{- if .Values.enabled }}
enabled: true
{{- else if .Values.disabled }}
enabled: false
{{- else }}
enabled: default
{{- end }}

# Sherpack
{%- if values.enabled %}
enabled: true
{%- elif values.disabled %}
enabled: false
{%- else %}
enabled: default
{%- endif %}
```

#### Loops

```yaml
# Helm - simple range
{{- range .Values.items }}
- {{ . }}
{{- end }}

# Sherpack
{%- for item in values.items %}
- {{ item }}
{%- endfor %}

# Helm - range with index
{{- range $index, $value := .Values.items }}
- {{ $index }}: {{ $value }}
{{- end }}

# Sherpack
{%- for value in values.items %}
- {{ loop.index0 }}: {{ value }}
{%- endfor %}
```

### Native Operators

Sherpack converts Helm's function-based syntax to native Jinja2 operators:

#### Comparison

| Helm | Sherpack |
|------|----------|
| `{{ eq .Values.a .Values.b }}` | `{{ values.a == values.b }}` |
| `{{ ne .Values.a "test" }}` | `{{ values.a != "test" }}` |
| `{{ lt .Values.a 10 }}` | `{{ values.a < 10 }}` |
| `{{ le .Values.a 10 }}` | `{{ values.a <= 10 }}` |
| `{{ gt .Values.a 5 }}` | `{{ values.a > 5 }}` |
| `{{ ge .Values.a 5 }}` | `{{ values.a >= 5 }}` |

#### Math Operations

| Helm | Sherpack |
|------|----------|
| `{{ add 1 2 }}` | `{{ 1 + 2 }}` |
| `{{ sub 10 5 }}` | `{{ 10 - 5 }}` |
| `{{ mul 3 4 }}` | `{{ 3 * 4 }}` |
| `{{ div 10 2 }}` | `{{ 10 / 2 }}` |
| `{{ mod 10 3 }}` | `{{ 10 % 3 }}` |

#### Logical Operations

| Helm | Sherpack |
|------|----------|
| `{{ and .Values.a .Values.b }}` | `{{ values.a and values.b }}` |
| `{{ or .Values.a .Values.b }}` | `{{ values.a or values.b }}` |
| `{{ not .Values.a }}` | `{{ not values.a }}` |

### Index Access

```yaml
# Helm - index function
{{ index .Values.list 0 }}
{{ index .Values.map "key" }}
{{ index .Values.nested "a" "b" "c" }}

# Sherpack - bracket notation
{{ values.list[0] }}
{{ values.map["key"] }}
{{ values.nested["a"]["b"]["c"] }}
```

### Ternary/Conditional Expression

```yaml
# Helm
{{ ternary "yes" "no" .Values.enabled }}

# Sherpack - Pythonic inline if
{{ "yes" if values.enabled else "no" }}
```

### Coalesce (First Non-Empty)

```yaml
# Helm
{{ coalesce .Values.custom .Values.default "fallback" }}

# Sherpack
{{ values.custom or values.default or "fallback" }}
```

### String Concatenation

```yaml
# Helm - printf function
{{ printf "%s-%s" .Release.Name .Chart.Name }}
{{ printf "prefix-%s-suffix" .Values.name }}

# Sherpack - ~ concatenation operator
{{ release.name ~ "-" ~ pack.name }}
{{ "prefix-" ~ values.name ~ "-suffix" }}
```

### List and Dict Literals

```yaml
# Helm
{{ list 1 2 3 }}
{{ dict "key1" .Values.a "key2" .Values.b }}

# Sherpack - native syntax
{{ [1, 2, 3] }}
{{ {"key1": values.a, "key2": values.b} }}
```

### Contains/In Operator

```yaml
# Helm
{{ if contains "needle" .Values.haystack }}
{{ if contains $name .Release.Name }}

# Sherpack
{% if "needle" in values.haystack %}
{% if name in release.name %}
```

### Range Generation

```yaml
# Helm
{{- range until 5 }}
{{- range untilStep 0 10 2 }}

# Sherpack
{%- for i in range(5) %}
{%- for i in range(0, 10, 2) %}
```

### Filter Mappings

| Helm Filter | Sherpack Filter |
|-------------|-----------------|
| `toYaml` | `toyaml` |
| `toJson` | `tojson` |
| `b64enc` | `b64encode` |
| `b64dec` | `b64decode` |
| `quote` | `quote` |
| `squote` | `squote` |
| `indent N` | `indent(N)` |
| `nindent N` | `nindent(N)` |
| `upper` | `upper` |
| `lower` | `lower` |
| `title` | `title` |
| `trim` | `trim` |
| `trimPrefix "-"` | `trimprefix("-")` |
| `trimSuffix "-"` | `trimsuffix("-")` |
| `trunc 63` | `trunc(63)` |
| `sha256sum` | `sha256` |
| `default "x"` | `default("x")` |
| `required "msg"` | `required("msg")` |
| `hasPrefix "x"` | `startswith("x")` |
| `hasSuffix "x"` | `endswith("x")` |

### Macros (Templates/Includes)

```yaml
# Helm - _helpers.tpl
{{- define "myapp.fullname" -}}
{{- printf "%s-%s" .Release.Name .Chart.Name | trunc 63 }}
{{- end }}

# Template usage
{{ include "myapp.fullname" . }}

# Sherpack - _helpers.j2
{%- macro fullname() -%}
{{ (release.name ~ "-" ~ pack.name) | trunc(63) }}
{%- endmacro -%}

# Macro usage (auto-import added)
{{ fullname() }}
```

## Unsupported Features

Some Helm features are **intentionally not supported** because they are
anti-patterns in a GitOps workflow:

### Cryptographic Functions

| Function | Alternative |
|----------|-------------|
| `genCA` | Use [cert-manager](https://cert-manager.io) CRDs |
| `genSelfSignedCert` | Use cert-manager or pre-generated certs |
| `genPrivateKey` | Use [external-secrets](https://external-secrets.io) |
| `htpasswd` | Pre-generate and store in external secrets |
| `randAlphaNum` | Pre-generate and store in values.yaml |

**Why?** Generating secrets in templates means:
- Different output on each render
- Secrets regenerated on upgrade (breaks applications)
- Cannot be reviewed in Git diff
- Incompatible with GitOps (ArgoCD, Flux)

### Files API

| Function | Alternative |
|----------|-------------|
| `.Files.Get` | Embed content in values.yaml |
| `.Files.GetBytes` | Embed base64 in values.yaml |
| `.Files.Glob` | List files explicitly in values |
| `.Files.Lines` | Embed as list in values |
| `.Files.AsConfig` | Use ConfigMap resource directly |
| `.Files.AsSecrets` | Use Secret resource directly |

**Why?** External file access:
- Breaks reproducibility
- Files may not exist in CI/CD environment
- Content not visible in Git review

### Runtime Functions

| Function | Alternative |
|----------|-------------|
| `lookup` | Use explicit values (returns `{}` in template mode) |
| `getHostByName` | Use explicit hostname/IP in values |

**Why?** Runtime queries:
- Break GitOps (different results per cluster)
- `helm template` cannot query cluster
- Cannot be tested in CI

## Migration Checklist

### Before Converting

1. Ensure chart validates: `helm lint ./my-chart`
2. Test chart renders: `helm template test ./my-chart`
3. Review for unsupported features

### After Converting

```bash
# 1. Validate pack structure
sherpack lint ./my-sherpack-pack

# 2. Test rendering
sherpack template test-release ./my-sherpack-pack

# 3. Review unsupported feature warnings
# Check __UNSUPPORTED_*__ markers in templates
grep -r "__UNSUPPORTED_" ./my-sherpack-pack/templates
```

### Manual Fixes

Some patterns require manual adjustment:

#### With Blocks

Go template's `with` creates a new context scope. The converter uses
`{% if %}` with a context variable, but may need review:

```yaml
# Helm
{{- with .Values.ingress }}
host: {{ .host }}
{{- end }}

# Converted (may need review)
{%- if values.ingress %}{%- set _with_ctx = values.ingress %}
host: {{ _with_ctx.host }}
{%- endif %}

# Manual fix (simpler)
{%- if values.ingress %}
host: {{ values.ingress.host }}
{%- endif %}
```

#### Complex tpl Usage

Dynamic templates with `tpl` are converted but limited to 10 recursion depth:

```yaml
# Helm
{{ tpl .Values.customTemplate . }}

# Sherpack (with protection)
{{ tpl(values.customTemplate) }}
```

## File Structure Mapping

| Helm | Sherpack |
|------|----------|
| `Chart.yaml` | `Pack.yaml` |
| `Chart.lock` | `Pack.lock.yaml` |
| `values.yaml` | `values.yaml` |
| `values.schema.json` | `values.schema.yaml` or `.json` |
| `templates/_helpers.tpl` | `templates/_helpers.j2` |
| `templates/*.yaml` | `templates/*.yaml` |
| `charts/` | `packs/` |
| `.helmignore` | `.sherpackignore` |

## Pack.yaml Format

```yaml
# Sherpack Pack.yaml
apiVersion: sherpack/v1
kind: application  # or "library"

metadata:
  name: my-app
  version: 1.0.0
  appVersion: "1.25.0"
  description: My application

  home: https://example.com
  sources:
    - https://github.com/example/repo

  keywords:
    - web
    - app

  maintainers:
    - name: John Doe
      email: john@example.com

# Dependencies
dependencies:
  - name: postgresql
    version: "^12.0"
    repository: https://charts.bitnami.com/bitnami
```

## Best Practices

### Use Jinja2 Idioms

```yaml
# Instead of
{{ values.name | default("") }}

# Use
{{ values.name or "" }}

# Instead of
{% if values.items %}{% for item in values.items %}...{% endfor %}{% endif %}

# Use
{% for item in values.items | default([]) %}...{% endfor %}
```

### Leverage Python-like Syntax

```yaml
# List comprehension style (in filters)
{{ values.items | selectattr("enabled") | list }}

# Dictionary access with default
{{ values.config.get("key", "default") }}
```

### Clear Variable Names

```yaml
# Avoid
{%- set _ = values.something %}

# Use descriptive names
{%- set db_config = values.database %}
```

## Troubleshooting

### "undefined variable" Errors

Check that variables are converted correctly:
- `.Values.x` → `values.x`
- `.Chart.Name` → `pack.name`
- `$.Values.x` (root access) → `values.x`

### Macro Not Found

Ensure imports are present at the top of template files:

```yaml
{%- from "_helpers.j2" import fullname, labels -%}
```

### Type Errors in Filters

Some filters require specific types:
- `indent(N)` requires integer N
- `split()` requires string input
- `join()` requires iterable input

## Getting Help

- Documentation: https://sherpack.dev/docs
- GitHub Issues: https://github.com/sherpack/sherpack/issues
- Migration Guide: https://sherpack.dev/docs/helm-migration
