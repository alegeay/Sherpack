---
id: install-upgrade
title: Installation & Mise à niveau
sidebar_position: 1
---

# Installation & Mise à niveau

Déployez et mettez à jour vos applications sur Kubernetes.

## Installation

Installer un pack dans le cluster :

```bash
sherpack install <nom> <pack> [options]
```

### Installation basique

```bash
# Installer depuis un répertoire
sherpack install myapp ./mypack

# Installer depuis une archive
sherpack install myapp mypack-1.0.0.tar.gz

# Avec un namespace
sherpack install myapp ./mypack -n production
```

### Avec des valeurs

```bash
# Remplacer des valeurs
sherpack install myapp ./mypack --set app.replicas=3

# Utiliser un fichier de valeurs
sherpack install myapp ./mypack -f production.yaml

# Combiner les deux
sherpack install myapp ./mypack -f base.yaml --set image.tag=v2.0.0
```

### Attendre la disponibilité

```bash
# Attendre que toutes les ressources soient prêtes
sherpack install myapp ./mypack --wait

# Avec un timeout
sherpack install myapp ./mypack --wait --timeout 10m
```

### Installation atomique

Rollback automatique en cas d'échec :

```bash
sherpack install myapp ./mypack --atomic
```

### Exécution à blanc

Prévisualiser sans appliquer :

```bash
sherpack install myapp ./mypack --dry-run
```

## Mise à niveau

Mettre à niveau une release existante :

```bash
sherpack upgrade <nom> <pack> [options]
```

### Mise à niveau basique

```bash
# Mettre à niveau avec une nouvelle version du pack
sherpack upgrade myapp ./mypack

# Mettre à niveau avec de nouvelles valeurs
sherpack upgrade myapp ./mypack --set app.replicas=5
```

### Gestion des valeurs

```bash
# Réinitialiser aux valeurs par défaut du pack, puis appliquer les nouvelles valeurs
sherpack upgrade myapp ./mypack --reset-values --set image.tag=v2

# Réutiliser les valeurs précédentes, remplacer des valeurs spécifiques
sherpack upgrade myapp ./mypack --reuse-values --set image.tag=v2

# Réinitialiser puis réutiliser (réinitialiser les défauts, garder les valeurs utilisateur)
sherpack upgrade myapp ./mypack --reset-then-reuse-values
```

### Diff avant mise à niveau

Prévisualiser les changements :

```bash
sherpack upgrade myapp ./mypack --diff
```

La sortie montre ce qui va changer :

```diff
--- deployed
+++ pending
@@ -10,7 +10,7 @@
 spec:
-  replicas: 3
+  replicas: 5
   template:
```

### Installer ou mettre à niveau

Installer si n'existe pas, sinon mettre à niveau :

```bash
sherpack upgrade myapp ./mypack --install
```

## Référence des options

### Options d'installation

| Option | Description |
|--------|-------------|
| `-n, --namespace` | Namespace cible |
| `-f, --values` | Fichier de valeurs (répétable) |
| `--set` | Définir une valeur (répétable) |
| `--wait` | Attendre la disponibilité |
| `--timeout` | Timeout d'attente [défaut : 5m] |
| `--atomic` | Rollback en cas d'échec |
| `--dry-run` | Ne pas appliquer |
| `--create-namespace` | Créer le namespace s'il manque |

### Options de mise à niveau

| Option | Description |
|--------|-------------|
| `-n, --namespace` | Namespace cible |
| `-f, --values` | Fichier de valeurs (répétable) |
| `--set` | Définir une valeur (répétable) |
| `--wait` | Attendre la disponibilité |
| `--timeout` | Timeout d'attente |
| `--atomic` | Rollback en cas d'échec |
| `--dry-run` | Ne pas appliquer |
| `--diff` | Afficher le diff avant application |
| `--reuse-values` | Réutiliser les valeurs précédentes |
| `--reset-values` | Réinitialiser aux défauts |
| `--install` | Installer si n'existe pas |

## Flux d'installation

1. Charger le pack et fusionner les valeurs
2. Valider contre le schéma (si présent)
3. Rendre les templates
4. Stocker la release comme "pending-install"
5. Exécuter les hooks pre-install
6. Appliquer les ressources (Server-Side Apply)
7. Attendre la santé (si `--wait`)
8. Exécuter les hooks post-install
9. Mettre à jour la release vers "deployed"

## Flux de mise à niveau

1. Obtenir la release actuelle
2. Charger le pack et fusionner les valeurs
3. Rendre les nouveaux templates
4. Stocker la release comme "pending-upgrade"
5. Exécuter les hooks pre-upgrade
6. Appliquer les changements de ressources
7. Attendre la santé (si `--wait`)
8. Exécuter les hooks post-upgrade
9. Marquer la précédente comme "superseded", la nouvelle comme "deployed"
