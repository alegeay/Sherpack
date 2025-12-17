# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Sherpack is a Kubernetes package manager written in Rust that uses Jinja2 templating (via MiniJinja) instead of Go templates. It's designed as a simpler, faster alternative to Helm.

## Build Commands

```bash
# Build (debug)
cargo build --workspace

# Build (release)
cargo build --release

# Run tests
cargo test --workspace

# Run a single test
cargo test --workspace test_name

# Lint
cargo clippy --workspace

# Format
cargo fmt --all

# Run CLI directly
cargo run -p sherpack-cli -- <command>

# Example: render templates
cargo run -p sherpack-cli -- template my-release fixtures/demo-pack
```

## Architecture

The project is a Cargo workspace with three crates:

### `sherpack-core`
Core types and data structures:
- `Pack` / `LoadedPack` - Package definition (equivalent to Helm Chart), loaded from `Pack.yaml`
- `Values` - Configuration values with deep merge support from `values.yaml` and `--set` flags
- `Release` - Deployment state (name, namespace)
- `TemplateContext` - Combined context passed to templates (`values`, `release`, `pack`, `capabilities`)

### `sherpack-engine`
MiniJinja-based template engine:
- `Engine` / `EngineBuilder` - Template compilation and rendering
- `filters.rs` - Helm-compatible filters: `toyaml`, `tojson`, `b64encode`, `indent`, `nindent`, `quote`, `kebabcase`, etc.
- `functions.rs` - Template functions: `get()`, `ternary()`, `now()`, `uuidv4()`, `fail()`

### `sherpack-cli`
CLI application using Clap:
- `template` - Render templates (main command)
- `create` - Scaffold new pack
- `lint` - Validate pack structure
- `show` - Display pack info

## Pack Structure

A pack (package) contains:
```
mypack/
├── Pack.yaml       # Required: metadata (name, version, description)
├── values.yaml     # Required: default configuration values
└── templates/      # Required: Jinja2 template files
```

## Template Context

Templates receive these variables:
- `values.*` - Merged values from values.yaml and overrides
- `release.name` - Release name from CLI
- `release.namespace` - Target namespace
- `pack.name` / `pack.version` - From Pack.yaml
- `capabilities.kubeVersion` - Kubernetes version

## Testing

Integration test fixtures are in `fixtures/`:
- `fixtures/simple-pack/` - Basic test fixture
- `fixtures/demo-pack/` - Comprehensive demo with all features

Snapshot tests use `insta` for YAML output verification.
