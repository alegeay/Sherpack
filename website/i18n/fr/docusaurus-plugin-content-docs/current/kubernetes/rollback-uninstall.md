---
id: rollback-uninstall
title: Rollback & Désinstallation
sidebar_position: 2
---

# Rollback & Désinstallation

Revenez aux versions précédentes ou supprimez complètement les releases.

## Rollback

Revenir à une révision précédente :

```bash
sherpack rollback <nom> <révision>
```

### Voir l'historique d'abord

```bash
# Voir les révisions disponibles
sherpack history myapp
```

Sortie :

```
REVISION  STATUS      UPDATED                   DESCRIPTION
1         superseded  2024-01-10T10:00:00Z      Install complete
2         superseded  2024-01-11T14:30:00Z      Upgrade complete
3         deployed    2024-01-12T09:15:00Z      Upgrade complete
```

### Rollback vers une révision

```bash
# Rollback vers la révision 1
sherpack rollback myapp 1

# Avec attente
sherpack rollback myapp 1 --wait
```

### Options de rollback

| Option | Description |
|--------|-------------|
| `-n, --namespace` | Namespace |
| `--wait` | Attendre la fin du rollback |
| `--timeout` | Timeout d'attente |
| `--dry-run` | Prévisualiser sans appliquer |

### Flux de rollback

1. Obtenir la révision cible depuis le stockage
2. Stocker la nouvelle release comme "pending-rollback"
3. Exécuter les hooks pre-rollback
4. Appliquer les ressources depuis la révision cible
5. Attendre la santé (si `--wait`)
6. Exécuter les hooks post-rollback
7. Marquer la nouvelle release comme "deployed"

## Désinstallation

Supprimer une release du cluster :

```bash
sherpack uninstall <nom>
```

### Désinstallation basique

```bash
# Désinstaller une release
sherpack uninstall myapp

# Dans un namespace spécifique
sherpack uninstall myapp -n production
```

### Conserver l'historique

Préserver l'historique de release pour l'audit :

```bash
sherpack uninstall myapp --keep-history
```

### Attendre la suppression

Attendre que toutes les ressources soient supprimées :

```bash
sherpack uninstall myapp --wait
```

### Exécution à blanc

Prévisualiser ce qui sera supprimé :

```bash
sherpack uninstall myapp --dry-run
```

### Options de désinstallation

| Option | Description |
|--------|-------------|
| `-n, --namespace` | Namespace |
| `--keep-history` | Conserver les enregistrements de release |
| `--wait` | Attendre la suppression |
| `--timeout` | Timeout d'attente |
| `--dry-run` | Prévisualiser sans supprimer |

### Flux de désinstallation

1. Obtenir la release actuelle
2. Mettre à jour l'état vers "uninstalling"
3. Exécuter les hooks pre-delete
4. Supprimer toutes les ressources de la release
5. Attendre la suppression (si `--wait`)
6. Exécuter les hooks post-delete
7. Supprimer ou marquer la release comme "uninstalled"

## Politique de ressources

Les ressources avec l'annotation `sherpack.io/resource-policy: keep` sont préservées :

```yaml
apiVersion: v1
kind: PersistentVolumeClaim
metadata:
  name: {{ release.name }}-data
  annotations:
    sherpack.io/resource-policy: keep
```

Ce PVC ne sera pas supprimé pendant la désinstallation ou la mise à niveau.

## Récupérer les releases bloquées

Si une release est bloquée dans un état en attente :

```bash
# Vérifier le statut
sherpack status myapp

# La sortie montre un état bloqué
# Status: pending-upgrade (stale)

# Récupérer
sherpack recover myapp
```

Cela réinitialise la release à son dernier état connu bon.

## Lister les releases

Voir toutes les releases installées :

```bash
# Namespace actuel
sherpack list

# Tous les namespaces
sherpack list -A

# Inclure les désinstallées (avec --keep-history)
sherpack list --all
```

Sortie :

```
NAME    NAMESPACE   REVISION  STATUS    UPDATED
myapp   default     3         deployed  2024-01-12T09:15:00Z
nginx   production  1         deployed  2024-01-10T08:00:00Z
```

## Statut de release

Obtenir le statut détaillé :

```bash
sherpack status myapp
```

Sortie :

```
Name: myapp
Namespace: default
Revision: 3
Status: deployed
Updated: 2024-01-12T09:15:00Z

Resources:
  Deployment/myapp: Ready (3/3 replicas)
  Service/myapp: Active
  ConfigMap/myapp-config: Created
```

Avec les détails des ressources :

```bash
sherpack status myapp --show-resources
```
