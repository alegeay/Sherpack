---
id: oci-push
title: OCI Push
sidebar_position: 3
---

# OCI Push

Publiez des packs vers des registries de conteneurs compatibles OCI.

## Aperçu

Les registries OCI (Open Container Initiative) peuvent stocker plus que des images de conteneurs. Sherpack utilise des artefacts OCI pour distribuer des packs via :

- Docker Hub
- GitHub Container Registry (ghcr.io)
- Amazon ECR
- Google Artifact Registry
- Azure Container Registry
- Tout registry compatible OCI

## Commande Push

```bash
sherpack push <archive> <destination>
```

### Push Basique

```bash
# Empaqueter d'abord
sherpack package ./mypack

# Publier vers le registry
sherpack push mypack-1.0.0.tar.gz oci://ghcr.io/myorg/mypack:1.0.0
```

### Format de Destination

```
oci://registry/repository:tag

Exemples :
oci://ghcr.io/myorg/mypack:1.0.0
oci://docker.io/myuser/mypack:latest
oci://registry.example.com/charts/mypack:v1
```

## Authentification

### Docker Config

Utilise les identifiants Docker existants :

```bash
# Se connecter au registry d'abord
docker login ghcr.io

# Push utilise les identifiants docker
sherpack push mypack-1.0.0.tar.gz oci://ghcr.io/myorg/mypack:1.0.0
```

### Variables d'Environnement

```bash
export SHERPACK_OCI_USERNAME=myuser
export SHERPACK_OCI_PASSWORD=mytoken

sherpack push mypack-1.0.0.tar.gz oci://registry.example.com/mypack:1.0.0
```

### Flags CLI

```bash
sherpack push mypack-1.0.0.tar.gz oci://registry.example.com/mypack:1.0.0 \
  --username myuser \
  --password mytoken
```

## Configuration Spécifique aux Registries

### GitHub Container Registry

```bash
# Créer un personal access token avec packages:write
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
# Obtenir le login ECR
aws ecr get-login-password | docker login --username AWS --password-stdin 123456789.dkr.ecr.us-east-1.amazonaws.com

sherpack push mypack-1.0.0.tar.gz oci://123456789.dkr.ecr.us-east-1.amazonaws.com/mypack:1.0.0
```

## Intégration CI/CD

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

## Stratégie de Tagging

### Versionnement Sémantique

```bash
# Version spécifique
sherpack push mypack-1.0.0.tar.gz oci://ghcr.io/myorg/mypack:1.0.0

# Alias de version majeure
sherpack push mypack-1.0.0.tar.gz oci://ghcr.io/myorg/mypack:1

# Latest
sherpack push mypack-1.0.0.tar.gz oci://ghcr.io/myorg/mypack:latest
```

### Basé sur Git

```bash
# SHA de commit
sherpack push mypack.tar.gz oci://ghcr.io/myorg/mypack:sha-abc123

# Branche
sherpack push mypack.tar.gz oci://ghcr.io/myorg/mypack:main
```

## Pull depuis OCI

```bash
# Pull depuis un registry OCI
sherpack pull oci://ghcr.io/myorg/mypack:1.0.0

# Installer directement
sherpack install myapp oci://ghcr.io/myorg/mypack:1.0.0
```
