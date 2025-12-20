---
id: search-pull
title: Recherche & Téléchargement
sidebar_position: 2
---

# Recherche & Téléchargement

Trouvez et téléchargez des packs depuis les repositories.

## Recherche

Recherchez dans tous les repositories configurés :

```bash
sherpack search <requête>
```

### Recherche Basique

```bash
sherpack search nginx
```

Sortie :

```
NAME                REPOSITORY  VERSION   DESCRIPTION
nginx               stable      1.0.0     NGINX web server
nginx-ingress       stable      4.5.0     Ingress controller
bitnami/nginx       bitnami     15.0.0    NGINX Open Source
```

### Options de Recherche

```bash
# Rechercher dans un repository spécifique
sherpack search nginx --repo stable

# Afficher toutes les versions
sherpack search nginx --versions

# Sortie JSON
sherpack search nginx --json
```

### Sortie JSON

```json
[
  {
    "name": "nginx",
    "repository": "stable",
    "version": "1.0.0",
    "description": "NGINX web server",
    "versions": ["1.0.0", "0.9.0", "0.8.0"]
  }
]
```

## Pull

Téléchargez un pack depuis un repository :

```bash
sherpack pull <référence>
```

### Formats de Référence

```bash
# repo/nom:version
sherpack pull stable/nginx:1.0.0

# repo/nom (dernière version)
sherpack pull stable/nginx

# Référence OCI
sherpack pull oci://ghcr.io/myorg/nginx:1.0.0
```

### Options de Pull

```bash
# Spécifier la version séparément
sherpack pull stable/nginx --ver 1.0.0

# Chemin de sortie personnalisé
sherpack pull stable/nginx -o ./nginx-pack.tar.gz

# Extraire dans un répertoire
sherpack pull stable/nginx --untar -o ./nginx/
```

### Sortie

```
Pulling: stable/nginx:1.0.0
  Repository: https://charts.example.com
  Size: 4.2 KB

Downloaded: nginx-1.0.0.tar.gz
Digest: sha256:a1b2c3d4...
```

## Cache Local

Les packs téléchargés sont mis en cache localement :

```
~/.cache/sherpack/
└── packs/
    ├── stable/
    │   └── nginx-1.0.0.tar.gz
    └── bitnami/
        └── nginx-15.0.0.tar.gz
```

### Vider le Cache

```bash
# Vider tous les packs en cache
sherpack cache clean

# Vider un repository spécifique
sherpack cache clean --repo stable
```

## Installer depuis un Repository

Pull et installation en une seule commande :

```bash
# Télécharge si non mis en cache, puis installe
sherpack install myapp stable/nginx:1.0.0

# Avec des valeurs
sherpack install myapp stable/nginx --set replicas=3
```

## Index de Recherche

Sherpack maintient un index SQLite local pour une recherche rapide :

- Mis à jour avec `sherpack repo update`
- Supporte la recherche en texte intégral
- Inclut la description et les mots-clés

### Emplacement de l'Index

```
~/.cache/sherpack/index.db
```

### Reconstruire l'Index

```bash
sherpack repo update --rebuild-index
```
