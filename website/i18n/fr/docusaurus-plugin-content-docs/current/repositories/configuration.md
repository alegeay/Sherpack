---
id: configuration
title: Configuration des Repositories
sidebar_position: 1
---

# Configuration des Repositories

Configurez des repositories pour télécharger et partager des packs.

## Types de Repository

| Type | Format d'URL | Description |
|------|--------------|-------------|
| HTTP | `https://...` | Repository HTTP standard |
| OCI | `oci://...` | Registry de conteneurs |
| File | `file://...` | Répertoire local |

## Ajouter des Repositories

### Repository HTTP

```bash
# Repository public
sherpack repo add stable https://charts.example.com

# Avec authentification
sherpack repo add private https://charts.example.com \
  --username admin \
  --password secret

# Avec token
sherpack repo add github https://charts.github.io/repo \
  --token ghp_xxxx
```

### Registry OCI

```bash
# Docker Hub
sherpack repo add dockerhub oci://registry-1.docker.io/myorg

# GitHub Container Registry
sherpack repo add ghcr oci://ghcr.io/myorg/charts

# Registry privé
sherpack repo add private-oci oci://registry.example.com/charts \
  --username admin \
  --password secret
```

### Repository Local

```bash
sherpack repo add local file:///home/user/charts
```

## Lister les Repositories

```bash
sherpack repo list
```

Sortie :

```
NAME        URL                                 TYPE
stable      https://charts.example.com          http
ghcr        oci://ghcr.io/myorg/charts         oci
local       file:///home/user/charts           file
```

Avec le statut d'authentification :

```bash
sherpack repo list --auth
```

```
NAME        URL                                 TYPE    AUTH
stable      https://charts.example.com          http    none
private     https://private.example.com         http    basic
ghcr        oci://ghcr.io/myorg/charts         oci     token
```

## Mettre à Jour l'Index du Repository

```bash
# Mettre à jour tous les repositories
sherpack repo update

# Mettre à jour un repository spécifique
sherpack repo update stable
```

## Supprimer un Repository

```bash
sherpack repo remove stable
```

## Fichier de Configuration

Les repositories sont stockés dans `~/.config/sherpack/repositories.yaml` :

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

## Variables d'Environnement

```bash
# Emplacement de configuration par défaut
export SHERPACK_CONFIG=~/.config/sherpack

# Identifiants pour les repositories
export SHERPACK_REPO_STABLE_USERNAME=admin
export SHERPACK_REPO_STABLE_PASSWORD=secret
```

## Sécurité des Identifiants

- Les mots de passe sont chiffrés au repos
- Les identifiants ne sont jamais envoyés après des redirections cross-origin
- L'authentification par token est préférée pour CI/CD

### Docker Credential Helpers

Sherpack peut utiliser les credential helpers Docker :

```bash
# Utilise ~/.docker/config.json
sherpack repo add ghcr oci://ghcr.io/myorg/charts --use-docker-auth
```
