---
id: hooks
title: Hooks
sidebar_position: 3
---

# Hooks

Hooks are special resources that run at specific points in the release lifecycle.

## Hook Phases

| Phase | When | Use Case |
|-------|------|----------|
| `pre-install` | Before install | Database migrations |
| `post-install` | After install | Notifications, setup |
| `pre-upgrade` | Before upgrade | Backup data |
| `post-upgrade` | After upgrade | Cache clear |
| `pre-rollback` | Before rollback | Prepare for revert |
| `post-rollback` | After rollback | Restore state |
| `pre-delete` | Before uninstall | Cleanup external resources |
| `post-delete` | After uninstall | Final notifications |
| `test` | On `sherpack test` | Integration tests |

## Creating Hooks

Add annotations to mark a resource as a hook:

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

## Hook Annotations

### sherpack.io/hook

The hook phase(s). Can be comma-separated for multiple phases:

```yaml
annotations:
  sherpack.io/hook: pre-install,pre-upgrade
```

### sherpack.io/hook-weight

Execution order within a phase (lower runs first):

```yaml
annotations:
  sherpack.io/hook: pre-install
  sherpack.io/hook-weight: "-5"  # Runs before weight 0
```

### sherpack.io/hook-delete-policy

When to delete the hook resource:

| Policy | Behavior |
|--------|----------|
| `hook-succeeded` | Delete after successful completion |
| `hook-failed` | Delete after failure |
| `before-hook-creation` | Delete existing before creating new |

```yaml
annotations:
  sherpack.io/hook-delete-policy: hook-succeeded,hook-failed
```

## Common Hook Patterns

### Database Migration

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

### Post-Install Notification

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

### Test Hook

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

Run tests:

```bash
sherpack test myapp
```

## Hook Execution

1. Hooks are sorted by weight (ascending)
2. Each hook is created and monitored
3. For Jobs/Pods: wait for completion
4. On failure: hook execution stops (unless `--no-hooks`)
5. Delete policies are applied

## Skipping Hooks

```bash
# Skip all hooks
sherpack install myapp ./mypack --no-hooks

# Skip specific phases
sherpack upgrade myapp ./mypack --skip-hooks pre-upgrade
```

## Hook Failures

If a hook fails:

- With `--atomic`: The entire operation rolls back
- Without `--atomic`: Operation stops, release marked as "failed"

Check hook status:

```bash
sherpack status myapp
```

```
Hooks:
  pre-install/migrate: Succeeded
  post-install/notify: Failed (exit code 1)
```
