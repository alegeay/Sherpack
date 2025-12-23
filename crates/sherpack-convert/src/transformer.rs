//! Go template to Jinja2 transformer
//!
//! Transforms Go template AST into idiomatic Jinja2 syntax.
//!
//! # Philosophy
//!
//! This transformer prioritizes **Jinja2 elegance** over Helm compatibility.
//! Instead of replicating Go template's quirky syntax, we convert to natural
//! Jinja2 patterns:
//!
//! | Helm (Go template)              | Sherpack (Jinja2)                |
//! |---------------------------------|----------------------------------|
//! | `{{ index .Values.list 0 }}`    | `{{ values.list[0] }}`           |
//! | `{{ add 1 2 }}`                 | `{{ 1 + 2 }}`                    |
//! | `{{ ternary "a" "b" .X }}`      | `{{ "a" if x else "b" }}`        |
//! | `{{ printf "%s-%s" a b }}`      | `{{ a ~ "-" ~ b }}`              |
//! | `{{ coalesce .A .B "c" }}`      | `{{ a or b or "c" }}`            |

use crate::ast::*;
use crate::type_inference::{InferredType, TypeContext, TypeHeuristics};
use phf::phf_map;

// =============================================================================
// FILTER MAPPINGS - Direct 1:1 conversions
// =============================================================================

static FILTER_MAP: phf::Map<&'static str, &'static str> = phf_map! {
    // Serialization
    "toYaml" => "toyaml",
    "toJson" => "tojson",
    "toPrettyJson" => "tojson_pretty",
    "fromYaml" => "fromyaml",
    "fromJson" => "fromjson",

    // Encoding
    "b64enc" => "b64encode",
    "b64dec" => "b64decode",

    // Quoting
    "quote" => "quote",
    "squote" => "squote",

    // String case
    "upper" => "upper",
    "lower" => "lower",
    "title" => "title",
    "camelcase" => "camelcase",
    "snakecase" => "snakecase",
    "kebabcase" => "kebabcase",
    "swapcase" => "swapcase",

    // String manipulation
    "trim" => "trim",
    "trimPrefix" => "trimprefix",
    "trimSuffix" => "trimsuffix",
    "trimAll" => "trim",
    "trunc" => "trunc",
    "abbrev" => "trunc",
    "repeat" => "repeat",
    "replace" => "replace",
    "wrap" => "wordwrap",
    "wrapWith" => "wordwrap",

    // String testing
    "hasPrefix" => "startswith",
    "hasSuffix" => "endswith",

    // Indentation
    "indent" => "indent",
    "nindent" => "nindent",

    // Lists
    "first" => "first",
    "last" => "last",
    "rest" => "list[1:]",
    "initial" => "list[:-1]",
    "reverse" => "reverse",
    "uniq" => "unique",
    "sortAlpha" => "sort",

    // Dict
    "hasKey" => "haskey",
    "keys" => "keys",
    "values" => "values",
    "merge" => "merge",
    "mergeOverwrite" => "merge",
    "deepCopy" => "deepcopy",

    // Type conversion
    "toString" => "string",
    "toStrings" => "tostrings",
    "int" => "int",
    "int64" => "int",
    "float64" => "float",

    // Validation
    "required" => "required",
    "empty" => "empty",

    // Crypto hashes
    "sha256sum" => "sha256",
    "sha1sum" => "sha1",
    "adler32sum" => "adler32",

    // Regex (direct mapping - require sherpack-engine support)
    "regexMatch" => "regex_match",
    "regexFind" => "regex_search",
    "regexFindAll" => "regex_findall",
    "regexReplaceAll" => "regex_replace",
    "regexSplit" => "split",
};

// =============================================================================
// FEATURES - Categorized by support level
// =============================================================================

/// Features that convert to native Jinja2 operators (no function needed)
static NATIVE_OPERATORS: phf::Map<&'static str, &'static str> = phf_map! {
    // Comparison → native operators
    "eq" => "==",
    "ne" => "!=",
    "lt" => "<",
    "le" => "<=",
    "gt" => ">",
    "ge" => ">=",
    // Logical → native operators
    "and" => "and",
    "or" => "or",
    // Math → native operators
    "add" => "+",
    "sub" => "-",
    "mul" => "*",
    "div" => "/",
    "mod" => "%",
};

/// Features that are intentionally unsupported (anti-patterns)
static UNSUPPORTED_FEATURES: phf::Map<&'static str, &'static str> = phf_map! {
    // Crypto - should be external (cert-manager, external-secrets)
    "genCA" => "Use cert-manager or pre-generated certificates in values",
    "genSelfSignedCert" => "Use cert-manager or pre-generated certificates",
    "genSignedCert" => "Use cert-manager for certificate management",
    "genPrivateKey" => "Use external secret management",
    "htpasswd" => "Use external secret management or pre-computed values",
    "derivePassword" => "Use external secret management",
    "encryptAES" => "Use external secret management",
    "decryptAES" => "Use external secret management",
    "randBytes" => "Use external secret management for random data",
    // Note: randAlphaNum, randAlpha, randNumeric are now converted to generate_secret()
    "randAscii" => "Use external secret management",

    // DNS - runtime dependency, breaks GitOps
    "getHostByName" => "Use explicit IP/hostname in values (GitOps compatible)",

    // Files - complexity, use ConfigMaps instead
    "Files.Get" => "Embed file content in values.yaml or use ConfigMap",
    "Files.GetBytes" => "Embed base64 content in values.yaml",
    "Files.Glob" => "List files explicitly in values.yaml",
    "Files.Lines" => "Embed content as list in values.yaml",
    "Files.AsConfig" => "Use native ConfigMap in templates",
    "Files.AsSecrets" => "Use native Secret in templates",

    // Lookup - anti-GitOps (runtime cluster query)
    "lookup" => "Returns {} in template mode - use explicit values for GitOps",
};

// =============================================================================
// WARNING TYPES
// =============================================================================

/// Warning severity level
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WarningSeverity {
    /// Converted but review recommended
    Info,
    /// Manual adjustment may be needed
    Warning,
    /// Feature not supported - alternative provided
    Unsupported,
}

/// Rich warning with context and alternatives
#[derive(Debug, Clone)]
pub struct TransformWarning {
    /// Warning severity
    pub severity: WarningSeverity,
    /// The pattern that triggered the warning
    pub pattern: String,
    /// Human-readable message
    pub message: String,
    /// Suggested alternative or fix
    pub suggestion: Option<String>,
    /// Link to documentation
    pub doc_link: Option<String>,
}

impl TransformWarning {
    pub fn info(pattern: &str, message: &str) -> Self {
        Self {
            severity: WarningSeverity::Info,
            pattern: pattern.to_string(),
            message: message.to_string(),
            suggestion: None,
            doc_link: None,
        }
    }

    pub fn warning(pattern: &str, message: &str) -> Self {
        Self {
            severity: WarningSeverity::Warning,
            pattern: pattern.to_string(),
            message: message.to_string(),
            suggestion: None,
            doc_link: None,
        }
    }

    pub fn unsupported(pattern: &str, alternative: &str) -> Self {
        Self {
            severity: WarningSeverity::Unsupported,
            pattern: pattern.to_string(),
            message: format!("'{}' is not supported in Sherpack", pattern),
            suggestion: Some(alternative.to_string()),
            doc_link: Some("https://sherpack.dev/docs/helm-migration".to_string()),
        }
    }

    pub fn with_suggestion(mut self, suggestion: &str) -> Self {
        self.suggestion = Some(suggestion.to_string());
        self
    }
}

// =============================================================================
// TRANSFORMER
// =============================================================================

/// Block type for tracking nested structures
#[derive(Debug, Clone)]
enum BlockType {
    If,
    Range,
    With,
    Define,
}

/// Transformer for converting Go template AST to idiomatic Jinja2
pub struct Transformer {
    /// Track nested blocks for proper end tag generation
    block_stack: Vec<BlockType>,
    /// Warnings generated during transformation
    #[allow(dead_code)]
    warnings: Vec<TransformWarning>,
    /// Chart name prefix to strip from include/template calls
    chart_prefix: Option<String>,
    /// Current context variable (for range/with blocks)
    context_var: Option<String>,
    /// Type context for smarter conversion (dict vs list detection)
    type_context: Option<TypeContext>,
    /// Counter for auto-generated secret names (Cell for interior mutability)
    secret_counter: std::cell::Cell<usize>,
}

impl Default for Transformer {
    fn default() -> Self {
        Self::new()
    }
}

impl Transformer {
    pub fn new() -> Self {
        Self {
            block_stack: Vec::new(),
            warnings: Vec::new(),
            chart_prefix: None,
            context_var: None,
            type_context: None,
            secret_counter: std::cell::Cell::new(0),
        }
    }

    /// Set the chart name prefix to strip from include calls
    pub fn with_chart_prefix(mut self, prefix: &str) -> Self {
        self.chart_prefix = Some(format!("{}.", prefix));
        self
    }

    /// Set the type context for smarter dict/list detection
    pub fn with_type_context(mut self, ctx: TypeContext) -> Self {
        self.type_context = Some(ctx);
        self
    }

    /// Get warnings generated during transformation
    pub fn warnings(&self) -> &[TransformWarning] {
        &self.warnings
    }

    #[allow(dead_code)]
    fn add_warning(&mut self, warning: TransformWarning) {
        self.warnings.push(warning);
    }

    /// Get the next auto-generated secret name
    fn next_secret_name(&self) -> String {
        let n = self.secret_counter.get();
        self.secret_counter.set(n + 1);
        format!("auto-secret-{}", n + 1)
    }

    /// Transform a Go template AST to Jinja2 string
    pub fn transform(&mut self, template: &Template) -> String {
        template
            .elements
            .iter()
            .map(|e| self.transform_element(e))
            .collect()
    }

    fn transform_element(&mut self, element: &Element) -> String {
        match element {
            Element::RawText(text) => text.clone(),
            Element::Action(action) => self.transform_action(action),
        }
    }

    fn transform_action(&mut self, action: &Action) -> String {
        let trim_left = if action.trim_left { "-" } else { "" };
        let trim_right = if action.trim_right { "-" } else { "" };

        match &action.body {
            // Comments: {{/* ... */}} → {# ... #}
            ActionBody::Comment(text) => {
                format!("{{# {} #}}", text.trim())
            }

            // If: {{- if .X }} → {%- if x %}
            ActionBody::If(pipeline) => {
                self.block_stack.push(BlockType::If);
                format!(
                    "{{%{} if {} %}}",
                    trim_left,
                    self.transform_pipeline(pipeline)
                )
            }

            // Else if: {{- else if .X }} → {%- elif x %}
            ActionBody::ElseIf(pipeline) => {
                format!(
                    "{{%{} elif {} %}}",
                    trim_left,
                    self.transform_pipeline(pipeline)
                )
            }

            // Else: {{- else }} → {%- else %}
            ActionBody::Else => {
                format!("{{%{} else %}}", trim_left)
            }

            // End: {{- end }} → {%- endif/endfor/endmacro %}
            ActionBody::End => {
                let block = self.block_stack.pop();
                let end_tag = match &block {
                    Some(BlockType::If) => "endif",
                    Some(BlockType::Range) => "endfor",
                    Some(BlockType::With) => "endif",
                    Some(BlockType::Define) => "endmacro",
                    None => "endif",
                };

                // Restore context after with block
                if let Some(BlockType::With) = &block {
                    self.context_var = None;
                }

                // endmacro doesn't support trim on closing
                if matches!(block, Some(BlockType::Define)) {
                    format!("{{%{} {} %}}", trim_left, end_tag)
                } else if trim_right == "-" {
                    format!("{{%{} {} -%}}", trim_left, end_tag)
                } else {
                    format!("{{%{} {} %}}", trim_left, end_tag)
                }
            }

            // Range: {{- range $k, $v := .Dict }} → {%- for k, v in dict | dictsort %}
            //        {{- range $i, $v := .List }} → {%- for v in list %}{#- i = loop.index0 #}
            ActionBody::Range { vars, pipeline } => {
                let value_var = vars
                    .as_ref()
                    .map(|v| v.value_var.trim_start_matches('$').to_string())
                    .unwrap_or_else(|| "item".to_string());

                let index_var = vars.as_ref().and_then(|v| {
                    v.index_var
                        .as_ref()
                        .map(|i| i.trim_start_matches('$').to_string())
                });

                self.block_stack.push(BlockType::Range);

                let collection = self.transform_pipeline(pipeline);

                // Determine if this is a dictionary iteration
                let is_dict = self.is_dict_type(&collection);

                match (&index_var, is_dict) {
                    // Dictionary with key variable: for key, value in dict | dictsort
                    (Some(key_var), true) => {
                        format!(
                            "{{%{} for {}, {} in {} | dictsort %}}",
                            trim_left, key_var, value_var, collection
                        )
                    }
                    // List with index: for value in list, with index comment
                    (Some(idx), false) => {
                        format!(
                            "{{%{} for {} in {} %}}{{#- {} = loop.index0 #}}",
                            trim_left, value_var, collection, idx
                        )
                    }
                    // No index variable: simple iteration
                    (None, _) => {
                        format!("{{%{} for {} in {} %}}", trim_left, value_var, collection)
                    }
                }
            }

            // With: {{- with .X }} → {%- if x %}{%- set _ctx = x %}
            ActionBody::With(pipeline) => {
                let ctx_value = self.transform_pipeline(pipeline);
                let ctx_var = "_with_ctx".to_string();

                self.block_stack.push(BlockType::With);
                self.context_var = Some(ctx_var.clone());

                // with becomes: if value, set context, use context
                format!(
                    "{{%{} if {} %}}{{%- set {} = {} %}}",
                    trim_left, ctx_value, ctx_var, ctx_value
                )
            }

            // Define: {{- define "name" }} → {%- macro name() %}
            ActionBody::Define(name) => {
                let macro_name = self.strip_chart_prefix(name);
                self.block_stack.push(BlockType::Define);
                format!("{{%{} macro {}() %}}", trim_left, macro_name)
            }

            // Template/Include: {{ template "name" . }} → {{ name() }}
            ActionBody::Template { name, .. } => {
                let macro_name = self.strip_chart_prefix(name);
                format!("{{{{ {}() }}}}", macro_name)
            }

            // Block: {{- block "name" . }} → {%- block name %}
            ActionBody::Block { name, .. } => {
                let block_name = self.strip_chart_prefix(name);
                self.block_stack.push(BlockType::Define);
                format!("{{%{} block {} %}}", trim_left, block_name)
            }

            // Pipeline: {{ .X | filter }} or {{ $x := value }}
            ActionBody::Pipeline(pipeline) => {
                // Variable declaration: $x := value → {% set x = value %}
                if let Some(ref var_name) = pipeline.decl {
                    let clean_var = var_name.trim_start_matches('$');
                    let value = if pipeline.commands.is_empty() {
                        "none".to_string()
                    } else {
                        let pipe_without_decl = Pipeline {
                            decl: None,
                            commands: pipeline.commands.clone(),
                        };
                        self.transform_pipeline(&pipe_without_decl)
                    };
                    format!("{{%{} set {} = {} %}}", trim_left, clean_var, value)
                } else {
                    // Regular expression: {{ value | filter }}
                    format!(
                        "{{{{{} {} {}}}}}",
                        trim_left,
                        self.transform_pipeline(pipeline),
                        trim_right
                    )
                }
            }
        }
    }

    fn transform_pipeline(&self, pipeline: &Pipeline) -> String {
        let mut parts = Vec::new();

        for (i, cmd) in pipeline.commands.iter().enumerate() {
            let is_filter = i > 0;
            parts.push(self.transform_command(cmd, is_filter));
        }

        let result = parts.join(" | ");

        // Post-process special markers
        self.post_process_pipeline(&result)
    }

    fn post_process_pipeline(&self, result: &str) -> String {
        let mut output = result.to_string();

        // Convert _in_(needle) marker to proper "in" syntax
        // "haystack | _in_(needle)" → "(needle in haystack)"
        if let Some(idx) = output.find(" | _in_(") {
            let haystack = &output[..idx];
            let rest = &output[idx + 8..]; // Skip " | _in_("
            if let Some(end) = rest.find(')') {
                let needle = &rest[..end];
                return format!("({} in {})", needle, haystack);
            }
        }

        // Convert _or_(default) marker to proper "or" syntax for Helm compatibility
        // "value | _or_(default)" → "(value or default)"
        // Handles chained defaults: "a | _or_(b) | _or_(c)" → "(a or b or c)"
        while let Some(idx) = output.find(" | _or_(") {
            let value = &output[..idx];
            let rest = &output[idx + 8..]; // Skip " | _or_("
            if let Some(end) = rest.find(')') {
                let default_val = &rest[..end];
                let remaining = &rest[end + 1..];
                output = format!("({} or {}){}", value, default_val, remaining);
            } else {
                break;
            }
        }

        output
    }

    fn transform_command(&self, cmd: &Command, as_filter: bool) -> String {
        match cmd {
            Command::Field(field) => self.transform_field(field),

            Command::Variable(name) => {
                // "." or "" = current context
                if name == "." || name.is_empty() {
                    if let Some(ref ctx) = self.context_var {
                        return ctx.clone();
                    }
                    return "item".to_string();
                }
                // "$" = root context
                if name == "$" {
                    return "values".to_string(); // Access root
                }
                // "$var" → "var"
                name.trim_start_matches('$').to_string()
            }

            Command::Function { name, args } => self.transform_function(name, args, as_filter),

            Command::Parenthesized(pipeline) => {
                format!("({})", self.transform_pipeline(pipeline))
            }

            Command::Literal(lit) => self.transform_literal(lit),
        }
    }

    /// Transform a function call - the heart of idiomatic conversion
    fn transform_function(&self, name: &str, args: &[Argument], as_filter: bool) -> String {
        // 0. Convert random functions to generate_secret()
        //    randAlphaNum(16) → generate_secret("auto-secret-N", 16)
        //    randAlpha(16) → generate_secret("auto-secret-N", 16, "alpha")
        //    randNumeric(6) → generate_secret("auto-secret-N", 6, "numeric")
        if let Some(charset) = match name {
            "randAlphaNum" => Some(None),       // Default charset (alphanumeric)
            "randAlpha" => Some(Some("alpha")), // Alpha only
            "randNumeric" => Some(Some("numeric")), // Numeric only
            _ => None,
        } {
            if let Some(length_arg) = args.first() {
                let length = self.transform_argument(length_arg);
                let secret_name = self.next_secret_name();
                return match charset {
                    None => format!(
                        "generate_secret(\"{}\", {}) {{# RENAME: give meaningful name #}}",
                        secret_name, length
                    ),
                    Some(cs) => format!(
                        "generate_secret(\"{}\", {}, \"{}\") {{# RENAME: give meaningful name #}}",
                        secret_name, length, cs
                    ),
                };
            }
        }

        // 1. Check for unsupported features first
        if let Some(alternative) = UNSUPPORTED_FEATURES.get(name) {
            // Return a placeholder with comment
            return format!(
                "__UNSUPPORTED_{}__ {{# {} #}}",
                name.to_uppercase(),
                alternative
            );
        }

        // 2. Native Jinja2 operators (most elegant conversion)
        if let Some(result) = self.transform_to_native_operator(name, args) {
            return result;
        }

        // 3. Special function handling
        if let Some(result) = self.transform_special_function(name, args) {
            return result;
        }

        // 4. Filter transformation
        if as_filter {
            return self.transform_as_filter(name, args);
        }

        // 5. Regular function call
        self.transform_as_function(name, args)
    }

    /// Convert to native Jinja2 operators - the most elegant transformations
    fn transform_to_native_operator(&self, name: &str, args: &[Argument]) -> Option<String> {
        // Comparison and math operators
        if let Some(&op) = NATIVE_OPERATORS.get(name) {
            if args.len() >= 2 {
                let left = self.transform_argument(&args[0]);
                let right = self.transform_argument(&args[1]);
                return Some(format!("({} {} {})", left, op, right));
            } else if args.len() == 1 {
                // Single arg - just return it (for truthiness check)
                return Some(self.transform_argument(&args[0]));
            }
        }

        // not(x) → not x
        if name == "not"
            && let Some(arg) = args.first()
        {
            return Some(format!("not {}", self.transform_argument(arg)));
        }

        // index(collection, key) → collection[key]
        if name == "index" && args.len() >= 2 {
            let base = self.transform_argument(&args[0]);
            let indices: Vec<String> = args[1..]
                .iter()
                .map(|a| format!("[{}]", self.transform_argument(a)))
                .collect();
            return Some(format!("{}{}", base, indices.join("")));
        }

        // printf("%s-%s", a, b) → (a ~ "-" ~ b)
        if name == "printf" {
            return Some(self.transform_printf(args));
        }

        // ternary("yes", "no", condition) → ("yes" if condition else "no")
        if name == "ternary" && args.len() >= 3 {
            let yes = self.transform_argument(&args[0]);
            let no = self.transform_argument(&args[1]);
            let cond = self.transform_argument(&args[2]);
            return Some(format!("({} if {} else {})", yes, cond, no));
        }

        // coalesce(a, b, c) → (a or b or c)
        if name == "coalesce" && !args.is_empty() {
            let parts: Vec<String> = args.iter().map(|a| self.transform_argument(a)).collect();
            return Some(format!("({})", parts.join(" or ")));
        }

        // contains(needle, haystack) → (needle in haystack)
        if name == "contains" && args.len() >= 2 {
            let needle = self.transform_argument(&args[0]);
            let haystack = self.transform_argument(&args[1]);
            return Some(format!("({} in {})", needle, haystack));
        }

        // default(value, default) - Use `or` for Helm compatibility
        // Helm: default "value" .X → X if X else "value"
        // Jinja2's `default` only triggers on undefined, not empty strings
        // Using `or` matches Helm's behavior (falsy values trigger default)
        if name == "default" && args.len() >= 2 {
            let default_val = self.transform_argument(&args[0]);
            let actual_val = self.transform_argument(&args[1]);
            return Some(format!("({} or {})", actual_val, default_val));
        }

        // len(x) → x | length
        if name == "len" && args.len() == 1 {
            let val = self.transform_argument(&args[0]);
            return Some(format!("{} | length", val));
        }

        // list(a, b, c) → [a, b, c]
        if name == "list" {
            let items: Vec<String> = args.iter().map(|a| self.transform_argument(a)).collect();
            return Some(format!("[{}]", items.join(", ")));
        }

        // dict("k1", v1, "k2", v2) → {"k1": v1, "k2": v2}
        if name == "dict" {
            return Some(self.transform_dict(args));
        }

        // join(sep, list) → list | join(sep)
        if name == "join" && args.len() >= 2 {
            let sep = self.transform_argument(&args[0]);
            let list = self.transform_argument(&args[1]);
            return Some(format!("{} | join({})", list, sep));
        }

        // split(sep, str) → str | split(sep)
        if (name == "split" || name == "splitList") && args.len() >= 2 {
            let sep = self.transform_argument(&args[0]);
            let string = self.transform_argument(&args[1]);
            return Some(format!("{} | split({})", string, sep));
        }

        // until(n) → range(n)
        if name == "until" && !args.is_empty() {
            let n = self.transform_argument(&args[0]);
            return Some(format!("range({})", n));
        }

        // untilStep(start, end, step) → range(start, end, step)
        if name == "untilStep" && args.len() >= 3 {
            let start = self.transform_argument(&args[0]);
            let end = self.transform_argument(&args[1]);
            let step = self.transform_argument(&args[2]);
            return Some(format!("range({}, {}, {})", start, end, step));
        }

        // seq(n) → range(1, n+1)
        if name == "seq" && !args.is_empty() {
            let n = self.transform_argument(&args[0]);
            return Some(format!("range(1, {} + 1)", n));
        }

        // max/min with multiple args
        if name == "max" && args.len() >= 2 {
            let vals: Vec<String> = args.iter().map(|a| self.transform_argument(a)).collect();
            return Some(format!("[{}] | max", vals.join(", ")));
        }
        if name == "min" && args.len() >= 2 {
            let vals: Vec<String> = args.iter().map(|a| self.transform_argument(a)).collect();
            return Some(format!("[{}] | min", vals.join(", ")));
        }

        // merge(a, b) → a | merge(b)
        if name == "merge" && args.len() >= 2 {
            let base = self.transform_argument(&args[0]);
            let overlay = self.transform_argument(&args[1]);
            return Some(format!("({} | merge({}))", base, overlay));
        }

        // semverCompare(constraint, version) → version | semverCompare(constraint)
        // Note: semverCompare is a Sherpack function that we need to implement
        if name == "semverCompare" && args.len() >= 2 {
            let constraint = self.transform_argument(&args[0]);
            let version = self.transform_argument(&args[1]);
            return Some(format!("({} | semver_match({}))", version, constraint));
        }

        // Type conversion functions → filters
        // int(x) → (x | int), int64(x) → (x | int), float64(x) → (x | float)
        if (name == "int" || name == "int64") && args.len() == 1 {
            let val = self.transform_argument(&args[0]);
            return Some(format!("({} | int)", val));
        }
        if name == "float64" && args.len() == 1 {
            let val = self.transform_argument(&args[0]);
            return Some(format!("({} | float)", val));
        }

        None
    }

    /// Handle special functions that need custom transformation
    fn transform_special_function(&self, name: &str, args: &[Argument]) -> Option<String> {
        // include("name", context) → name()
        if name == "include" || name == "template" {
            return Some(self.transform_include(args));
        }

        // toYaml/toJson as functions (not filters)
        if name == "toYaml" && args.len() == 1 {
            let val = self.transform_argument(&args[0]);
            return Some(format!("{} | toyaml", val));
        }
        if name == "toJson" && args.len() == 1 {
            let val = self.transform_argument(&args[0]);
            return Some(format!("{} | tojson", val));
        }

        // tpl(template, context) → tpl(template)
        if name == "tpl" && !args.is_empty() {
            let template = self.transform_argument(&args[0]);
            return Some(format!("tpl({})", template));
        }

        // lookup returns {} - add warning
        if name == "lookup" {
            // Lookup returns empty dict in template mode (GitOps compatible)
            return Some("{}".to_string());
        }

        // print(x) → x (Go's print just outputs the value)
        if name == "print" && args.len() == 1 {
            return Some(self.transform_argument(&args[0]));
        }

        // now() → now
        if name == "now" {
            return Some("now()".to_string());
        }

        // uuidv4() → uuidv4()
        if name == "uuidv4" {
            return Some("uuidv4()".to_string());
        }

        // fail(msg) → fail(msg)
        if name == "fail" && !args.is_empty() {
            let msg = self.transform_argument(&args[0]);
            return Some(format!("fail({})", msg));
        }

        // get(dict, key) → dict[key] | default(none)
        if name == "get" && args.len() >= 2 {
            let dict = self.transform_argument(&args[0]);
            let key = self.transform_argument(&args[1]);
            return Some(format!("{}[{}]", dict, key));
        }

        // hasKey(dict, key) → key in dict
        if name == "hasKey" && args.len() >= 2 {
            let dict = self.transform_argument(&args[0]);
            let key = self.transform_argument(&args[1]);
            return Some(format!("({} in {})", key, dict));
        }

        // dig("a", "b", "c", default, dict) → dict.a.b.c | default(default)
        if name == "dig" && args.len() >= 2 {
            // Last arg is the dict, second-to-last is default
            let dict = self.transform_argument(&args[args.len() - 1]);
            let default = if args.len() >= 3 {
                self.transform_argument(&args[args.len() - 2])
            } else {
                "none".to_string()
            };
            let keys: Vec<String> = args[..args.len().saturating_sub(2)]
                .iter()
                .filter_map(|a| {
                    if let Argument::Literal(Literal::String(s)) = a {
                        Some(s.clone())
                    } else {
                        None
                    }
                })
                .collect();
            if keys.is_empty() {
                return Some(format!("{} | default({})", dict, default));
            }
            return Some(format!(
                "{}.{} | default({})",
                dict,
                keys.join("."),
                default
            ));
        }

        // empty(x) - check if value is empty
        if name == "empty" && args.len() == 1 {
            let val = self.transform_argument(&args[0]);
            return Some(format!("{} | empty", val));
        }

        None
    }

    /// Transform printf to string concatenation
    fn transform_printf(&self, args: &[Argument]) -> String {
        if args.is_empty() {
            return "\"\"".to_string();
        }

        // Get format string
        let format_str = match &args[0] {
            Argument::Literal(Literal::String(s)) => s.clone(),
            _ => return self.transform_argument(&args[0]),
        };

        // Simple case: no format specifiers
        if !format_str.contains('%') {
            return format!("\"{}\"", format_str);
        }

        // Get format arguments
        let format_args: Vec<String> = args[1..]
            .iter()
            .map(|a| self.transform_argument(a))
            .collect();

        // Split by %s, %d, %v, etc. and rebuild with ~
        let mut result = String::new();
        let mut arg_idx = 0;
        let mut chars = format_str.chars().peekable();
        let mut current_literal = String::new();

        while let Some(c) = chars.next() {
            if c == '%' {
                if let Some(&next) = chars.peek() {
                    match next {
                        's' | 'd' | 'v' | 'f' | 'g' | 't' => {
                            chars.next(); // consume the format char

                            // Add accumulated literal
                            if !current_literal.is_empty() {
                                if !result.is_empty() {
                                    result.push_str(" ~ ");
                                }
                                result.push_str(&format!("\"{}\"", current_literal));
                                current_literal.clear();
                            }

                            // Add argument
                            if arg_idx < format_args.len() {
                                if !result.is_empty() {
                                    result.push_str(" ~ ");
                                }
                                result.push_str(&format_args[arg_idx]);
                                arg_idx += 1;
                            }
                        }
                        '%' => {
                            chars.next();
                            current_literal.push('%');
                        }
                        _ => {
                            current_literal.push(c);
                        }
                    }
                } else {
                    current_literal.push(c);
                }
            } else {
                current_literal.push(c);
            }
        }

        // Add remaining literal
        if !current_literal.is_empty() {
            if !result.is_empty() {
                result.push_str(" ~ ");
            }
            result.push_str(&format!("\"{}\"", current_literal));
        }

        if result.is_empty() {
            "\"\"".to_string()
        } else {
            format!("({})", result)
        }
    }

    /// Transform dict("k1", v1, "k2", v2) to {"k1": v1, "k2": v2}
    fn transform_dict(&self, args: &[Argument]) -> String {
        let mut pairs = Vec::new();
        let mut i = 0;

        while i + 1 < args.len() {
            let key = self.transform_argument(&args[i]);
            let value = self.transform_argument(&args[i + 1]);
            pairs.push(format!("{}: {}", key, value));
            i += 2;
        }

        format!("{{{}}}", pairs.join(", "))
    }

    /// Transform as a Jinja2 filter
    fn transform_as_filter(&self, name: &str, args: &[Argument]) -> String {
        // Special case: contains as filter (piped)
        if name == "contains"
            && let Some(arg) = args.first()
        {
            let needle = self.transform_argument(arg);
            return format!("_in_({})", needle);
        }

        // Special case: default filter uses `or` for Helm compatibility
        // {{ .X | default "value" }} → (x or "value")
        // Returns a marker that transform_pipeline will handle
        if name == "default" {
            if let Some(arg) = args.first() {
                let default_val = self.transform_argument(arg);
                return format!("_or_({})", default_val);
            }
        }

        // Look up filter name mapping
        let filter_name = FILTER_MAP.get(name).copied().unwrap_or(name);

        if args.is_empty() {
            filter_name.to_string()
        } else {
            let args_str: Vec<String> = args.iter().map(|a| self.transform_argument(a)).collect();
            format!("{}({})", filter_name, args_str.join(", "))
        }
    }

    /// Transform as a function call
    fn transform_as_function(&self, name: &str, args: &[Argument]) -> String {
        let args_str: Vec<String> = args.iter().map(|a| self.transform_argument(a)).collect();
        format!("{}({})", name, args_str.join(", "))
    }

    fn transform_argument(&self, arg: &Argument) -> String {
        match arg {
            Argument::Field(field) => self.transform_field(field),
            Argument::Variable(name) => {
                if name == "." || name.is_empty() {
                    if let Some(ref ctx) = self.context_var {
                        return ctx.clone();
                    }
                    return "item".to_string();
                }
                if name == "$" {
                    return "values".to_string();
                }
                name.trim_start_matches('$').to_string()
            }
            Argument::Literal(lit) => self.transform_literal(lit),
            Argument::Pipeline(pipeline) => {
                format!("({})", self.transform_pipeline(pipeline))
            }
        }
    }

    fn transform_field(&self, field: &FieldAccess) -> String {
        // Handle root marker ($)
        let is_root = field.is_root;

        if field.path.is_empty() {
            // Just "." - refers to current context
            if let Some(ref ctx) = self.context_var {
                return ctx.clone();
            }
            return "item".to_string();
        }

        let first = field.path[0].as_str();
        let rest: Vec<&str> = field.path[1..].iter().map(|s| s.as_str()).collect();

        // Note: is_root is currently unused but kept for future root context handling
        let _ = is_root;
        let prefix = "";

        match first {
            "Values" => {
                if rest.is_empty() {
                    format!("{}values", prefix)
                } else {
                    format!("{}values.{}", prefix, rest.join("."))
                }
            }
            "Release" => {
                let prop = rest.first().copied().unwrap_or("");
                match prop {
                    "Name" => format!("{}release.name", prefix),
                    "Namespace" => format!("{}release.namespace", prefix),
                    "Service" => "\"Sherpack\"".to_string(),
                    "IsInstall" => format!("{}release.is_install", prefix),
                    "IsUpgrade" => format!("{}release.is_upgrade", prefix),
                    "Revision" => format!("{}release.revision", prefix),
                    _ if prop.is_empty() => format!("{}release", prefix),
                    _ => format!("{}release.{}", prefix, to_snake_case(prop)),
                }
            }
            "Chart" => {
                let prop = rest.first().copied().unwrap_or("");
                match prop {
                    "Name" => format!("{}pack.name", prefix),
                    "Version" => format!("{}pack.version", prefix),
                    "AppVersion" => format!("{}pack.appVersion", prefix),
                    "Description" => format!("{}pack.description", prefix),
                    _ if prop.is_empty() => format!("{}pack", prefix),
                    _ => format!("{}pack.{}", prefix, to_snake_case(prop)),
                }
            }
            "Capabilities" => {
                let prop = rest.first().copied().unwrap_or("");
                match prop {
                    "KubeVersion" => {
                        if rest.len() > 1 {
                            let sub = rest[1];
                            match sub {
                                "Version" | "GitVersion" => {
                                    format!("{}capabilities.kubeVersion.version", prefix)
                                }
                                "Major" => format!("{}capabilities.kubeVersion.major", prefix),
                                "Minor" => format!("{}capabilities.kubeVersion.minor", prefix),
                                _ => format!(
                                    "{}capabilities.kubeVersion.{}",
                                    prefix,
                                    to_snake_case(sub)
                                ),
                            }
                        } else {
                            format!("{}capabilities.kubeVersion", prefix)
                        }
                    }
                    "APIVersions" => format!("{}capabilities.apiVersions", prefix),
                    _ if prop.is_empty() => format!("{}capabilities", prefix),
                    _ => format!("{}capabilities.{}", prefix, to_snake_case(prop)),
                }
            }
            "Template" => {
                let prop = rest.first().copied().unwrap_or("");
                match prop {
                    "Name" => format!("{}template.name", prefix),
                    "BasePath" => format!("{}template.basePath", prefix),
                    _ if prop.is_empty() => format!("{}template", prefix),
                    _ => format!("{}template.{}", prefix, prop),
                }
            }
            "Files" => {
                // Files access is unsupported - will be caught by function handling
                let full_path = std::iter::once(first)
                    .chain(rest.iter().copied())
                    .collect::<Vec<_>>()
                    .join(".");
                format!("__UNSUPPORTED_FILES__ {{# {} #}}", full_path)
            }
            _ => {
                // Generic field access - could be inside a range/with block
                let full_path = std::iter::once(first)
                    .chain(rest.iter().copied())
                    .collect::<Vec<_>>()
                    .join(".");

                // If we're in a with context, prefix with context var
                if let Some(ref ctx) = self.context_var {
                    format!("{}.{}", ctx, full_path)
                } else {
                    full_path
                }
            }
        }
    }

    fn transform_literal(&self, lit: &Literal) -> String {
        match lit {
            Literal::String(s) => format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\"")),
            Literal::Char(c) => format!("\"{}\"", c),
            Literal::Int(n) => n.to_string(),
            Literal::Float(n) => n.to_string(),
            Literal::Bool(b) => if *b { "true" } else { "false" }.to_string(),
            Literal::Nil => "none".to_string(),
        }
    }

    fn transform_include(&self, args: &[Argument]) -> String {
        if args.is_empty() {
            return "MISSING_INCLUDE_NAME()".to_string();
        }

        // First arg is the template name
        let name = match &args[0] {
            Argument::Literal(Literal::String(s)) => self.strip_chart_prefix(s),
            _ => "DYNAMIC_INCLUDE".to_string(),
        };

        format!("{}()", name)
    }

    fn strip_chart_prefix(&self, name: &str) -> String {
        let stripped = if let Some(ref prefix) = self.chart_prefix {
            name.strip_prefix(prefix.as_str()).unwrap_or(name)
        } else {
            name
        };

        // Convert dots to underscores for valid Jinja2 macro names
        stripped.trim_matches('"').replace(['.', '-'], "_")
    }

    /// Determines if a collection path refers to a dictionary type
    ///
    /// Uses type context from values.yaml if available, otherwise falls back
    /// to heuristics based on common Helm naming patterns.
    fn is_dict_type(&self, collection: &str) -> bool {
        // First, try the type context if available
        if let Some(ref ctx) = self.type_context {
            match ctx.get_type(collection) {
                InferredType::Dict => return true,
                InferredType::List => return false,
                InferredType::Scalar => return false,
                InferredType::Unknown => {
                    // Fall through to heuristics
                }
            }
        }

        // Fall back to heuristics based on common naming patterns
        TypeHeuristics::guess_type(collection)
            .map(|t| t == InferredType::Dict)
            .unwrap_or(false)
    }
}

/// Convert PascalCase/camelCase to snake_case
fn to_snake_case(s: &str) -> String {
    let mut result = String::new();
    for (i, c) in s.chars().enumerate() {
        if c.is_uppercase() {
            if i > 0 {
                result.push('_');
            }
            result.push(c.to_lowercase().next().unwrap());
        } else {
            result.push(c);
        }
    }
    result
}

// =============================================================================
// TESTS
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser;

    fn transform(input: &str) -> String {
        let ast = parser::parse(input).expect("Failed to parse");
        let mut transformer = Transformer::new();
        transformer.transform(&ast)
    }

    fn transform_with_prefix(input: &str, prefix: &str) -> String {
        let ast = parser::parse(input).expect("Failed to parse");
        let mut transformer = Transformer::new().with_chart_prefix(prefix);
        transformer.transform(&ast)
    }

    // =========================================================================
    // Basic syntax
    // =========================================================================

    #[test]
    fn test_raw_text() {
        assert_eq!(transform("hello world"), "hello world");
    }

    #[test]
    fn test_comment() {
        assert_eq!(
            transform("{{/* This is a comment */}}"),
            "{# This is a comment #}"
        );
    }

    #[test]
    fn test_simple_variable() {
        assert_eq!(transform("{{ .Values.name }}"), "{{ values.name }}");
    }

    #[test]
    fn test_trim_whitespace() {
        assert_eq!(transform("{{- .Values.name -}}"), "{{- values.name -}}");
    }

    // =========================================================================
    // Native operators (idiomatic Jinja2)
    // =========================================================================

    #[test]
    fn test_comparison_eq() {
        assert_eq!(
            transform("{{ eq .Values.a .Values.b }}"),
            "{{ (values.a == values.b) }}"
        );
    }

    #[test]
    fn test_comparison_ne() {
        assert_eq!(
            transform("{{ ne .Values.a \"test\" }}"),
            "{{ (values.a != \"test\") }}"
        );
    }

    #[test]
    fn test_math_add() {
        assert_eq!(transform("{{ add 1 2 }}"), "{{ (1 + 2) }}");
    }

    #[test]
    fn test_math_operations() {
        assert_eq!(transform("{{ sub 10 5 }}"), "{{ (10 - 5) }}");
        assert_eq!(transform("{{ mul 3 4 }}"), "{{ (3 * 4) }}");
        assert_eq!(transform("{{ div 10 2 }}"), "{{ (10 / 2) }}");
        assert_eq!(transform("{{ mod 10 3 }}"), "{{ (10 % 3) }}");
    }

    #[test]
    fn test_ternary() {
        assert_eq!(
            transform("{{ ternary \"yes\" \"no\" .Values.enabled }}"),
            "{{ (\"yes\" if values.enabled else \"no\") }}"
        );
    }

    #[test]
    fn test_coalesce() {
        assert_eq!(
            transform("{{ coalesce .Values.a .Values.b \"default\" }}"),
            "{{ (values.a or values.b or \"default\") }}"
        );
    }

    #[test]
    fn test_default_function() {
        // default as function: default "fallback" .X → (X or "fallback")
        assert_eq!(
            transform("{{ default \"fallback\" .Values.x }}"),
            "{{ (values.x or \"fallback\") }}"
        );
    }

    #[test]
    fn test_default_filter() {
        // default as filter: .X | default "fallback" → (X or "fallback")
        assert_eq!(
            transform("{{ .Values.x | default \"fallback\" }}"),
            "{{ (values.x or \"fallback\") }}"
        );
    }

    #[test]
    fn test_default_chained() {
        // Chained defaults: .a | default .b | default "c"
        assert_eq!(
            transform("{{ .Values.a | default .Values.b | default \"c\" }}"),
            "{{ ((values.a or values.b) or \"c\") }}"
        );
    }

    #[test]
    fn test_index_list() {
        assert_eq!(
            transform("{{ index .Values.list 0 }}"),
            "{{ values.list[0] }}"
        );
    }

    #[test]
    fn test_index_map() {
        assert_eq!(
            transform("{{ index .Values.map \"key\" }}"),
            "{{ values.map[\"key\"] }}"
        );
    }

    #[test]
    fn test_index_nested() {
        assert_eq!(
            transform("{{ index .Values.data \"a\" \"b\" }}"),
            "{{ values.data[\"a\"][\"b\"] }}"
        );
    }

    // =========================================================================
    // Printf → string concatenation
    // =========================================================================

    #[test]
    fn test_printf_simple() {
        assert_eq!(
            transform("{{ printf \"%s-%s\" .Release.Name .Chart.Name }}"),
            "{{ (release.name ~ \"-\" ~ pack.name) }}"
        );
    }

    #[test]
    fn test_printf_complex() {
        assert_eq!(
            transform("{{ printf \"prefix-%s-suffix\" .Values.name }}"),
            "{{ (\"prefix-\" ~ values.name ~ \"-suffix\") }}"
        );
    }

    // =========================================================================
    // Contains → in operator
    // =========================================================================

    #[test]
    fn test_contains_function() {
        assert_eq!(
            transform("{{ contains \"needle\" .Values.haystack }}"),
            "{{ (\"needle\" in values.haystack) }}"
        );
    }

    #[test]
    fn test_contains_in_if() {
        assert_eq!(
            transform("{{- if contains $name .Release.Name }}yes{{- end }}"),
            "{%- if (name in release.name) %}yes{%- endif %}"
        );
    }

    // =========================================================================
    // Control structures
    // =========================================================================

    #[test]
    fn test_if_else() {
        assert_eq!(
            transform("{{- if .Values.x }}yes{{- else }}no{{- end }}"),
            "{%- if values.x %}yes{%- else %}no{%- endif %}"
        );
    }

    #[test]
    fn test_if_elif() {
        assert_eq!(
            transform("{{- if .Values.a }}a{{- else if .Values.b }}b{{- end }}"),
            "{%- if values.a %}a{%- elif values.b %}b{%- endif %}"
        );
    }

    #[test]
    fn test_range() {
        assert_eq!(
            transform("{{- range .Values.items }}{{ . }}{{- end }}"),
            "{%- for item in values.items %}{{ item }}{%- endfor %}"
        );
    }

    #[test]
    fn test_range_with_variable() {
        assert_eq!(
            transform("{{- range $item := .Values.list }}{{ $item }}{{- end }}"),
            "{%- for item in values.list %}{{ item }}{%- endfor %}"
        );
    }

    // =========================================================================
    // Variable declarations
    // =========================================================================

    #[test]
    fn test_variable_declaration() {
        assert_eq!(
            transform("{{- $name := .Values.name }}{{ $name }}"),
            "{%- set name = values.name %}{{ name }}"
        );
    }

    // =========================================================================
    // Include/define
    // =========================================================================

    #[test]
    fn test_define() {
        assert_eq!(
            transform("{{- define \"myapp.name\" }}test{{- end }}"),
            "{%- macro myapp_name() %}test{%- endmacro %}"
        );
    }

    #[test]
    fn test_include() {
        assert_eq!(
            transform_with_prefix("{{ include \"myapp.fullname\" . }}", "myapp"),
            "{{ fullname() }}"
        );
    }

    // =========================================================================
    // Filters
    // =========================================================================

    #[test]
    fn test_filter_pipeline() {
        assert_eq!(
            transform("{{ .Values.name | quote }}"),
            "{{ values.name | quote }}"
        );
    }

    #[test]
    fn test_filter_with_arg() {
        assert_eq!(
            transform("{{ .Values.text | indent 4 }}"),
            "{{ values.text | indent(4) }}"
        );
    }

    #[test]
    fn test_filter_chain() {
        assert_eq!(
            transform("{{ .Values.config | toYaml | nindent 4 }}"),
            "{{ values.config | toyaml | nindent(4) }}"
        );
    }

    // =========================================================================
    // List/Dict functions
    // =========================================================================

    #[test]
    fn test_list() {
        assert_eq!(transform("{{ list 1 2 3 }}"), "{{ [1, 2, 3] }}");
    }

    #[test]
    fn test_dict() {
        assert_eq!(
            transform("{{ dict \"key1\" .Values.a \"key2\" .Values.b }}"),
            "{{ {\"key1\": values.a, \"key2\": values.b} }}"
        );
    }

    // =========================================================================
    // Range generation
    // =========================================================================

    #[test]
    fn test_until() {
        assert_eq!(transform("{{ until 5 }}"), "{{ range(5) }}");
    }

    #[test]
    fn test_until_step() {
        assert_eq!(transform("{{ untilStep 0 10 2 }}"), "{{ range(0, 10, 2) }}");
    }

    // =========================================================================
    // Field mappings
    // =========================================================================

    #[test]
    fn test_release_service() {
        assert_eq!(transform("{{ .Release.Service }}"), "{{ \"Sherpack\" }}");
    }

    #[test]
    fn test_chart_appversion() {
        assert_eq!(
            transform("{{ .Chart.AppVersion }}"),
            "{{ pack.appVersion }}"
        );
    }

    #[test]
    fn test_capabilities() {
        assert_eq!(
            transform("{{ .Capabilities.KubeVersion.Version }}"),
            "{{ capabilities.kubeVersion.version }}"
        );
    }

    // =========================================================================
    // Dictionary iteration with TypeContext
    // =========================================================================

    #[test]
    fn test_range_dict_with_type_context() {
        use crate::type_inference::TypeContext;

        let yaml = r#"
controller:
  containerPort:
    http: 80
    https: 443
"#;
        let ctx = TypeContext::from_yaml(yaml).unwrap();
        let mut transformer = Transformer::new().with_type_context(ctx);

        let input = crate::parser::parse("{{- range $key, $value := .Values.controller.containerPort }}{{ $key }}: {{ $value }}{{- end }}").unwrap();
        let result = transformer.transform(&input);

        assert_eq!(
            result,
            "{%- for key, value in values.controller.containerPort | dictsort %}{{ key }}: {{ value }}{%- endfor %}"
        );
    }

    #[test]
    fn test_range_list_with_type_context() {
        use crate::type_inference::TypeContext;

        let yaml = r#"
controller:
  extraEnvs:
    - name: FOO
      value: bar
"#;
        let ctx = TypeContext::from_yaml(yaml).unwrap();
        let mut transformer = Transformer::new().with_type_context(ctx);

        let input = crate::parser::parse(
            "{{- range $i, $env := .Values.controller.extraEnvs }}{{ $env }}{{- end }}",
        )
        .unwrap();
        let result = transformer.transform(&input);

        // List iteration should NOT use dictsort
        assert_eq!(
            result,
            "{%- for env in values.controller.extraEnvs %}{#- i = loop.index0 #}{{ env }}{%- endfor %}"
        );
    }

    #[test]
    fn test_range_dict_heuristic() {
        // Without type context, should use heuristics
        let mut transformer = Transformer::new();

        // "containerPort" is in DICT_SUFFIXES
        let input = crate::parser::parse(
            "{{- range $key, $value := .Values.controller.containerPort }}{{ $key }}{{- end }}",
        )
        .unwrap();
        let result = transformer.transform(&input);

        assert_eq!(
            result,
            "{%- for key, value in values.controller.containerPort | dictsort %}{{ key }}{%- endfor %}"
        );
    }

    #[test]
    fn test_range_annotations_heuristic() {
        let mut transformer = Transformer::new();

        // "annotations" is in DICT_SUFFIXES
        let input = crate::parser::parse("{{- range $k, $v := .Values.podAnnotations }}{{- end }}")
            .unwrap();
        let result = transformer.transform(&input);

        assert_eq!(
            result,
            "{%- for k, v in values.podAnnotations | dictsort %}{%- endfor %}"
        );
    }
}
