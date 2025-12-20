---
id: installation
title: Installation
sidebar_position: 1
---

# Installation

## From Source

```bash
# Clone the repository
git clone https://github.com/alegeay/sherpack.git
cd sherpack

# Build release binary
cargo build --release

# Install to your PATH
cp target/release/sherpack ~/.local/bin/
```

## Requirements

- **Rust 1.85+** (Edition 2024)
- For Kubernetes operations: `kubectl` configured with cluster access

## Verify Installation

```bash
sherpack --version
# sherpack 0.1.0

sherpack --help
```

## Shell Completions

Generate shell completions for your shell:

```bash
# Bash
sherpack completions bash > ~/.local/share/bash-completion/completions/sherpack

# Zsh
sherpack completions zsh > ~/.zfunc/_sherpack

# Fish
sherpack completions fish > ~/.config/fish/completions/sherpack.fish
```
