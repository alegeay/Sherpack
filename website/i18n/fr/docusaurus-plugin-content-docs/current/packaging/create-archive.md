---
id: create-archive
title: Créer des Archives
sidebar_position: 1
---

# Créer des Archives

Empaquetez votre pack dans une archive distribuable.

## Commande Package

```bash
sherpack package <pack> [options]
```

### Utilisation de Base

```bash
# Empaqueter avec le nom par défaut (nom-version.tar.gz)
sherpack package ./mypack
# Crée : mypack-1.0.0.tar.gz

# Chemin de sortie personnalisé
sherpack package ./mypack -o /tmp/release.tar.gz
```

## Format d'Archive

L'archive est un tarball compressé avec gzip contenant :

```
mypack-1.0.0.tar.gz
├── MANIFEST              # Sommes de contrôle SHA256
├── Pack.yaml             # Métadonnées du pack
├── values.yaml           # Valeurs par défaut
├── values.schema.yaml    # Schéma (si présent)
└── templates/
    ├── deployment.yaml
    ├── service.yaml
    └── configmap.yaml
```

## Fichier MANIFEST

Le MANIFEST contient les informations d'intégrité :

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

## Builds Reproductibles

Les archives sont reproductibles :

- Les horodatages de modification des fichiers sont normalisés à 0
- Les fichiers sont triés par ordre alphabétique
- Compression gzip cohérente

Le même contenu de pack produit toujours la même archive.

## Sortie

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

## Bonnes Pratiques

1. **Versionnez vos packs** - Mettez à jour `version` dans Pack.yaml pour chaque version
2. **Incluez un schéma** - Aide les utilisateurs à comprendre les configurations valides
3. **Testez avant d'empaqueter** - Exécutez `sherpack lint` et `sherpack template`
4. **Signez les archives** - Utilisez `sherpack sign` pour la sécurité de la chaîne d'approvisionnement
