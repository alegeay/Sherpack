---
id: search-pull
title: Search & Pull
sidebar_position: 2
---

# Search & Pull

Find and download packs from repositories.

## Search

Search across all configured repositories:

```bash
sherpack search <query>
```

### Basic Search

```bash
sherpack search nginx
```

Output:

```
NAME                REPOSITORY  VERSION   DESCRIPTION
nginx               stable      1.0.0     NGINX web server
nginx-ingress       stable      4.5.0     Ingress controller
bitnami/nginx       bitnami     15.0.0    NGINX Open Source
```

### Search Options

```bash
# Search specific repository
sherpack search nginx --repo stable

# Show all versions
sherpack search nginx --versions

# JSON output
sherpack search nginx --json
```

### JSON Output

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

Download a pack from a repository:

```bash
sherpack pull <reference>
```

### Reference Formats

```bash
# repo/name:version
sherpack pull stable/nginx:1.0.0

# repo/name (latest version)
sherpack pull stable/nginx

# OCI reference
sherpack pull oci://ghcr.io/myorg/nginx:1.0.0
```

### Pull Options

```bash
# Specify version separately
sherpack pull stable/nginx --ver 1.0.0

# Custom output path
sherpack pull stable/nginx -o ./nginx-pack.tar.gz

# Extract to directory
sherpack pull stable/nginx --untar -o ./nginx/
```

### Output

```
Pulling: stable/nginx:1.0.0
  Repository: https://charts.example.com
  Size: 4.2 KB

Downloaded: nginx-1.0.0.tar.gz
Digest: sha256:a1b2c3d4...
```

## Local Cache

Downloaded packs are cached locally:

```
~/.cache/sherpack/
└── packs/
    ├── stable/
    │   └── nginx-1.0.0.tar.gz
    └── bitnami/
        └── nginx-15.0.0.tar.gz
```

### Clear Cache

```bash
# Clear all cached packs
sherpack cache clean

# Clear specific repository
sherpack cache clean --repo stable
```

## Install from Repository

Pull and install in one command:

```bash
# Pulls if not cached, then installs
sherpack install myapp stable/nginx:1.0.0

# With values
sherpack install myapp stable/nginx --set replicas=3
```

## Search Index

Sherpack maintains a local SQLite index for fast search:

- Updated on `sherpack repo update`
- Supports full-text search
- Includes description and keywords

### Index Location

```
~/.cache/sherpack/index.db
```

### Rebuild Index

```bash
sherpack repo update --rebuild-index
```
