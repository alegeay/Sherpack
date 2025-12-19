//! Template engine based on MiniJinja
//!
//! This module provides the core rendering engine for Sherpack templates,
//! built on top of MiniJinja with Helm-compatible filters and functions.

use indexmap::IndexMap;
use minijinja::Environment;
use sherpack_core::{LoadedPack, TemplateContext};
use std::collections::HashMap;

use crate::error::{EngineError, RenderReport, RenderResultWithReport, Result, TemplateError};
use crate::filters;
use crate::functions;

/// Prefix character for helper templates (skipped during rendering)
const HELPER_TEMPLATE_PREFIX: char = '_';

/// Pattern to identify NOTES templates
const NOTES_TEMPLATE_PATTERN: &str = "notes";

/// Result of rendering a pack
#[derive(Debug)]
pub struct RenderResult {
    /// Rendered manifests by filename (IndexMap preserves insertion order)
    pub manifests: IndexMap<String, String>,

    /// Post-install notes (if NOTES.txt exists)
    pub notes: Option<String>,
}

/// Template engine builder
pub struct EngineBuilder {
    strict_mode: bool,
}

impl Default for EngineBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl EngineBuilder {
    pub fn new() -> Self {
        Self { strict_mode: true }
    }

    /// Set strict mode (fail on undefined variables)
    pub fn strict(mut self, strict: bool) -> Self {
        self.strict_mode = strict;
        self
    }

    /// Build the engine
    pub fn build(self) -> Engine {
        Engine::new(self.strict_mode)
    }
}

/// The template engine
pub struct Engine {
    strict_mode: bool,
}

impl Engine {
    /// Create a new engine
    ///
    /// # Arguments
    /// * `strict_mode` - If true, uses Chainable undefined behavior (Helm-compatible).
    ///                   If false, uses Lenient mode (empty strings for undefined).
    ///
    /// # Prefer using convenience methods
    /// For clearer code, prefer `Engine::strict()` or `Engine::lenient()`.
    pub fn new(strict_mode: bool) -> Self {
        Self { strict_mode }
    }

    /// Create a strict mode engine (Helm-compatible, recommended)
    ///
    /// Uses `UndefinedBehavior::Chainable` which allows accessing properties
    /// on undefined values, returning undefined instead of error.
    #[must_use]
    pub fn strict() -> Self {
        Self { strict_mode: true }
    }

    /// Create a lenient mode engine
    ///
    /// Uses `UndefinedBehavior::Lenient` which returns empty strings
    /// for undefined values.
    #[must_use]
    pub fn lenient() -> Self {
        Self { strict_mode: false }
    }

    /// Create a builder for more configuration options
    #[must_use]
    pub fn builder() -> EngineBuilder {
        EngineBuilder::new()
    }

    /// Create a configured MiniJinja environment
    fn create_environment(&self) -> Environment<'static> {
        let mut env = Environment::new();

        // Configure behavior
        // Use Chainable mode by default - allows accessing properties on undefined values
        // (returns undefined instead of error), matching Helm's Go template behavior.
        // This is essential for converted charts where values may be optional.
        if self.strict_mode {
            env.set_undefined_behavior(minijinja::UndefinedBehavior::Chainable);
        } else {
            env.set_undefined_behavior(minijinja::UndefinedBehavior::Lenient);
        }

        // Register custom filters
        env.add_filter("toyaml", filters::toyaml);
        env.add_filter("tojson", filters::tojson);
        env.add_filter("tojson_pretty", filters::tojson_pretty);
        env.add_filter("b64encode", filters::b64encode);
        env.add_filter("b64decode", filters::b64decode);
        env.add_filter("quote", filters::quote);
        env.add_filter("squote", filters::squote);
        env.add_filter("nindent", filters::nindent);
        env.add_filter("indent", filters::indent);
        env.add_filter("required", filters::required);
        env.add_filter("empty", filters::empty);
        env.add_filter("haskey", filters::haskey);
        env.add_filter("keys", filters::keys);
        env.add_filter("merge", filters::merge);
        env.add_filter("sha256", filters::sha256sum);
        env.add_filter("trunc", filters::trunc);
        env.add_filter("trimprefix", filters::trimprefix);
        env.add_filter("trimsuffix", filters::trimsuffix);
        env.add_filter("snakecase", filters::snakecase);
        env.add_filter("kebabcase", filters::kebabcase);
        env.add_filter("tostrings", filters::tostrings);
        env.add_filter("semver_match", filters::semver_match);
        env.add_filter("int", filters::int);
        env.add_filter("float", filters::float);

        // Register global functions
        env.add_function("fail", functions::fail);
        env.add_function("dict", functions::dict);
        env.add_function("list", functions::list);
        env.add_function("get", functions::get);
        env.add_function("coalesce", functions::coalesce);
        env.add_function("ternary", functions::ternary);
        env.add_function("uuidv4", functions::uuidv4);
        env.add_function("tostring", functions::tostring);
        env.add_function("toint", functions::toint);
        env.add_function("tofloat", functions::tofloat);
        env.add_function("now", functions::now);
        env.add_function("printf", functions::printf);
        env.add_function("tpl", functions::tpl);
        env.add_function("tpl_ctx", functions::tpl_ctx);
        env.add_function("lookup", functions::lookup);

        env
    }

    /// Render a single template string
    pub fn render_string(
        &self,
        template: &str,
        context: &TemplateContext,
        template_name: &str,
    ) -> Result<String> {
        let env = self.create_environment();

        // Add template to environment
        let mut env = env;
        env.add_template_owned(template_name.to_string(), template.to_string())
            .map_err(|e| {
                EngineError::Template(TemplateError::from_minijinja(e, template_name, template))
            })?;

        // Get template and render
        let tmpl = env.get_template(template_name).map_err(|e| {
            EngineError::Template(TemplateError::from_minijinja(e, template_name, template))
        })?;

        // Build context
        let ctx = minijinja::context! {
            values => &context.values,
            release => &context.release,
            pack => &context.pack,
            capabilities => &context.capabilities,
            template => &context.template,
        };

        tmpl.render(ctx).map_err(|e| {
            EngineError::Template(TemplateError::from_minijinja(e, template_name, template))
        })
    }

    /// Render all templates in a pack
    ///
    /// This is a convenience wrapper around `render_pack_collect_errors` that
    /// returns the first error encountered, suitable for most use cases.
    pub fn render_pack(
        &self,
        pack: &LoadedPack,
        context: &TemplateContext,
    ) -> Result<RenderResult> {
        let result = self.render_pack_collect_errors(pack, context);

        // If there were any errors, return the first one
        if result.report.has_errors() {
            // Get the first error from the report
            let first_error = result
                .report
                .errors_by_template
                .into_values()
                .next()
                .and_then(|errors| errors.into_iter().next());

            return Err(match first_error {
                Some(err) => EngineError::Template(err),
                None => EngineError::Template(TemplateError::simple("Unknown template error")),
            });
        }

        Ok(RenderResult {
            manifests: result.manifests,
            notes: result.notes,
        })
    }

    /// Render all templates in a pack, collecting all errors instead of stopping at the first
    ///
    /// Unlike `render_pack`, this method continues after errors and returns
    /// a comprehensive report of all issues found.
    pub fn render_pack_collect_errors(
        &self,
        pack: &LoadedPack,
        context: &TemplateContext,
    ) -> RenderResultWithReport {
        let mut report = RenderReport::new();
        let mut manifests = IndexMap::new();
        let mut notes = None;

        let template_files = match pack.template_files() {
            Ok(files) => files,
            Err(e) => {
                report.add_error(
                    "<pack>".to_string(),
                    TemplateError::simple(format!("Failed to list templates: {}", e)),
                );
                return RenderResultWithReport {
                    manifests,
                    notes,
                    report,
                };
            }
        };

        // Create environment with all templates loaded
        let mut env = self.create_environment();
        let templates_dir = &pack.templates_dir;

        // Track template sources for error reporting
        let mut template_sources: HashMap<String, String> = HashMap::new();

        // Load all templates - continue even if some fail to parse
        for file_path in &template_files {
            let rel_path = file_path.strip_prefix(templates_dir).unwrap_or(file_path);
            let template_name = rel_path.to_string_lossy().into_owned();

            let content = match std::fs::read_to_string(file_path) {
                Ok(c) => c,
                Err(e) => {
                    report.add_error(
                        template_name,
                        TemplateError::simple(format!("Failed to read template: {}", e)),
                    );
                    continue;
                }
            };

            // Store content first, then add to environment
            // This avoids cloning content twice
            if let Err(e) = env.add_template_owned(template_name.clone(), content.clone()) {
                report.add_error(
                    template_name.clone(),
                    TemplateError::from_minijinja_enhanced(
                        e,
                        &template_name,
                        &content,
                        Some(&context.values),
                    ),
                );
            }
            // Store after attempting to add (content is still valid)
            template_sources.insert(template_name, content);
        }

        // Add context as globals so imported macros can access them
        // This is necessary because MiniJinja macros don't automatically get the render context
        env.add_global("values", minijinja::Value::from_serialize(&context.values));
        env.add_global("release", minijinja::Value::from_serialize(&context.release));
        env.add_global("pack", minijinja::Value::from_serialize(&context.pack));
        env.add_global("capabilities", minijinja::Value::from_serialize(&context.capabilities));
        env.add_global("template", minijinja::Value::from_serialize(&context.template));

        // Build render context (still needed for direct template rendering)
        let ctx = minijinja::context! {
            values => &context.values,
            release => &context.release,
            pack => &context.pack,
            capabilities => &context.capabilities,
            template => &context.template,
        };

        // Render each non-helper template, collecting errors
        for file_path in &template_files {
            let rel_path = file_path.strip_prefix(templates_dir).unwrap_or(file_path);
            let template_name = rel_path.to_string_lossy().into_owned();

            // Skip helper templates (prefixed with '_')
            let file_stem = rel_path
                .file_name()
                .map(|s| s.to_string_lossy())
                .unwrap_or_default();

            if file_stem.starts_with(HELPER_TEMPLATE_PREFIX) {
                continue;
            }

            // Try to get template (may have failed during loading)
            let tmpl = match env.get_template(&template_name) {
                Ok(t) => t,
                Err(_) => {
                    // Error already recorded during loading
                    continue;
                }
            };

            // Try to render
            match tmpl.render(&ctx) {
                Ok(rendered) => {
                    // Process successful render
                    if template_name.to_lowercase().contains(NOTES_TEMPLATE_PATTERN) {
                        notes = Some(rendered);
                    } else {
                        let trimmed = rendered.trim();
                        if !trimmed.is_empty() && trimmed != "---" {
                            let output_name = template_name
                                .trim_end_matches(".j2")
                                .trim_end_matches(".jinja2");
                            manifests.insert(output_name.to_string(), rendered);
                        }
                    }
                    report.add_success(template_name);
                }
                Err(e) => {
                    // Get template source for error context
                    // Use empty string only if template was never loaded (shouldn't happen)
                    let content = template_sources
                        .get(&template_name)
                        .map(String::as_str)
                        .unwrap_or("");

                    report.add_error(
                        template_name.clone(),
                        TemplateError::from_minijinja_enhanced(
                            e,
                            &template_name,
                            content,
                            Some(&context.values),
                        ),
                    );
                }
            }
        }

        RenderResultWithReport {
            manifests,
            notes,
            report,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sherpack_core::{PackMetadata, ReleaseInfo, Values};
    use semver::Version;

    fn create_test_context() -> TemplateContext {
        let values = Values::from_yaml(r#"
image:
  repository: nginx
  tag: "1.25"
replicas: 3
"#)
        .unwrap();

        let release = ReleaseInfo::for_install("myapp", "default");

        let pack = PackMetadata {
            name: "mypack".to_string(),
            version: Version::new(1, 0, 0),
            description: None,
            app_version: Some("2.0.0".to_string()),
            kube_version: None,
            home: None,
            icon: None,
            sources: vec![],
            keywords: vec![],
            maintainers: vec![],
            annotations: Default::default(),
        };

        TemplateContext::new(values, release, &pack)
    }

    #[test]
    fn test_render_simple() {
        let engine = Engine::new(true);
        let ctx = create_test_context();

        let template = "replicas: {{ values.replicas }}";
        let result = engine.render_string(template, &ctx, "test.yaml").unwrap();

        assert_eq!(result, "replicas: 3");
    }

    #[test]
    fn test_render_with_filters() {
        let engine = Engine::new(true);
        let ctx = create_test_context();

        let template = r#"image: {{ values.image | toyaml | nindent(2) }}"#;
        let result = engine.render_string(template, &ctx, "test.yaml").unwrap();

        assert!(result.contains("repository: nginx"));
        assert!(result.contains("tag:"));
    }

    #[test]
    fn test_render_release_info() {
        let engine = Engine::new(true);
        let ctx = create_test_context();

        let template = "name: {{ release.name }}\nnamespace: {{ release.namespace }}";
        let result = engine.render_string(template, &ctx, "test.yaml").unwrap();

        assert!(result.contains("name: myapp"));
        assert!(result.contains("namespace: default"));
    }

    #[test]
    fn test_chainable_undefined_returns_empty() {
        // With UndefinedBehavior::Chainable, undefined keys return empty string
        // This matches Helm's behavior for optional values
        let engine = Engine::new(true);
        let ctx = create_test_context();

        let template = "value: {{ values.undefined_key }}";
        let result = engine.render_string(template, &ctx, "test.yaml");

        // Chainable mode: undefined attributes return empty, not error
        assert!(result.is_ok());
        let output = result.unwrap();
        assert_eq!(output.trim(), "value:");
    }

    #[test]
    fn test_chainable_typo_returns_empty() {
        // With UndefinedBehavior::Chainable, even top-level undefined vars return empty
        // This is intentional for Helm compatibility (optional values pattern)
        let engine = Engine::new(true);
        let ctx = create_test_context();

        // Common typo: "value" instead of "values"
        let template = "name: {{ value.app.name }}";
        let result = engine.render_string(template, &ctx, "test.yaml");

        // Chainable mode allows this - returns empty
        // Users should rely on linting and unknown filter errors to catch typos
        assert!(result.is_ok());
        let output = result.unwrap();
        assert_eq!(output.trim(), "name:");
    }

    #[test]
    fn test_render_string_unknown_filter() {
        let engine = Engine::new(true);
        let ctx = create_test_context();

        let template = "name: {{ values.image.repository | unknownfilter }}";
        let result = engine.render_string(template, &ctx, "test.yaml");

        assert!(result.is_err());
    }

    #[test]
    fn test_render_result_with_report_structure() {
        use crate::error::{RenderReport, RenderResultWithReport};

        // Test successful result
        let result = RenderResultWithReport {
            manifests: {
                let mut m = IndexMap::new();
                m.insert("deployment.yaml".to_string(), "apiVersion: v1".to_string());
                m
            },
            notes: Some("Install notes".to_string()),
            report: RenderReport::new(),
        };

        assert!(result.is_success());
        assert_eq!(result.manifests.len(), 1);
        assert!(result.notes.is_some());
    }

    #[test]
    fn test_render_result_partial_success() {
        use crate::error::{RenderReport, RenderResultWithReport, TemplateError};

        let mut report = RenderReport::new();
        report.add_success("good.yaml".to_string());
        report.add_error(
            "bad.yaml".to_string(),
            TemplateError::simple("undefined variable"),
        );

        let result = RenderResultWithReport {
            manifests: {
                let mut m = IndexMap::new();
                m.insert("good.yaml".to_string(), "content".to_string());
                m
            },
            notes: None,
            report,
        };

        // Not a success because there was an error
        assert!(!result.is_success());
        // But we still have partial results
        assert_eq!(result.manifests.len(), 1);
        assert!(result.manifests.contains_key("good.yaml"));
    }
}
