# sherpack-engine

Jinja2 templating engine for Sherpack with Kubernetes-specific filters and functions.

## Overview

`sherpack-engine` provides a MiniJinja-based template engine optimized for Kubernetes manifest generation. It includes Helm-compatible filters, contextual error messages with suggestions, and multi-error collection for comprehensive feedback.

## Features

- **Full Jinja2 Syntax** - Powered by MiniJinja with complete Jinja2 support
- **25+ Kubernetes Filters** - `toyaml`, `b64encode`, `indent`, `quote`, and more
- **Template Functions** - `dict()`, `list()`, `get()`, `now()`, `uuidv4()`
- **Smart Error Messages** - Fuzzy matching suggestions for typos
- **Multi-Error Collection** - Continue rendering to find all errors at once
- **Macro Support** - Define reusable template components

## Quick Start

```rust
use sherpack_engine::{Engine, EngineBuilder};
use sherpack_core::{TemplateContext, Values, Release, Pack};

// Build engine with templates
let engine = EngineBuilder::new()
    .add_template("deployment.yaml", r#"
apiVersion: apps/v1
kind: Deployment
metadata:
  name: {{ release.name }}
  labels:
    app: {{ values.name | default("myapp") }}
spec:
  replicas: {{ values.replicas | default(1) }}
  template:
    spec:
      containers:
        - name: {{ values.name }}
          image: {{ values.image.repository }}:{{ values.image.tag }}
          env:
            {{- values.env | toyaml | indent(12) }}
"#)?
    .build()?;

// Create context
let context = TemplateContext::new(&values, &release, &pack, "1.28.0");

// Render
let result = engine.render("deployment.yaml", &context)?;
println!("{}", result.content);
```

## Filters

### Serialization Filters

| Filter | Description | Example |
|--------|-------------|---------|
| `toyaml` | Convert to YAML | `{{ config \| toyaml }}` |
| `tojson` | Convert to JSON | `{{ data \| tojson }}` |
| `tojson_pretty` | Pretty JSON | `{{ data \| tojson_pretty }}` |

### Encoding Filters

| Filter | Description | Example |
|--------|-------------|---------|
| `b64encode` | Base64 encode | `{{ secret \| b64encode }}` |
| `b64decode` | Base64 decode | `{{ encoded \| b64decode }}` |
| `sha256` | SHA256 hash | `{{ content \| sha256 }}` |

### String Filters

| Filter | Description | Example |
|--------|-------------|---------|
| `quote` | Double-quote string | `{{ name \| quote }}` → `"name"` |
| `squote` | Single-quote string | `{{ name \| squote }}` → `'name'` |
| `indent(n)` | Indent each line | `{{ yaml \| indent(4) }}` |
| `nindent(n)` | Newline + indent | `{{ yaml \| nindent(4) }}` |
| `trim` | Remove whitespace | `{{ text \| trim }}` |
| `trimPrefix(p)` | Remove prefix | `{{ s \| trimPrefix("v") }}` |
| `trimSuffix(s)` | Remove suffix | `{{ s \| trimSuffix(".txt") }}` |

### Case Conversion

| Filter | Description | Example |
|--------|-------------|---------|
| `upper` | UPPERCASE | `{{ name \| upper }}` |
| `lower` | lowercase | `{{ name \| lower }}` |
| `title` | Title Case | `{{ name \| title }}` |
| `camelcase` | camelCase | `{{ name \| camelcase }}` |
| `kebabcase` | kebab-case | `{{ name \| kebabcase }}` |
| `snakecase` | snake_case | `{{ name \| snakecase }}` |

### Type Conversion

| Filter | Description | Example |
|--------|-------------|---------|
| `int` | Convert to integer | `{{ "42" \| int }}` |
| `float` | Convert to float | `{{ "3.14" \| float }}` |
| `tostring` | Convert to string | `{{ 42 \| tostring }}` |
| `default(val)` | Default value | `{{ x \| default("none") }}` |
| `required(msg)` | Require value | `{{ x \| required("x is required") }}` |

### List/Dict Filters

| Filter | Description | Example |
|--------|-------------|---------|
| `keys` | Get object keys | `{{ config \| keys }}` |
| `values` | Get object values | `{{ config \| values }}` |
| `first` | First element | `{{ list \| first }}` |
| `last` | Last element | `{{ list \| last }}` |
| `join(sep)` | Join list | `{{ items \| join(", ") }}` |
| `sortAlpha` | Sort alphabetically | `{{ items \| sortAlpha }}` |
| `uniq` | Remove duplicates | `{{ items \| uniq }}` |
| `compact` | Remove empty/null | `{{ items \| compact }}` |
| `has(key)` | Check key exists | `{{ config \| has("debug") }}` |

### Semantic Versioning

| Filter | Description | Example |
|--------|-------------|---------|
| `semver` | Parse version | `{{ "1.2.3" \| semver }}` |
| `semverCompare(c, v)` | Compare versions | `{{ semverCompare(">=1.0.0", version) }}` |

## Functions

### Data Construction

```jinja2
{# Create a dict #}
{% set labels = dict("app", name, "version", version) %}

{# Create a list #}
{% set ports = list(80, 443, 8080) %}

{# Get with default #}
{{ get(values, "image.tag", "latest") }}
```

### Conditionals

```jinja2
{# Ternary operator #}
{{ ternary("enabled", "disabled", feature_flag) }}

{# Coalesce (first non-null) - use native Jinja2 #}
{{ value1 or value2 or "default" }}
```

### Utilities

```jinja2
{# Current timestamp #}
{{ now() }}
{{ now("%Y-%m-%d") }}

{# Generate UUID #}
{{ uuidv4() }}

{# Fail with message #}
{% if not values.required_field %}
  {{ fail("required_field must be set") }}
{% endif %}
```

### Template Inclusion

```jinja2
{# Include another template #}
{% include "partials/_helpers.tpl" %}

{# Define reusable macro #}
{% macro labels(name, version) %}
app.kubernetes.io/name: {{ name }}
app.kubernetes.io/version: {{ version }}
{% endmacro %}

{# Use macro #}
metadata:
  labels:
    {{ labels(name, version) | indent(4) }}
```

### Dynamic Templating (tpl)

```jinja2
{# Render a string as template #}
{% set template_string = "Hello {{ name }}" %}
{{ tpl(template_string) }}

{# Useful for values that contain templates #}
annotations:
  {{ tpl(values.customAnnotations) | indent(2) }}
```

## Error Handling

### Contextual Suggestions

The engine provides helpful suggestions when errors occur:

```
Error: undefined variable `valeus`
  --> templates/deployment.yaml:15:12
   |
15 |   image: {{ valeus.image.tag }}
   |            ^^^^^^ undefined
   |
   = help: Did you mean `values`?
   = note: Available variables: values, release, pack, capabilities
```

### Multi-Error Collection

Collect all errors in a single pass:

```rust
use sherpack_engine::{Engine, RenderReport};

let result = engine.render_with_report("template.yaml", &context);

match result {
    Ok(report) if report.has_errors() => {
        println!("Rendered with {} errors:", report.errors.len());
        for error in &report.errors {
            println!("  - {} at line {}", error.message, error.line);
        }
        // Still get partial output
        println!("Partial output:\n{}", report.content);
    }
    Ok(report) => println!("{}", report.content),
    Err(e) => println!("Fatal error: {}", e),
}
```

### Available Suggestions

When a filter or function is not found, the engine suggests alternatives:

```rust
use sherpack_engine::{AVAILABLE_FILTERS, AVAILABLE_FUNCTIONS};

// List all available filters
for filter in AVAILABLE_FILTERS {
    println!("Filter: {}", filter);
}

// List all available functions
for func in AVAILABLE_FUNCTIONS {
    println!("Function: {}", func);
}
```

## Engine Configuration

### Builder Pattern

```rust
use sherpack_engine::EngineBuilder;

let engine = EngineBuilder::new()
    // Add templates from strings
    .add_template("main.yaml", template_content)?

    // Load templates from directory
    .add_templates_from_dir("./templates/")?

    // Configure error collection
    .collect_errors(true)

    // Build the engine
    .build()?;
```

### Render Options

```rust
// Simple render (stops on first error)
let output = engine.render("template.yaml", &context)?;

// Render with error collection
let report = engine.render_with_report("template.yaml", &context)?;

// Render all templates
let results = engine.render_all(&context)?;
```

## Template Syntax Reference

### Variables

```jinja2
{{ values.name }}
{{ values.image.tag | default("latest") }}
{{ release.name }}-{{ release.namespace }}
```

### Control Flow

```jinja2
{% if values.debug %}
  debug: true
{% elif values.verbose %}
  verbose: true
{% else %}
  production: true
{% endif %}

{% for item in values.items %}
- name: {{ item.name }}
  value: {{ item.value }}
{% endfor %}

{% for key, value in values.labels %}
  {{ key }}: {{ value }}
{% endfor %}
```

### Whitespace Control

```jinja2
{# Remove whitespace before #}
{%- if condition %}

{# Remove whitespace after #}
{% if condition -%}

{# Remove both #}
{%- if condition -%}
```

### Comments

```jinja2
{# This is a comment #}

{#
  Multi-line
  comment
#}
```

### Raw Blocks

```jinja2
{% raw %}
  This {{ will not }} be interpreted
{% endraw %}
```

## Integration with sherpack-core

```rust
use sherpack_core::{LoadedPack, Values, Release, TemplateContext};
use sherpack_engine::EngineBuilder;

// Load pack
let pack = LoadedPack::load("./my-pack")?;

// Build engine from pack templates
let mut builder = EngineBuilder::new();
for template in pack.list_templates()? {
    let content = std::fs::read_to_string(&template)?;
    let name = template.file_name().unwrap().to_str().unwrap();
    builder = builder.add_template(name, &content)?;
}
let engine = builder.build()?;

// Load values with overrides
let mut values = Values::from_file(&pack.values_path)?;
values.merge(&overrides);

// Create context and render
let release = Release::new("my-release", "default");
let context = TemplateContext::new(&values, &release, &pack.pack, "1.28.0");

for template_name in engine.template_names() {
    let output = engine.render(template_name, &context)?;
    println!("--- {} ---\n{}", template_name, output.content);
}
```

## Dependencies

- `minijinja` - Jinja2 template engine
- `sherpack-core` - Core types
- `serde_yaml` / `serde_json` - Serialization
- `base64` - Encoding
- `sha2` - Hashing
- `chrono` - Date/time
- `strsim` - Fuzzy matching for suggestions
- `miette` - Error reporting

## License

MIT OR Apache-2.0
