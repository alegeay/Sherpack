---
id: dependencies
title: Dépendances
sidebar_position: 4
---

# Dépendances

Gérez les dépendances de pack avec verrouillage de version.

## Déclarer des Dépendances

Ajoutez des dépendances à `Pack.yaml` :

```yaml title="Pack.yaml"
apiVersion: sherpack/v1
kind: application
metadata:
  name: myapp
  version: 1.0.0

dependencies:
  - name: redis
    version: ">=7.0.0"
    repository: https://charts.bitnami.com/bitnami

  - name: postgresql
    version: "~15.0.0"
    repository: https://charts.bitnami.com/bitnami
    condition: postgresql.enabled

  - name: common
    version: "*"
    repository: https://charts.bitnami.com/bitnami
    alias: helpers
```

## Contraintes de Version

| Contrainte | Signification |
|------------|---------------|
| `1.0.0` | Version exacte |
| `>=1.0.0` | Version minimale |
| `<=2.0.0` | Version maximale |
| `>=1.0.0,<2.0.0` | Plage |
| `~1.2.0` | Mises à jour de patch (permet 1.2.x) |
| `^1.2.0` | Mises à jour mineures (permet 1.x.x) |
| `*` | N'importe quelle version |

## Commandes de Dépendances

### Lister les Dépendances

```bash
sherpack dependency list ./mypack
```

Sortie :

```
DEPENDENCY   VERSION     REPOSITORY                              STATUS
redis        >=7.0.0     https://charts.bitnami.com/bitnami     not installed
postgresql   ~15.0.0     https://charts.bitnami.com/bitnami     not installed
common       *           https://charts.bitnami.com/bitnami     not installed
```

### Update (Résoudre & Verrouiller)

```bash
sherpack dependency update ./mypack
```

Crée/met à jour `Pack.lock.yaml` :

```yaml
pack_yaml_digest: sha256:a1b2c3d4...
policy: version
dependencies:
  - name: redis
    version: "7.2.4"
    repository: https://charts.bitnami.com/bitnami
    digest: sha256:abc123...

  - name: postgresql
    version: "15.2.0"
    repository: https://charts.bitnami.com/bitnami
    digest: sha256:def456...

  - name: common
    version: "2.4.0"
    repository: https://charts.bitnami.com/bitnami
    digest: sha256:789abc...
```

### Build (Télécharger)

```bash
sherpack dependency build ./mypack
```

Télécharge dans le répertoire `packs/` :

```
mypack/
├── Pack.yaml
├── Pack.lock.yaml
├── packs/
│   ├── redis-7.2.4.tar.gz
│   ├── postgresql-15.2.0.tar.gz
│   └── common-2.4.0.tar.gz
```

### Afficher l'Arborescence

```bash
sherpack dependency tree ./mypack
```

```
myapp@1.0.0
├── redis@7.2.4
│   └── common@2.4.0
├── postgresql@15.2.0
│   └── common@2.4.0
└── common@2.4.0 (alias: helpers)
```

## Politiques de Verrouillage

Configurez à quel point les versions sont verrouillées strictement :

```bash
sherpack dependency update ./mypack --policy strict
```

| Politique | Comportement |
|-----------|--------------|
| `strict` | Version exacte + SHA doivent correspondre |
| `version` | La version doit correspondre (par défaut) |
| `semver-patch` | Autoriser les mises à jour de patch (1.2.3 → 1.2.4) |
| `semver-minor` | Autoriser les mises à jour mineures (1.2.3 → 1.3.0) |

## Dépendances Conditionnelles

Activez/désactivez les dépendances en fonction des valeurs :

```yaml title="Pack.yaml"
dependencies:
  - name: postgresql
    version: "~15.0.0"
    repository: https://charts.bitnami.com
    condition: postgresql.enabled
```

```yaml title="values.yaml"
postgresql:
  enabled: true
```

## Alias de Dépendances

Utilisez le même pack plusieurs fois avec des noms différents :

```yaml
dependencies:
  - name: redis
    version: "7.0.0"
    repository: https://charts.bitnami.com
    alias: cache

  - name: redis
    version: "7.0.0"
    repository: https://charts.bitnami.com
    alias: session
```

Accès dans les templates :

```yaml
cache: {{ values.cache.host }}
session: {{ values.session.host }}
```

## Dépendances en Diamant

Lorsque des dépendances partagent une dépendance commune :

```
myapp
├── redis → common@2.4.0
└── postgresql → common@2.5.0
```

Sherpack détecte ce conflit :

```
Error: Diamond dependency conflict

  common required at incompatible versions:
    - redis requires common@2.4.0
    - postgresql requires common@2.5.0

Resolution options:
  1. Pin common version in myapp
  2. Use dependency aliases
  3. Update dependencies to compatible versions
```

## Utiliser les Dépendances dans les Templates

Les dépendances sont disponibles dans le répertoire `packs/` :

```yaml
{% include "packs/common/templates/_helpers.tpl" %}
```

Ou importez des valeurs :

```yaml title="values.yaml"
redis:
  # Valeurs passées à la dépendance redis
  replica:
    replicaCount: 3
```
