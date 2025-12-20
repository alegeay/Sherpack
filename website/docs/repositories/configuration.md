---
id: configuration
title: Repository Configuration
sidebar_position: 1
---

# Repository Configuration

Configure repositories to download and share packs.

## Repository Types

| Type | URL Format | Description |
|------|------------|-------------|
| HTTP | `https://...` | Standard HTTP repository |
| OCI | `oci://...` | Container registry |
| File | `file://...` | Local directory |

## Add Repositories

### HTTP Repository

```bash
# Public repository
sherpack repo add stable https://charts.example.com

# With authentication
sherpack repo add private https://charts.example.com \
  --username admin \
  --password secret

# With token
sherpack repo add github https://charts.github.io/repo \
  --token ghp_xxxx
```

### OCI Registry

```bash
# Docker Hub
sherpack repo add dockerhub oci://registry-1.docker.io/myorg

# GitHub Container Registry
sherpack repo add ghcr oci://ghcr.io/myorg/charts

# Private registry
sherpack repo add private-oci oci://registry.example.com/charts \
  --username admin \
  --password secret
```

### Local Repository

```bash
sherpack repo add local file:///home/user/charts
```

## List Repositories

```bash
sherpack repo list
```

Output:

```
NAME        URL                                 TYPE
stable      https://charts.example.com          http
ghcr        oci://ghcr.io/myorg/charts         oci
local       file:///home/user/charts           file
```

With authentication status:

```bash
sherpack repo list --auth
```

```
NAME        URL                                 TYPE    AUTH
stable      https://charts.example.com          http    none
private     https://private.example.com         http    basic
ghcr        oci://ghcr.io/myorg/charts         oci     token
```

## Update Repository Index

```bash
# Update all repositories
sherpack repo update

# Update specific repository
sherpack repo update stable
```

## Remove Repository

```bash
sherpack repo remove stable
```

## Configuration File

Repositories are stored in `~/.config/sherpack/repositories.yaml`:

```yaml
repositories:
  - name: stable
    url: https://charts.example.com
    type: http

  - name: private
    url: https://private.example.com
    type: http
    auth:
      username: admin
      password: encrypted:xxx

  - name: ghcr
    url: oci://ghcr.io/myorg/charts
    type: oci
    auth:
      token: encrypted:xxx
```

## Environment Variables

```bash
# Default config location
export SHERPACK_CONFIG=~/.config/sherpack

# Repository credentials
export SHERPACK_REPO_STABLE_USERNAME=admin
export SHERPACK_REPO_STABLE_PASSWORD=secret
```

## Credential Security

- Passwords are encrypted at rest
- Credentials never sent after cross-origin redirects
- Token auth preferred for CI/CD

### Docker Credential Helpers

Sherpack can use Docker credential helpers:

```bash
# Uses ~/.docker/config.json
sherpack repo add ghcr oci://ghcr.io/myorg/charts --use-docker-auth
```
