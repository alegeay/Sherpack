---
id: filters
title: Filters
sidebar_position: 2
---

# Filters

Filters transform values in templates. Use them with the pipe `|` operator.

## Serialization

### toyaml

Convert object to YAML string:

```yaml
config: |
  {{ values.config | toyaml | indent(2) }}
```

### tojson

Convert object to compact JSON:

```yaml
annotations:
  config: {{ values.config | tojson | quote }}
```

### tojson_pretty

Convert object to formatted JSON:

```yaml
data:
  config.json: |
    {{ values.config | tojson_pretty | indent(4) }}
```

## Encoding

### b64encode

Base64 encode a string:

```yaml
apiVersion: v1
kind: Secret
data:
  password: {{ values.password | b64encode }}
```

### b64decode

Base64 decode a string:

```yaml
decoded: {{ values.encoded | b64decode }}
```

### sha256

SHA256 hash of a string:

```yaml
annotations:
  checksum/config: {{ values.config | tojson | sha256 }}
```

## String Manipulation

### quote / squote

Wrap in quotes:

```yaml
name: {{ values.name | quote }}      # "myapp"
name: {{ values.name | squote }}     # 'myapp'
```

### upper / lower

Change case:

```yaml
env: {{ values.env | upper }}        # PRODUCTION
name: {{ values.name | lower }}      # myapp
```

### title

Title case:

```yaml
title: {{ values.name | title }}     # My App
```

### snakecase / kebabcase / camelcase

Case conversion:

```yaml
snake: {{ "myAppName" | snakecase }}  # my_app_name
kebab: {{ "myAppName" | kebabcase }}  # my-app-name
camel: {{ "my_app_name" | camelcase }} # myAppName
```

### trunc

Truncate to n characters:

```yaml
short: {{ values.hash | trunc(8) }}
```

### trimprefix / trimsuffix

Remove prefix/suffix:

```yaml
path: {{ "/api/v1" | trimprefix("/") }}     # api/v1
name: {{ "app.yaml" | trimsuffix(".yaml") }} # app
```

### replace

Replace substring:

```yaml
safe: {{ values.name | replace("_", "-") }}
```

### trim

Remove leading/trailing whitespace:

```yaml
clean: {{ values.input | trim }}
```

## Indentation

### indent

Add spaces to each line:

```yaml
data: |
{{ values.config | toyaml | indent(2) }}
```

### nindent

Newline + indent (most common for nested YAML):

```yaml
spec:
  containers:
    - name: app
      resources:
        {{ values.resources | toyaml | nindent(8) }}
```

## Collections

### keys

Get object keys as array:

```yaml
{% for key in values.env | keys %}
- {{ key }}
{% endfor %}
```

### haskey

Check if key exists:

```yaml
{% if values.config | haskey("tls") %}
tls:
  enabled: true
{% endif %}
```

### merge

Merge two objects (right overrides left):

```yaml
{% set merged = defaults | merge(overrides) %}
```

### dictsort

Sort object for iteration:

```yaml
{% for key, value in values.labels | dictsort %}
{{ key }}: {{ value | quote }}
{% endfor %}
```

### first / last

Get first/last element:

```yaml
primary: {{ values.hosts | first }}
final: {{ values.hosts | last }}
```

### default

Default value if undefined:

```yaml
replicas: {{ values.replicas | default(1) }}
tag: {{ values.image.tag | default("latest") }}
```

### join

Join array with separator:

```yaml
hosts: {{ values.hosts | join(",") }}
```

### length

Get length:

```yaml
count: {{ values.items | length }}
```

## Validation

### required

Fail if undefined or empty:

```yaml
name: {{ values.name | required }}
```

With custom message:

```yaml
name: {{ values.name | required("name is required") }}
```

### empty

Check if empty:

```yaml
{% if values.items | empty %}
# No items configured
{% endif %}
```

## Type Conversion

### int

Convert to integer:

```yaml
port: {{ values.port | int }}
replicas: {{ "5" | int }}
```

Works with strings, floats, and booleans:

```yaml
{{ "42" | int }}       # 42
{{ 3.14 | int }}       # 3
{{ true | int }}       # 1
{{ false | int }}      # 0
```

### float

Convert to float:

```yaml
ratio: {{ values.ratio | float }}
threshold: {{ "0.95" | float }}
```

### string

Convert to string:

```yaml
port: {{ values.port | string }}
env:
  COUNT: {{ values.count | string | quote }}
```

## Chaining Filters

Filters can be chained:

```yaml
# Convert to YAML, indent, and add newline prefix
resources:
  {{ values.resources | toyaml | nindent(2) }}

# Hash config for change detection
checksum: {{ values.config | tojson | sha256 | trunc(16) }}

# Safe default with transformation
name: {{ values.name | default("app") | kebabcase | trunc(63) }}
```
