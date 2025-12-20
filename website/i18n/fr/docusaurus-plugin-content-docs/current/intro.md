---
id: intro
title: Introduction
sidebar_position: 1
slug: /
---

# Sherpack

Un gestionnaire de paquets Kubernetes **ultra-rapide** avec templating **Jinja2**.

## Pourquoi Sherpack ?

- **Syntaxe Jinja2** - Templating Python familier au lieu des templates Go
- **Ultra-rapide** - Binaire Rust de ~5MB, démarrage instantané
- **Cycle de vie complet** - Install, upgrade, rollback, uninstall avec hooks
- **Validation de schéma** - JSON Schema avec messages d'erreur utiles
- **Signature de paquets** - Signatures cryptographiques Minisign
- **Support OCI** - Push/pull depuis n'importe quel registre OCI

## Démarrage rapide

```bash
# Installer
curl -fsSL https://sherpack.dev/install.sh | sh

# Créer un pack
sherpack create myapp

# Rendre les templates
sherpack template my-release ./myapp

# Installer sur Kubernetes
sherpack install my-release ./myapp -n production
```

## Comparaison avec Helm

| Fonctionnalité | Sherpack | Helm |
|----------------|----------|------|
| Templating | Jinja2 | Go templates |
| Taille binaire | ~5MB | ~50MB |
| Runtime | Aucun | Aucun |
| Validation | JSON Schema | values.schema.json |
| Signature | Minisign | GPG |
| Registres | HTTP + OCI | HTTP + OCI |

## Structure d'un Pack

```
myapp/
├── Pack.yaml           # Métadonnées du pack
├── values.yaml         # Valeurs par défaut
├── values.schema.yaml  # Schéma de validation (optionnel)
└── templates/          # Templates Jinja2
    ├── deployment.yaml
    ├── service.yaml
    └── _helpers.tpl
```

## Prochaines étapes

- [Installation](/docs/getting-started/installation) - Installer Sherpack
- [Démarrage rapide](/docs/getting-started/quick-start) - Votre premier déploiement
- [Référence CLI](/docs/cli-reference) - Toutes les commandes
