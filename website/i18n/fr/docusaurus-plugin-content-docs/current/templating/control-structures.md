---
id: control-structures
title: Structures de contrôle
sidebar_position: 4
---

# Structures de contrôle

Jinja2 fournit des structures de contrôle puissantes pour la logique conditionnelle et l'itération.

## Conditions

### if / elif / else

```yaml
{% if values.ingress.enabled %}
apiVersion: networking.k8s.io/v1
kind: Ingress
metadata:
  name: {{ release.name }}
{% endif %}
```

Avec else :

```yaml
{% if values.service.type == "LoadBalancer" %}
type: LoadBalancer
{% elif values.service.type == "NodePort" %}
type: NodePort
{% else %}
type: ClusterIP
{% endif %}
```

### Conditions inline

```yaml
replicas: {{ 3 if values.highAvailability else 1 }}
```

### Véracité

Ces valeurs sont considérées comme **fausses** :
- `false`
- `0`
- `""` (chaîne vide)
- `[]` (liste vide)
- `{}` (objet vide)
- `null` / `none`

```yaml
{% if values.env %}
# env est défini et non vide
{% endif %}

{% if not values.disabled %}
# disabled est faux ou non défini
{% endif %}
```

## Boucles

### for

Itérer sur des listes :

```yaml
{% for port in values.ports %}
- name: {{ port.name }}
  port: {{ port.port }}
  targetPort: {{ port.targetPort }}
{% endfor %}
```

Itérer sur des objets (utiliser `dictsort` pour un ordre déterministe) :

```yaml
{% for key, value in values.env | dictsort %}
- name: {{ key }}
  value: {{ value | quote }}
{% endfor %}
```

### Variables de boucle

| Variable | Description |
|----------|-------------|
| `loop.index` | Itération courante (base 1) |
| `loop.index0` | Itération courante (base 0) |
| `loop.first` | Vrai si première itération |
| `loop.last` | Vrai si dernière itération |
| `loop.length` | Nombre total d'éléments |

```yaml
{% for item in values.items %}
{{ loop.index }}. {{ item }}{% if not loop.last %},{% endif %}
{% endfor %}
```

### Filtrage dans les boucles

```yaml
{% for host in values.hosts if host.enabled %}
- host: {{ host.name }}
{% endfor %}
```

### Gestion des boucles vides

```yaml
{% for item in values.items %}
- {{ item }}
{% else %}
# Aucun élément configuré
{% endfor %}
```

## Variables

### set

Créer ou modifier des variables :

```yaml
{% set fullName = release.name ~ "-" ~ pack.name %}
{% set replicas = values.replicas | default(1) %}

metadata:
  name: {{ fullName }}
spec:
  replicas: {{ replicas }}
```

### Affectation par bloc

```yaml
{% set labels %}
app.kubernetes.io/name: {{ release.name }}
app.kubernetes.io/version: {{ pack.version }}
{% endset %}

metadata:
  labels:
    {{ labels | indent(4) }}
```

## Contrôle des espaces

### Supprimer les espaces

Utiliser `-` pour supprimer les espaces :

```yaml
{%- if condition -%}
trimmed
{%- endif -%}
```

| Syntaxe | Effet |
|--------|--------|
| `{%-` | Supprimer avant |
| `-%}` | Supprimer après |
| `{{-` | Supprimer avant |
| `-}}` | Supprimer après |

### Modèle courant

```yaml
metadata:
  labels:
    {%- for key, value in values.labels | dictsort %}
    {{ key }}: {{ value | quote }}
    {%- endfor %}
```

## Macros

### Définir des blocs réutilisables

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

### Utiliser les macros

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

### Inclure d'autres templates

```yaml
{% include "configmap.yaml" %}
```

### Avec contexte

```yaml
{% include "partial.yaml" with context %}
```

## Exemple complet

```yaml title="templates/deployment.yaml"
{# Définir les valeurs calculées #}
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
