---
id: control-structures
title: Control Structures
sidebar_position: 4
---

# Control Structures

Jinja2 provides powerful control structures for conditional logic and iteration.

## Conditionals

### if / elif / else

```yaml
{% if values.ingress.enabled %}
apiVersion: networking.k8s.io/v1
kind: Ingress
metadata:
  name: {{ release.name }}
{% endif %}
```

With else:

```yaml
{% if values.service.type == "LoadBalancer" %}
type: LoadBalancer
{% elif values.service.type == "NodePort" %}
type: NodePort
{% else %}
type: ClusterIP
{% endif %}
```

### Inline Conditionals

```yaml
replicas: {{ 3 if values.highAvailability else 1 }}
```

### Truthiness

These are considered **false**:
- `false`
- `0`
- `""` (empty string)
- `[]` (empty list)
- `{}` (empty object)
- `null` / `none`

```yaml
{% if values.env %}
# env is defined and not empty
{% endif %}

{% if not values.disabled %}
# disabled is false or not set
{% endif %}
```

## Loops

### for

Iterate over lists:

```yaml
{% for port in values.ports %}
- name: {{ port.name }}
  port: {{ port.port }}
  targetPort: {{ port.targetPort }}
{% endfor %}
```

Iterate over objects (use `dictsort` for deterministic order):

```yaml
{% for key, value in values.env | dictsort %}
- name: {{ key }}
  value: {{ value | quote }}
{% endfor %}
```

### Loop Variables

| Variable | Description |
|----------|-------------|
| `loop.index` | Current iteration (1-based) |
| `loop.index0` | Current iteration (0-based) |
| `loop.first` | True if first iteration |
| `loop.last` | True if last iteration |
| `loop.length` | Total number of items |

```yaml
{% for item in values.items %}
{{ loop.index }}. {{ item }}{% if not loop.last %},{% endif %}
{% endfor %}
```

### Filtering in Loops

```yaml
{% for host in values.hosts if host.enabled %}
- host: {{ host.name }}
{% endfor %}
```

### Empty Loop Handling

```yaml
{% for item in values.items %}
- {{ item }}
{% else %}
# No items configured
{% endfor %}
```

## Variables

### set

Create or modify variables:

```yaml
{% set fullName = release.name ~ "-" ~ pack.name %}
{% set replicas = values.replicas | default(1) %}

metadata:
  name: {{ fullName }}
spec:
  replicas: {{ replicas }}
```

### Block Assignment

```yaml
{% set labels %}
app.kubernetes.io/name: {{ release.name }}
app.kubernetes.io/version: {{ pack.version }}
{% endset %}

metadata:
  labels:
    {{ labels | indent(4) }}
```

## Whitespace Control

### Trim Whitespace

Use `-` to trim whitespace:

```yaml
{%- if condition -%}
trimmed
{%- endif -%}
```

| Syntax | Effect |
|--------|--------|
| `{%-` | Trim before |
| `-%}` | Trim after |
| `{{-` | Trim before |
| `-}}` | Trim after |

### Common Pattern

```yaml
metadata:
  labels:
    {%- for key, value in values.labels | dictsort %}
    {{ key }}: {{ value | quote }}
    {%- endfor %}
```

## Macros

### Define Reusable Blocks

```yaml title="templates/_helpers.tpl"
{% macro labels(name, version) %}
app.kubernetes.io/name: {{ name }}
app.kubernetes.io/version: {{ version }}
app.kubernetes.io/managed-by: sherpack
{% endmacro %}

{% macro selectorLabels(name) %}
app.kubernetes.io/name: {{ name }}
{% endmacro %}
```

### Use Macros

```yaml
{% from "_helpers.tpl" import labels, selectorLabels %}

metadata:
  labels:
    {{ labels(release.name, pack.version) | indent(4) }}
spec:
  selector:
    matchLabels:
      {{ selectorLabels(release.name) | indent(6) }}
```

## Include

### Include Other Templates

```yaml
{% include "configmap.yaml" %}
```

### With Context

```yaml
{% include "partial.yaml" with context %}
```

## Complete Example

```yaml title="templates/deployment.yaml"
{# Set computed values #}
{% set fullName = release.name ~ "-" ~ pack.name %}
{% set labels = {
  "app.kubernetes.io/name": pack.name,
  "app.kubernetes.io/instance": release.name,
  "app.kubernetes.io/version": pack.version
} %}

apiVersion: apps/v1
kind: Deployment
metadata:
  name: {{ fullName | trunc(63) }}
  labels:
    {%- for key, value in labels | dictsort %}
    {{ key }}: {{ value | quote }}
    {%- endfor %}
spec:
  replicas: {{ ternary(3, 1, values.highAvailability) }}
  selector:
    matchLabels:
      app.kubernetes.io/name: {{ pack.name }}
      app.kubernetes.io/instance: {{ release.name }}
  template:
    metadata:
      labels:
        {%- for key, value in labels | dictsort %}
        {{ key }}: {{ value | quote }}
        {%- endfor %}
    spec:
      containers:
        - name: {{ values.app.name | kebabcase }}
          image: {{ values.image.repository }}:{{ values.image.tag }}
          {%- if values.ports %}
          ports:
            {%- for port in values.ports %}
            - name: {{ port.name }}
              containerPort: {{ port.containerPort }}
            {%- endfor %}
          {%- endif %}
          {%- if values.env %}
          env:
            {%- for key, value in values.env | dictsort %}
            - name: {{ key }}
              value: {{ value | quote }}
            {%- endfor %}
          {%- endif %}
          resources:
            {{ values.resources | toyaml | nindent(12) }}
```
