---
id: health-checks
title: Vérifications de santé
sidebar_position: 4
---

# Vérifications de santé

Sherpack peut attendre que les ressources soient en bonne santé avant de terminer une opération.

## Utilisation de --wait

Activez les vérifications de santé avec le flag `--wait` :

```bash
sherpack install myapp ./mypack --wait
sherpack upgrade myapp ./mypack --wait --timeout 10m
```

## Vérifications de santé par défaut

### Deployments

Attend que :
- Tous les réplicas soient prêts
- La mise à jour progressive soit terminée
- Aucun pod en crash loop

### StatefulSets

Attend que :
- Tous les réplicas soient prêts
- Les pods soient créés dans l'ordre

### DaemonSets

Attend que :
- Le nombre désiré de pods soit planifié
- Tous les pods soient prêts

### Jobs

Attend que :
- Le job soit terminé (succès ou échec)

### Services

Pour les services LoadBalancer :
- Attend l'assignation de l'IP externe

## Timeout

Définissez le temps d'attente maximum :

```bash
# Par défaut : 5 minutes
sherpack install myapp ./mypack --wait

# Timeout personnalisé
sherpack install myapp ./mypack --wait --timeout 15m
```

## Vérifications de santé personnalisées

Ajoutez des annotations pour des vérifications de santé personnalisées :

### Vérification de santé HTTP

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: {{ release.name }}
  annotations:
    sherpack.io/health-check: http
    sherpack.io/health-check-url: "http://{{ release.name }}:8080/health"
    sherpack.io/health-check-interval: "5s"
    sherpack.io/health-check-timeout: "30s"
```

### Vérification de santé par commande

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: {{ release.name }}
  annotations:
    sherpack.io/health-check: command
    sherpack.io/health-check-command: '["./healthcheck.sh"]'
    sherpack.io/health-check-interval: "10s"
```

## Annotations de vérification de santé

| Annotation | Description | Défaut |
|------------|-------------|--------|
| `sherpack.io/health-check` | Type : `http`, `command`, `none` | auto |
| `sherpack.io/health-check-url` | Point de terminaison HTTP | - |
| `sherpack.io/health-check-command` | Commande (tableau JSON) | - |
| `sherpack.io/health-check-interval` | Intervalle de vérification | `5s` |
| `sherpack.io/health-check-timeout` | Timeout total | depuis `--timeout` |

## Ignorer les vérifications de santé

Ignorer l'attente pour des ressources spécifiques :

```yaml
metadata:
  annotations:
    sherpack.io/health-check: none
```

## Sortie de statut

Pendant `--wait` :

```
Waiting for resources to be ready...
  Deployment/myapp: 2/3 replicas ready
  Deployment/myapp: 3/3 replicas ready ✓
  Service/myapp-lb: Waiting for LoadBalancer IP...
  Service/myapp-lb: 203.0.113.10 ✓
All resources ready!
```

En cas de timeout :

```
Error: Timeout waiting for resources
  Deployment/myapp: 1/3 replicas ready (timeout after 5m)

Use 'kubectl describe deployment myapp' for more details
```

## Bonnes pratiques

1. **Utilisez toujours `--wait` en CI/CD** pour garantir le succès du déploiement
2. **Définissez des timeouts appropriés** pour les applications à démarrage lent
3. **Utilisez des vérifications de santé HTTP** pour une disponibilité précise de l'application
4. **Combinez avec `--atomic`** pour un rollback automatique en cas d'échec :

```bash
sherpack upgrade myapp ./mypack --wait --atomic --timeout 10m
```
