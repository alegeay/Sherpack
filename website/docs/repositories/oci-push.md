---
id: oci-push
title: OCI Push
sidebar_position: 3
---

# OCI Push

Push packs to OCI-compliant container registries.

## Overview

OCI (Open Container Initiative) registries can store more than container images. Sherpack uses OCI artifacts to distribute packs via:

- Docker Hub
- GitHub Container Registry (ghcr.io)
- Amazon ECR
- Google Artifact Registry
- Azure Container Registry
- Any OCI-compliant registry

## Push Command

```bash
sherpack push <archive> <destination>
```

### Basic Push

```bash
# Package first
sherpack package ./mypack

# Push to registry
sherpack push mypack-1.0.0.tar.gz oci://ghcr.io/myorg/mypack:1.0.0
```

### Destination Format

```
oci://registry/repository:tag

Examples:
oci://ghcr.io/myorg/mypack:1.0.0
oci://docker.io/myuser/mypack:latest
oci://registry.example.com/charts/mypack:v1
```

## Authentication

### Docker Config

Uses existing Docker credentials:

```bash
# Login to registry first
docker login ghcr.io

# Push uses docker credentials
sherpack push mypack-1.0.0.tar.gz oci://ghcr.io/myorg/mypack:1.0.0
```

### Environment Variables

```bash
export SHERPACK_OCI_USERNAME=myuser
export SHERPACK_OCI_PASSWORD=mytoken

sherpack push mypack-1.0.0.tar.gz oci://registry.example.com/mypack:1.0.0
```

### CLI Flags

```bash
sherpack push mypack-1.0.0.tar.gz oci://registry.example.com/mypack:1.0.0 \
  --username myuser \
  --password mytoken
```

## Registry-Specific Setup

### GitHub Container Registry

```bash
# Create personal access token with packages:write
echo $GITHUB_TOKEN | docker login ghcr.io -u USERNAME --password-stdin

# Push
sherpack push mypack-1.0.0.tar.gz oci://ghcr.io/myorg/mypack:1.0.0
```

### Docker Hub

```bash
docker login

sherpack push mypack-1.0.0.tar.gz oci://docker.io/myuser/mypack:1.0.0
```

### Amazon ECR

```bash
# Get ECR login
aws ecr get-login-password | docker login --username AWS --password-stdin 123456789.dkr.ecr.us-east-1.amazonaws.com

sherpack push mypack-1.0.0.tar.gz oci://123456789.dkr.ecr.us-east-1.amazonaws.com/mypack:1.0.0
```

## CI/CD Integration

### GitHub Actions

```yaml
- name: Push to GHCR
  env:
    GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
  run: |
    echo "$GITHUB_TOKEN" | docker login ghcr.io -u ${{ github.actor }} --password-stdin
    sherpack package ./mypack
    sherpack push mypack-*.tar.gz oci://ghcr.io/${{ github.repository }}:${{ github.ref_name }}
```

### GitLab CI

```yaml
push:
  script:
    - docker login -u $CI_REGISTRY_USER -p $CI_REGISTRY_PASSWORD $CI_REGISTRY
    - sherpack package ./mypack
    - sherpack push mypack-*.tar.gz oci://$CI_REGISTRY_IMAGE:$CI_COMMIT_TAG
```

## Tagging Strategy

### Semantic Versioning

```bash
# Specific version
sherpack push mypack-1.0.0.tar.gz oci://ghcr.io/myorg/mypack:1.0.0

# Major version alias
sherpack push mypack-1.0.0.tar.gz oci://ghcr.io/myorg/mypack:1

# Latest
sherpack push mypack-1.0.0.tar.gz oci://ghcr.io/myorg/mypack:latest
```

### Git-based

```bash
# Commit SHA
sherpack push mypack.tar.gz oci://ghcr.io/myorg/mypack:sha-abc123

# Branch
sherpack push mypack.tar.gz oci://ghcr.io/myorg/mypack:main
```

## Pull from OCI

```bash
# Pull from OCI registry
sherpack pull oci://ghcr.io/myorg/mypack:1.0.0

# Install directly
sherpack install myapp oci://ghcr.io/myorg/mypack:1.0.0
```
