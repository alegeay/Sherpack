# Changelog

All notable changes to Sherpack are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.4.0] - 2026-05-03

The "Helm-migration-ready" release. Closes the last hard blockers for
porting existing Helm charts (real `lookup()`, `fromJson`/`fromYaml`,
`helm test` runner, `helm repo index` equivalent), ships a dedicated
bilingual migration guide, and brings the Rust toolchain up to the
latest stable.

### Added

- **Cluster-aware `lookup()`** — Helm-compatible 4-arg signature
  (`apiVersion`, `kind`, `namespace`, `name`). Returns `{}` during
  `sherpack template` (deterministic, GitOps-safe), reads the live
  cluster during `sherpack install` / `sherpack upgrade`. Per-call
  timeout (default 5 s, configurable via `SHERPACK_LOOKUP_TIMEOUT_SECS`),
  per-render cache, and aggregated `tracing::warn!` notices when a
  non-empty result is consumed. The Helm chart converter now preserves
  `lookup(...)` calls instead of rewriting them to `{}`. Dedicated
  user guide at `docs/LOOKUP.md`.
- **`fromJson` / `fromYaml`** filters and global functions — parse
  inline JSON or YAML strings into values. Available as both
  `value | fromjson` and `fromjson(value)` to match Helm idioms.
- **`sherpack test <release>`** — runs the `test`-phase hooks of the
  latest stored release against the cluster. Reports per-hook PASS/FAIL
  with duration; exits non-zero on any failure.
- **`sherpack repo index <DIR>`** — generates a Helm-compatible
  `index.yaml` from a directory of `*.tgz` archives. Supports
  `--url <BASE>` to prepend an absolute URL and `--merge <PATH>` to
  add only new entries to an existing index.
- **`sherpack completion <SHELL>`** — emits a shell-completion script
  for `bash`, `zsh`, `fish`, `powershell`, or `elvish` (via
  `clap_complete`).
- **Structured logging** — the CLI installs `tracing-subscriber` with
  `EnvFilter`. Default level shows `warn` for sherpack crates and
  silences noisy deps; `--debug` raises sherpack crates to `debug`,
  and `RUST_LOG=...` overrides as expected.
- **Bilingual Helm-to-Sherpack migration guide** in the Docusaurus
  site (`/migrating-from-helm`, EN + FR), with animated terminal
  hero, SVG migration-flow diagram, syntax cheat sheet, command
  equivalence table, `lookup()` lifecycle SVG, parity matrix,
  gotchas, FAQ, and CTAs.
- **Dedicated `docs/LOOKUP.md`** user guide covering modes, return
  shapes, caching, timeouts, GitOps caveats, and migration patterns.
- **Sherpack branding** in the Docusaurus site — replaces the default
  Docusaurus logo and favicon with a custom 3-chevron mark in the
  Terminal Noir cyan palette.

### Changed

- **Bumped Rust MSRV from 1.88 to 1.95** (latest stable, January 2026).
- **`Engine` builder** gains `with_cluster_reader(reader)` and a
  `lookup_state()` accessor; the registered `lookup` function picks
  up the reader when present and falls back to a no-op `{}` otherwise.
- **`KubeClient` no longer holds a long-lived `Engine`** — install /
  upgrade build a per-render engine via `engine_with_lookup()` so each
  operation gets a fresh cache and reader.
- **Dependency updates** (deps batch #48 and individual merges):
  `kube` 2.0 → 3.1, `k8s-openapi` 0.26 → 0.27, `rand` 0.9 → 0.10
  (with `RngExt` import fix), `jsonschema` 0.38 → 0.45, `rusqlite`
  0.38 → 0.39, `minisign` 0.8 → 0.9, `react-dom` 19.2.3 → 19.2.4
  (website), `@easyops-cn/docusaurus-search-local` 0.52.2 → 0.55.1
  (website), `actions/download-artifact` v7 → v8 (CI).
- **Documentation overhaul** — `HELM_COMPARISON.md`,
  `HELM_FEATURE_GAP_ANALYSIS.md`, and `KILLER_FEATURES.md` reconciled
  against actual code state; per-feature status (🟢 / 🟡 / 🔴) added
  to `KILLER_FEATURES.md` to stop reading like marketing. Test counts
  refreshed everywhere (685 passing, 0 failing across 6 crates).

### Fixed

- `clippy::explicit-counter-loop` in `sherpack-engine/src/error.rs`
  (`calculate_span` now uses `enumerate()`).
- `clippy::unnecessary-sort-by` across all storage drivers
  (`sort_by_key(|r| Reverse(r.version))` instead of manual `cmp`).
- Typo in installation docs: `sherpack completions` →
  `sherpack completion` (matches the actual command name).

### Internal

- 685 tests passing (up from 654 at 0.3.0): added 8 cluster-reader
  tests, 7 fromjson/fromyaml tests, 4 transformer tests for the
  preserved `lookup`, 6 KubeClusterReader tests, 3 integration tests
  for `repo index`, plus the engine/lookup integration tests.
- Added `tracing` to `sherpack-kube` and `tracing` + `tracing-subscriber`
  to `sherpack-cli`.

## [0.3.0] - earlier

Initial public-facing release. See git history for details.

[0.4.0]: https://github.com/alegeay/Sherpack/releases/tag/v0.4.0
[0.3.0]: https://github.com/alegeay/Sherpack/releases/tag/v0.3.0
