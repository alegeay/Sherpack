---
id: functions
title: Functions
sidebar_position: 3
---

# Functions

Les functions sont appelées avec des parenthèses et peuvent prendre des arguments.

## Accès aux données

### get

Accès sécurisé avec valeur par défaut :

```yaml
# Accès simple
timeout: {{ get(values, "timeout", 30) }}

# Accès imbriqué avec notation à points
port: {{ get(values, "service.port", 80) }}

# Accès à un objet
host: {{ get(values.ingress, "host", "localhost") }}
```

### ternary

Sélection conditionnelle de valeur :

```yaml
# ternary(valeur_vraie, valeur_fausse, condition)
env: {{ ternary("production", "development", release.namespace == "prod") }}

# Cas d'usage courants
replicas: {{ ternary(3, 1, values.highAvailability) }}
pullPolicy: {{ ternary("Always", "IfNotPresent", values.image.tag == "latest") }}
```

## Conversion de types

### tostring

Convertir en chaîne :

```yaml
port: {{ tostring(values.port) }}  # "8080"
```

### toint

Convertir en entier :

```yaml
replicas: {{ toint(values.replicas) }}  # 3
```

### tofloat

Convertir en flottant :

```yaml
ratio: {{ tofloat(values.ratio) }}  # 0.5
```

## Génération

### now

Timestamp ISO actuel :

```yaml
annotations:
  deployed-at: {{ now() }}
  # Sortie: 2024-01-15T10:30:00Z
```

### uuidv4

Générer un UUID aléatoire :

```yaml
metadata:
  annotations:
    deployment-id: {{ uuidv4() }}
    # Sortie: 550e8400-e29b-41d4-a716-446655440000
```

### generate_secret

Générer des secrets idempotents avec différents jeux de caractères :

```yaml
apiVersion: v1
kind: Secret
metadata:
  name: {{ release.name }}-secrets
type: Opaque
data:
  # Alphanumérique (défaut) - 24 caractères
  db-password: {{ generate_secret("db-password", 24) | b64encode }}

  # Hexadécimal - 32 caractères
  api-key: {{ generate_secret("api-key", 32, "hex") | b64encode }}

  # Numérique uniquement - 6 chiffres
  pin-code: {{ generate_secret("pin-code", 6, "numeric") | b64encode }}

  # Lettres uniquement
  token: {{ generate_secret("token", 16, "alpha") | b64encode }}
```

**Signature :** `generate_secret(nom, longueur, charset?)`

| Charset | Caractères | Exemple |
|---------|------------|---------|
| `alphanumeric` (défaut) | `a-zA-Z0-9` | `ZyitwTXQeYUNX5tC` |
| `hex` | `0-9a-f` | `3b56ff6fe00929f0` |
| `numeric` | `0-9` | `529607` |
| `alpha` | `a-zA-Z` | `QeYUNXtCuvmTB` |
| `base64` | Alphabet Base64 | `+/aB3xZ=` |
| `urlsafe` | Base64 URL-safe | `_-aB3xZ` |

**Caractéristique clé : Idempotent** - Le même nom retourne toujours la même valeur au sein d'une session de rendu :

```yaml
# Les trois appels retournent la MÊME valeur
first: {{ generate_secret("shared-key", 16) }}
second: {{ generate_secret("shared-key", 16) }}
third: {{ generate_secret("shared-key", 16) }}
```

:::tip Compatible GitOps
Contrairement au `randAlphaNum` de Helm, `generate_secret` est conçu pour les workflows GitOps.
L'état peut être persisté entre les rendus, garantissant que les secrets ne changent pas à chaque upgrade.
:::

## Gestion des erreurs

### fail

Échouer avec un message d'erreur personnalisé :

```yaml
{% if not values.required.field %}
{{ fail("required.field must be set") }}
{% endif %}

# Avec condition
{{ fail("Database password required") if not values.db.password }}
```

## Exemples d'utilisation

### Accès imbriqué sécurisé

```yaml
# Au lieu de crasher sur des clés manquantes
apiVersion: {{ get(values, "apiVersion", "apps/v1") }}
kind: {{ get(values, "kind", "Deployment") }}

# Profondément imbriqué
tlsSecret: {{ get(values, "ingress.tls.secretName", release.name ~ "-tls") }}
```

### Configuration dynamique

```yaml
# Paramètres basés sur l'environnement
{% set isProd = release.namespace == "production" %}

spec:
  replicas: {{ ternary(3, 1, isProd) }}
  template:
    spec:
      containers:
        - resources:
            limits:
              cpu: {{ ternary("1000m", "100m", isProd) }}
              memory: {{ ternary("1Gi", "256Mi", isProd) }}
```

### Validation avec fail

```yaml
# Exiger certaines valeurs
{% if not values.image.repository %}
{{ fail("image.repository is required") }}
{% endif %}

{% if values.replicas > 10 %}
{{ fail("replicas cannot exceed 10") }}
{% endif %}

# Valider les combinaisons
{% if values.ingress.enabled and not values.ingress.host %}
{{ fail("ingress.host is required when ingress is enabled") }}
{% endif %}
```

### Coercition de types

```yaml
# Assurer une chaîne pour les annotations
annotations:
  replicas: {{ tostring(values.replicas) }}

# Assurer un entier pour les specs
spec:
  replicas: {{ toint(values.replicas) }}
```

### Suivi des déploiements

```yaml
metadata:
  annotations:
    # Identifiant unique de déploiement
    deployment.kubernetes.io/revision: {{ uuidv4() | trunc(8) }}

    # Timestamp pour le suivi
    deployed-at: {{ now() }}
```
