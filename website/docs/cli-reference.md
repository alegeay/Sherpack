---
id: cli-reference
title: CLI Reference
sidebar_position: 100
---

# CLI Reference

Complete reference for all Sherpack commands.

## Global Options

All commands support:

| Option | Description |
|--------|-------------|
| `--debug` | Enable debug output |
| `-h, --help` | Print help |
| `-V, --version` | Print version |

---

## Templating Commands

### template

Render templates to stdout or files.

```bash
sherpack template <NAME> <PACK> [OPTIONS]
```

| Option | Description |
|--------|-------------|
| `-n, --namespace <NS>` | Target namespace [default: default] |
| `-f, --values <FILE>` | Values file (repeatable) |
| `--set <KEY=VALUE>` | Override values (repeatable) |
| `-o, --output <DIR>` | Output directory |
| `-s, --show-only <NAME>` | Only render specified template |
| `--show-values` | Display computed values |
| `--skip-schema` | Skip schema validation |

### lint

Validate pack structure.

```bash
sherpack lint <PACK> [OPTIONS]
```

| Option | Description |
|--------|-------------|
| `--strict` | Fail on undefined variables |
| `--skip-schema` | Skip schema validation |

### validate

Validate values against schema.

```bash
sherpack validate <PACK> [OPTIONS]
```

| Option | Description |
|--------|-------------|
| `-f, --values <FILE>` | Additional values file |
| `--set <KEY=VALUE>` | Override values |
| `--json` | JSON output |
| `-v, --verbose` | Verbose output |

### show

Display pack information.

```bash
sherpack show <PACK> [OPTIONS]
```

| Option | Description |
|--------|-------------|
| `--all` | Show all information |
| `--values` | Show default values |

### create

Scaffold a new pack.

```bash
sherpack create <NAME> [OPTIONS]
```

| Option | Description |
|--------|-------------|
| `-o, --output <DIR>` | Output directory |

### convert

Convert Helm chart to Sherpack pack.

```bash
sherpack convert <CHART> [OPTIONS]
```

| Option | Description |
|--------|-------------|
| `-o, --output <DIR>` | Output directory (default: `<chartname>-sherpack`) |
| `--force` | Overwrite existing output |
| `--dry-run` | Preview without writing |
| `-v, --verbose` | Detailed output |

**Conversion Examples:**

| Go Template | Jinja2 |
|-------------|--------|
| `{{ .Values.name }}` | `{{ values.name }}` |
| `{{ include "helper" . }}` | `{{ helper() }}` |
| `{{- if .Values.enabled }}` | `{% if values.enabled %}` |
| `{{ range .Values.items }}` | `{% for item in values.items %}` |
| `{{ .Release.Name }}` | `{{ release.name }}` |
| `{{ default "foo" .Values.x }}` | `{{ values.x \| default("foo") }}` |

---

## Packaging Commands

### package

Create archive from pack.

```bash
sherpack package <PACK> [OPTIONS]
```

| Option | Description |
|--------|-------------|
| `-o, --output <FILE>` | Output file |

### inspect

Show archive contents.

```bash
sherpack inspect <ARCHIVE> [OPTIONS]
```

| Option | Description |
|--------|-------------|
| `--manifest` | Show raw manifest |
| `--checksums` | Show file checksums |

### keygen

Generate signing keypair.

```bash
sherpack keygen [OPTIONS]
```

| Option | Description |
|--------|-------------|
| `-o, --output <DIR>` | Output directory |
| `--no-password` | Don't encrypt private key |
| `--force` | Overwrite existing keys |

### sign

Sign archive.

```bash
sherpack sign <ARCHIVE> -k <KEY> [OPTIONS]
```

| Option | Description |
|--------|-------------|
| `-k, --key <FILE>` | Private key file |
| `-c, --comment <TEXT>` | Trusted comment |

### verify

Verify archive.

```bash
sherpack verify <ARCHIVE> [OPTIONS]
```

| Option | Description |
|--------|-------------|
| `-k, --key <FILE>` | Public key file |
| `--require-signature` | Fail if no signature |

---

## Kubernetes Commands

### install

Install pack to cluster.

```bash
sherpack install <NAME> <PACK> [OPTIONS]
```

| Option | Description |
|--------|-------------|
| `-n, --namespace <NS>` | Namespace |
| `-f, --values <FILE>` | Values file |
| `--set <KEY=VALUE>` | Override values |
| `--wait` | Wait for ready |
| `--timeout <DURATION>` | Wait timeout [default: 5m] |
| `--atomic` | Rollback on failure |
| `--dry-run` | Don't apply |
| `--create-namespace` | Create namespace |

### upgrade

Upgrade existing release.

```bash
sherpack upgrade <NAME> <PACK> [OPTIONS]
```

| Option | Description |
|--------|-------------|
| `-n, --namespace <NS>` | Namespace |
| `-f, --values <FILE>` | Values file |
| `--set <KEY=VALUE>` | Override values |
| `--wait` | Wait for ready |
| `--timeout <DURATION>` | Wait timeout |
| `--atomic` | Rollback on failure |
| `--dry-run` | Don't apply |
| `--diff` | Show diff |
| `--reuse-values` | Reuse previous values |
| `--reset-values` | Reset to defaults |
| `--install` | Install if not exists |

### uninstall

Remove release.

```bash
sherpack uninstall <NAME> [OPTIONS]
```

| Option | Description |
|--------|-------------|
| `-n, --namespace <NS>` | Namespace |
| `--keep-history` | Keep release records |
| `--wait` | Wait for deletion |
| `--dry-run` | Don't delete |

### rollback

Rollback to previous revision.

```bash
sherpack rollback <NAME> <REVISION> [OPTIONS]
```

| Option | Description |
|--------|-------------|
| `-n, --namespace <NS>` | Namespace |
| `--wait` | Wait for rollback |
| `--dry-run` | Don't apply |

### list

List releases.

```bash
sherpack list [OPTIONS]
```

| Option | Description |
|--------|-------------|
| `-n, --namespace <NS>` | Filter by namespace |
| `-A, --all-namespaces` | All namespaces |
| `-a, --all` | Include superseded |
| `-o, --output <FMT>` | Output format |

### history

Show release history.

```bash
sherpack history <NAME> [OPTIONS]
```

| Option | Description |
|--------|-------------|
| `-n, --namespace <NS>` | Namespace |
| `--max <N>` | Maximum revisions |

### status

Show release status.

```bash
sherpack status <NAME> [OPTIONS]
```

| Option | Description |
|--------|-------------|
| `-n, --namespace <NS>` | Namespace |
| `--show-resources` | Show resource status |

### recover

Recover stale release.

```bash
sherpack recover <NAME> [OPTIONS]
```

| Option | Description |
|--------|-------------|
| `-n, --namespace <NS>` | Namespace |

---

## Repository Commands

### repo add

Add repository.

```bash
sherpack repo add <NAME> <URL> [OPTIONS]
```

| Option | Description |
|--------|-------------|
| `--username <USER>` | Username |
| `--password <PASS>` | Password |
| `--token <TOKEN>` | Token |

### repo list

List repositories.

```bash
sherpack repo list [OPTIONS]
```

| Option | Description |
|--------|-------------|
| `--auth` | Show auth status |

### repo update

Update repository index.

```bash
sherpack repo update [NAME]
```

### repo remove

Remove repository.

```bash
sherpack repo remove <NAME>
```

### search

Search for packs.

```bash
sherpack search <QUERY> [OPTIONS]
```

| Option | Description |
|--------|-------------|
| `-r, --repo <NAME>` | Search specific repo |
| `--versions` | Show all versions |
| `--json` | JSON output |

### pull

Download pack.

```bash
sherpack pull <PACK> [OPTIONS]
```

| Option | Description |
|--------|-------------|
| `--ver <VERSION>` | Specific version |
| `-o, --output <PATH>` | Output path |
| `--untar` | Extract to directory |

### push

Push to OCI registry.

```bash
sherpack push <ARCHIVE> <DESTINATION>
```

---

## Dependency Commands

### dependency list

List dependencies.

```bash
sherpack dependency list <PACK>
```

### dependency update

Resolve and lock dependencies.

```bash
sherpack dependency update <PACK> [OPTIONS]
```

| Option | Description |
|--------|-------------|
| `--policy <POLICY>` | Lock policy |

### dependency build

Download locked dependencies.

```bash
sherpack dependency build <PACK> [OPTIONS]
```

| Option | Description |
|--------|-------------|
| `--verify` | Verify checksums |

### dependency tree

Show dependency tree.

```bash
sherpack dependency tree <PACK>
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
