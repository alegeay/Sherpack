---
id: hooks
title: Hooks
sidebar_position: 3
---

# Hooks

Les hooks sont des ressources spéciales qui s'exécutent à des points précis du cycle de vie d'une release.

## Phases de hooks

| Phase | Quand | Cas d'usage |
|-------|------|-------------|
| `pre-install` | Avant l'installation | Migrations de base de données |
| `post-install` | Après l'installation | Notifications, configuration |
| `pre-upgrade` | Avant la mise à niveau | Sauvegarde de données |
| `post-upgrade` | Après la mise à niveau | Nettoyage de cache |
| `pre-rollback` | Avant le rollback | Préparer le retour en arrière |
| `post-rollback` | Après le rollback | Restaurer l'état |
| `pre-delete` | Avant la désinstallation | Nettoyage des ressources externes |
| `post-delete` | Après la désinstallation | Notifications finales |
| `test` | Sur `sherpack test` | Tests d'intégration |

## Créer des hooks

Ajoutez des annotations pour marquer une ressource comme hook :

```yaml title="templates/pre-install-job.yaml"
apiVersion: batch/v1
kind: Job
metadata:
  name: {{ release.name }}-migrate
  annotations:
    sherpack.io/hook: pre-install
    sherpack.io/hook-weight: "0"
    sherpack.io/hook-delete-policy: hook-succeeded
spec:
  template:
    spec:
      containers:
        - name: migrate
          image: {{ values.image.repository }}:{{ values.image.tag }}
          command: ["./migrate.sh"]
      restartPolicy: Never
  backoffLimit: 3
```

## Annotations de hooks

### sherpack.io/hook

La ou les phases du hook. Peuvent être séparées par des virgules pour plusieurs phases :

```yaml
annotations:
  sherpack.io/hook: pre-install,pre-upgrade
```

### sherpack.io/hook-weight

Ordre d'exécution dans une phase (les poids inférieurs s'exécutent en premier) :

```yaml
annotations:
  sherpack.io/hook: pre-install
  sherpack.io/hook-weight: "-5"  # S'exécute avant le poids 0
```

### sherpack.io/hook-delete-policy

Quand supprimer la ressource hook :

| Politique | Comportement |
|-----------|--------------|
| `hook-succeeded` | Supprimer après réussite |
| `hook-failed` | Supprimer après échec |
| `before-hook-creation` | Supprimer l'existant avant d'en créer un nouveau |

```yaml
annotations:
  sherpack.io/hook-delete-policy: hook-succeeded,hook-failed
```

## Patterns de hooks courants

### Migration de base de données

```yaml title="templates/migrate-job.yaml"
apiVersion: batch/v1
kind: Job
metadata:
  name: {{ release.name }}-migrate
  annotations:
    sherpack.io/hook: pre-install,pre-upgrade
    sherpack.io/hook-weight: "-10"
    sherpack.io/hook-delete-policy: before-hook-creation
spec:
  template:
    spec:
      containers:
        - name: migrate
          image: {{ values.image.repository }}:{{ values.image.tag }}
          command: ["./manage.py", "migrate"]
          env:
            - name: DATABASE_URL
              valueFrom:
                secretKeyRef:
                  name: {{ release.name }}-db
                  key: url
      restartPolicy: Never
  backoffLimit: 1
```

### Notification post-installation

```yaml title="templates/notify-job.yaml"
apiVersion: batch/v1
kind: Job
metadata:
  name: {{ release.name }}-notify
  annotations:
    sherpack.io/hook: post-install,post-upgrade
    sherpack.io/hook-weight: "100"
    sherpack.io/hook-delete-policy: hook-succeeded
spec:
  template:
    spec:
      containers:
        - name: notify
          image: curlimages/curl:latest
          command:
            - curl
            - -X
            - POST
            - -d
            - '{"text":"{{ release.name }} deployed to {{ release.namespace }}"}'
            - {{ values.slack.webhook | quote }}
      restartPolicy: Never
```

### Hook de test

```yaml title="templates/test-job.yaml"
apiVersion: batch/v1
kind: Job
metadata:
  name: {{ release.name }}-test
  annotations:
    sherpack.io/hook: test
    sherpack.io/hook-delete-policy: before-hook-creation
spec:
  template:
    spec:
      containers:
        - name: test
          image: {{ values.image.repository }}:{{ values.image.tag }}
          command: ["./run-tests.sh"]
          env:
            - name: APP_URL
              value: "http://{{ release.name }}:{{ values.service.port }}"
      restartPolicy: Never
```

Exécuter les tests :

```bash
sherpack test myapp
```

## Exécution des hooks

1. Les hooks sont triés par poids (croissant)
2. Chaque hook est créé et surveillé
3. Pour Jobs/Pods : attendre la fin
4. En cas d'échec : l'exécution des hooks s'arrête (sauf si `--no-hooks`)
5. Les politiques de suppression sont appliquées

## Ignorer les hooks

```bash
# Ignorer tous les hooks
sherpack install myapp ./mypack --no-hooks

# Ignorer des phases spécifiques
sherpack upgrade myapp ./mypack --skip-hooks pre-upgrade
```

## Échecs de hooks

Si un hook échoue :

- Avec `--atomic` : Toute l'opération fait un rollback
- Sans `--atomic` : L'opération s'arrête, la release est marquée comme "failed"

Vérifier le statut du hook :

```bash
sherpack status myapp
```

```
Hooks:
  pre-install/migrate: Succeeded
  post-install/notify: Failed (exit code 1)
```
