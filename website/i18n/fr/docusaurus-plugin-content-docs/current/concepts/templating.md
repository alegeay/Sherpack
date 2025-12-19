---
id: templating
title: Templating Jinja2
sidebar_position: 3
---

# Templating Jinja2

Sherpack utilise [MiniJinja](https://github.com/mitsuhiko/minijinja), une implémentation Rust de Jinja2.

## Syntaxe de base

### Expressions

```yaml
# Variables
name: {{ values.name }}

# Accès aux propriétés
image: {{ values.image.repository }}:{{ values.image.tag }}

# Valeurs par défaut
port: {{ values.port | default(8080) }}
```

### Blocs de contrôle

```yaml
{% if values.enabled %}
enabled: true
{% endif %}

{% for item in values.items %}
- {{ item }}
{% endfor %}
```

### Commentaires

```yaml
{# Ceci est un commentaire #}
```

## Variables disponibles

### values

Les valeurs fusionnées depuis values.yaml et les overrides :

```yaml
replicas: {{ values.replicaCount }}
image: {{ values.image.repository }}
```

### release

Informations sur le release :

```yaml
name: {{ release.name }}              # Nom du release
namespace: {{ release.namespace }}    # Namespace cible
```

### pack

Métadonnées du pack :

```yaml
app: {{ pack.name }}                  # Nom du pack
version: {{ pack.version }}           # Version
description: {{ pack.description }}   # Description
```

### capabilities

Informations sur le cluster :

```yaml
kubeVersion: {{ capabilities.kubeVersion }}
```

## Structures de contrôle

### Conditions

```yaml
{% if values.ingress.enabled %}
apiVersion: networking.k8s.io/v1
kind: Ingress
...
{% endif %}

{% if values.env == "production" %}
replicas: 3
{% elif values.env == "staging" %}
replicas: 2
{% else %}
replicas: 1
{% endif %}
```

### Boucles

```yaml
{% for key, value in values.env %}
- name: {{ key }}
  value: {{ value | quote }}
{% endfor %}

{% for host in values.ingress.hosts %}
- host: {{ host.name }}
  paths:
    {% for path in host.paths %}
    - path: {{ path }}
    {% endfor %}
{% endfor %}
```

### Assignation

```yaml
{% set fullName = release.name ~ "-" ~ pack.name %}
name: {{ fullName }}
```

## Filtres courants

```yaml
# Sérialisation
{{ values.config | toyaml | indent(2) }}
{{ values.data | tojson }}

# Strings
{{ values.name | upper }}
{{ values.name | lower }}
{{ "my-app" | kebabcase }}
{{ "my_app" | snakecase }}

# Encodage
{{ values.secret | b64encode }}
{{ values.encoded | b64decode }}

# Indentation
{{ values.config | toyaml | indent(4) }}
{{ values.config | toyaml | nindent(4) }}

# Sécurité
{{ values.name | quote }}
{{ values.count | required("count est requis") }}
```

## Macros (Helpers)

### Définition

```yaml
# templates/_helpers.tpl
{% macro fullname() %}
{{- release.name }}-{{ pack.name }}
{%- endmacro %}

{% macro labels() %}
app.kubernetes.io/name: {{ pack.name }}
app.kubernetes.io/instance: {{ release.name }}
{%- endmacro %}
```

### Utilisation

```yaml
{% from "_helpers.tpl" import fullname, labels %}

metadata:
  name: {{ fullname() }}
  labels:
    {{ labels() | indent(4) }}
```

## Whitespace control

```yaml
# Supprimer les espaces avant
{{- values.name }}

# Supprimer les espaces après
{{ values.name -}}

# Les deux
{{- values.name -}}

# Dans les blocs
{%- if condition %}
...
{%- endif %}
```

## Prochaines étapes

- [Filtres](/docs/templating/filters) - Tous les filtres disponibles
- [Fonctions](/docs/templating/functions) - Fonctions built-in
- [Variables de contexte](/docs/templating/context-variables) - Référence complète
