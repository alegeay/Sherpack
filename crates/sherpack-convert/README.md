# sherpack-convert

Helm chart to Sherpack pack converter - Transform Go templates into elegant Jinja2 syntax.

## Overview

`sherpack-convert` provides automated conversion of Helm charts to Sherpack packs. Rather than simply replicating Go template's function-based syntax, it transforms templates into idiomatic Jinja2 patterns that are more readable and maintainable.

## Philosophy

**Jinja2 elegance over Helm compatibility.** Instead of creating 1:1 mappings of Go template quirks, we convert to natural Jinja2 patterns:

| Helm (Go template) | Sherpack (Jinja2) |
|--------------------|-------------------|
| `{{ index .Values.list 0 }}` | `{{ values.list[0] }}` |
| `{{ add 1 2 }}` | `{{ 1 + 2 }}` |
| `{{ ternary "a" "b" .X }}` | `{{ "a" if x else "b" }}` |
| `{{ printf "%s-%s" a b }}` | `{{ a ~ "-" ~ b }}` |
| `{{ coalesce .A .B "c" }}` | `{{ a or b or "c" }}` |
| `{{ include "chart.name" . }}` | `{{ chart_name() }}` |
| `{{- if .Values.x -}}` | `{%- if values.x -%}` |

## Quick Start

```rust
use std::path::Path;
use sherpack_convert::{convert, convert_with_options, ConvertOptions};

// Simple conversion
let result = convert(
    Path::new("./my-helm-chart"),
    Path::new("./my-sherpack-pack"),
)?;

println!("Converted {} files", result.converted_files.len());
println!("Warnings: {}", result.warnings.len());

// With options
let result = convert_with_options(
    Path::new("./helm-chart"),
    Path::new("./sherpack-pack"),
    ConvertOptions {
        force: true,      // Overwrite existing
        dry_run: false,   // Actually write files
        verbose: true,    // Print progress
    },
)?;
```

## Conversion Process

### 1. Chart.yaml → Pack.yaml

```yaml
# Helm Chart.yaml
apiVersion: v2
name: my-app
version: 1.0.0
appVersion: "2.0"
description: My application
type: application
dependencies:
  - name: postgresql
    version: "12.x.x"
    repository: https://charts.bitnami.com/bitnami
    condition: postgresql.enabled
```

Converts to:

```yaml
# Sherpack Pack.yaml
apiVersion: sherpack/v1
kind: application

metadata:
  name: my-app
  version: 1.0.0
  appVersion: "2.0"
  description: My application

dependencies:
  - name: postgresql
    version: "12.x.x"
    repository: https://charts.bitnami.com/bitnami
    condition: postgresql.enabled
```

### 2. Template Conversion

#### Variables

```go
{{/* Helm */}}
{{ .Values.image.tag }}
{{ .Release.Name }}
{{ .Chart.Name }}
{{ .Capabilities.KubeVersion }}
```

```jinja2
{# Sherpack #}
{{ values.image.tag }}
{{ release.name }}
{{ pack.name }}
{{ capabilities.kubeVersion }}
```

#### Conditionals

```go
{{/* Helm */}}
{{- if .Values.ingress.enabled }}
...
{{- else if .Values.service.enabled }}
...
{{- else }}
...
{{- end }}
```

```jinja2
{# Sherpack #}
{%- if values.ingress.enabled %}
...
{%- elif values.service.enabled %}
...
{%- else %}
...
{%- endif %}
```

#### Loops

```go
{{/* Helm */}}
{{- range .Values.hosts }}
- host: {{ . }}
{{- end }}

{{- range $key, $value := .Values.labels }}
{{ $key }}: {{ $value }}
{{- end }}
```

```jinja2
{# Sherpack #}
{%- for host in values.hosts %}
- host: {{ host }}
{%- endfor %}

{%- for key, value in values.labels %}
{{ key }}: {{ value }}
{%- endfor %}
```

#### With Blocks

```go
{{/* Helm */}}
{{- with .Values.nodeSelector }}
nodeSelector:
  {{- toYaml . | nindent 2 }}
{{- end }}
```

```jinja2
{# Sherpack #}
{%- if values.nodeSelector %}
nodeSelector:
  {{ values.nodeSelector | toyaml | nindent(2) }}
{%- endif %}
```

### 3. Macro Conversion (Three-Pass System)

Helm's `define`/`include` pattern is converted to Jinja2 macros:

```go
{{/* Helm _helpers.tpl */}}
{{- define "mychart.fullname" -}}
{{- printf "%s-%s" .Release.Name .Chart.Name | trunc 63 | trimSuffix "-" }}
{{- end }}

{{- define "mychart.labels" -}}
app.kubernetes.io/name: {{ include "mychart.name" . }}
app.kubernetes.io/instance: {{ .Release.Name }}
{{- end }}
```

```jinja2
{# Sherpack _helpers.tpl #}
{% macro mychart_fullname() %}
{{- (release.name ~ "-" ~ pack.name)[:63] | trimSuffix("-") -}}
{% endmacro %}

{% macro mychart_labels() %}
app.kubernetes.io/name: {{ mychart_name() }}
app.kubernetes.io/instance: {{ release.name }}
{% endmacro %}
```

**Three-Pass Conversion:**

1. **Pass 1**: Extract all macro definitions (`define` blocks)
2. **Pass 2**: Build dependency graph between macros
3. **Pass 3**: Generate import statements and convert calls

### 4. Filter/Function Mapping

| Helm | Sherpack | Notes |
|------|----------|-------|
| `toYaml` | `toyaml` | Lowercase in Sherpack |
| `toJson` | `tojson` | |
| `b64enc` | `b64encode` | Full name |
| `b64dec` | `b64decode` | |
| `indent N` | `indent(N)` | Function syntax |
| `nindent N` | `nindent(N)` | |
| `quote` | `quote` | Same |
| `squote` | `squote` | Same |
| `upper` | `upper` | Same |
| `lower` | `lower` | Same |
| `title` | `title` | Same |
| `trim` | `trim` | Same |
| `trimPrefix` | `trimPrefix` | Same |
| `trimSuffix` | `trimSuffix` | Same |
| `default X` | `default(X)` | Function syntax |
| `required MSG` | `required(MSG)` | |
| `printf FMT args...` | Native `~` or `format` | |
| `ternary A B C` | `A if C else B` | Native Jinja2 |
| `coalesce A B C` | `A or B or C` | Native Jinja2 |
| `list A B C` | `[A, B, C]` | Native Jinja2 |
| `dict K1 V1 K2 V2` | `{"K1": V1, "K2": V2}` | Native Jinja2 |
| `add A B` | `A + B` | Native operators |
| `sub A B` | `A - B` | |
| `mul A B` | `A * B` | |
| `div A B` | `A / B` | |
| `mod A B` | `A % B` | |
| `and A B` | `A and B` | |
| `or A B` | `A or B` | |
| `not A` | `not A` | |
| `eq A B` | `A == B` | |
| `ne A B` | `A != B` | |
| `lt A B` | `A < B` | |
| `le A B` | `A <= B` | |
| `gt A B` | `A > B` | |
| `ge A B` | `A >= B` | |
| `empty X` | `not X` | |
| `len X` | `X \| length` | |
| `first X` | `X \| first` | |
| `last X` | `X \| last` | |
| `has KEY OBJ` | `OBJ \| has(KEY)` | |
| `hasKey OBJ KEY` | `OBJ \| has(KEY)` | Reordered |
| `keys OBJ` | `OBJ \| keys` | |
| `values OBJ` | `OBJ \| values` | |
| `include NAME CTX` | `NAME()` | Macro call |

## Unsupported Features

Some Helm features are intentionally not converted because they are anti-patterns in GitOps:

### Cryptographic Functions

```go
{{/* NOT SUPPORTED */}}
{{ genCA "my-ca" 365 }}
{{ genPrivateKey "ecdsa" }}
{{ genSelfSignedCert ... }}
```

**Why:** Generates different output each time → non-deterministic manifests.
**Alternative:** Use [cert-manager](https://cert-manager.io/) or [external-secrets](https://external-secrets.io/).

### Random Functions

```go
{{/* NOT SUPPORTED */}}
{{ randAlphaNum 32 }}
{{ randAlpha 10 }}
{{ randNumeric 8 }}
```

**Why:** Different on every render → drift in GitOps.
**Alternative:** Pre-generate values or use external-secrets.

### Files API

```go
{{/* NOT SUPPORTED */}}
{{ .Files.Get "config/settings.json" }}
{{ .Files.Glob "files/*" }}
{{ .Files.AsConfig }}
```

**Why:** Complex file system operations during templating.
**Alternative:** Embed content in `values.yaml` or create ConfigMaps.

### DNS/Network Lookups

```go
{{/* NOT SUPPORTED */}}
{{ getHostByName "myservice" }}
```

**Why:** Runtime cluster dependency → non-deterministic.
**Alternative:** Use explicit values or DNS-based discovery at runtime.

### Lookup Function

```go
{{/* PARTIALLY SUPPORTED */}}
{{ lookup "v1" "Secret" "ns" "name" }}
```

**Why:** Queries live cluster state → breaks `helm template`.
**Conversion:** Returns empty dict `{}` (same as `helm template`).

## Warning System

The converter produces detailed warnings for patterns that need attention:

```rust
use sherpack_convert::{ConversionWarning, WarningSeverity, WarningCategory};

let result = convert(source, dest)?;

for warning in &result.warnings {
    match warning.severity {
        WarningSeverity::Unsupported => {
            println!("UNSUPPORTED: {} at {}:{}",
                warning.pattern, warning.file, warning.line);
            if let Some(suggestion) = &warning.suggestion {
                println!("  Alternative: {}", suggestion);
            }
        }
        WarningSeverity::Warning => {
            println!("WARNING: {}", warning.message);
        }
        WarningSeverity::Info => {
            println!("INFO: {}", warning.message);
        }
    }
}
```

### Warning Categories

| Category | Description |
|----------|-------------|
| `UnsupportedFunction` | Function cannot be converted |
| `PartialConversion` | Converted but may need review |
| `DeprecatedPattern` | Helm pattern not recommended |
| `ComplexExpression` | May need manual adjustment |
| `MacroDependency` | Cross-chart macro reference |

## API Reference

### Core Types

```rust
/// Conversion options
pub struct ConvertOptions {
    /// Overwrite existing output directory
    pub force: bool,
    /// Don't write files, just return results
    pub dry_run: bool,
    /// Print verbose progress
    pub verbose: bool,
}

/// Conversion result
pub struct ConversionResult {
    /// Successfully converted files
    pub converted_files: Vec<ConvertedFile>,
    /// Warnings generated during conversion
    pub warnings: Vec<ConversionWarning>,
    /// Files that were copied without conversion
    pub copied_files: Vec<PathBuf>,
}

/// A converted file
pub struct ConvertedFile {
    pub source: PathBuf,
    pub destination: PathBuf,
    pub original_content: String,
    pub converted_content: String,
}
```

### Low-Level API

For more control over the conversion process:

```rust
use sherpack_convert::{Converter, parser, transformer};

// Create converter
let converter = Converter::new();

// Convert single template
let jinja2 = converter.convert_template(go_template_content)?;

// Parse Go template to AST
let ast = parser::parse(go_template_content)?;

// Transform AST to Jinja2
let output = transformer::transform(&ast)?;
```

## Architecture

```
sherpack-convert/
├── src/
│   ├── lib.rs          # Public API
│   ├── parser.rs       # Go template parser (pest)
│   ├── ast.rs          # Abstract syntax tree
│   ├── transformer.rs  # AST → Jinja2 transformer
│   ├── converter.rs    # High-level conversion logic
│   ├── chart.rs        # Chart.yaml → Pack.yaml
│   └── error.rs        # Error types
├── src/go_template.pest # PEG grammar for Go templates
```

### Parser (pest)

The Go template parser is built using [pest](https://pest.rs/) with a PEG grammar:

```pest
template = { (text | action)* }
action = { "{{" ~ whitespace_control? ~ inner ~ whitespace_control? ~ "}}" }
inner = { comment | range | if_block | with_block | define | include | ... }
```

### Transformer

The transformer walks the AST and generates Jinja2:

```rust
impl Transformer {
    fn transform_node(&self, node: &Node) -> Result<String> {
        match node {
            Node::Text(s) => Ok(s.clone()),
            Node::Variable(expr) => self.transform_variable(expr),
            Node::If { condition, body, else_body } => {
                self.transform_if(condition, body, else_body)
            }
            Node::Range { var, iter, body } => {
                self.transform_range(var, iter, body)
            }
            // ...
        }
    }
}
```

## Testing

The converter includes comprehensive snapshot tests:

```bash
# Run all tests
cargo test -p sherpack-convert

# Update snapshots
cargo insta review
```

Example test:

```rust
#[test]
fn test_convert_if_else() {
    let input = r#"{{- if .Values.enabled }}
enabled: true
{{- else }}
enabled: false
{{- end }}"#;

    let output = convert_template(input).unwrap();
    insta::assert_snapshot!(output);
}
```

## Dependencies

- `pest` / `pest_derive` - PEG parser generator
- `phf` - Perfect hash maps for filter/function lookup
- `sherpack-core` - Core types
- `walkdir` - Directory traversal
- `regex` - Pattern matching
- `miette` - Error reporting

## License

MIT OR Apache-2.0
