# sherpack-core

Core types and utilities for Sherpack - the Kubernetes package manager with Jinja2 templates.

## Overview

`sherpack-core` provides the foundational data structures and utilities used throughout the Sherpack ecosystem. This crate is dependency-free from Kubernetes APIs, making it suitable for offline operations like templating, packaging, and validation.

## Features

- **Pack Definition** - Complete package metadata and dependency management
- **Values System** - Deep merge support for configuration values
- **Release Tracking** - Deployment state management
- **Schema Validation** - JSON Schema and simplified Sherpack format support
- **Archive Operations** - Create, extract, and verify `.tgz` packages
- **Manifest Generation** - SHA256 checksums for integrity verification

## Modules

### `pack` - Package Definition

The core `Pack` structure represents a Sherpack package (equivalent to a Helm Chart).

```rust
use sherpack_core::{Pack, PackMetadata, PackKind, Dependency, ResolvePolicy, LoadedPack};

// Load a pack from disk
let pack = LoadedPack::load("./my-pack")?;
println!("Pack: {} v{}", pack.pack.metadata.name, pack.pack.metadata.version);

// Access dependencies
for dep in &pack.pack.dependencies {
    println!("  Depends on: {} @ {}", dep.name, dep.version);
}
```

#### Pack.yaml Structure

```yaml
apiVersion: sherpack/v1
kind: application  # or 'library'

metadata:
  name: my-app
  version: 1.0.0
  description: My application
  appVersion: "2.0"
  keywords:
    - web
    - api
  maintainers:
    - name: John Doe
      email: john@example.com

dependencies:
  - name: redis
    version: "^17.0.0"
    repository: https://charts.bitnami.com/bitnami
    condition: redis.enabled      # Evaluated against values.yaml
    enabled: true                 # Static enable/disable
    resolve: when-enabled         # always | when-enabled | never
    alias: cache                  # Optional rename
```

#### Dependency Resolution Control

```rust
use sherpack_core::{Dependency, ResolvePolicy};

let dep = Dependency {
    name: "redis".to_string(),
    version: "^17.0.0".to_string(),
    repository: "https://example.com".to_string(),
    enabled: true,                          // Static flag
    condition: Some("redis.enabled".into()), // Dynamic condition
    resolve: ResolvePolicy::WhenEnabled,    // Resolution policy
    tags: vec![],
    alias: None,
};

// Check if dependency should be resolved
let values = serde_json::json!({ "redis": { "enabled": true } });
assert!(dep.should_resolve(&values));
```

### `values` - Configuration Values

Deep merge support for layered configuration.

```rust
use sherpack_core::{Values, parse_set_values};

// Load from file
let mut values = Values::from_file("values.yaml")?;

// Parse CLI --set arguments
let overrides = parse_set_values(&[
    "image.tag=v2.0".to_string(),
    "replicas=5".to_string(),
    "debug=true".to_string(),
])?;

// Deep merge (overrides win)
values.merge(&overrides);

// Access nested values
let tag = values.get("image.tag"); // Some("v2.0")
```

#### Merge Semantics

| Base | Overlay | Result |
|------|---------|--------|
| `{ a: 1 }` | `{ a: 2 }` | `{ a: 2 }` (scalar: replace) |
| `{ a: { b: 1 } }` | `{ a: { c: 2 } }` | `{ a: { b: 1, c: 2 } }` (object: merge) |
| `[1, 2]` | `[3, 4]` | `[3, 4]` (array: replace, not append) |

### `schema` - Values Validation

Dual-format schema support for validating configuration.

```rust
use sherpack_core::{Schema, SchemaValidator};

// Load schema (auto-detects format)
let schema = Schema::load("values.schema.yaml")?;
let validator = SchemaValidator::new(&schema)?;

// Validate values
let values = serde_json::json!({
    "replicas": 3,
    "image": { "tag": "latest" }
});

match validator.validate(&values) {
    Ok(result) if result.is_valid() => println!("Valid!"),
    Ok(result) => {
        for error in result.errors {
            println!("Error at {}: {}", error.path, error.message);
        }
    }
    Err(e) => println!("Schema error: {}", e),
}

// Extract defaults from schema
let defaults = schema.extract_defaults();
```

#### Supported Schema Formats

**JSON Schema (standard)**
```yaml
$schema: "https://json-schema.org/draft/2020-12/schema"
type: object
properties:
  replicas:
    type: integer
    minimum: 1
    default: 1
required:
  - replicas
```

**Sherpack Simplified Format**
```yaml
schemaVersion: "1"
properties:
  replicas:
    type: int
    required: true
    default: 1
    min: 1
    description: Number of pod replicas
  image.tag:
    type: string
    default: latest
```

### `release` - Deployment State

Track deployment lifecycle.

```rust
use sherpack_core::{Release, ReleaseStatus, ReleaseInfo};
use chrono::Utc;

let release = Release {
    name: "my-app".to_string(),
    namespace: "production".to_string(),
    revision: 5,
    status: ReleaseStatus::Deployed,
    info: ReleaseInfo {
        first_deployed: Utc::now(),
        last_deployed: Utc::now(),
        description: "Upgrade to v2.0".to_string(),
    },
};
```

#### Release Statuses

| Status | Description |
|--------|-------------|
| `Pending` | Installation/upgrade in progress |
| `Deployed` | Successfully deployed |
| `Failed` | Deployment failed |
| `Superseded` | Replaced by newer revision |
| `Uninstalling` | Uninstall in progress |
| `Uninstalled` | Successfully uninstalled |

### `context` - Template Context

Build the context passed to templates.

```rust
use sherpack_core::{TemplateContext, Release, Values, Pack};

let context = TemplateContext::new(
    &values,
    &release,
    &pack,
    "1.28.0", // Kubernetes version
);

// Serialize for template engine
let ctx_value = context.to_value()?;
// Contains: values, release, pack, capabilities
```

### `archive` - Package Archives

Create and manage `.tgz` packages with integrity verification.

```rust
use sherpack_core::{create_archive, extract_archive, verify_archive, list_archive};
use std::path::Path;

// Create archive
create_archive(
    Path::new("./my-pack"),
    Path::new("./my-pack-1.0.0.tgz"),
)?;

// List contents
for entry in list_archive(Path::new("./my-pack-1.0.0.tgz"))? {
    println!("{}: {} bytes", entry.path, entry.size);
}

// Extract
extract_archive(
    Path::new("./my-pack-1.0.0.tgz"),
    Path::new("./extracted/"),
)?;

// Verify integrity
let result = verify_archive(Path::new("./my-pack-1.0.0.tgz"))?;
if result.is_valid {
    println!("Archive integrity verified");
} else {
    for mismatch in result.mismatched {
        println!("Corrupted: {}", mismatch.path);
    }
}
```

### `manifest` - Integrity Manifests

SHA256 checksums for all pack files.

```rust
use sherpack_core::{Manifest, FileEntry};
use std::path::Path;

// Generate manifest for a directory
let manifest = Manifest::generate(Path::new("./my-pack"))?;

// Manifest format (YAML)
// sherpack-manifest-version: "1"
// files:
//   Pack.yaml: sha256:abc123...
//   values.yaml: sha256:def456...
//   templates/deployment.yaml: sha256:...

// Verify against manifest
let result = manifest.verify(Path::new("./my-pack"))?;
match result {
    VerificationResult::Valid => println!("All files match"),
    VerificationResult::Mismatch(files) => {
        for f in files {
            println!("Modified: {}", f.path);
        }
    }
}
```

## Error Handling

All errors are strongly typed using `thiserror`:

```rust
use sherpack_core::{CoreError, ValidationErrorInfo};

match pack_operation() {
    Err(CoreError::PackNotFound { path }) => {
        println!("Pack not found: {}", path.display());
    }
    Err(CoreError::ValidationFailed { errors }) => {
        for error in errors {
            println!("Validation error at {}: {}", error.path, error.message);
        }
    }
    Err(e) => println!("Error: {}", e),
    Ok(_) => {}
}
```

## Dependencies

- `serde` / `serde_yaml` / `serde_json` - Serialization
- `semver` - Semantic versioning
- `jsonschema` - JSON Schema validation
- `sha2` - SHA256 checksums
- `tar` / `flate2` - Archive operations
- `chrono` - Timestamps
- `thiserror` - Error handling

## License

MIT OR Apache-2.0
