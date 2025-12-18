---
id: templating
title: Templating Overview
sidebar_position: 3
---

# Templating Overview

Sherpack uses **Jinja2** templating via the MiniJinja engine. If you're familiar with Python's Jinja2 or Ansible templates, you'll feel right at home.

## Basic Syntax

### Expressions

Output values with double curly braces:

```yaml
name: {{ values.app.name }}
replicas: {{ values.app.replicas }}
```

### Statements

Control flow with `{% %}`:

```yaml
{% if values.ingress.enabled %}
apiVersion: networking.k8s.io/v1
kind: Ingress
{% endif %}
```

### Comments

```yaml
{# This is a comment and won't appear in output #}
```

## Comparison with Helm/Go Templates

| Feature | Sherpack (Jinja2) | Helm (Go) |
|---------|-------------------|-----------|
| Output | `{{ value }}` | `{{ .Values.value }}` |
| If | `{% if cond %}` | `{{- if .cond }}` |
| For | `{% for x in list %}` | `{{- range .list }}` |
| Set | `{% set x = 1 %}` | `{{- $x := 1 }}` |
| Filter | `{{ x \| upper }}` | `{{ upper .x }}` |
| Comment | `{# comment #}` | `{{/* comment */}}` |

## Example Template

```yaml title="templates/deployment.yaml"
apiVersion: apps/v1
kind: Deployment
metadata:
  name: {{ release.name }}
  namespace: {{ release.namespace }}
  labels:
    app.kubernetes.io/name: {{ release.name }}
    app.kubernetes.io/version: {{ pack.version }}
    {# Add custom labels #}
    {% for key, value in values.labels | dictsort %}
    {{ key }}: {{ value | quote }}
    {% endfor %}
spec:
  replicas: {{ values.replicas | default(1) }}
  selector:
    matchLabels:
      app.kubernetes.io/name: {{ release.name }}
  template:
    metadata:
      labels:
        app.kubernetes.io/name: {{ release.name }}
      annotations:
        {# Trigger rollout on config change #}
        checksum/config: {{ values.config | tojson | sha256 | trunc(16) }}
    spec:
      containers:
        - name: {{ values.app.name | kebabcase }}
          image: {{ values.image.repository }}:{{ values.image.tag }}
          {% if values.env %}
          env:
            {% for key, value in values.env | dictsort %}
            - name: {{ key }}
              value: {{ value | quote }}
            {% endfor %}
          {% endif %}
          resources:
            {{ values.resources | toyaml | nindent(12) }}
```

## Whitespace Control

Control whitespace with `-`:

```yaml
{#- Trims whitespace before -#}
{%- if condition -%}   {# Trims both sides #}
trimmed content
{%- endif -%}
```

## String Concatenation

Use `~` to concatenate strings:

```yaml
fullname: {{ release.name ~ "-" ~ pack.name ~ "-" ~ pack.version }}
```

## Next Steps

- [Context Variables](/docs/templating/context-variables) - What's available in templates
- [Filters](/docs/templating/filters) - Transform values
- [Functions](/docs/templating/functions) - Built-in functions
- [Control Structures](/docs/templating/control-structures) - If, for, set
