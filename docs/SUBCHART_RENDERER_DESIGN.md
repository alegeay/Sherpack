# SubchartRenderer - Design Document

## Overview

This document describes the design and implementation plan for automatic subchart rendering in Sherpack, providing Helm-compatible behavior while maintaining Rust idioms.

## Goals

1. **Automatic subchart discovery** - Find subcharts in `charts/` directory
2. **Condition-based inclusion** - Enable/disable subcharts via values
3. **Proper value scoping** - Each subchart sees only its scoped values
4. **Recursive support** - Handle subcharts of subcharts
5. **Manifest combination** - Merge outputs with proper namespacing

## Non-Goals (Phase 1)

- Hook orchestration across subcharts (complex, defer to later)
- Remote subchart fetching during render (handled by `sherpack-repo`)
- CRD ordering across subcharts

---

## API Design

### New Types in `sherpack-engine`

```rust
// src/subchart.rs

use std::path::PathBuf;
use sherpack_core::{LoadedPack, Dependency, Values};

/// Configuration for subchart rendering
#[derive(Debug, Clone)]
pub struct SubchartConfig {
    /// Maximum depth for nested subcharts (default: 10)
    pub max_depth: usize,

    /// Directory name for subcharts (default: "charts")
    pub subcharts_dir: String,

    /// Whether to fail on missing subcharts referenced in dependencies
    pub strict: bool,
}

impl Default for SubchartConfig {
    fn default() -> Self {
        Self {
            max_depth: 10,
            subcharts_dir: "charts".to_string(),
            strict: false,
        }
    }
}

/// Information about a discovered subchart
#[derive(Debug)]
pub struct SubchartInfo {
    /// Effective name (alias if set, otherwise directory name)
    pub name: String,

    /// Path to the subchart directory
    pub path: PathBuf,

    /// Loaded pack (lazily loaded)
    pub pack: LoadedPack,

    /// Whether enabled based on condition evaluation
    pub enabled: bool,

    /// The dependency definition from parent Pack.yaml (if any)
    pub dependency: Option<Dependency>,

    /// Reason if disabled
    pub disabled_reason: Option<String>,
}

/// Result of subchart discovery
#[derive(Debug, Default)]
pub struct DiscoveryResult {
    /// Successfully discovered subcharts
    pub subcharts: Vec<SubchartInfo>,

    /// Warnings during discovery (e.g., invalid Pack.yaml)
    pub warnings: Vec<String>,

    /// Missing subcharts referenced in dependencies
    pub missing: Vec<String>,
}
```

### PackRenderer

```rust
// src/pack_renderer.rs

use indexmap::IndexMap;
use crate::{Engine, RenderResult, RenderResultWithReport, RenderReport};
use crate::subchart::{SubchartConfig, SubchartInfo, DiscoveryResult};
use sherpack_core::{LoadedPack, TemplateContext, Values};

/// Orchestrates rendering of a pack and its subcharts
pub struct PackRenderer {
    engine: Engine,
    config: SubchartConfig,
}

impl PackRenderer {
    /// Create a new PackRenderer with default config
    pub fn new(engine: Engine) -> Self {
        Self {
            engine,
            config: SubchartConfig::default(),
        }
    }

    /// Create with custom configuration
    pub fn with_config(engine: Engine, config: SubchartConfig) -> Self {
        Self { engine, config }
    }

    /// Create a builder for more options
    pub fn builder() -> PackRendererBuilder {
        PackRendererBuilder::default()
    }

    /// Discover subcharts in a pack
    pub fn discover_subcharts(
        &self,
        pack: &LoadedPack,
        values: &Values,
    ) -> DiscoveryResult {
        // Implementation below
    }

    /// Render a pack and all enabled subcharts
    ///
    /// This is the main entry point. It:
    /// 1. Discovers all subcharts
    /// 2. Evaluates conditions against values
    /// 3. Renders enabled subcharts with scoped values
    /// 4. Renders the parent pack
    /// 5. Combines all manifests
    pub fn render(
        &self,
        pack: &LoadedPack,
        context: &TemplateContext,
    ) -> Result<RenderResult, EngineError> {
        let result = self.render_collect_errors(pack, context);
        if result.report.has_errors() {
            // Return first error
            Err(/* ... */)
        } else {
            Ok(RenderResult {
                manifests: result.manifests,
                notes: result.notes,
            })
        }
    }

    /// Render with full error collection
    pub fn render_collect_errors(
        &self,
        pack: &LoadedPack,
        context: &TemplateContext,
    ) -> RenderResultWithReport {
        self.render_recursive(pack, context, 0)
    }

    /// Internal recursive renderer
    fn render_recursive(
        &self,
        pack: &LoadedPack,
        context: &TemplateContext,
        depth: usize,
    ) -> RenderResultWithReport {
        // Implementation below
    }
}

/// Builder for PackRenderer
#[derive(Default)]
pub struct PackRendererBuilder {
    strict_mode: bool,
    max_depth: Option<usize>,
    subcharts_dir: Option<String>,
}

impl PackRendererBuilder {
    pub fn strict(mut self, strict: bool) -> Self {
        self.strict_mode = strict;
        self
    }

    pub fn max_depth(mut self, depth: usize) -> Self {
        self.max_depth = Some(depth);
        self
    }

    pub fn subcharts_dir(mut self, dir: impl Into<String>) -> Self {
        self.subcharts_dir = Some(dir.into());
        self
    }

    pub fn build(self) -> PackRenderer {
        let engine = if self.strict_mode {
            Engine::strict()
        } else {
            Engine::lenient()
        };

        let config = SubchartConfig {
            max_depth: self.max_depth.unwrap_or(10),
            subcharts_dir: self.subcharts_dir.unwrap_or_else(|| "charts".to_string()),
            strict: self.strict_mode,
        };

        PackRenderer { engine, config }
    }
}
```

---

## Implementation Details

### Phase 1: Subchart Discovery

```rust
impl PackRenderer {
    pub fn discover_subcharts(
        &self,
        pack: &LoadedPack,
        values: &Values,
    ) -> DiscoveryResult {
        let mut result = DiscoveryResult::default();
        let subcharts_dir = pack.root.join(&self.config.subcharts_dir);

        // Build a map of dependencies by name for condition lookup
        let deps_by_name: HashMap<&str, &Dependency> = pack.pack.dependencies
            .iter()
            .map(|d| (d.effective_name(), d))
            .collect();

        // Scan the subcharts directory
        if !subcharts_dir.exists() {
            return result;
        }

        for entry in std::fs::read_dir(&subcharts_dir).ok().into_iter().flatten() {
            let entry = match entry {
                Ok(e) => e,
                Err(e) => {
                    result.warnings.push(format!("Failed to read entry: {}", e));
                    continue;
                }
            };

            let path = entry.path();
            if !path.is_dir() {
                continue;
            }

            let dir_name = match path.file_name().and_then(|n| n.to_str()) {
                Some(n) => n.to_string(),
                None => continue,
            };

            // Try to load as a pack
            let subchart_pack = match LoadedPack::load(&path) {
                Ok(p) => p,
                Err(e) => {
                    result.warnings.push(format!(
                        "Failed to load subchart '{}': {}",
                        dir_name, e
                    ));
                    continue;
                }
            };

            // Find matching dependency definition
            let dependency = deps_by_name.get(dir_name.as_str()).copied().cloned();

            // Determine effective name (alias if set)
            let name = dependency
                .as_ref()
                .and_then(|d| d.alias.clone())
                .unwrap_or_else(|| dir_name.clone());

            // Evaluate condition
            let (enabled, disabled_reason) = self.evaluate_condition(
                &dependency,
                values,
            );

            result.subcharts.push(SubchartInfo {
                name,
                path,
                pack: subchart_pack,
                enabled,
                dependency,
                disabled_reason,
            });
        }

        // Check for missing subcharts from dependencies
        for dep in &pack.pack.dependencies {
            if dep.enabled {
                let name = dep.effective_name();
                let found = result.subcharts.iter().any(|s| s.name == name);
                if !found {
                    result.missing.push(name.to_string());
                }
            }
        }

        // Sort by name for deterministic output
        result.subcharts.sort_by(|a, b| a.name.cmp(&b.name));

        result
    }
}
```

### Phase 2: Condition Evaluation

```rust
impl PackRenderer {
    /// Evaluate if a subchart is enabled based on its condition
    fn evaluate_condition(
        &self,
        dependency: &Option<Dependency>,
        values: &Values,
    ) -> (bool, Option<String>) {
        let Some(dep) = dependency else {
            // No dependency definition = always enabled
            return (true, None);
        };

        // Check static enabled flag
        if !dep.enabled {
            return (false, Some("Statically disabled (enabled: false)".to_string()));
        }

        // Check condition
        if let Some(condition) = &dep.condition {
            let condition_met = evaluate_condition_path(condition, values.inner());
            if !condition_met {
                return (false, Some(format!("Condition '{}' evaluated to false", condition)));
            }
        }

        (true, None)
    }
}

/// Evaluate a dot-path condition against values
///
/// Supports paths like "redis.enabled", "features.cache.memory"
fn evaluate_condition_path(condition: &str, values: &serde_json::Value) -> bool {
    let parts: Vec<&str> = condition.split('.').collect();

    let mut current = values;
    for part in &parts {
        match current.get(part) {
            Some(v) => current = v,
            None => return false,
        }
    }

    // Coerce to boolean
    match current {
        serde_json::Value::Bool(b) => *b,
        serde_json::Value::Null => false,
        serde_json::Value::String(s) => !s.is_empty() && s != "false" && s != "0",
        serde_json::Value::Number(n) => n.as_f64().map(|f| f != 0.0).unwrap_or(false),
        serde_json::Value::Array(a) => !a.is_empty(),
        serde_json::Value::Object(o) => !o.is_empty(),
    }
}
```

### Phase 3: Recursive Rendering

```rust
impl PackRenderer {
    fn render_recursive(
        &self,
        pack: &LoadedPack,
        context: &TemplateContext,
        depth: usize,
    ) -> RenderResultWithReport {
        let mut report = RenderReport::new();
        let mut all_manifests = IndexMap::new();
        let mut notes = None;

        // Check depth limit
        if depth > self.config.max_depth {
            report.add_warning(
                "subchart",
                format!("Maximum subchart depth ({}) exceeded", self.config.max_depth),
            );
            return RenderResultWithReport {
                manifests: all_manifests,
                notes,
                report,
            };
        }

        // Discover subcharts
        let discovery = self.discover_subcharts(pack, &context.values);

        // Add discovery warnings
        for warning in discovery.warnings {
            report.add_warning("subchart_discovery", warning);
        }

        // Warn about missing subcharts
        for missing in discovery.missing {
            if self.config.strict {
                report.add_error(
                    "<subchart>".to_string(),
                    TemplateError::simple(format!("Missing subchart: {}", missing)),
                );
            } else {
                report.add_warning("subchart_missing", format!("Subchart '{}' not found", missing));
            }
        }

        // Render each enabled subchart
        for subchart in discovery.subcharts {
            if !subchart.enabled {
                // Log why it was skipped
                if let Some(reason) = &subchart.disabled_reason {
                    report.add_warning(
                        "subchart_disabled",
                        format!("Subchart '{}' disabled: {}", subchart.name, reason),
                    );
                }
                continue;
            }

            // Load subchart's default values
            let subchart_defaults = match Values::from_file(&subchart.pack.values_path) {
                Ok(v) => v,
                Err(e) => {
                    report.add_warning(
                        "subchart_values",
                        format!("Failed to load values for '{}': {}", subchart.name, e),
                    );
                    Values::new()
                }
            };

            // Scope values for this subchart
            let scoped_values = Values::for_subchart(
                subchart_defaults,
                &context.values,
                &subchart.name,
            );

            // Create context for subchart
            let subchart_context = TemplateContext::new(
                scoped_values,
                context.release.clone(),
                &subchart.pack.pack.metadata,
            );

            // Recursively render subchart (handles its own subcharts)
            let subchart_result = self.render_recursive(
                &subchart.pack,
                &subchart_context,
                depth + 1,
            );

            // Merge subchart manifests with prefix
            for (name, manifest) in subchart_result.manifests {
                let prefixed_name = format!("{}/{}", subchart.name, name);
                all_manifests.insert(prefixed_name, manifest);
            }

            // Merge report
            for (template, errors) in subchart_result.report.errors_by_template {
                let prefixed = format!("{}/{}", subchart.name, template);
                for error in errors {
                    report.add_error(prefixed.clone(), error);
                }
            }
            for issue in subchart_result.report.issues {
                report.add_issue(issue);
            }

            // Subchart notes are typically not shown
        }

        // Render parent pack
        let parent_result = self.engine.render_pack_collect_errors(pack, context);

        // Merge parent manifests (after subcharts so they appear at the end)
        all_manifests.extend(parent_result.manifests);
        notes = parent_result.notes;

        // Merge parent report
        for (template, errors) in parent_result.report.errors_by_template {
            for error in errors {
                report.add_error(template.clone(), error);
            }
        }
        for issue in parent_result.report.issues {
            report.add_issue(issue);
        }
        for success in parent_result.report.successful_templates {
            report.add_success(success);
        }

        RenderResultWithReport {
            manifests: all_manifests,
            notes,
            report,
        }
    }
}
```

---

## Integration

### CLI Integration

```rust
// In sherpack-cli/src/commands/template.rs

// Before:
let engine = Engine::strict();
let result = engine.render_pack(&pack, &context)?;

// After:
let renderer = PackRenderer::builder()
    .strict(true)
    .build();
let result = renderer.render(&pack, &context)?;
```

### Kube Integration

```rust
// In sherpack-kube/src/client.rs

impl<S: StorageDriver> KubeClient<S> {
    pub async fn install(&self, options: InstallOptions) -> Result<Release> {
        // Use PackRenderer instead of Engine
        let renderer = PackRenderer::new(Engine::strict());
        let result = renderer.render(&pack, &context)?;

        // ... apply manifests ...
    }
}
```

---

## File Structure

```
crates/sherpack-engine/src/
├── lib.rs              # Add: pub mod subchart; pub mod pack_renderer;
├── engine.rs           # Unchanged
├── subchart.rs         # NEW: SubchartConfig, SubchartInfo, DiscoveryResult
├── pack_renderer.rs    # NEW: PackRenderer, PackRendererBuilder
├── files_object.rs
├── filters.rs
├── functions.rs
├── error.rs
└── suggestions.rs
```

---

## Testing Strategy

### Unit Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_pack_with_subcharts() -> TempDir {
        let dir = TempDir::new().unwrap();

        // Create parent pack
        create_pack(&dir, "parent", "1.0.0", &["redis", "postgresql"]);

        // Create subcharts
        create_subchart(&dir, "redis", "7.0.0");
        create_subchart(&dir, "postgresql", "15.0.0");

        dir
    }

    #[test]
    fn test_discover_subcharts() { ... }

    #[test]
    fn test_condition_evaluation() { ... }

    #[test]
    fn test_render_with_subcharts() { ... }

    #[test]
    fn test_recursive_subcharts() { ... }

    #[test]
    fn test_disabled_subchart_not_rendered() { ... }

    #[test]
    fn test_value_scoping() { ... }

    #[test]
    fn test_manifest_prefixing() { ... }

    #[test]
    fn test_max_depth_limit() { ... }
}
```

### Integration Tests

```rust
// In sherpack-cli/tests/integration_tests.rs

mod subchart_rendering {
    #[test]
    fn test_template_with_subcharts() { ... }

    #[test]
    fn test_subchart_condition() { ... }

    #[test]
    fn test_subchart_values_override() { ... }
}
```

---

## Migration Path

1. **Phase 1**: Implement `PackRenderer` alongside `Engine`
2. **Phase 2**: Add feature flag `--with-subcharts` to CLI
3. **Phase 3**: Make `PackRenderer` the default
4. **Phase 4**: Deprecate direct `Engine::render_pack` for packs with subcharts

---

## Helm Compatibility Notes

| Feature | Helm | Sherpack |
|---------|------|----------|
| Subchart directory | `charts/` | `charts/` (configurable) |
| Value scoping | `.<subchart>.` | `Values::for_subchart()` |
| Global values | `.global` | `global` key preserved |
| Conditions | `condition: key.enabled` | Same syntax |
| Aliases | `alias: newname` | `Dependency::alias` |
| Tags | Multiple conditions | Not yet implemented |
| Import-values | Complex merging | Not yet implemented |

---

## Estimated Effort

| Phase | Description | Estimated Lines | Complexity |
|-------|-------------|-----------------|------------|
| 1 | Types & structures | ~150 | Low |
| 2 | Discovery | ~200 | Medium |
| 3 | Condition evaluation | ~50 | Low |
| 4 | PackRenderer core | ~300 | High |
| 5 | Recursive support | ~100 | Medium |
| 6 | CLI/Kube integration | ~100 | Low |
| 7 | Tests | ~400 | Medium |
| **Total** | | **~1300** | |

---

## Open Questions

1. **Should disabled subcharts' CRDs still be installed?**
   - Helm: No
   - Proposal: No, follow Helm

2. **How to handle conflicting resource names across subcharts?**
   - Helm: User's responsibility
   - Proposal: Warn but don't error

3. **Should subchart NOTES.txt be shown?**
   - Helm: Only parent's NOTES.txt
   - Proposal: Same, collect for debugging

4. **Hook ordering across subcharts?**
   - Complex topic, defer to separate design doc
