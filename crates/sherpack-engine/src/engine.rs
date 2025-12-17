//! Template engine based on MiniJinja

use minijinja::Environment;
use sherpack_core::{LoadedPack, TemplateContext};
use std::collections::HashMap;

use crate::error::{EngineError, RenderReport, RenderResultWithReport, Result, TemplateError};
use crate::filters;
use crate::functions;

/// Result of rendering a pack
#[derive(Debug)]
pub struct RenderResult {
    /// Rendered manifests by filename
    pub manifests: HashMap<String, String>,

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
    /// Create a new engine with default settings
    pub fn new(strict_mode: bool) -> Self {
        Self { strict_mode }
    }

    /// Create a builder
    pub fn builder() -> EngineBuilder {
        EngineBuilder::new()
    }

    /// Create a configured MiniJinja environment
    fn create_environment(&self) -> Environment<'static> {
        let mut env = Environment::new();

        // Configure behavior
        if self.strict_mode {
            env.set_undefined_behavior(minijinja::UndefinedBehavior::Strict);
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
    pub fn render_pack(
        &self,
        pack: &LoadedPack,
        context: &TemplateContext,
    ) -> Result<RenderResult> {
        let template_files = pack.template_files().map_err(|e| {
            EngineError::Template(TemplateError::simple(format!(
                "Failed to list templates: {}",
                e
            )))
        })?;

        let mut manifests = HashMap::new();
        let mut notes = None;

        // First, load all helper templates (those starting with _)
        let mut env = self.create_environment();
        let templates_dir = &pack.templates_dir;

        // Load all templates into the environment
        for file_path in &template_files {
            let rel_path = file_path
                .strip_prefix(templates_dir)
                .unwrap_or(file_path);
            let template_name = rel_path.to_string_lossy().to_string();
            let content = std::fs::read_to_string(file_path)?;

            env.add_template_owned(template_name, content)
                .map_err(|e| {
                    let content = std::fs::read_to_string(file_path).unwrap_or_default();
                    EngineError::Template(TemplateError::from_minijinja(
                        e,
                        &file_path.to_string_lossy(),
                        &content,
                    ))
                })?;
        }

        // Build context value - use direct references for structs (context! macro serializes)
        let ctx = minijinja::context! {
            values => &context.values,
            release => &context.release,
            pack => &context.pack,
            capabilities => &context.capabilities,
            template => &context.template,
        };

        // Now render each non-helper template
        for file_path in &template_files {
            let rel_path = file_path
                .strip_prefix(templates_dir)
                .unwrap_or(file_path);
            let template_name = rel_path.to_string_lossy().to_string();

            // Skip helper templates (starting with _)
            let file_stem = rel_path
                .file_name()
                .map(|s| s.to_string_lossy())
                .unwrap_or_default();

            if file_stem.starts_with('_') {
                continue;
            }

            let tmpl = env.get_template(&template_name).map_err(|e| {
                let content = std::fs::read_to_string(file_path).unwrap_or_default();
                EngineError::Template(TemplateError::from_minijinja(
                    e,
                    &template_name,
                    &content,
                ))
            })?;

            let rendered = tmpl.render(&ctx).map_err(|e| {
                let content = std::fs::read_to_string(file_path).unwrap_or_default();
                EngineError::Template(TemplateError::from_minijinja(
                    e,
                    &template_name,
                    &content,
                ))
            })?;

            // Check if it's NOTES.txt
            if template_name.to_lowercase().contains("notes") {
                notes = Some(rendered);
            } else {
                // Skip empty rendered templates
                let trimmed = rendered.trim();
                if !trimmed.is_empty() && trimmed != "---" {
                    // Generate output filename (remove .j2 extension if present)
                    let output_name = template_name
                        .trim_end_matches(".j2")
                        .trim_end_matches(".jinja2");

                    manifests.insert(output_name.to_string(), rendered);
                }
            }
        }

        Ok(RenderResult { manifests, notes })
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
        let mut manifests = HashMap::new();
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
            let template_name = rel_path.to_string_lossy().to_string();

            let content = match std::fs::read_to_string(file_path) {
                Ok(c) => c,
                Err(e) => {
                    report.add_error(
                        template_name.clone(),
                        TemplateError::simple(format!("Failed to read template: {}", e)),
                    );
                    continue;
                }
            };

            template_sources.insert(template_name.clone(), content.clone());

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
                // Continue loading other templates
            }
        }

        // Build render context
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
            let template_name = rel_path.to_string_lossy().to_string();

            // Skip helper templates
            let file_stem = rel_path
                .file_name()
                .map(|s| s.to_string_lossy())
                .unwrap_or_default();

            if file_stem.starts_with('_') {
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
                    if template_name.to_lowercase().contains("notes") {
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
                    let content = template_sources.get(&template_name).cloned().unwrap_or_default();

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
    fn test_undefined_error() {
        let engine = Engine::new(true);
        let ctx = create_test_context();

        let template = "value: {{ values.undefined_key }}";
        let result = engine.render_string(template, &ctx, "test.yaml");

        assert!(result.is_err());
    }
}
