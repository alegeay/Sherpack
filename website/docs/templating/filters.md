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

### sha1

SHA-1 hash (for compatibility, prefer sha256):

```yaml
checksum: {{ values.data | sha1 }}
```

### sha512

SHA-512 hash:

```yaml
checksum: {{ values.data | sha512 }}
```

### md5

MD5 hash (for checksums only, not cryptographically secure):

```yaml
checksum: {{ values.data | md5 }}
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

### snakecase / kebabcase / camelcase / pascalcase

Case conversion:

```yaml
snake: {{ "myAppName" | snakecase }}   # my_app_name
kebab: {{ "myAppName" | kebabcase }}   # my-app-name
camel: {{ "my_app_name" | camelcase }} # myAppName
pascal: {{ "foo_bar" | pascalcase }}   # FooBar
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

### repeat

Repeat a string N times:

```yaml
separator: {{ "-" | repeat(40) }}
```

### substr

Extract substring:

```yaml
prefix: {{ values.name | substr(0, 5) }}   # first 5 chars
suffix: {{ values.name | substr(5) }}      # from position 5 to end
```

### wrap

Word wrap at specified width:

```yaml
wrapped: {{ values.description | wrap(80) }}
```

### hasprefix / hassuffix

Check string prefix/suffix:

```yaml
{% if values.name | hasprefix("v") %}
  version: {{ values.name | trimprefix("v") }}
{% endif %}

{% if values.file | hassuffix(".yaml") %}
  # YAML file
{% endif %}
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

## Path Functions

### basename

Extract filename from path:

```yaml
file: {{ "/etc/nginx/nginx.conf" | basename }}  # nginx.conf
```

### dirname

Extract directory from path:

```yaml
dir: {{ "/etc/nginx/nginx.conf" | dirname }}  # /etc/nginx
```

### extname

Extract file extension (without dot):

```yaml
ext: {{ "archive.tar.gz" | extname }}  # gz
```

### cleanpath

Normalize path (resolve `.` and `..`):

```yaml
path: {{ "a/b/../c/./d" | cleanpath }}  # a/c/d
```

## Regex Functions

### regex_match

Check if string matches pattern:

```yaml
{% if values.version | regex_match("^v[0-9]+") %}
  # Starts with v followed by number
{% endif %}
```

### regex_replace

Replace matches with replacement (supports capture groups `$1`, `$2`, etc.):

```yaml
normalized: {{ values.name | regex_replace("[^a-z0-9]", "-") }}
version: {{ "v1.2.3" | regex_replace("v([0-9]+)", "version-$1") }}
```

### regex_find

Find first match:

```yaml
port: {{ "server:8080" | regex_find("[0-9]+") }}  # 8080
```

### regex_find_all

Find all matches:

```yaml
{% for num in "a1b2c3" | regex_find_all("[0-9]+") %}
- {{ num }}
{% endfor %}
# Results: 1, 2, 3
```

## Advanced Collections

### values

Get all values from a dict as a list:

```yaml
{% for v in values.config | values %}
- {{ v }}
{% endfor %}
```

### pick

Select only specified keys from dict:

```yaml
{% set subset = values.config | pick("name", "version") %}
```

### omit

Exclude specified keys from dict:

```yaml
{% set safe = values.config | omit("password", "secret") %}
```

### tostrings

Convert list elements to strings:

```yaml
ports: {{ values.ports | tostrings | join(",") }}
```

With prefix/suffix:

```yaml
{{ values.ports | tostrings(prefix="port-") }}           # ["port-80", "port-443"]
{{ values.ports | tostrings(suffix="/TCP") }}            # ["80/TCP", "443/TCP"]
{{ values.items | tostrings(skip_empty=true) }}          # Skip empty values
```

## List Operations

### append

Append item to end of list:

```yaml
{% set hosts = values.hosts | append("localhost") %}
```

### prepend

Prepend item to start of list:

```yaml
{% set hosts = values.hosts | prepend("primary.example.com") %}
```

### concat

Concatenate two lists:

```yaml
{% set all = values.hosts | concat(values.extra_hosts) %}
```

### without

Remove specified values from list:

```yaml
{% set filtered = values.items | without("deprecated", "old") %}
```

### compact

Remove empty/falsy values from list:

```yaml
{% set cleaned = ["a", "", null, "b"] | compact %}  # ["a", "b"]
```

## Math Functions

### abs

Absolute value:

```yaml
diff: {{ values.delta | abs }}
```

### floor / ceil

Round down/up:

```yaml
rounded_down: {{ 3.7 | floor }}  # 3
rounded_up: {{ 3.2 | ceil }}     # 4
```

## Version Comparison

### semver_match

Compare version against semver constraint:

```yaml
{% if capabilities.kubeVersion | semver_match(">=1.21.0") %}
  # Kubernetes 1.21 or later
{% endif %}

{% if values.version | semver_match("^2.0.0") %}
  # Compatible with 2.x
{% endif %}
```

Supports operators: `>=`, `<=`, `>`, `<`, `^` (compatible), `~` (approximately).

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

# Regex + case transformation
safe_name: {{ values.name | regex_replace("[^a-zA-Z0-9]", "_") | snakecase }}
```
