---
id: create-archive
title: Creating Archives
sidebar_position: 1
---

# Creating Archives

Package your pack into a distributable archive.

## Package Command

```bash
sherpack package <pack> [options]
```

### Basic Usage

```bash
# Package with default name (name-version.tar.gz)
sherpack package ./mypack
# Creates: mypack-1.0.0.tar.gz

# Custom output path
sherpack package ./mypack -o /tmp/release.tar.gz
```

## Archive Format

The archive is a gzip-compressed tarball containing:

```
mypack-1.0.0.tar.gz
├── MANIFEST              # SHA256 checksums
├── Pack.yaml             # Pack metadata
├── values.yaml           # Default values
├── values.schema.yaml    # Schema (if present)
└── templates/
    ├── deployment.yaml
    ├── service.yaml
    └── configmap.yaml
```

## MANIFEST File

The MANIFEST contains integrity information:

```toml
sherpack-manifest-version: 1

[files]
Pack.yaml = "sha256:a1b2c3d4e5f6..."
values.yaml = "sha256:b2c3d4e5f6g7..."
values.schema.yaml = "sha256:c3d4e5f6g7h8..."
templates/deployment.yaml = "sha256:d4e5f6g7h8i9..."
templates/service.yaml = "sha256:e5f6g7h8i9j0..."

[digest]
archive = "sha256:f6g7h8i9j0k1..."
```

## Reproducible Builds

Archives are reproducible:

- File modification times are normalized to 0
- Files are sorted alphabetically
- Consistent gzip compression

The same pack content always produces the same archive.

## Output

```
Packaging: ./mypack
  Name: mypack
  Version: 1.0.0

Contents:
  MANIFEST                    234 B
  Pack.yaml                   156 B
  values.yaml                 892 B
  values.schema.yaml          1.2 KB
  templates/deployment.yaml   1.8 KB
  templates/service.yaml      456 B

Created: mypack-1.0.0.tar.gz (4.2 KB)
Digest: sha256:a1b2c3d4e5f6789...
```

## Best Practices

1. **Version your packs** - Update `version` in Pack.yaml for each release
2. **Include schema** - Helps users understand valid configurations
3. **Test before packaging** - Run `sherpack lint` and `sherpack template`
4. **Sign archives** - Use `sherpack sign` for supply chain security
