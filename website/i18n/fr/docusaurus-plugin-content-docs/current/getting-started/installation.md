---
id: installation
title: Installation
sidebar_position: 1
---

# Installation

Installez Sherpack sur votre système.

## Script d'installation (Recommandé)

```bash
curl -fsSL https://sherpack.dev/install.sh | sh
```

## Cargo (Rust)

```bash
cargo install sherpack
```

## Depuis les sources

```bash
git clone https://github.com/alegeay/sherpack.git
cd sherpack
cargo build --release
cp target/release/sherpack ~/.local/bin/
```

## Vérification

```bash
sherpack --version
# sherpack 0.1.0
```

## Configuration du shell

### Bash

```bash
echo 'eval "$(sherpack completion bash)"' >> ~/.bashrc
```

### Zsh

```bash
echo 'eval "$(sherpack completion zsh)"' >> ~/.zshrc
```

### Fish

```bash
sherpack completion fish > ~/.config/fish/completions/sherpack.fish
```

## Configuration Kubernetes

Sherpack utilise votre configuration kubectl existante :

```bash
# Vérifier la connexion
kubectl cluster-info

# Sherpack utilisera le même contexte
sherpack list
```

## Prochaines étapes

Maintenant que Sherpack est installé, passez au [Démarrage rapide](/docs/getting-started/quick-start).
