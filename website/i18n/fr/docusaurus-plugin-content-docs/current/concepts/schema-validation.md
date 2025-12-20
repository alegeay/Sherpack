---
id: schema-validation
title: Validation de schéma
sidebar_position: 4
---

# Validation de schéma

Validez vos valeurs avec JSON Schema avant le déploiement.

## Formats supportés

Sherpack supporte deux formats de schéma :

### Format simplifié Sherpack

```yaml
# values.schema.yaml
schemaVersion: "1.0"
title: Configuration MyApp
required:
  - image

properties:
  replicaCount:
    type: integer
    minimum: 1
    maximum: 100
    default: 1

  image:
    type: object
    required:
      - repository
    properties:
      repository:
        type: string
      tag:
        type: string
        default: latest
```

### JSON Schema standard

```json
{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "type": "object",
  "required": ["image"],
  "properties": {
    "replicaCount": {
      "type": "integer",
      "minimum": 1,
      "default": 1
    }
  }
}
```

## Types de données

| Type | Description | Exemple |
|------|-------------|---------|
| `string` | Chaîne de caractères | `"hello"` |
| `integer` | Nombre entier | `42` |
| `number` | Nombre décimal | `3.14` |
| `boolean` | Booléen | `true` |
| `array` | Tableau | `[1, 2, 3]` |
| `object` | Objet | `{key: value}` |

## Contraintes

### Nombres

```yaml
port:
  type: integer
  minimum: 1
  maximum: 65535
  default: 8080
```

### Chaînes

```yaml
name:
  type: string
  minLength: 1
  maxLength: 63
  pattern: "^[a-z][a-z0-9-]*$"
```

### Énumérations

```yaml
pullPolicy:
  type: string
  enum:
    - Always
    - IfNotPresent
    - Never
  default: IfNotPresent
```

### Tableaux

```yaml
hosts:
  type: array
  items:
    type: string
  minItems: 1
  uniqueItems: true
```

### Objets

```yaml
resources:
  type: object
  properties:
    limits:
      type: object
      properties:
        cpu:
          type: string
        memory:
          type: string
```

## Valeurs par défaut

Les valeurs par défaut du schéma sont appliquées automatiquement :

```yaml
# Schema
properties:
  replicaCount:
    type: integer
    default: 3

# values.yaml (vide ou sans replicaCount)
{}

# Résultat
replicaCount: 3
```

## Commandes

### Valider

```bash
# Validation simple
sherpack validate ./mypack

# Avec valeurs personnalisées
sherpack validate ./mypack -f custom.yaml

# Sortie JSON (pour CI)
sherpack validate ./mypack --json
```

### Lint avec validation

```bash
sherpack lint ./mypack
```

### Ignorer la validation

```bash
sherpack template release ./pack --skip-schema
sherpack lint ./pack --skip-schema
```

## Messages d'erreur

Sherpack fournit des messages d'erreur utiles avec suggestions :

```
Error: Schema validation failed

  ✗ values.replicaCount: expected integer, got string
    Hint: Change "3" to 3 (without quotes)

  ✗ values.image.pullPoilcy: unknown property
    Did you mean: pullPolicy?

  ✗ values.service.port: 70000 is greater than maximum 65535
```

## Bonnes pratiques

1. **Documentez avec `description`** :
   ```yaml
   replicaCount:
     type: integer
     description: Nombre de réplicas du déploiement
   ```

2. **Utilisez des valeurs par défaut sensibles**

3. **Marquez les champs obligatoires** :
   ```yaml
   required:
     - image
   ```

4. **Utilisez des patterns pour valider les formats**
