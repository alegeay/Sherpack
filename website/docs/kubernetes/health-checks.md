---
id: health-checks
title: Health Checks
sidebar_position: 4
---

# Health Checks

Sherpack can wait for resources to be healthy before completing an operation.

## Using --wait

Enable health checking with the `--wait` flag:

```bash
sherpack install myapp ./mypack --wait
sherpack upgrade myapp ./mypack --wait --timeout 10m
```

## Default Health Checks

### Deployments

Waits for:
- All replicas to be ready
- Rolling update to complete
- No pods in crash loop

### StatefulSets

Waits for:
- All replicas to be ready
- Pods created in order

### DaemonSets

Waits for:
- Desired number of pods scheduled
- All pods ready

### Jobs

Waits for:
- Job completion (succeeded or failed)

### Services

For LoadBalancer services:
- Waits for external IP assignment

## Timeout

Set maximum wait time:

```bash
# Default: 5 minutes
sherpack install myapp ./mypack --wait

# Custom timeout
sherpack install myapp ./mypack --wait --timeout 15m
```

## Custom Health Checks

Add annotations for custom health checks:

### HTTP Health Check

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

### Command Health Check

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

## Health Check Annotations

| Annotation | Description | Default |
|------------|-------------|---------|
| `sherpack.io/health-check` | Type: `http`, `command`, `none` | auto |
| `sherpack.io/health-check-url` | HTTP endpoint | - |
| `sherpack.io/health-check-command` | Command (JSON array) | - |
| `sherpack.io/health-check-interval` | Check interval | `5s` |
| `sherpack.io/health-check-timeout` | Total timeout | from `--timeout` |

## Skip Health Checks

Skip waiting for specific resources:

```yaml
metadata:
  annotations:
    sherpack.io/health-check: none
```

## Status Output

During `--wait`:

```
Waiting for resources to be ready...
  Deployment/myapp: 2/3 replicas ready
  Deployment/myapp: 3/3 replicas ready ✓
  Service/myapp-lb: Waiting for LoadBalancer IP...
  Service/myapp-lb: 203.0.113.10 ✓
All resources ready!
```

On timeout:

```
Error: Timeout waiting for resources
  Deployment/myapp: 1/3 replicas ready (timeout after 5m)

Use 'kubectl describe deployment myapp' for more details
```

## Best Practices

1. **Always use `--wait` in CI/CD** to ensure deployment success
2. **Set appropriate timeouts** for slow-starting applications
3. **Use HTTP health checks** for accurate application readiness
4. **Combine with `--atomic`** for automatic rollback on failure:

```bash
sherpack upgrade myapp ./mypack --wait --atomic --timeout 10m
```
