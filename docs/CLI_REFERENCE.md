# Sherpack CLI Reference

Complete reference for all Sherpack commands.

## Global Options

All commands support these options:

| Option | Description |
|--------|-------------|
| `--debug` | Enable debug output |
| `-h, --help` | Print help information |
| `-V, --version` | Print version |

---

## Templating Commands

### `sherpack template`

Render templates to stdout or files.

```bash
sherpack template <NAME> <PACK> [OPTIONS]
```

**Arguments:**
| Argument | Description |
|----------|-------------|
| `<NAME>` | Release name (used in templates as `release.name`) |
| `<PACK>` | Path to pack directory or archive |

**Options:**
| Option | Description |
|--------|-------------|
| `-n, --namespace <NS>` | Target namespace [default: default] |
| `-f, --values <FILE>` | Values file (can be repeated) |
| `--set <KEY=VALUE>` | Override values (can be repeated) |
| `-o, --output <DIR>` | Output directory (instead of stdout) |
| `-s, --show-only <NAME>` | Only render specified template |
| `--show-values` | Display computed values |
| `--skip-schema` | Skip schema validation |
| `--kube-version <VER>` | Kubernetes version [default: 1.28.0] |

**Examples:**
```bash
# Basic rendering
sherpack template myapp ./mypack

# With namespace and overrides
sherpack template myapp ./mypack -n production --set app.replicas=5

# Multiple value files
sherpack template myapp ./mypack -f base.yaml -f production.yaml

# Output to directory
sherpack template myapp ./mypack -o ./manifests/

# Show only one template
sherpack template myapp ./mypack -s deployment
```

---

### `sherpack lint`

Validate pack structure and templates.

```bash
sherpack lint <PACK> [OPTIONS]
```

**Options:**
| Option | Description |
|--------|-------------|
| `--strict` | Fail on undefined variables |
| `--skip-schema` | Skip schema validation |

**Examples:**
```bash
# Basic linting
sherpack lint ./mypack

# Strict mode (fail on undefined)
sherpack lint ./mypack --strict
```

**Checks performed:**
- Pack.yaml exists and is valid YAML
- Required metadata fields present (name, version)
- values.yaml exists and is valid YAML
- templates/ directory exists
- Template syntax is valid
- Schema validation (if schema exists)

---

### `sherpack validate`

Validate values against JSON Schema.

```bash
sherpack validate <PACK> [OPTIONS]
```

**Options:**
| Option | Description |
|--------|-------------|
| `-f, --values <FILE>` | Additional values file |
| `--set <KEY=VALUE>` | Override values |
| `--json` | Output results as JSON |
| `-v, --verbose` | Show detailed validation |

**Examples:**
```bash
# Basic validation
sherpack validate ./mypack

# With overrides
sherpack validate ./mypack --set app.replicas=100

# JSON output for CI
sherpack validate ./mypack --json
```

**JSON Output Format:**
```json
{
  "valid": false,
  "pack": "mypack",
  "version": "1.0.0",
  "errors": [
    {
      "path": "app.replicas",
      "message": "100 is greater than maximum 10"
    }
  ]
}
```

---

### `sherpack show`

Display pack information.

```bash
sherpack show <PACK> [OPTIONS]
```

**Options:**
| Option | Description |
|--------|-------------|
| `--all` | Show all information |
| `--values` | Show default values |
| `--readme` | Show README content |

**Examples:**
```bash
# Show pack metadata
sherpack show ./mypack

# Show everything
sherpack show ./mypack --all

# Show values only
sherpack show ./mypack --values
```

---

### `sherpack create`

Scaffold a new pack.

```bash
sherpack create <NAME> [OPTIONS]
```

**Options:**
| Option | Description |
|--------|-------------|
| `-o, --output <DIR>` | Output directory [default: .] |
| `--starter <TYPE>` | Starter template (basic, web, api) |

**Examples:**
```bash
# Create in current directory
sherpack create myapp

# Create in specific directory
sherpack create myapp -o ~/projects/
```

**Generated structure:**
```
myapp/
├── Pack.yaml
├── values.yaml
├── values.schema.yaml
└── templates/
    ├── deployment.yaml
    └── service.yaml
```

---

## Packaging Commands

### `sherpack package`

Create archive from pack directory.

```bash
sherpack package <PACK> [OPTIONS]
```

**Options:**
| Option | Description |
|--------|-------------|
| `-o, --output <FILE>` | Output file [default: {name}-{version}.tar.gz] |

**Examples:**
```bash
# Package with default name
sherpack package ./mypack
# Creates: mypack-1.0.0.tar.gz

# Custom output
sherpack package ./mypack -o /tmp/mypack.tar.gz
```

**Archive contents:**
```
MANIFEST              # SHA256 checksums
Pack.yaml
values.yaml
values.schema.yaml    # If present
templates/
    deployment.yaml
    service.yaml
```

---

### `sherpack inspect`

Show archive contents and manifest.

```bash
sherpack inspect <ARCHIVE> [OPTIONS]
```

**Options:**
| Option | Description |
|--------|-------------|
| `--manifest` | Show raw manifest |
| `--checksums` | Show file checksums |

**Examples:**
```bash
# Basic inspection
sherpack inspect mypack-1.0.0.tar.gz

# Show checksums
sherpack inspect mypack-1.0.0.tar.gz --checksums

# Show raw manifest
sherpack inspect mypack-1.0.0.tar.gz --manifest
```

---

### `sherpack keygen`

Generate Minisign signing keypair.

```bash
sherpack keygen [OPTIONS]
```

**Options:**
| Option | Description |
|--------|-------------|
| `-o, --output <DIR>` | Output directory [default: .] |
| `--no-password` | Don't encrypt private key |
| `--force` | Overwrite existing keys |

**Examples:**
```bash
# Generate with password
sherpack keygen -o ~/.sherpack/keys

# Generate without password (for CI)
sherpack keygen -o ~/.sherpack/keys --no-password
```

**Generated files:**
- `sherpack.key` - Private key (keep secret!)
- `sherpack.pub` - Public key (distribute)

---

### `sherpack sign`

Sign archive with private key.

```bash
sherpack sign <ARCHIVE> -k <KEY> [OPTIONS]
```

**Options:**
| Option | Description |
|--------|-------------|
| `-k, --key <FILE>` | Private key file (required) |
| `-c, --comment <TEXT>` | Trusted comment |

**Examples:**
```bash
# Sign archive
sherpack sign mypack-1.0.0.tar.gz -k ~/.sherpack/keys/sherpack.key

# With comment
sherpack sign mypack-1.0.0.tar.gz -k key.key -c "Release v1.0.0"
```

**Output:**
Creates `mypack-1.0.0.tar.gz.minisig` signature file.

---

### `sherpack verify`

Verify archive integrity and signature.

```bash
sherpack verify <ARCHIVE> [OPTIONS]
```

**Options:**
| Option | Description |
|--------|-------------|
| `-k, --key <FILE>` | Public key file |
| `--require-signature` | Fail if no signature |

**Examples:**
```bash
# Verify integrity only
sherpack verify mypack-1.0.0.tar.gz

# Verify with signature
sherpack verify mypack-1.0.0.tar.gz -k sherpack.pub

# Require signature
sherpack verify mypack-1.0.0.tar.gz -k sherpack.pub --require-signature
```

**Output:**
```
Verifying: mypack-1.0.0.tar.gz

Integrity check:     [OK] All file checksums match
Signature check:     [OK] Signature valid (key: RW...)

Archive verified successfully
```

---

## Kubernetes Commands

### `sherpack install`

Install pack to Kubernetes cluster.

```bash
sherpack install <NAME> <PACK> [OPTIONS]
```

**Arguments:**
| Argument | Description |
|----------|-------------|
| `<NAME>` | Release name |
| `<PACK>` | Path to pack or archive |

**Options:**
| Option | Description |
|--------|-------------|
| `-n, --namespace <NS>` | Target namespace [default: default] |
| `-f, --values <FILE>` | Values file (repeatable) |
| `--set <KEY=VALUE>` | Override values (repeatable) |
| `--wait` | Wait for resources to be ready |
| `--timeout <DURATION>` | Wait timeout [default: 5m] |
| `--atomic` | Rollback on failure |
| `--dry-run` | Don't apply, just render |
| `--create-namespace` | Create namespace if missing |

**Examples:**
```bash
# Basic install
sherpack install myapp ./mypack

# Production install
sherpack install myapp ./mypack \
  -n production \
  -f production-values.yaml \
  --wait \
  --atomic

# Dry run
sherpack install myapp ./mypack --dry-run
```

---

### `sherpack upgrade`

Upgrade existing release.

```bash
sherpack upgrade <NAME> <PACK> [OPTIONS]
```

**Options:**
| Option | Description |
|--------|-------------|
| `-n, --namespace <NS>` | Namespace |
| `-f, --values <FILE>` | Values file |
| `--set <KEY=VALUE>` | Override values |
| `--wait` | Wait for ready |
| `--timeout <DURATION>` | Wait timeout |
| `--atomic` | Rollback on failure |
| `--reuse-values` | Reuse previous values |
| `--reset-values` | Reset to defaults |
| `--install` | Install if not exists |
| `--dry-run` | Don't apply |
| `--diff` | Show diff before applying |

**Examples:**
```bash
# Upgrade with new values
sherpack upgrade myapp ./mypack --set app.replicas=5

# Upgrade, keeping previous values
sherpack upgrade myapp ./mypack --reuse-values --set app.tag=v2

# Show diff before upgrade
sherpack upgrade myapp ./mypack --diff

# Install or upgrade
sherpack upgrade myapp ./mypack --install
```

---

### `sherpack uninstall`

Remove release from cluster.

```bash
sherpack uninstall <NAME> [OPTIONS]
```

**Options:**
| Option | Description |
|--------|-------------|
| `-n, --namespace <NS>` | Namespace |
| `--keep-history` | Keep release history |
| `--dry-run` | Don't delete |
| `--wait` | Wait for deletion |
| `--timeout <DURATION>` | Wait timeout |

**Examples:**
```bash
# Uninstall
sherpack uninstall myapp

# Keep history for audit
sherpack uninstall myapp --keep-history

# Dry run
sherpack uninstall myapp --dry-run
```

---

### `sherpack rollback`

Rollback to previous revision.

```bash
sherpack rollback <NAME> <REVISION> [OPTIONS]
```

**Arguments:**
| Argument | Description |
|----------|-------------|
| `<NAME>` | Release name |
| `<REVISION>` | Target revision number |

**Options:**
| Option | Description |
|--------|-------------|
| `-n, --namespace <NS>` | Namespace |
| `--wait` | Wait for rollback |
| `--timeout <DURATION>` | Wait timeout |
| `--dry-run` | Don't apply |

**Examples:**
```bash
# Rollback to revision 1
sherpack rollback myapp 1

# Rollback with wait
sherpack rollback myapp 1 --wait
```

---

### `sherpack list`

List installed releases.

```bash
sherpack list [OPTIONS]
```

**Options:**
| Option | Description |
|--------|-------------|
| `-n, --namespace <NS>` | Filter by namespace |
| `-A, --all-namespaces` | All namespaces |
| `-a, --all` | Include superseded/uninstalled |
| `-o, --output <FMT>` | Output format (table, json, yaml) |

**Examples:**
```bash
# List in current namespace
sherpack list

# All namespaces
sherpack list -A

# JSON output
sherpack list -o json
```

---

### `sherpack history`

Show release history.

```bash
sherpack history <NAME> [OPTIONS]
```

**Options:**
| Option | Description |
|--------|-------------|
| `-n, --namespace <NS>` | Namespace |
| `--max <N>` | Maximum revisions to show |

**Examples:**
```bash
# Show history
sherpack history myapp

# Last 5 revisions
sherpack history myapp --max 5
```

---

### `sherpack status`

Show release status.

```bash
sherpack status <NAME> [OPTIONS]
```

**Options:**
| Option | Description |
|--------|-------------|
| `-n, --namespace <NS>` | Namespace |
| `-o, --output <FMT>` | Output format |
| `--show-resources` | Show resource status |

**Examples:**
```bash
# Show status
sherpack status myapp

# With resources
sherpack status myapp --show-resources
```

---

### `sherpack recover`

Recover stale release (stuck in pending state).

```bash
sherpack recover <NAME> [OPTIONS]
```

**Options:**
| Option | Description |
|--------|-------------|
| `-n, --namespace <NS>` | Namespace |

**Examples:**
```bash
# Recover stale release
sherpack recover myapp
```

---

## Repository Commands

### `sherpack repo add`

Add a repository.

```bash
sherpack repo add <NAME> <URL> [OPTIONS]
```

**Options:**
| Option | Description |
|--------|-------------|
| `--username <USER>` | Username for auth |
| `--password <PASS>` | Password for auth |
| `--token <TOKEN>` | Token for auth |

**Examples:**
```bash
# Add HTTP repository
sherpack repo add stable https://charts.example.com

# With authentication
sherpack repo add private https://charts.example.com \
  --username admin \
  --password secret

# OCI registry
sherpack repo add oci oci://registry.example.com/charts
```

---

### `sherpack repo list`

List configured repositories.

```bash
sherpack repo list [OPTIONS]
```

**Options:**
| Option | Description |
|--------|-------------|
| `--auth` | Show authentication status |

**Examples:**
```bash
# List repos
sherpack repo list

# Show auth status
sherpack repo list --auth
```

---

### `sherpack repo update`

Update repository index.

```bash
sherpack repo update [NAME]
```

**Examples:**
```bash
# Update all repos
sherpack repo update

# Update specific repo
sherpack repo update stable
```

---

### `sherpack repo remove`

Remove a repository.

```bash
sherpack repo remove <NAME>
```

**Examples:**
```bash
sherpack repo remove stable
```

---

### `sherpack search`

Search for packs across repositories.

```bash
sherpack search <QUERY> [OPTIONS]
```

**Options:**
| Option | Description |
|--------|-------------|
| `-r, --repo <NAME>` | Search in specific repo |
| `--versions` | Show all versions |
| `--json` | JSON output |

**Examples:**
```bash
# Search all repos
sherpack search nginx

# Search specific repo
sherpack search nginx --repo stable

# Show versions
sherpack search nginx --versions
```

---

### `sherpack pull`

Download pack from repository.

```bash
sherpack pull <PACK> [OPTIONS]
```

**Arguments:**
| Argument | Description |
|----------|-------------|
| `<PACK>` | Pack reference (repo/name:version or oci://...) |

**Options:**
| Option | Description |
|--------|-------------|
| `--ver <VERSION>` | Specific version |
| `-o, --output <PATH>` | Output file/directory |
| `--untar` | Extract to directory |

**Examples:**
```bash
# Pull from repo
sherpack pull stable/nginx:1.0.0

# Pull and extract
sherpack pull stable/nginx --untar -o ./nginx/

# Pull from OCI
sherpack pull oci://registry.example.com/charts/nginx:1.0.0
```

---

### `sherpack push`

Push archive to OCI registry.

```bash
sherpack push <ARCHIVE> <DESTINATION>
```

**Arguments:**
| Argument | Description |
|----------|-------------|
| `<ARCHIVE>` | Archive file to push |
| `<DESTINATION>` | OCI destination (oci://registry/repo:tag) |

**Examples:**
```bash
sherpack push myapp-1.0.0.tar.gz oci://registry.example.com/charts/myapp:1.0.0
```

---

## Dependency Commands

### `sherpack dependency list`

List pack dependencies.

```bash
sherpack dependency list <PACK>
```

**Examples:**
```bash
sherpack dependency list ./mypack
```

---

### `sherpack dependency update`

Resolve and lock dependencies.

```bash
sherpack dependency update <PACK> [OPTIONS]
```

**Options:**
| Option | Description |
|--------|-------------|
| `--policy <POLICY>` | Lock policy (strict, version, semver-patch, semver-minor) |

**Examples:**
```bash
# Update with default policy
sherpack dependency update ./mypack

# Strict policy (SHA verification)
sherpack dependency update ./mypack --policy strict
```

**Creates/updates:** `Pack.lock.yaml`

---

### `sherpack dependency build`

Download locked dependencies.

```bash
sherpack dependency build <PACK> [OPTIONS]
```

**Options:**
| Option | Description |
|--------|-------------|
| `--verify` | Verify checksums |

**Examples:**
```bash
# Download dependencies
sherpack dependency build ./mypack

# With verification
sherpack dependency build ./mypack --verify
```

**Downloads to:** `packs/` directory

---

### `sherpack dependency tree`

Show dependency tree.

```bash
sherpack dependency tree <PACK>
```

**Examples:**
```bash
sherpack dependency tree ./mypack
```

**Output:**
```
myapp@1.0.0
├── redis@7.0.0
│   └── common@1.0.0
└── postgresql@15.0.0
    └── common@1.0.0
```

---

## Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Success |
| 1 | General error |
| 2 | Validation error |
| 3 | Template error |
| 4 | IO error |
| 5 | Kubernetes error |

---

## Environment Variables

| Variable | Description |
|----------|-------------|
| `KUBECONFIG` | Kubernetes config file |
| `SHERPACK_NAMESPACE` | Default namespace |
| `SHERPACK_DEBUG` | Enable debug output |
| `SHERPACK_CONFIG` | Config directory |
| `SHERPACK_CACHE` | Cache directory |

---

## Configuration Files

### Repository Configuration

Location: `~/.config/sherpack/repositories.yaml`

```yaml
repositories:
  - name: stable
    url: https://charts.example.com
    type: http
  - name: oci
    url: oci://registry.example.com/charts
    type: oci
```

### Search Cache

Location: `~/.cache/sherpack/index.db` (SQLite FTS5)
