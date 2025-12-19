---
id: values
title: Valeurs
sidebar_position: 2
---

# Valeurs

Les valeurs sont la configuration de votre déploiement.

## Ordre de fusion

Les valeurs sont fusionnées dans cet ordre (les dernières gagnent) :

```
1. Valeurs par défaut du schéma
2. values.yaml du pack
3. Fichiers -f (dans l'ordre)
4. Flags --set
```

## values.yaml

Le fichier de base avec les valeurs par défaut :

```yaml
replicaCount: 1

image:
  repository: nginx
  tag: latest
  pullPolicy: IfNotPresent

service:
  type: ClusterIP
  port: 80

resources:
  limits:
    cpu: 100m
    memory: 128Mi
  requests:
    cpu: 50m
    memory: 64Mi
```

## Fichiers de valeurs (-f)

Passez des fichiers de valeurs supplémentaires :

```bash
# Un fichier
sherpack template release ./pack -f production.yaml

# Plusieurs fichiers (fusionnés dans l'ordre)
sherpack template release ./pack -f base.yaml -f production.yaml -f secrets.yaml
```

Exemple `production.yaml` :

```yaml
replicaCount: 3

resources:
  limits:
    cpu: 500m
    memory: 512Mi
```

## Overrides en ligne (--set)

```bash
# Valeur simple
sherpack template release ./pack --set replicaCount=5

# Valeur imbriquée
sherpack template release ./pack --set image.tag=v2.0.0

# Plusieurs valeurs
sherpack template release ./pack --set replicaCount=3 --set image.tag=v2.0.0

# Tableau
sherpack template release ./pack --set "hosts={a.com,b.com}"

# Chaîne avec guillemets
sherpack template release ./pack --set 'annotation=my\ value'
```

## Accès dans les templates

```yaml
# Accès simple
replicas: {{ values.replicaCount }}

# Accès imbriqué
image: {{ values.image.repository }}:{{ values.image.tag }}

# Valeur par défaut
port: {{ values.service.port | default(80) }}

# Vérification d'existence
{% if values.ingress.enabled %}
...
{% endif %}

# Itération
{% for key, value in values.env %}
- name: {{ key }}
  value: {{ value | quote }}
{% endfor %}
```

## Fusion profonde

Les objets sont fusionnés récursivement :

```yaml
# values.yaml
config:
  debug: false
  timeout: 30
  features:
    a: true
    b: true

# production.yaml
config:
  debug: true
  features:
    b: false
    c: true

# Résultat
config:
  debug: true      # écrasé
  timeout: 30      # conservé
  features:
    a: true        # conservé
    b: false       # écrasé
    c: true        # ajouté
```

## Afficher les valeurs

```bash
# Afficher les valeurs calculées
sherpack template release ./pack --show-values

# Afficher les valeurs par défaut du pack
sherpack show ./pack --values
```
