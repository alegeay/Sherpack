# Sherpack v0.1.0 - Initial Release

**A modern Kubernetes package manager written in Rust with Jinja2 templating**

Sherpack is a simpler, faster alternative to Helm that replaces Go templates with the intuitive Jinja2 syntax. This initial release includes full lifecycle management for Kubernetes applications.

## Highlights

- **43,500+ lines of Rust** across 6 crates
- **600 tests** with cross-platform support (Linux, macOS, Windows)
- **Complete Helm feature parity** with significant improvements
- **Automatic Helm chart conversion** to Sherpack packs

---

## Core Features

### Jinja2 Template Engine (`sherpack-engine`)

**25+ Helm-compatible filters:**
- String: `quote`, `squote`, `upper`, `lower`, `title`, `trim`, `trimPrefix`, `trimSuffix`, `replace`, `split`, `join`
- Case conversion: `camelcase`, `snakecase`, `kebabcase`
- Encoding: `b64encode`, `b64decode`, `sha256`, `sha1`, `md5`
- YAML/JSON: `toyaml`, `tojson`, `fromyaml`, `fromjson`
- Formatting: `indent`, `nindent`, `wrap`
- Type: `int`, `float`, `string`, `list`, `dict`, `default`
- Collections: `keys`, `values`, `first`, `last`, `sortAlpha`, `uniq`, `compact`, `reverse`, `slice`

**Template functions:**
- `get(object, key, default)` - Safe nested access with defaults
- `ternary(condition, true_val, false_val)` - Inline conditionals
- `now()` - Current timestamp
- `uuidv4()` - Generate UUIDs
- `fail(message)` - Abort with error
- `tostring()`, `toint()`, `tofloat()` - Type conversions
- `include(template, context)` - Include other templates
- `lookup(apiVersion, kind, namespace, name)` - K8s resource lookup

**Files API (sandboxed):**
- `files.get(path)` - Read file content
- `files.glob(pattern)` - Match files by glob
- `files.lines(path)` - Read as lines
- `files.exists(path)` - Check existence

### Schema Validation (`sherpack-core`)

**Dual format support:**
- JSON Schema (draft-07)
- Sherpack simplified YAML format

**Features:**
- Automatic default value extraction
- Type validation with detailed errors
- `sherpack validate` command with JSON output
- Integration with `lint` and `template` commands
- `--skip-schema` flag to bypass validation

### Improved Error Messages

- **Fuzzy matching** with Levenshtein distance for typo suggestions
- **Context-aware suggestions** for undefined variables
- **Available keys display** when property not found
- **Multi-error collection** - continues after errors, shows all issues
- **Grouped errors** by template file

---

## Packaging & Signing

### Archive Management
- `sherpack package` - Create `.tar.gz` archives with SHA256 MANIFEST
- `sherpack inspect` - Show archive contents and checksums
- `sherpack verify` - Verify integrity and signatures
- Reproducible builds (deterministic timestamps)

### Cryptographic Signing (Minisign)
- `sherpack keygen` - Generate Ed25519 keypair
- `sherpack sign` - Sign archives with private key
- `sherpack verify -k pubkey` - Verify signatures
- Trusted comments with signer info

---

## Kubernetes Integration (`sherpack-kube`)

### Full Lifecycle Management
- `sherpack install` - Deploy to cluster
- `sherpack upgrade` - Update existing release
- `sherpack uninstall` - Remove release
- `sherpack rollback` - Revert to previous revision
- `sherpack list` - List installed releases
- `sherpack history` - Show release history
- `sherpack status` - Show release status
- `sherpack recover` - Recover stale release

### Server-Side Apply
- Uses Kubernetes Server-Side Apply for safer updates
- Automatic resource discovery
- Correct creation ordering (Namespace → RBAC → Workloads → Services)

### Storage Drivers
- **Secrets** (default) - Store release data in K8s Secrets
- **ConfigMap** - Store in ConfigMaps
- **File** - Local filesystem storage
- **Mock** - For testing without cluster
- Chunked storage for large releases (>1MB)

### Hooks (11 phases)
- `pre-install`, `post-install`
- `pre-upgrade`, `post-upgrade`
- `pre-rollback`, `post-rollback`
- `pre-delete`, `post-delete`
- `test`, `test-success`, `test-failure`
- Weight-based ordering
- Delete policies: `before-hook-creation`, `hook-succeeded`, `hook-failed`

### Health Checks
- Deployment/StatefulSet readiness monitoring
- HTTP probes
- Command probes
- Configurable timeouts

### CRD Handling
- Automatic CRD detection
- Safe upgrade strategies (Replace, ServerSideApply, Skip)
- Protection against accidental deletion
- Schema validation and migration analysis
- Dry-run support

### Three-Way Merge Diff
- Visual diff between current, desired, and live states
- Color-coded output
- Conflict detection

---

## Repository Management (`sherpack-repo`)

### Repository Backends
- **HTTP** - Standard HTTP repositories with ETag caching
- **OCI** - OCI registry support via `oci-distribution`
- **File** - Local filesystem repositories

### Commands
- `sherpack repo add/list/update/remove` - Manage repositories
- `sherpack search` - Search for packs (FTS5 full-text search)
- `sherpack pull` - Download pack from repository
- `sherpack push` - Push to OCI registry

### Dependency Management
- `sherpack dependency list` - List dependencies
- `sherpack dependency update` - Update dependencies
- `sherpack dependency build` - Download all dependencies
- `sherpack dependency tree` - Show dependency tree

### Lock Files (`Pack.lock.yaml`)
- Version policies: Strict, Version, SemverPatch, SemverMinor
- Diamond dependency conflict detection
- Reproducible builds

### Security
- Secure credential storage
- Cross-origin redirect protection
- SQLite cache with WAL mode

---

## Helm Chart Converter (`sherpack-convert`)

### Automatic Conversion
- `sherpack convert <chart> -o <output>` - Convert Helm chart to Sherpack
- Three-pass macro handling for complex templates
- PEG grammar parser for Go templates

### Supported Constructs
- `{{ .Values.x }}` → `{{ values.x }}`
- `{{ if }}...{{ else }}...{{ end }}` → `{% if %}...{% else %}...{% endif %}`
- `{{ range }}...{{ end }}` → `{% for %}...{% endfor %}`
- `{{ with }}...{{ end }}` → Context handling
- `{{ define "name" }}` → `{% macro name() %}`
- `{{ include "name" . }}` → `{{ name() }}`
- `{{ template "name" . }}` → `{{ name() }}`
- Pipeline syntax: `| quote`, `| indent 4`
- Helm functions: `default`, `required`, `toYaml`, `include`, etc.

### Chart.yaml to Pack.yaml
- Automatic metadata conversion
- Dependency mapping
- API version handling

---

## CLI Features

### Developer Experience
- Beautiful error messages with `miette`
- Progress indicators with `indicatif`
- Colored output
- JSON output for CI/CD (`--json` flag)

### Template Commands
- `sherpack template <release> <pack>` - Render templates
- `sherpack lint <pack>` - Validate pack structure
- `sherpack validate <pack>` - Validate against schema
- `sherpack show <pack>` - Display pack info
- `sherpack create <name>` - Scaffold new pack

### Value Overrides
- `-f values.yaml` - Override with file
- `--set key=value` - Override individual values
- `--set-string key=value` - Force string type
- `--set-json key='{"a":1}'` - JSON values

---

## Documentation

### Docusaurus Website
- Complete documentation in English and French
- Getting started guides
- API reference
- Architecture overview

### Markdown Documentation
- `docs/ARCHITECTURE.md` - System architecture
- `docs/CLI_REFERENCE.md` - Command reference
- `docs/TUTORIAL.md` - Step-by-step tutorial
- `docs/HELM_COMPARISON.md` - Helm vs Sherpack
- `docs/CONVERSION.md` - Chart conversion guide

---

## Platform Support

- **Linux** (x86_64, aarch64)
- **macOS** (x86_64, aarch64/Apple Silicon)
- **Windows** (x86_64)
- MSRV: Rust 1.88

---

## Project Structure

```
sherpack/
├── crates/
│   ├── sherpack-core/     # Core types, values, schema, archive
│   ├── sherpack-engine/   # Jinja2 template engine
│   ├── sherpack-convert/  # Helm chart converter
│   ├── sherpack-kube/     # Kubernetes integration
│   ├── sherpack-repo/     # Repository management
│   └── sherpack-cli/      # CLI application
├── docs/                  # Documentation
├── fixtures/              # Test fixtures
└── website/               # Docusaurus site
```

---

## Installation

### From Source
```bash
cargo install --path crates/sherpack-cli
```

### Pre-built Binaries
Download from GitHub Releases for your platform.

---

## Quick Start

```bash
# Create a new pack
sherpack create my-app

# Edit templates and values
cd my-app
# ... edit files ...

# Lint and validate
sherpack lint .
sherpack validate .

# Render templates
sherpack template my-release .

# Install to cluster
sherpack install my-release .

# Package for distribution
sherpack package . -o my-app-1.0.0.tar.gz
sherpack sign my-app-1.0.0.tar.gz -k ~/.sherpack/sherpack.key
```

---

## License

MIT License

---

## Acknowledgments

Built with:
- [MiniJinja](https://github.com/mitsuhiko/minijinja) - Jinja2 engine
- [kube-rs](https://github.com/kube-rs/kube) - Kubernetes client
- [pest](https://pest.rs/) - PEG parser
- [Clap](https://clap.rs/) - CLI framework
- [miette](https://github.com/zkat/miette) - Error reporting
