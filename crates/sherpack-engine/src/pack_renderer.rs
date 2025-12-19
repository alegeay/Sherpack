//! Pack renderer with subchart support
//!
//! This module provides `PackRenderer`, which orchestrates the rendering
//! of a pack and all its subcharts with proper value scoping.

use std::collections::HashMap;
use indexmap::IndexMap;

use serde_json::Value as JsonValue;
use sherpack_core::{LoadedPack, TemplateContext, Values, Dependency};

use crate::engine::Engine;
use crate::error::{EngineError, RenderReport, RenderIssue, TemplateError};
use crate::subchart::{SubchartConfig, SubchartInfo, DiscoveryResult};

/// Result of rendering a pack (with or without subcharts)
#[derive(Debug)]
pub struct PackRenderResult {
    /// Rendered manifests by filename (IndexMap preserves insertion order)
    /// Subchart manifests are prefixed: "redis/deployment.yaml"
    pub manifests: IndexMap<String, String>,

    /// Post-install notes (from parent pack only)
    pub notes: Option<String>,

    /// Discovery information about subcharts
    pub discovery: DiscoveryResult,
}

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

    /// Get a reference to the underlying engine
    pub fn engine(&self) -> &Engine {
        &self.engine
    }

    /// Get a reference to the config
    pub fn config(&self) -> &SubchartConfig {
        &self.config
    }

    /// Discover subcharts in a pack
    ///
    /// This scans the subcharts directory (default: `charts/`) for valid packs
    /// and evaluates their conditions against the provided values.
    pub fn discover_subcharts(
        &self,
        pack: &LoadedPack,
        values: &JsonValue,
    ) -> DiscoveryResult {
        let mut result = DiscoveryResult::new();
        let subcharts_dir = pack.root.join(&self.config.subcharts_dir);

        // Build a map of dependencies by name for condition lookup
        let deps_by_name: HashMap<&str, &Dependency> = pack
            .pack
            .dependencies
            .iter()
            .map(|d| (d.effective_name(), d))
            .collect();

        // Check if subcharts directory exists
        if !subcharts_dir.exists() {
            // Not an error - pack may not have subcharts
            return result;
        }

        // Scan the subcharts directory
        let entries = match std::fs::read_dir(&subcharts_dir) {
            Ok(e) => e,
            Err(e) => {
                result.warnings.push(format!(
                    "Failed to read subcharts directory '{}': {}",
                    subcharts_dir.display(),
                    e
                ));
                return result;
            }
        };

        for entry in entries {
            let entry = match entry {
                Ok(e) => e,
                Err(e) => {
                    result.warnings.push(format!("Failed to read directory entry: {}", e));
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
            let dependency = deps_by_name.get(dir_name.as_str()).cloned().cloned();

            // Determine effective name (alias if set)
            let name = dependency
                .as_ref()
                .and_then(|d| d.alias.clone())
                .unwrap_or_else(|| dir_name.clone());

            // Evaluate condition
            let (enabled, disabled_reason) = self.evaluate_condition(&dependency, values);

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

    /// Evaluate if a subchart is enabled based on its condition
    fn evaluate_condition(
        &self,
        dependency: &Option<Dependency>,
        values: &JsonValue,
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
            let condition_met = evaluate_condition_path(condition, values);
            if !condition_met {
                return (
                    false,
                    Some(format!("Condition '{}' evaluated to false", condition)),
                );
            }
        }

        (true, None)
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
    ) -> Result<PackRenderResult, EngineError> {
        let result = self.render_collect_errors(pack, context);

        if result.report.has_errors() {
            // Return first error
            let first_error = result
                .report
                .errors_by_template
                .into_values()
                .next()
                .and_then(|errors| errors.into_iter().next());

            return Err(match first_error {
                Some(err) => EngineError::Template(Box::new(err)),
                None => EngineError::Template(Box::new(TemplateError::simple(
                    "Unknown template error during subchart rendering",
                ))),
            });
        }

        Ok(PackRenderResult {
            manifests: result.manifests,
            notes: result.notes,
            discovery: result.discovery,
        })
    }

    /// Render with full error collection
    pub fn render_collect_errors(
        &self,
        pack: &LoadedPack,
        context: &TemplateContext,
    ) -> PackRenderResultWithReport {
        self.render_recursive(pack, context, 0)
    }

    /// Internal recursive renderer
    fn render_recursive(
        &self,
        pack: &LoadedPack,
        context: &TemplateContext,
        depth: usize,
    ) -> PackRenderResultWithReport {
        let mut report = RenderReport::new();
        let mut all_manifests = IndexMap::new();
        let mut notes = None;

        // Check depth limit
        if depth > self.config.max_depth {
            report.add_warning(
                "subchart",
                format!(
                    "Maximum subchart depth ({}) exceeded, stopping recursion",
                    self.config.max_depth
                ),
            );
            return PackRenderResultWithReport {
                manifests: all_manifests,
                notes,
                report,
                discovery: DiscoveryResult::new(),
            };
        }

        // Discover subcharts
        let discovery = self.discover_subcharts(pack, &context.values);

        // Add discovery warnings to report
        for warning in &discovery.warnings {
            report.add_warning("subchart_discovery", warning.clone());
        }

        // Handle missing subcharts
        for missing in &discovery.missing {
            if self.config.strict {
                report.add_error(
                    format!("<subchart:{}>", missing),
                    TemplateError::simple(format!(
                        "Missing subchart '{}' referenced in dependencies",
                        missing
                    )),
                );
            } else {
                report.add_warning(
                    "subchart_missing",
                    format!("Subchart '{}' not found in {}/", missing, self.config.subcharts_dir),
                );
            }
        }

        // Render each enabled subchart
        for subchart in &discovery.subcharts {
            if !subchart.enabled {
                // Log why it was skipped
                if let Some(reason) = &subchart.disabled_reason {
                    report.add_issue(RenderIssue::warning(
                        "subchart_disabled",
                        format!("Subchart '{}' disabled: {}", subchart.name, reason),
                    ));
                }
                continue;
            }

            // Load subchart's default values
            let subchart_defaults = if subchart.pack.values_path.exists() {
                match Values::from_file(&subchart.pack.values_path) {
                    Ok(v) => v,
                    Err(e) => {
                        report.add_warning(
                            "subchart_values",
                            format!(
                                "Failed to load values.yaml for '{}': {}",
                                subchart.name, e
                            ),
                        );
                        Values::new()
                    }
                }
            } else {
                Values::new()
            };

            // Scope values for this subchart
            let scoped_values = Values::for_subchart_json(
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

            // Merge subchart errors with prefix
            for (template, errors) in subchart_result.report.errors_by_template {
                let prefixed = format!("{}/{}", subchart.name, template);
                for error in errors {
                    report.add_error(prefixed.clone(), error);
                }
            }

            // Merge issues
            for issue in subchart_result.report.issues {
                report.add_issue(issue);
            }

            // Subchart notes are typically not shown (only parent's notes)
        }

        // Render parent pack
        let parent_result = self.engine.render_pack_collect_errors(pack, context);

        // Merge parent manifests (after subcharts for proper ordering)
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

        PackRenderResultWithReport {
            manifests: all_manifests,
            notes,
            report,
            discovery,
        }
    }
}

/// Result type that includes discovery info and error report
#[derive(Debug)]
pub struct PackRenderResultWithReport {
    /// Rendered manifests (may be partial if errors occurred)
    pub manifests: IndexMap<String, String>,

    /// Post-install notes
    pub notes: Option<String>,

    /// Error and warning report
    pub report: RenderReport,

    /// Subchart discovery results
    pub discovery: DiscoveryResult,
}

impl PackRenderResultWithReport {
    /// Check if rendering was fully successful (no errors)
    pub fn is_success(&self) -> bool {
        !self.report.has_errors()
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
    /// Enable strict mode for the engine (fail on undefined variables)
    pub fn strict(mut self, strict: bool) -> Self {
        self.strict_mode = strict;
        self
    }

    /// Set maximum depth for nested subcharts
    pub fn max_depth(mut self, depth: usize) -> Self {
        self.max_depth = Some(depth);
        self
    }

    /// Set the subcharts directory name (default: "charts")
    pub fn subcharts_dir(mut self, dir: impl Into<String>) -> Self {
        self.subcharts_dir = Some(dir.into());
        self
    }

    /// Build the PackRenderer
    pub fn build(self) -> PackRenderer {
        let engine = if self.strict_mode {
            Engine::strict()
        } else {
            Engine::lenient()
        };

        let mut config = SubchartConfig::default();
        if let Some(depth) = self.max_depth {
            config.max_depth = depth;
        }
        if let Some(dir) = self.subcharts_dir {
            config.subcharts_dir = dir;
        }
        if self.strict_mode {
            config.strict = true;
        }

        PackRenderer { engine, config }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_evaluate_condition_path_bool() {
        let values = serde_json::json!({
            "redis": {
                "enabled": true
            },
            "postgresql": {
                "enabled": false
            }
        });

        assert!(evaluate_condition_path("redis.enabled", &values));
        assert!(!evaluate_condition_path("postgresql.enabled", &values));
    }

    #[test]
    fn test_evaluate_condition_path_missing() {
        let values = serde_json::json!({
            "redis": {}
        });

        assert!(!evaluate_condition_path("redis.enabled", &values));
        assert!(!evaluate_condition_path("nonexistent.path", &values));
    }

    #[test]
    fn test_evaluate_condition_path_truthy() {
        let values = serde_json::json!({
            "string_yes": "yes",
            "string_empty": "",
            "number_one": 1,
            "number_zero": 0,
            "array_full": [1, 2],
            "array_empty": []
        });

        assert!(evaluate_condition_path("string_yes", &values));
        assert!(!evaluate_condition_path("string_empty", &values));
        assert!(evaluate_condition_path("number_one", &values));
        assert!(!evaluate_condition_path("number_zero", &values));
        assert!(evaluate_condition_path("array_full", &values));
        assert!(!evaluate_condition_path("array_empty", &values));
    }

    #[test]
    fn test_pack_renderer_builder() {
        let renderer = PackRenderer::builder()
            .strict(true)
            .max_depth(5)
            .subcharts_dir("deps")
            .build();

        assert_eq!(renderer.config.max_depth, 5);
        assert_eq!(renderer.config.subcharts_dir, "deps");
        assert!(renderer.config.strict);
    }

    #[test]
    fn test_pack_render_result_with_report_success() {
        let result = PackRenderResultWithReport {
            manifests: IndexMap::new(),
            notes: None,
            report: RenderReport::new(),
            discovery: DiscoveryResult::new(),
        };

        assert!(result.is_success());
    }

    #[test]
    fn test_discover_subcharts_with_fixture() {
        use std::path::PathBuf;

        let fixture_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("fixtures/pack-with-subcharts");

        if !fixture_path.exists() {
            // Skip if fixture doesn't exist
            return;
        }

        let pack = LoadedPack::load(&fixture_path).expect("Failed to load fixture");
        let renderer = PackRenderer::new(Engine::lenient());

        let values = serde_json::json!({
            "redis": { "enabled": true },
            "postgresql": { "enabled": false }
        });

        let discovery = renderer.discover_subcharts(&pack, &values);

        // Should find both subcharts
        assert_eq!(discovery.subcharts.len(), 2);

        // Redis should be enabled
        let redis = discovery.subcharts.iter().find(|s| s.name == "redis");
        assert!(redis.is_some());
        assert!(redis.unwrap().enabled);

        // PostgreSQL should be disabled (statically disabled in Pack.yaml)
        let pg = discovery.subcharts.iter().find(|s| s.name == "postgresql");
        assert!(pg.is_some());
        assert!(!pg.unwrap().enabled);
    }

    #[test]
    fn test_render_pack_with_subcharts() {
        use std::path::PathBuf;
        use sherpack_core::ReleaseInfo;

        let fixture_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("fixtures/pack-with-subcharts");

        if !fixture_path.exists() {
            return;
        }

        let pack = LoadedPack::load(&fixture_path).expect("Failed to load fixture");
        let renderer = PackRenderer::new(Engine::lenient());

        let values = Values::from_yaml(r#"
global:
  imageRegistry: docker.io
  pullPolicy: IfNotPresent
app:
  name: my-application
  replicas: 2
  image:
    repository: myapp
    tag: "1.0.0"
redis:
  enabled: true
  replicas: 3
  auth:
    enabled: true
    password: secret123
postgresql:
  enabled: false
"#).expect("Failed to parse values");

        let release = ReleaseInfo::for_install("test-release", "default");
        let context = TemplateContext::new(values, release, &pack.pack.metadata);

        let result = renderer.render(&pack, &context).expect("Render failed");

        // Should have parent manifest
        assert!(result.manifests.contains_key("deployment.yaml"));

        // Should have redis subchart manifest (prefixed)
        assert!(result.manifests.contains_key("redis/deployment.yaml"));

        // Should NOT have postgresql manifest (disabled)
        let has_postgresql = result.manifests.keys().any(|k| k.starts_with("postgresql/"));
        assert!(!has_postgresql, "PostgreSQL should be disabled");

        // Verify redis manifest uses scoped values
        let redis_manifest = result.manifests.get("redis/deployment.yaml").unwrap();
        assert!(redis_manifest.contains("replicas: 3"), "Should use parent's redis.replicas=3");
        assert!(redis_manifest.contains("REDIS_PASSWORD"), "Auth should be enabled");

        // Verify parent manifest has correct content
        let parent_manifest = result.manifests.get("deployment.yaml").unwrap();
        assert!(parent_manifest.contains("test-release-my-application"));
        assert!(parent_manifest.contains("REDIS_HOST"));
        assert!(!parent_manifest.contains("DATABASE_HOST"), "PostgreSQL env should not be present");
    }

    #[test]
    fn test_subchart_global_values_passed() {
        use std::path::PathBuf;
        use sherpack_core::ReleaseInfo;

        let fixture_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("fixtures/pack-with-subcharts");

        if !fixture_path.exists() {
            return;
        }

        let pack = LoadedPack::load(&fixture_path).expect("Failed to load fixture");
        let renderer = PackRenderer::new(Engine::lenient());

        let values = Values::from_yaml(r#"
global:
  imageRegistry: my-registry.io
  pullPolicy: Always
app:
  name: my-app
  replicas: 1
  image:
    repository: myapp
    tag: "1.0"
redis:
  enabled: true
postgresql:
  enabled: false
"#).expect("Failed to parse values");

        let release = ReleaseInfo::for_install("test", "default");
        let context = TemplateContext::new(values, release, &pack.pack.metadata);

        let result = renderer.render(&pack, &context).expect("Render failed");

        // Redis manifest should use global.imageRegistry
        let redis_manifest = result.manifests.get("redis/deployment.yaml").unwrap();
        assert!(redis_manifest.contains("my-registry.io"), "Should use global imageRegistry");
    }
}
