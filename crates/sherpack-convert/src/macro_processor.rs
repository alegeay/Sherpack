//! Macro variable scoping post-processor
//!
//! This module resolves bare variable references inside macro definitions to their
//! fully qualified paths (e.g., `chroot` → `values.controller.image.chroot`).
//!
//! The problem arises from Helm's `with` blocks that set a context:
//! ```go
//! {{- with .Values.controller.image }}
//!   {{- if .chroot }}...
//! {{- end }}
//! ```
//!
//! When converted to Jinja2 macros, the context is lost and variables become bare.
//! This post-processor fixes that by searching values.yaml for matching keys.

use regex::Regex;
use serde_yaml::Value;
use std::collections::HashMap;

/// A macro definition with its name and body
#[derive(Debug, Clone)]
pub struct MacroDefinition {
    pub name: String,
    pub body: String,
    pub start_offset: usize,
    pub end_offset: usize,
}

/// Result of resolving a variable
#[derive(Debug, Clone)]
pub enum ResolvedVariable {
    /// Single unambiguous path found
    Unique(String),
    /// Multiple paths found - needs manual resolution
    Ambiguous(Vec<String>),
    /// No matching path found
    NotFound,
}

/// Post-processor for macro variable scoping
#[derive(Debug)]
pub struct MacroPostProcessor {
    /// Map of variable name to all paths where it appears in values.yaml
    variable_paths: HashMap<String, Vec<String>>,
}

impl MacroPostProcessor {
    /// Create a new processor from values.yaml content
    pub fn from_yaml(yaml_content: &str) -> Result<Self, serde_yaml::Error> {
        let value: Value = serde_yaml::from_str(yaml_content)?;
        let mut variable_paths = HashMap::new();
        Self::collect_paths(&value, "values", &mut variable_paths);
        Ok(Self { variable_paths })
    }

    /// Create from a pre-parsed YAML value
    pub fn from_value(value: &Value) -> Self {
        let mut variable_paths = HashMap::new();
        Self::collect_paths(value, "values", &mut variable_paths);
        Self { variable_paths }
    }

    /// Recursively collect all variable paths from the YAML structure
    fn collect_paths(value: &Value, current_path: &str, paths: &mut HashMap<String, Vec<String>>) {
        match value {
            Value::Mapping(map) => {
                for (key, val) in map {
                    if let Value::String(key_str) = key {
                        let new_path = format!("{}.{}", current_path, key_str);
                        // Record this key's full path
                        paths
                            .entry(key_str.clone())
                            .or_insert_with(Vec::new)
                            .push(new_path.clone());
                        // Recurse into nested values
                        Self::collect_paths(val, &new_path, paths);
                    }
                }
            }
            Value::Sequence(seq) => {
                for (idx, val) in seq.iter().enumerate() {
                    let new_path = format!("{}[{}]", current_path, idx);
                    Self::collect_paths(val, &new_path, paths);
                }
            }
            _ => {}
        }
    }

    /// Resolve a bare variable name to its full path
    pub fn resolve(&self, variable: &str) -> ResolvedVariable {
        match self.variable_paths.get(variable) {
            Some(paths) if paths.len() == 1 => ResolvedVariable::Unique(paths[0].clone()),
            Some(paths) if paths.len() > 1 => ResolvedVariable::Ambiguous(paths.clone()),
            _ => ResolvedVariable::NotFound,
        }
    }

    /// Resolve a variable with a hint about the expected parent path segment
    ///
    /// For example, if the variable is `chroot` and the hint is `image`,
    /// prefer `values.controller.image.chroot` over other paths.
    pub fn resolve_with_hint(&self, variable: &str, hint: &str) -> ResolvedVariable {
        match self.variable_paths.get(variable) {
            Some(paths) if paths.len() == 1 => ResolvedVariable::Unique(paths[0].clone()),
            Some(paths) if paths.len() > 1 => {
                // The full path should end with {hint}.{variable}
                let expected_suffix = format!(".{}.{}", hint, variable);

                let matching: Vec<_> = paths
                    .iter()
                    .filter(|p| p.ends_with(&expected_suffix))
                    .cloned()
                    .collect();

                match matching.len() {
                    1 => ResolvedVariable::Unique(matching[0].clone()),
                    0 => {
                        // Fall back to containing the hint anywhere in path
                        let fallback: Vec<_> = paths
                            .iter()
                            .filter(|p| p.contains(&format!(".{}.", hint)))
                            .cloned()
                            .collect();
                        match fallback.len() {
                            1 => ResolvedVariable::Unique(fallback[0].clone()),
                            0 => ResolvedVariable::Ambiguous(paths.clone()),
                            _ => ResolvedVariable::Ambiguous(fallback),
                        }
                    }
                    _ => ResolvedVariable::Ambiguous(matching),
                }
            }
            _ => ResolvedVariable::NotFound,
        }
    }

    /// Extract all macro definitions from a template
    pub fn extract_macros(content: &str) -> Vec<MacroDefinition> {
        let mut macros = Vec::new();
        // Match {%- macro name(...) %}...{%- endmacro %}
        let macro_re = Regex::new(
            r"(?s)\{%-?\s*macro\s+(\w+)\s*\([^)]*\)\s*%\}(.*?)\{%-?\s*endmacro\s*%\}"
        ).unwrap();

        for caps in macro_re.captures_iter(content) {
            let full_match = caps.get(0).unwrap();
            macros.push(MacroDefinition {
                name: caps[1].to_string(),
                body: caps[2].to_string(),
                start_offset: full_match.start(),
                end_offset: full_match.end(),
            });
        }

        macros
    }

    /// Find bare variable references in a macro body
    ///
    /// Returns variable names that are:
    /// - Not prefixed with `values.`, `release.`, `pack.`, `capabilities.`, `_with_ctx.`
    /// - Not Jinja2 keywords or builtin functions
    /// - Inside expressions ({{ }}) or control structures ({% %})
    /// - Not loop or set variable declarations
    pub fn find_bare_variables(macro_body: &str) -> Vec<String> {
        let mut bare_vars = Vec::new();

        // Known prefixes for qualified variables
        let qualified_prefixes = [
            "values.", "release.", "pack.", "capabilities.", "_with_ctx.",
            "loop.", "item.", "key.", "value.", "self.",
        ];

        // Known Jinja2/template keywords and builtins to ignore
        let keywords = [
            "true", "false", "none", "null", "and", "or", "not", "in", "is",
            "if", "else", "elif", "endif", "for", "endfor", "set", "endset",
            "macro", "endmacro", "import", "from", "include", "block", "endblock",
            "extends", "call", "filter", "raw", "endraw", "with", "endwith",
            // Common filter/function names
            "nindent", "indent", "quote", "toyaml", "tojson", "trunc", "default",
            "trimsuffix", "trimprefix", "replace", "lower", "upper", "title",
            "dictsort", "merge", "tpl", "toString", "semver_match", "b64encode",
            "len",
        ];

        // First, find all loop and set variable declarations to exclude them
        // Pattern: {% for VAR, VAR in ... %} or {% set VAR = ... %}
        let mut declared_vars: Vec<String> = Vec::new();

        let for_vars_pattern = Regex::new(
            r"\{%-?\s*for\s+(\w+)(?:\s*,\s*(\w+))?\s+in\s+"
        ).unwrap();

        let set_pattern = Regex::new(
            r"\{%-?\s*set\s+(\w+)\s*="
        ).unwrap();

        for caps in for_vars_pattern.captures_iter(macro_body) {
            declared_vars.push(caps[1].to_string());
            if let Some(m) = caps.get(2) {
                declared_vars.push(m.as_str().to_string());
            }
        }

        for caps in set_pattern.captures_iter(macro_body) {
            declared_vars.push(caps[1].to_string());
        }

        // Match variable references in expressions
        // Look for: {{ var }}, {{ var | filter }}, {{ var.something }}, {% if var %}
        let var_pattern = Regex::new(
            r"\{\{[^}]*?([a-zA-Z_][a-zA-Z0-9_]*)\s*(?:[|}\.]|\s*\)|\s*~|\s*==|\s*!=|\s*<|\s*>|\s*and|\s*or|\s*%\})"
        ).unwrap();

        let control_pattern = Regex::new(
            r"\{%[^%]*?(?:if|elif)\s+([a-zA-Z_][a-zA-Z0-9_]*)\s*(?:[|%}\.]|\s*\)|\s*~|\s*==|\s*!=)"
        ).unwrap();

        // Also catch standalone variable references in various contexts
        let standalone_pattern = Regex::new(
            r"(?:^|[^a-zA-Z0-9_\.])([a-zA-Z_][a-zA-Z0-9_]*)(?:\s*\||\s*\}|\s*~|\s*==|\s*!=|\s*<|\s*>|\s+and|\s+or|\s*\))"
        ).unwrap();

        for pattern in [&var_pattern, &control_pattern, &standalone_pattern] {
            for caps in pattern.captures_iter(macro_body) {
                let var_name = caps[1].to_string();

                // Skip if it's a keyword
                if keywords.contains(&var_name.to_lowercase().as_str()) {
                    continue;
                }

                // Skip if it's a declared loop/set variable
                if declared_vars.contains(&var_name) {
                    continue;
                }

                // Check if this is a qualified variable (has a prefix before it)
                let var_pos = caps.get(1).unwrap().start();
                let before = &macro_body[..var_pos];
                let is_qualified = qualified_prefixes.iter().any(|prefix| {
                    before.ends_with(prefix) || before.ends_with(&format!("({})", prefix.trim_end_matches('.')))
                });

                if !is_qualified && !bare_vars.contains(&var_name) {
                    bare_vars.push(var_name);
                }
            }
        }

        bare_vars
    }

    /// Process a template, resolving bare variables in macros
    ///
    /// Returns the processed content and a list of variables that couldn't be resolved
    pub fn process(&self, content: &str) -> (String, Vec<UnresolvedVariable>) {
        let mut result = content.to_string();
        let mut unresolved = Vec::new();

        let macros = Self::extract_macros(content);

        for macro_def in macros {
            let bare_vars = Self::find_bare_variables(&macro_def.body);

            // Derive hints from macro name, ordered by specificity
            let hints = Self::derive_hints(&macro_def.name);

            for var in bare_vars {
                // Try each hint in order until we find a unique resolution
                let mut resolution = self.resolve(&var);

                for hint in &hints {
                    match &resolution {
                        ResolvedVariable::Unique(_) => break, // Already resolved
                        _ => {
                            resolution = self.resolve_with_hint(&var, hint);
                        }
                    }
                }

                match resolution {
                    ResolvedVariable::Unique(full_path) => {
                        // Replace this variable in the macro body
                        result = Self::replace_bare_variable(&result, &macro_def.name, &var, &full_path);
                    }
                    ResolvedVariable::Ambiguous(paths) => {
                        unresolved.push(UnresolvedVariable {
                            variable: var,
                            macro_name: macro_def.name.clone(),
                            candidates: paths,
                            reason: "Multiple matching paths found".to_string(),
                        });
                    }
                    ResolvedVariable::NotFound => {
                        // This might be a local variable or macro parameter - don't report
                    }
                }
            }
        }

        (result, unresolved)
    }

    /// Derive hints from the macro name, ordered by specificity
    ///
    /// Returns a list of hints to try, from most specific to least specific
    fn derive_hints(macro_name: &str) -> Vec<String> {
        let name_lower = macro_name.to_lowercase();
        let mut hints = Vec::new();

        // Common patterns for ingress-nginx and similar charts
        // More specific hints come first

        // Controller-related patterns
        if name_lower.contains("controller") {
            if name_lower.contains("image") {
                hints.push("controller.image".to_string());
            }
            hints.push("controller".to_string());
        }

        // Standalone image macros typically refer to controller.image
        if name_lower.contains("image") || name_lower.contains("digest") {
            hints.push("controller.image".to_string());
            hints.push("image".to_string());
        }

        // Backend-related patterns
        if name_lower.contains("backend") {
            if name_lower.contains("image") {
                hints.push("defaultBackend.image".to_string());
            }
            hints.push("defaultBackend".to_string());
        }

        // Webhook-related patterns
        if name_lower.contains("webhook") {
            hints.push("admissionWebhooks".to_string());
        }

        hints
    }

    /// Replace a bare variable with its qualified path within a specific macro
    fn replace_bare_variable(content: &str, macro_name: &str, var: &str, full_path: &str) -> String {
        // Build a regex that matches the macro and replaces the variable inside it
        let macro_pattern = format!(
            r"(?s)(\{{% *-? *macro {} *\([^)]*\) *%\}})(.*?)(\{{% *-? *endmacro *%\}})",
            regex::escape(macro_name)
        );

        let macro_re = Regex::new(&macro_pattern).unwrap();

        macro_re.replace(content, |caps: &regex::Captures| {
            let prefix = &caps[1];
            let body = &caps[2];
            let suffix = &caps[3];

            // Replace bare variable with qualified path in the body
            let new_body = Self::replace_bare_in_body(body, var, full_path);

            format!("{}{}{}", prefix, new_body, suffix)
        }).to_string()
    }

    /// Replace a bare variable in a macro body, being careful about context
    fn replace_bare_in_body(body: &str, var: &str, full_path: &str) -> String {
        // We need to be careful to only replace variables in expression contexts,
        // not inside string literals like "-chroot" or 'value'
        //
        // Strategy: Process the body segment by segment, tracking whether we're
        // inside a string literal or not

        let mut result = String::with_capacity(body.len() + 100);
        let chars: Vec<char> = body.chars().collect();
        let mut i = 0;
        let var_chars: Vec<char> = var.chars().collect();

        while i < chars.len() {
            let ch = chars[i];

            // Track string literals - skip content inside quotes
            if ch == '"' || ch == '\'' {
                let quote = ch;
                result.push(ch);
                i += 1;
                // Copy everything until closing quote
                while i < chars.len() && chars[i] != quote {
                    result.push(chars[i]);
                    i += 1;
                }
                if i < chars.len() {
                    result.push(chars[i]); // closing quote
                    i += 1;
                }
                continue;
            }

            // Check if we're at a potential variable match
            if Self::is_var_match(&chars, i, &var_chars) {
                let before_idx = if i > 0 { i - 1 } else { 0 };
                let after_idx = i + var_chars.len();

                // Check character before - should not be alphanumeric, underscore, or dot
                let valid_before = i == 0 || {
                    let bc = chars[before_idx];
                    !bc.is_alphanumeric() && bc != '_' && bc != '.'
                };

                // Check character after - should not be alphanumeric or underscore
                let valid_after = after_idx >= chars.len() || {
                    let ac = chars[after_idx];
                    !ac.is_alphanumeric() && ac != '_'
                };

                if valid_before && valid_after {
                    // Replace with full path
                    result.push_str(full_path);
                    i += var_chars.len();
                    continue;
                }
            }

            result.push(ch);
            i += 1;
        }

        result
    }

    /// Check if chars starting at position i match the variable
    fn is_var_match(chars: &[char], i: usize, var_chars: &[char]) -> bool {
        if i + var_chars.len() > chars.len() {
            return false;
        }
        for (j, vc) in var_chars.iter().enumerate() {
            if chars[i + j] != *vc {
                return false;
            }
        }
        true
    }
}

/// Information about a variable that couldn't be resolved
#[derive(Debug, Clone)]
pub struct UnresolvedVariable {
    pub variable: String,
    pub macro_name: String,
    pub candidates: Vec<String>,
    pub reason: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_VALUES: &str = r#"
controller:
  name: controller
  image:
    chroot: false
    image: ingress-nginx/controller
    tag: "v1.14.1"
    digest: sha256:abc123
    digestChroot: sha256:def456
defaultBackend:
  image:
    image: defaultbackend
    tag: "1.5"
"#;

    #[test]
    fn test_from_yaml() {
        let processor = MacroPostProcessor::from_yaml(TEST_VALUES).unwrap();
        assert!(processor.variable_paths.contains_key("chroot"));
        assert!(processor.variable_paths.contains_key("digest"));
        assert!(processor.variable_paths.contains_key("image"));
    }

    #[test]
    fn test_resolve_unique() {
        let processor = MacroPostProcessor::from_yaml(TEST_VALUES).unwrap();

        // chroot only exists in controller.image
        match processor.resolve("chroot") {
            ResolvedVariable::Unique(path) => {
                assert_eq!(path, "values.controller.image.chroot");
            }
            _ => panic!("Expected unique resolution for chroot"),
        }

        // digestChroot only exists in controller.image
        match processor.resolve("digestChroot") {
            ResolvedVariable::Unique(path) => {
                assert_eq!(path, "values.controller.image.digestChroot");
            }
            _ => panic!("Expected unique resolution for digestChroot"),
        }
    }

    #[test]
    fn test_resolve_ambiguous() {
        let processor = MacroPostProcessor::from_yaml(TEST_VALUES).unwrap();

        // "image" exists in both controller.image and defaultBackend.image
        match processor.resolve("image") {
            ResolvedVariable::Ambiguous(paths) => {
                assert!(paths.len() >= 2);
                assert!(paths.iter().any(|p| p.contains("controller.image.image")));
                assert!(paths.iter().any(|p| p.contains("defaultBackend.image.image")));
            }
            _ => panic!("Expected ambiguous resolution for image"),
        }
    }

    #[test]
    fn test_resolve_with_hint() {
        let processor = MacroPostProcessor::from_yaml(TEST_VALUES).unwrap();

        // With hint "controller.image", should resolve to controller.image.image
        match processor.resolve_with_hint("image", "controller.image") {
            ResolvedVariable::Unique(path) => {
                assert_eq!(path, "values.controller.image.image");
            }
            _ => panic!("Expected unique resolution with hint"),
        }
    }

    #[test]
    fn test_extract_macros() {
        let content = r#"
{%- macro image() %}
{{- (image ~ "-chroot") -}}
{%- endmacro %}

{%- macro imageDigest() %}
{%- if chroot %}
{{- ("@" ~ digestChroot) -}}
{%- endif %}
{%- endmacro %}
"#;

        let macros = MacroPostProcessor::extract_macros(content);
        assert_eq!(macros.len(), 2);
        assert_eq!(macros[0].name, "image");
        assert_eq!(macros[1].name, "imageDigest");
    }

    #[test]
    fn test_find_bare_variables() {
        let macro_body = r#"
{%- if chroot %}
{{- (image ~ "-chroot") -}}
{%- else %}
{{- (image) -}}
{%- endif %}
"#;

        let bare_vars = MacroPostProcessor::find_bare_variables(macro_body);
        assert!(bare_vars.contains(&"chroot".to_string()));
        assert!(bare_vars.contains(&"image".to_string()));
    }

    #[test]
    fn test_process_full() {
        let processor = MacroPostProcessor::from_yaml(TEST_VALUES).unwrap();

        let content = r#"
{%- macro imageDigest() %}
{%- if chroot %}
{{- ("@" ~ digestChroot) -}}
{%- endif %}
{%- endmacro %}
"#;

        let (processed, unresolved) = processor.process(content);

        // chroot and digestChroot should be resolved
        assert!(processed.contains("values.controller.image.chroot"));
        assert!(processed.contains("values.controller.image.digestChroot"));
        // No unresolved variables
        assert!(unresolved.is_empty(), "Unexpected unresolved: {:?}", unresolved);
    }

    #[test]
    fn test_no_false_positives() {
        let processor = MacroPostProcessor::from_yaml(TEST_VALUES).unwrap();

        let content = r#"
{%- macro test() %}
{%- set local_var = "test" %}
{{- local_var | upper -}}
{{- values.controller.name -}}
{%- endmacro %}
"#;

        let (processed, _) = processor.process(content);

        // Should not modify already-qualified variables or local variables
        assert!(processed.contains("values.controller.name"));
        assert!(processed.contains("local_var"));
        // Should not add extra "values." prefix
        assert!(!processed.contains("values.values."));
    }

    #[test]
    fn test_replace_bare_variable() {
        let content = r#"{%- macro image() %}
{%- if chroot %}
test
{%- endif %}
{%- endmacro %}"#;

        let result = MacroPostProcessor::replace_bare_variable(
            content,
            "image",
            "chroot",
            "values.controller.image.chroot",
        );

        assert!(result.contains("values.controller.image.chroot"));
        assert!(!result.contains(" chroot "));
    }
}
