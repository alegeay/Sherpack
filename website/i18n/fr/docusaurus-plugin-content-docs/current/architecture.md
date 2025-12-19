---
id: architecture
title: Architecture
sidebar_position: 101
---

# Architecture

Sherpack est construit comme un workspace Rust modulaire avec 5 crates.

## Vue d'ensemble des crates

```
sherpack/
├── crates/
│   ├── sherpack-core/     # Types de base
│   ├── sherpack-engine/   # Moteur de template
│   ├── sherpack-kube/     # Intégration Kubernetes
│   ├── sherpack-repo/     # Gestion des dépôts
│   └── sherpack-cli/      # Application CLI
```

### Dépendances

```
sherpack-cli
    ├── sherpack-core
    ├── sherpack-engine ─── sherpack-core
    ├── sherpack-kube ───── sherpack-core
    └── sherpack-repo ───── sherpack-core
```

## sherpack-core

Types de base partagés entre tous les crates.

### Types principaux

| Type | Description |
|------|-------------|
| `Pack` | Métadonnées du pack depuis Pack.yaml |
| `LoadedPack` | Pack avec fichiers chargés |
| `Values` | Valeurs de configuration avec fusion |
| `Release` | État du déploiement |
| `TemplateContext` | Contexte pour les templates |
| `Archive` | Opérations sur archives tar.gz |
| `Manifest` | Checksums SHA256 |
| `Schema` | Validation JSON Schema |

### Fusion des valeurs

```
Valeurs par défaut du schéma
    └── values.yaml
        └── fichiers -f (dans l'ordre)
            └── flags --set
```

## sherpack-engine

Moteur de template basé sur MiniJinja.

### Composants

| Composant | Description |
|-----------|-------------|
| `Engine` | Compilation et rendu des templates |
| `filters.rs` | 25+ filtres compatibles Helm |
| `functions.rs` | Fonctions built-in |
| `suggestions.rs` | Suggestions d'erreur avec matching flou |

### Catégories de filtres

- **Sérialisation** : `toyaml`, `tojson`, `tojson_pretty`
- **Encodage** : `b64encode`, `b64decode`, `sha256`
- **Chaînes** : `quote`, `upper`, `lower`, `kebabcase`, `snakecase`
- **Indentation** : `indent`, `nindent`
- **Collections** : `keys`, `haskey`, `merge`, `dictsort`
- **Validation** : `required`, `empty`, `default`

## sherpack-kube

Intégration Kubernetes.

### Composants

| Composant | Description |
|-----------|-------------|
| `KubeClient<S>` | Client principal avec opérations de cycle de vie |
| `ResourceManager` | Server-Side Apply avec Discovery |
| `StorageDriver` | Trait de stockage des releases |
| `HookExecutor` | Gestion du cycle de vie des hooks |
| `HealthChecker` | Health Deployment/StatefulSet |
| `DiffEngine` | Diff three-way merge |

### Backends de stockage

| Backend | Stockage |
|---------|----------|
| `SecretsDriver` | Kubernetes Secrets |
| `ConfigMapDriver` | Kubernetes ConfigMaps |
| `FileDriver` | Système de fichiers local |
| `MockDriver` | En mémoire (tests) |

### États des releases

```
PendingInstall → Deployed
                    ↓
              PendingUpgrade → Deployed
                    ↓              ↓
              PendingRollback  Failed
                    ↓
                Deployed
                    ↓
              Uninstalling → Uninstalled
```

## sherpack-repo

Gestion des dépôts et dépendances.

### Composants

| Composant | Description |
|-----------|-------------|
| `RepositoryBackend` | Interface unifiée |
| `HttpBackend` | Dépôts HTTP avec ETag |
| `OciBackend` | Registres OCI |
| `FileBackend` | Répertoires locaux |
| `IndexCache` | Recherche SQLite FTS5 |
| `DependencyResolver` | Résolution des versions |
| `LockFile` | Pack.lock.yaml |

### Politiques de verrouillage

| Politique | Comportement |
|-----------|--------------|
| `Strict` | Version + SHA |
| `Version` | Version uniquement (défaut) |
| `SemverPatch` | Autorise les mises à jour patch |
| `SemverMinor` | Autorise les mises à jour mineures |

## Tests

| Crate | Tests | Type |
|-------|-------|------|
| sherpack-core | 19 | Unitaires |
| sherpack-engine | 43 | Unitaires |
| sherpack-kube | 107 | Unitaires + Mock |
| sherpack-repo | 42 | Unitaires |
| sherpack-cli | 71 | Intégration |
| **Total** | **282** | |

## Dépendances clés

| Dépendance | Utilisation |
|------------|-------------|
| `minijinja` | Moteur de template |
| `kube` | Client Kubernetes |
| `oci-distribution` | Registres OCI |
| `rusqlite` | SQLite FTS5 |
| `minisign` | Signatures |
| `clap` | Parsing CLI |
| `serde` | Sérialisation |
| `tokio` | Runtime async |
