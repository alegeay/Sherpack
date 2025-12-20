# Sherpack Convert: Helm Chart → Sherpack Pack Converter

## Executive Summary

| Component | Complexity | Approach | LOC Estimate |
|-----------|------------|----------|--------------|
| **Chart.yaml → Pack.yaml** | Easy | YAML transform | ~150 |
| **values.yaml** | None | Copy as-is | 0 |
| **Go templates → Jinja2** | Hard | Pest parser + AST transform | ~1500 |
| **_helpers.tpl → _macros.j2** | Medium | Reuse template parser | ~300 |
| **charts/ → packs/** | Easy | Recursive conversion | ~100 |
| **CLI command** | Easy | Clap integration | ~150 |
| **Tests** | Medium | Fixture-based | ~400 |
| **Total** | | | **~2600 LOC** |

---

## 1. What Already Exists

### In Sherpack
- ✅ Pack.yaml structure (`sherpack-core/src/pack.rs`)
- ✅ Values handling (`sherpack-core/src/values.rs`)
- ✅ Jinja2 filters mapped to Helm equivalents (`sherpack-engine/src/filters.rs`)
- ✅ Jinja2 functions mapped to Helm equivalents (`sherpack-engine/src/functions.rs`)
- ✅ YAML parsing (`serde_yaml`)

### Dependencies to Add
```toml
[dependencies]
pest = "2.7"           # PEG parser generator
pest_derive = "2.7"    # Derive macro for pest
```

---

## 2. Component Details

### 2.1 Chart.yaml → Pack.yaml Converter

**Input (Helm Chart.yaml):**
```yaml
apiVersion: v2
name: my-app
version: 1.0.0
appVersion: "2.0.0"
description: My application
type: application
keywords:
  - web
  - api
maintainers:
  - name: John Doe
    email: john@example.com
dependencies:
  - name: postgresql
    version: "12.x"
    repository: https://charts.bitnami.com/bitnami
    condition: postgresql.enabled
```

**Output (Sherpack Pack.yaml):**
```yaml
apiVersion: sherpack/v1
kind: pack
name: my-app
version: 1.0.0
appVersion: "2.0.0"
description: My application
keywords:
  - web
  - api
maintainers:
  - name: John Doe
    email: john@example.com

dependencies:
  - name: postgresql
    version: "12.x"
    repository: https://charts.bitnami.com/bitnami
    condition: postgresql.enabled
```

**Transformation Rules:**
| Helm Field | Sherpack Field | Notes |
|------------|----------------|-------|
| `apiVersion: v2` | `apiVersion: sherpack/v1` | Fixed mapping |
| `type: application` | `kind: pack` | Rename + value map |
| `type: library` | `kind: library` | |
| `kubeVersion` | `kubeVersion` | Keep as-is |
| `dependencies` | `dependencies` | Keep structure |
| `icon` | `icon` | Keep as-is |
| `sources` | `sources` | Keep as-is |

**Implementation:**
```rust
// crates/sherpack-convert/src/chart.rs

use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct HelmChart {
    api_version: String,
    name: String,
    version: String,
    #[serde(default)]
    app_version: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default, rename = "type")]
    chart_type: Option<String>,
    // ... other fields
}

pub fn convert_chart(helm: &HelmChart) -> PackMetadata {
    PackMetadata {
        api_version: "sherpack/v1".to_string(),
        kind: match helm.chart_type.as_deref() {
            Some("library") => "library".to_string(),
            _ => "pack".to_string(),
        },
        name: helm.name.clone(),
        version: helm.version.parse().unwrap(),
        // ...
    }
}
```

---

### 2.2 Go Template Parser (Pest Grammar)

**Go Template Syntax Elements:**

| Element | Go Syntax | Example |
|---------|-----------|---------|
| Variable | `{{ .Values.x }}` | `{{ .Values.image.tag }}` |
| Pipeline | `{{ .X \| func }}` | `{{ .Values.name \| quote }}` |
| If | `{{- if .X }}...{{- end }}` | `{{- if .Values.enabled }}` |
| Else | `{{- else }}` | |
| Else If | `{{- else if .X }}` | |
| Range | `{{- range .X }}...{{- end }}` | `{{- range .Values.ports }}` |
| Range with vars | `{{- range $k, $v := .X }}` | |
| With | `{{- with .X }}...{{- end }}` | `{{- with .Values.config }}` |
| Define | `{{- define "name" }}...{{- end }}` | `{{- define "app.name" }}` |
| Include | `{{ include "name" . }}` | `{{ include "app.name" . }}` |
| Template | `{{ template "name" . }}` | |
| Comment | `{{/* comment */}}` | |
| Whitespace trim | `{{-` and `-}}` | |

**Pest Grammar (`go_template.pest`):**
```pest
// Go Template Grammar for Sherpack Converter

template = { SOI ~ (element)* ~ EOI }

element = {
    action
    | raw_text
}

raw_text = { (!("{{") ~ ANY)+ }

action = {
    action_open ~ action_body ~ action_close
}

action_open = { "{{" ~ "-"? }
action_close = { "-"? ~ "}}" }

action_body = {
    comment
    | if_action
    | else_action
    | else_if_action
    | end_action
    | range_action
    | with_action
    | define_action
    | template_action
    | block_action
    | pipeline
}

// Comments
comment = { "/*" ~ (!"*/" ~ ANY)* ~ "*/" }

// Control flow
if_action = { "if" ~ pipeline }
else_action = { "else" }
else_if_action = { "else" ~ "if" ~ pipeline }
end_action = { "end" }
range_action = { "range" ~ (range_vars ~ ":=")? ~ pipeline }
range_vars = { variable ~ ("," ~ variable)? }
with_action = { "with" ~ pipeline }

// Definitions
define_action = { "define" ~ string_literal }
template_action = { "template" ~ string_literal ~ pipeline? }
block_action = { "block" ~ string_literal ~ pipeline }

// Pipeline (the core expression system)
pipeline = { command ~ ("|" ~ command)* }

command = {
    function_call
    | field_access
    | variable
    | literal
}

function_call = { identifier ~ argument* }
argument = { pipeline | field_access | variable | literal }

// Field access: .Values.image.tag
field_access = { "."? ~ (identifier ~ ("." ~ identifier)*) }

// Variables: $var
variable = { "$" ~ identifier }

// Identifiers and literals
identifier = @{ (ASCII_ALPHA | "_") ~ (ASCII_ALPHANUMERIC | "_")* }
string_literal = { "\"" ~ (!"\"" ~ ANY)* ~ "\"" }
number = @{ "-"? ~ ASCII_DIGIT+ ~ ("." ~ ASCII_DIGIT+)? }
literal = { string_literal | number | "true" | "false" | "nil" }

WHITESPACE = _{ " " | "\t" | "\n" | "\r" }
```

---

### 2.3 AST → Jinja2 Transformer

**Transformation Rules:**

| Go Template | Jinja2 | Notes |
|-------------|--------|-------|
| `{{ .Values.x }}` | `{{ values.x }}` | Remove leading dot, lowercase |
| `{{ .Release.Name }}` | `{{ release.name }}` | Lowercase property |
| `{{ .Chart.Name }}` | `{{ pack.name }}` | Rename Chart → pack |
| `{{ .Capabilities.KubeVersion }}` | `{{ capabilities.kube_version }}` | snake_case |
| `{{ $.Values.x }}` | `{{ values.x }}` | Root access identical |
| `{{- if .X }}` | `{%- if x %}` | Different delimiters |
| `{{- else }}` | `{%- else %}` | |
| `{{- else if .X }}` | `{%- elif x %}` | elif not else if |
| `{{- end }}` | `{%- endif %}` or `{%- endfor %}` | Context-dependent |
| `{{- range .X }}` | `{%- for item in x %}` | Need variable name |
| `{{- range $i, $v := .X }}` | `{%- for v in x %}` + `loop.index0` | Loop vars |
| `{{- with .X }}` | `{%- with x %}` or inline | |
| `{{ include "name" . }}` | `{{ name() }}` | Convert to macro call |
| `{{ .X \| toYaml }}` | `{{ x \| toyaml }}` | Lowercase filter |
| `{{ .X \| indent 4 }}` | `{{ x \| indent(4) }}` | Function syntax |
| `{{/* comment */}}` | `{# comment #}` | Different syntax |

**Implementation:**
```rust
// crates/sherpack-convert/src/transform.rs

pub struct Transformer {
    /// Track if we're inside a range (for matching end → endfor)
    block_stack: Vec<BlockType>,
}

enum BlockType {
    If,
    Range { var_name: String },
    With,
    Define { name: String },
}

impl Transformer {
    pub fn transform(&mut self, ast: &Template) -> String {
        let mut output = String::new();
        for element in &ast.elements {
            output.push_str(&self.transform_element(element));
        }
        output
    }

    fn transform_element(&mut self, elem: &Element) -> String {
        match elem {
            Element::RawText(text) => text.clone(),
            Element::Action(action) => self.transform_action(action),
        }
    }

    fn transform_action(&mut self, action: &Action) -> String {
        let trim_left = action.trim_left;
        let trim_right = action.trim_right;

        match &action.body {
            ActionBody::If(pipeline) => {
                self.block_stack.push(BlockType::If);
                format!(
                    "{{%{} if {} %}}",
                    if trim_left { "-" } else { "" },
                    self.transform_pipeline(pipeline)
                )
            }
            ActionBody::ElseIf(pipeline) => {
                format!(
                    "{{%{} elif {} %}}",
                    if trim_left { "-" } else { "" },
                    self.transform_pipeline(pipeline)
                )
            }
            ActionBody::Else => {
                format!("{{%{} else %}}", if trim_left { "-" } else { "" })
            }
            ActionBody::End => {
                let block = self.block_stack.pop();
                let end_tag = match block {
                    Some(BlockType::If) => "endif",
                    Some(BlockType::Range { .. }) => "endfor",
                    Some(BlockType::With) => "endwith",
                    Some(BlockType::Define { .. }) => "endmacro",
                    None => "endif", // fallback
                };
                format!("{{%{} {} %}}", if trim_left { "-" } else { "" }, end_tag)
            }
            ActionBody::Range { vars, pipeline } => {
                let var_name = vars.as_ref()
                    .and_then(|v| v.value_var.as_ref())
                    .map(|s| s.as_str())
                    .unwrap_or("item");
                self.block_stack.push(BlockType::Range { var_name: var_name.to_string() });
                format!(
                    "{{%{} for {} in {} %}}",
                    if trim_left { "-" } else { "" },
                    var_name,
                    self.transform_pipeline(pipeline)
                )
            }
            ActionBody::Pipeline(pipeline) => {
                format!(
                    "{{{{{} {} {}}}}}",
                    if trim_left { "-" } else { "" },
                    self.transform_pipeline(pipeline),
                    if trim_right { "-" } else { "" }
                )
            }
            // ... other cases
        }
    }

    fn transform_pipeline(&self, pipeline: &Pipeline) -> String {
        let mut parts = vec![self.transform_command(&pipeline.commands[0])];

        for cmd in &pipeline.commands[1..] {
            parts.push(self.transform_filter(cmd));
        }

        parts.join(" | ")
    }

    fn transform_field_access(&self, field: &FieldAccess) -> String {
        let parts: Vec<&str> = field.path.iter().map(|s| s.as_str()).collect();

        match parts.as_slice() {
            ["Values", rest @ ..] => format!("values.{}", rest.join(".")),
            ["Release", "Name"] => "release.name".to_string(),
            ["Release", "Namespace"] => "release.namespace".to_string(),
            ["Chart", rest @ ..] => format!("pack.{}", to_snake_case(&rest.join("."))),
            ["Capabilities", rest @ ..] => format!("capabilities.{}", to_snake_case(&rest.join("."))),
            _ => parts.join(".").to_lowercase(),
        }
    }

    fn transform_filter(&self, cmd: &Command) -> String {
        match cmd {
            Command::FunctionCall { name, args } => {
                let filter_name = FILTER_MAP.get(name.as_str()).unwrap_or(name);
                if args.is_empty() {
                    filter_name.to_string()
                } else {
                    let args_str: Vec<String> = args.iter()
                        .map(|a| self.transform_arg(a))
                        .collect();
                    format!("{}({})", filter_name, args_str.join(", "))
                }
            }
            _ => self.transform_command(cmd),
        }
    }
}

/// Filter name mapping
static FILTER_MAP: phf::Map<&'static str, &'static str> = phf::phf_map! {
    "toYaml" => "toyaml",
    "toJson" => "tojson",
    "b64enc" => "b64encode",
    "b64dec" => "b64decode",
    "indent" => "indent",
    "nindent" => "nindent",
    "quote" => "quote",
    "squote" => "squote",
    "upper" => "upper",
    "lower" => "lower",
    "title" => "title",
    "trim" => "trim",
    "trimPrefix" => "trimprefix",
    "trimSuffix" => "trimsuffix",
    "replace" => "replace",
    "contains" => "contains",
    "hasPrefix" => "startswith",
    "hasSuffix" => "endswith",
    "repeat" => "repeat",
    "substr" => "slice",
    "trunc" => "trunc",
    "list" => "list",
    "dict" => "dict",
    "default" => "default",
    "empty" => "empty",
    "coalesce" => "coalesce",
    "ternary" => "ternary",
    "first" => "first",
    "last" => "last",
    "uniq" => "unique",
    "sortAlpha" => "sort",
    "reverse" => "reverse",
    "sha256sum" => "sha256",
    "required" => "required",
    "fail" => "fail",
};
```

---

### 2.4 _helpers.tpl → _macros.j2 Converter

**Input (Helm _helpers.tpl):**
```go
{{/*
Create a default fully qualified app name.
*/}}
{{- define "myapp.fullname" -}}
{{- if .Values.fullnameOverride }}
{{- .Values.fullnameOverride | trunc 63 | trimSuffix "-" }}
{{- else }}
{{- $name := default .Chart.Name .Values.nameOverride }}
{{- printf "%s-%s" .Release.Name $name | trunc 63 | trimSuffix "-" }}
{{- end }}
{{- end }}

{{- define "myapp.labels" -}}
helm.sh/chart: {{ include "myapp.chart" . }}
app.kubernetes.io/name: {{ include "myapp.name" . }}
app.kubernetes.io/instance: {{ .Release.Name }}
{{- end }}
```

**Output (Sherpack _macros.j2):**
```jinja
{#
Create a default fully qualified app name.
#}
{%- macro fullname() -%}
{%- if values.fullnameOverride %}
{{- values.fullnameOverride | trunc(63) | trimsuffix("-") }}
{%- else %}
{%- set name = values.nameOverride | default(pack.name) %}
{{- printf("%s-%s", release.name, name) | trunc(63) | trimsuffix("-") }}
{%- endif %}
{%- endmacro %}

{%- macro labels() -%}
helm.sh/chart: {{ chart() }}
app.kubernetes.io/name: {{ name() }}
app.kubernetes.io/instance: {{ release.name }}
{%- endmacro %}
```

**Key Transformations:**
1. `{{/* comment */}}` → `{# comment #}`
2. `{{- define "myapp.fullname" -}}` → `{%- macro fullname() -%}`
3. `{{- end }}` after define → `{%- endmacro %}`
4. `{{ include "myapp.name" . }}` → `{{ name() }}`
5. Strip chart name prefix from macro names (`myapp.` → empty)

---

### 2.5 Non-Convertible Elements (Warnings)

| Helm Feature | Reason | Recommendation |
|--------------|--------|----------------|
| `lookup` function | Runtime K8s API | Use hooks or sync-waves |
| `tpl` function | ✅ Now supported | Direct mapping |
| `$.Files.Get` | File access | Add to values instead |
| `$.Files.Glob` | File access | Add to values instead |
| Complex Sprig functions | No direct equivalent | Manual rewrite |
| `kindIs`, `typeIs` | Type checking | Use conditionals |
| `deepCopy`, `mustDeepCopy` | Memory ops | Usually not needed |

**Implementation:**
```rust
fn check_unsupported(&self, ast: &Template) -> Vec<Warning> {
    let mut warnings = Vec::new();

    for func in ast.find_function_calls() {
        if UNSUPPORTED_FUNCTIONS.contains(&func.name.as_str()) {
            warnings.push(Warning {
                location: func.location.clone(),
                message: format!(
                    "Function '{}' is not supported. {}",
                    func.name,
                    UNSUPPORTED_RECOMMENDATIONS.get(&func.name).unwrap_or(&"")
                ),
                severity: WarningSeverity::Warning,
            });
        }
    }

    warnings
}
```

---

## 3. CLI Interface

```bash
# Basic usage
sherpack convert ./my-helm-chart

# With output directory
sherpack convert ./my-helm-chart --output ./my-sherpack-pack

# Dry run (show what would be converted)
sherpack convert ./my-helm-chart --dry-run

# Force overwrite existing
sherpack convert ./my-helm-chart --output ./existing-pack --force

# Show detailed warnings
sherpack convert ./my-helm-chart --verbose

# Convert only specific files
sherpack convert ./my-helm-chart --only templates/deployment.yaml
```

**CLI Arguments:**
```rust
#[derive(Parser)]
pub struct ConvertArgs {
    /// Path to Helm chart
    #[arg()]
    chart_path: PathBuf,

    /// Output directory (default: <chart-name>-sherpack)
    #[arg(short, long)]
    output: Option<PathBuf>,

    /// Overwrite existing output
    #[arg(short, long)]
    force: bool,

    /// Show what would be converted without writing
    #[arg(long)]
    dry_run: bool,

    /// Show detailed conversion warnings
    #[arg(short, long)]
    verbose: bool,

    /// Convert only specific files
    #[arg(long)]
    only: Option<Vec<PathBuf>>,
}
```

---

## 4. Conversion Report

```
$ sherpack convert ./my-helm-chart --output ./my-pack

Converting Helm chart: my-helm-chart → my-pack

Files:
  ✓ Chart.yaml → Pack.yaml
  ✓ values.yaml (copied)
  ✓ templates/_helpers.tpl → templates/_macros.j2 (12 macros)
  ✓ templates/deployment.yaml → templates/deployment.yaml
  ✓ templates/service.yaml → templates/service.yaml
  ⚠ templates/tests/test-connection.yaml (contains lookup, skipped)

Warnings:
  templates/deployment.yaml:45 - 'lookup' function not supported
    Recommendation: Use sync-waves or hooks for resource dependencies

  templates/_helpers.tpl:23 - Complex Sprig function 'regexMatch'
    Recommendation: Use Jinja2 'match' filter or simplify logic

Summary:
  ✓ 5 files converted
  ⚠ 1 file skipped
  ⚠ 2 warnings

Run 'sherpack lint ./my-pack' to validate the converted pack.
```

---

## 5. Implementation Plan

### Phase 1: Foundation (~3 days)
1. Create `sherpack-convert` crate
2. Implement Chart.yaml → Pack.yaml converter
3. Add basic CLI command

### Phase 2: Parser (~5 days)
1. Write pest grammar for Go templates
2. Build AST data structures
3. Implement parser tests with real Helm templates

### Phase 3: Transformer (~5 days)
1. Implement AST → Jinja2 transformer
2. Handle all control flow constructs
3. Implement filter/function mapping

### Phase 4: Helpers (~2 days)
1. Handle `define` → `macro` conversion
2. Handle `include` → macro call conversion
3. Strip chart name prefixes

### Phase 5: Polish (~3 days)
1. Implement warnings for unsupported features
2. Generate detailed conversion report
3. Add comprehensive tests

**Total: ~18 days (or ~3 weeks)**

---

## 6. Test Strategy

### Unit Tests
- Parser: One test per grammar rule
- Transformer: One test per transformation rule
- Warnings: Test detection of unsupported features

### Integration Tests
- Convert real Helm charts (nginx, postgresql, etc.)
- Verify converted pack renders correctly
- Compare output with expected fixtures

### Fixture Charts
```
fixtures/
├── helm-charts/
│   ├── simple/           # Basic chart
│   ├── with-helpers/     # Uses _helpers.tpl
│   ├── with-subcharts/   # Has dependencies
│   ├── with-hooks/       # Uses Helm hooks
│   └── bitnami-nginx/    # Real-world chart
└── expected-packs/
    ├── simple/
    ├── with-helpers/
    └── ...
```

---

## 7. Dependencies

```toml
# crates/sherpack-convert/Cargo.toml
[dependencies]
sherpack-core = { path = "../sherpack-core" }
pest = "2.7"
pest_derive = "2.7"
phf = { version = "0.11", features = ["macros"] }
walkdir = "2.4"
miette = "7.0"  # For nice error reporting
```

---

## Sources

- [gtmpl-rust: Go templates for Rust](https://github.com/fiji-flo/gtmpl-rust)
- [Pest parser generator](https://pest.rs/)
- [Go text/template parse package](https://pkg.go.dev/text/template/parse)
- [Helm Template Functions](https://helm.sh/docs/chart_template_guide/function_list/)
