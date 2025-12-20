---
id: filters
title: Filters
sidebar_position: 2
---

# Filters

Les filters transforment les valeurs dans les templates. Utilisez-les avec l'opérateur pipe `|`.

## Sérialisation

### toyaml

Convertir un objet en chaîne YAML :

```yaml
config: |
  {{ values.config | toyaml | indent(2) }}
```

### tojson

Convertir un objet en JSON compact :

```yaml
annotations:
  config: {{ values.config | tojson | quote }}
```

### tojson_pretty

Convertir un objet en JSON formaté :

```yaml
data:
  config.json: |
    {{ values.config | tojson_pretty | indent(4) }}
```

## Encodage

### b64encode

Encoder en Base64 :

```yaml
apiVersion: v1
kind: Secret
data:
  password: {{ values.password | b64encode }}
```

### b64decode

Décoder du Base64 :

```yaml
decoded: {{ values.encoded | b64decode }}
```

### sha256

Hash SHA256 d'une chaîne :

```yaml
annotations:
  checksum/config: {{ values.config | tojson | sha256 }}
```

## Manipulation de chaînes

### quote / squote

Encadrer avec des guillemets :

```yaml
name: {{ values.name | quote }}      # "myapp"
name: {{ values.name | squote }}     # 'myapp'
```

### upper / lower

Changer la casse :

```yaml
env: {{ values.env | upper }}        # PRODUCTION
name: {{ values.name | lower }}      # myapp
```

### title

Casse titre :

```yaml
title: {{ values.name | title }}     # My App
```

### snakecase / kebabcase / camelcase

Conversion de casse :

```yaml
snake: {{ "myAppName" | snakecase }}  # my_app_name
kebab: {{ "myAppName" | kebabcase }}  # my-app-name
camel: {{ "my_app_name" | camelcase }} # myAppName
```

### trunc

Tronquer à n caractères :

```yaml
short: {{ values.hash | trunc(8) }}
```

### trimprefix / trimsuffix

Supprimer un préfixe/suffixe :

```yaml
path: {{ "/api/v1" | trimprefix("/") }}     # api/v1
name: {{ "app.yaml" | trimsuffix(".yaml") }} # app
```

### replace

Remplacer une sous-chaîne :

```yaml
safe: {{ values.name | replace("_", "-") }}
```

### trim

Supprimer les espaces au début/fin :

```yaml
clean: {{ values.input | trim }}
```

## Indentation

### indent

Ajouter des espaces à chaque ligne :

```yaml
data: |
{{ values.config | toyaml | indent(2) }}
```

### nindent

Nouvelle ligne + indentation (le plus courant pour du YAML imbriqué) :

```yaml
spec:
  containers:
    - name: app
      resources:
        {{ values.resources | toyaml | nindent(8) }}
```

## Collections

### keys

Obtenir les clés d'un objet sous forme de tableau :

```yaml
{% for key in values.env | keys %}
- {{ key }}
{% endfor %}
```

### haskey

Vérifier si une clé existe :

```yaml
{% if values.config | haskey("tls") %}
tls:
  enabled: true
{% endif %}
```

### merge

Fusionner deux objets (le droit écrase le gauche) :

```yaml
{% set merged = defaults | merge(overrides) %}
```

### dictsort

Trier un objet pour l'itération :

```yaml
{% for key, value in values.labels | dictsort %}
{{ key }}: {{ value | quote }}
{% endfor %}
```

### first / last

Obtenir le premier/dernier élément :

```yaml
primary: {{ values.hosts | first }}
final: {{ values.hosts | last }}
```

### default

Valeur par défaut si indéfinie :

```yaml
replicas: {{ values.replicas | default(1) }}
tag: {{ values.image.tag | default("latest") }}
```

### join

Joindre un tableau avec un séparateur :

```yaml
hosts: {{ values.hosts | join(",") }}
```

### length

Obtenir la longueur :

```yaml
count: {{ values.items | length }}
```

## Validation

### required

Échouer si indéfini ou vide :

```yaml
name: {{ values.name | required }}
```

Avec un message personnalisé :

```yaml
name: {{ values.name | required("name is required") }}
```

### empty

Vérifier si vide :

```yaml
{% if values.items | empty %}
# Aucun élément configuré
{% endif %}
```

## Conversion de types

### int

Convertir en entier :

```yaml
port: {{ values.port | int }}
replicas: {{ "5" | int }}
```

Fonctionne avec les chaînes, flottants et booléens :

```yaml
{{ "42" | int }}       # 42
{{ 3.14 | int }}       # 3
{{ true | int }}       # 1
{{ false | int }}      # 0
```

### float

Convertir en flottant :

```yaml
ratio: {{ values.ratio | float }}
threshold: {{ "0.95" | float }}
```

### string

Convertir en chaîne :

```yaml
port: {{ values.port | string }}
env:
  COUNT: {{ values.count | string | quote }}
```

## Chaînage de filters

Les filters peuvent être chaînés :

```yaml
# Convertir en YAML, indenter et ajouter un préfixe de nouvelle ligne
resources:
  {{ values.resources | toyaml | nindent(2) }}

# Hasher la config pour la détection de changements
checksum: {{ values.config | tojson | sha256 | trunc(16) }}

# Valeur par défaut sûre avec transformation
name: {{ values.name | default("app") | kebabcase | trunc(63) }}
```
