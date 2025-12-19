//! Template functions (global functions available in templates)

use minijinja::value::{Object, Rest};
use minijinja::{Error, ErrorKind, State, Value};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Maximum recursion depth for tpl function (prevents infinite loops)
const MAX_TPL_DEPTH: usize = 10;

/// Key for storing tpl recursion depth in State's temp storage
const TPL_DEPTH_KEY: &str = "__sherpack_tpl_depth";

/// Counter object for tracking tpl recursion depth
/// Implements Object trait so it can be stored in State's temp storage
#[derive(Debug, Default)]
struct TplDepthCounter(AtomicUsize);

impl Object for TplDepthCounter {
    fn repr(self: &Arc<Self>) -> minijinja::value::ObjectRepr {
        minijinja::value::ObjectRepr::Plain
    }
}

impl TplDepthCounter {
    fn increment(&self) -> usize {
        self.0.fetch_add(1, Ordering::SeqCst) + 1
    }

    fn decrement(&self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

/// Fail with a custom error message
///
/// Usage: {{ fail("Something went wrong") }}
pub fn fail(message: String) -> Result<Value, Error> {
    Err(Error::new(ErrorKind::InvalidOperation, message))
}

/// Create a dict from key-value pairs
///
/// Usage: {{ dict("key1", value1, "key2", value2) }}
pub fn dict(args: Vec<Value>) -> Result<Value, Error> {
    if !args.len().is_multiple_of(2) {
        return Err(Error::new(
            ErrorKind::InvalidOperation,
            "dict requires an even number of arguments (key-value pairs)",
        ));
    }

    let mut map = serde_json::Map::new();

    for chunk in args.chunks(2) {
        let key = chunk[0]
            .as_str()
            .ok_or_else(|| Error::new(ErrorKind::InvalidOperation, "dict keys must be strings"))?;
        let value: serde_json::Value = serde_json::to_value(&chunk[1])
            .map_err(|e| Error::new(ErrorKind::InvalidOperation, e.to_string()))?;
        map.insert(key.to_string(), value);
    }

    Ok(Value::from_serialize(serde_json::Value::Object(map)))
}

/// Create a list from values
///
/// Usage: {{ list("a", "b", "c") }}
pub fn list(args: Vec<Value>) -> Value {
    Value::from(args)
}

/// Get a value with a default if undefined
///
/// Usage: {{ get(values, "key", "default") }}
pub fn get(obj: Value, key: String, default: Option<Value>) -> Value {
    match obj.get_attr(&key) {
        Ok(v) if !v.is_undefined() => v,
        _ => default.unwrap_or(Value::UNDEFINED),
    }
}

/// Set a key in a dict (returns new dict, original unchanged)
///
/// Usage: {{ set(mydict, "newkey", "newvalue") }}
pub fn set(dict: Value, key: String, val: Value) -> Result<Value, Error> {
    use minijinja::value::ValueKind;

    match dict.kind() {
        ValueKind::Map => {
            let mut result = indexmap::IndexMap::new();

            // Copy existing entries
            if let Ok(iter) = dict.try_iter() {
                for k in iter {
                    if let Some(k_str) = k.as_str()
                        && let Ok(v) = dict.get_item(&k)
                    {
                        result.insert(k_str.to_string(), v);
                    }
                }
            }

            // Set the new value
            result.insert(key, val);
            Ok(Value::from_iter(result))
        }
        _ => Err(Error::new(
            ErrorKind::InvalidOperation,
            format!("set requires a dict, got {:?}", dict.kind()),
        )),
    }
}

/// Remove a key from a dict (returns new dict, original unchanged)
///
/// Usage: {{ unset(mydict, "keytoremove") }}
pub fn unset(dict: Value, key: String) -> Result<Value, Error> {
    use minijinja::value::ValueKind;

    match dict.kind() {
        ValueKind::Map => {
            let mut result = indexmap::IndexMap::new();

            if let Ok(iter) = dict.try_iter() {
                for k in iter {
                    if let Some(k_str) = k.as_str()
                        && k_str != key
                        && let Ok(v) = dict.get_item(&k)
                    {
                        result.insert(k_str.to_string(), v);
                    }
                }
            }

            Ok(Value::from_iter(result))
        }
        _ => Err(Error::new(
            ErrorKind::InvalidOperation,
            format!("unset requires a dict, got {:?}", dict.kind()),
        )),
    }
}

/// Deep get with path and default value
///
/// Usage: {{ dig(mydict, "a", "b", "c", "default") }}
/// Equivalent to mydict.a.b.c with fallback to default if any key is missing
pub fn dig(dict: Value, keys_and_default: Rest<Value>) -> Result<Value, Error> {
    let args: &[Value] = &keys_and_default;

    if args.is_empty() {
        return Err(Error::new(
            ErrorKind::InvalidOperation,
            "dig requires at least one key and a default value",
        ));
    }

    // Last argument is the default value, rest are keys
    let (keys, default_slice) = args.split_at(args.len() - 1);
    let default = default_slice.first().cloned().unwrap_or(Value::UNDEFINED);

    if keys.is_empty() {
        // Only default was provided, return the dict itself
        return Ok(dict);
    }

    // Traverse the path
    let mut current = dict;
    for key in keys {
        match key.as_str() {
            Some(k) => match current.get_attr(k) {
                Ok(v) if !v.is_undefined() => current = v,
                _ => return Ok(default),
            },
            None => {
                // Handle integer keys for lists
                if let Some(idx) = key.as_i64() {
                    match current.get_item(&Value::from(idx)) {
                        Ok(v) if !v.is_undefined() => current = v,
                        _ => return Ok(default),
                    }
                } else {
                    return Ok(default);
                }
            }
        }
    }

    Ok(current)
}

/// Return first non-empty value
///
/// Usage: {{ coalesce(a, b, c) }}
pub fn coalesce(args: Vec<Value>) -> Value {
    for arg in args {
        if !arg.is_undefined() && !arg.is_none() {
            if let Some(s) = arg.as_str() {
                if !s.is_empty() {
                    return arg;
                }
            } else {
                return arg;
            }
        }
    }
    Value::UNDEFINED
}

/// Ternary operator
///
/// Usage: {{ ternary(true_value, false_value, condition) }}
pub fn ternary(true_val: Value, false_val: Value, condition: Value) -> Value {
    if condition.is_true() {
        true_val
    } else {
        false_val
    }
}

/// Generate a UUID (v4)
///
/// Usage: {{ uuidv4() }}
pub fn uuidv4() -> String {
    // Simple UUID v4 generation without external dependency
    use std::time::{SystemTime, UNIX_EPOCH};

    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();

    let random_part = timestamp ^ (timestamp >> 32);

    format!(
        "{:08x}-{:04x}-4{:03x}-{:04x}-{:012x}",
        (random_part & 0xFFFFFFFF) as u32,
        ((random_part >> 32) & 0xFFFF) as u16,
        ((random_part >> 48) & 0x0FFF) as u16,
        (((random_part >> 60) & 0x3F) | 0x80) as u16 | ((random_part & 0xFF) << 8) as u16,
        (random_part ^ (random_part >> 16)) & 0xFFFFFFFFFFFF
    )
}

/// Convert a value to a string representation
///
/// Usage: {{ tostring(value) }}
pub fn tostring(value: Value) -> String {
    if let Some(s) = value.as_str() {
        s.to_string()
    } else {
        value.to_string()
    }
}

/// Convert a value to an integer
///
/// Usage: {{ toint(value) }}
pub fn toint(value: Value) -> Result<i64, Error> {
    if let Some(n) = value.as_i64() {
        Ok(n)
    } else if let Some(s) = value.as_str() {
        s.parse::<i64>().map_err(|_| {
            Error::new(
                ErrorKind::InvalidOperation,
                format!("cannot convert '{}' to int", s),
            )
        })
    } else {
        Err(Error::new(
            ErrorKind::InvalidOperation,
            format!("cannot convert {:?} to int", value),
        ))
    }
}

/// Convert a value to a float
///
/// Usage: {{ tofloat(value) }}
pub fn tofloat(value: Value) -> Result<f64, Error> {
    if let Some(n) = value.as_i64() {
        Ok(n as f64)
    } else if let Some(s) = value.as_str() {
        s.parse::<f64>().map_err(|_| {
            Error::new(
                ErrorKind::InvalidOperation,
                format!("cannot convert '{}' to float", s),
            )
        })
    } else {
        Err(Error::new(
            ErrorKind::InvalidOperation,
            format!("cannot convert {:?} to float", value),
        ))
    }
}

/// Get current timestamp
///
/// Usage: {{ now() }}
pub fn now() -> String {
    chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string()
}

/// Printf-style formatting
///
/// Usage: {{ printf("%s-%d", name, count) }}
///
/// Supports format specifiers: %s, %d, %f, %v, %%
pub fn printf(format: String, args: Vec<Value>) -> Result<String, Error> {
    // Pre-allocate with estimated size
    let mut result = String::with_capacity(format.len() + args.len() * 10);
    let mut chars = format.chars().peekable();
    let mut arg_idx = 0;

    while let Some(c) = chars.next() {
        if c != '%' {
            result.push(c);
            continue;
        }

        // Handle format specifier
        let format_char = match chars.next() {
            Some(fc) => fc,
            None => {
                // Trailing % at end of string
                result.push('%');
                break;
            }
        };

        // Handle escaped %%
        if format_char == '%' {
            result.push('%');
            continue;
        }

        // Need an argument for this specifier
        if arg_idx >= args.len() {
            return Err(Error::new(
                ErrorKind::InvalidOperation,
                "not enough arguments for format string",
            ));
        }

        let arg = &args[arg_idx];
        match format_char {
            's' | 'v' => result.push_str(&arg.to_string()),
            'd' => {
                if let Some(n) = arg.as_i64() {
                    result.push_str(&n.to_string());
                } else {
                    result.push_str(&arg.to_string());
                }
            }
            'f' => {
                if let Some(n) = arg.as_i64() {
                    result.push_str(&(n as f64).to_string());
                } else {
                    result.push_str(&arg.to_string());
                }
            }
            _ => {
                // Unknown format specifier, treat as %v
                result.push_str(&arg.to_string());
            }
        }
        arg_idx += 1;
    }

    Ok(result)
}

/// Evaluate a string as a template (Helm's tpl function)
///
/// Usage: {{ tpl(values.dynamicTemplate, ctx) }}
///
/// This allows template strings stored in values to contain Jinja expressions.
/// The context parameter provides the variables available to the nested template.
///
/// ## Security Features (Sherpack improvements over Helm)
///
/// - **Recursion limit**: Maximum depth of 10 to prevent infinite loops
/// - **Source tracking**: Better error messages showing template origin
///
/// ## Example
///
/// In values.yaml:
/// ```yaml
/// host: "{{ release.name }}.example.com"
/// ```
///
/// Then in template:
/// ```jinja
/// host: {{ tpl(values.host, {"release": release}) }}
/// ```
/// Result: `host: myrelease.example.com`
pub fn tpl(state: &State, template: String, context: Value) -> Result<String, Error> {
    // Skip if no template markers present (optimization)
    if !template.contains("{{") && !template.contains("{%") {
        return Ok(template);
    }

    // Check recursion depth to prevent infinite loops
    let depth = increment_tpl_depth(state)?;

    // Render the template string using the current environment
    let result = state.env().render_str(&template, context).map_err(|e| {
        // Enhance error message with tpl context
        Error::new(
            ErrorKind::InvalidOperation,
            format!(
                "tpl error (depth {}): {}\n  Template: \"{}\"",
                depth,
                e,
                truncate_for_error(&template, 60)
            ),
        )
    });

    // Decrement depth after rendering (for sibling tpl calls)
    decrement_tpl_depth(state);

    result
}

/// Increment tpl recursion depth, returning error if limit exceeded
fn increment_tpl_depth(state: &State) -> Result<usize, Error> {
    let counter = state.get_or_set_temp_object(TPL_DEPTH_KEY, TplDepthCounter::default);
    let depth = counter.increment();

    if depth > MAX_TPL_DEPTH {
        Err(Error::new(
            ErrorKind::InvalidOperation,
            format!(
                "tpl recursion depth {} exceeded maximum {} - possible infinite loop in values. \
                 Check for circular references in template strings.",
                depth, MAX_TPL_DEPTH
            ),
        ))
    } else {
        Ok(depth)
    }
}

/// Decrement tpl recursion depth
fn decrement_tpl_depth(state: &State) {
    let counter = state.get_or_set_temp_object(TPL_DEPTH_KEY, TplDepthCounter::default);
    counter.decrement();
}

/// Truncate string for error messages
fn truncate_for_error(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len])
    }
}

/// Kubernetes resource lookup (Helm-compatible)
///
/// Usage: {{ lookup("v1", "Secret", "default", "my-secret") }}
///
/// **IMPORTANT:** In template-only mode (sherpack template), this function
/// always returns an empty object, matching Helm's behavior.
///
/// Parameters:
/// - apiVersion: API version (e.g., "v1", "apps/v1")
/// - kind: Resource kind (e.g., "Secret", "ConfigMap", "Deployment")
/// - namespace: Namespace (empty string "" for cluster-scoped resources)
/// - name: Resource name (empty string "" to list all resources)
///
/// Return values:
/// - Single resource: Returns the resource as a dict
/// - List (name=""): Returns {"items": [...]} dict
/// - Not found / template mode: Returns empty dict {}
///
/// ## Why lookup returns empty in template mode
///
/// Like Helm, Sherpack separates template rendering from cluster operations:
/// - `sherpack template`: Pure rendering, no cluster access → lookup returns {}
/// - `sherpack install/upgrade`: Cluster access for apply, but lookup still empty
///
/// ## Alternatives to lookup
///
/// Instead of using lookup, consider these Sherpack patterns:
///
/// 1. **Check if resource exists**: Use sync-waves to create dependencies
///    ```yaml
///    sherpack.io/sync-wave: "0"  # Create first
///    ---
///    sherpack.io/sync-wave: "1"  # Created after wave 0 is ready
///    ```
///
/// 2. **Reuse existing secrets**: Use external-secrets or hooks
///    ```yaml
///    sherpack.io/hook: pre-install
///    sherpack.io/hook-weight: "-5"
///    ```
///
/// 3. **Conditional resources**: Use values-based conditions
///    ```jinja
///    {%- if values.existingSecret %}
///    secretName: {{ values.existingSecret }}
///    {%- else %}
///    secretName: {{ release.name }}-secret
///    {%- endif %}
///    ```
pub fn lookup(api_version: String, kind: String, namespace: String, name: String) -> Value {
    // Log what was requested (useful for debugging/migration from Helm)
    // In template mode, this always returns empty - matching Helm behavior
    let _ = (api_version, kind, namespace, name); // Acknowledge params

    // Return empty dict - same as Helm's `helm template` behavior
    // This ensures charts work in GitOps workflows and CI/CD pipelines
    Value::from_serialize(serde_json::json!({}))
}

/// Evaluate a string as a template with full context (convenience version)
///
/// Usage: {{ tpl_ctx(values.dynamicTemplate) }}
///
/// This version automatically passes the full template context (values, release, pack, etc.)
/// to the nested template, similar to Helm's `tpl $str .` pattern.
///
/// ## Security Features
///
/// - **Recursion limit**: Shares depth counter with `tpl()`, max depth 10
/// - **Full context**: Passes values, release, pack, capabilities, template
pub fn tpl_ctx(state: &State, template: String) -> Result<String, Error> {
    // Skip if no template markers present (optimization)
    if !template.contains("{{") && !template.contains("{%") {
        return Ok(template);
    }

    // Check recursion depth to prevent infinite loops
    let depth = increment_tpl_depth(state)?;

    // Build context from all available variables
    let mut ctx = serde_json::Map::new();

    // Try to lookup and add standard context variables
    for var in ["values", "release", "pack", "capabilities", "template"] {
        if let Some(v) = state.lookup(var)
            && !v.is_undefined()
            && let Ok(json_val) = serde_json::to_value(&v)
        {
            ctx.insert(var.to_string(), json_val);
        }
    }

    let context = Value::from_serialize(serde_json::Value::Object(ctx));

    let result = state.env().render_str(&template, context).map_err(|e| {
        Error::new(
            ErrorKind::InvalidOperation,
            format!(
                "tpl_ctx error (depth {}): {}\n  Template: \"{}\"",
                depth,
                e,
                truncate_for_error(&template, 60)
            ),
        )
    });

    // Decrement depth after rendering
    decrement_tpl_depth(state);

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dict() {
        let result = dict(vec![
            Value::from("key1"),
            Value::from("value1"),
            Value::from("key2"),
            Value::from(42),
        ])
        .unwrap();

        assert_eq!(result.get_attr("key1").unwrap().as_str(), Some("value1"));
    }

    #[test]
    fn test_list() {
        let result = list(vec![Value::from("a"), Value::from("b"), Value::from("c")]);
        assert_eq!(result.len(), Some(3));
    }

    #[test]
    fn test_ternary() {
        assert_eq!(
            ternary(Value::from("yes"), Value::from("no"), Value::from(true)).as_str(),
            Some("yes")
        );
        assert_eq!(
            ternary(Value::from("yes"), Value::from("no"), Value::from(false)).as_str(),
            Some("no")
        );
    }

    #[test]
    fn test_printf() {
        let result = printf(
            "Hello %s, you have %d messages".to_string(),
            vec![Value::from("Alice"), Value::from(5)],
        )
        .unwrap();
        assert_eq!(result, "Hello Alice, you have 5 messages");
    }

    #[test]
    fn test_tpl_integration() {
        use minijinja::Environment;

        // Test tpl via full environment (since it needs State)
        let mut env = Environment::new();
        env.add_function("tpl", super::tpl);

        let template = r#"{{ tpl("Hello {{ name }}!", {"name": "World"}) }}"#;
        let result = env.render_str(template, ()).unwrap();
        assert_eq!(result, "Hello World!");
    }

    #[test]
    fn test_tpl_no_markers() {
        use minijinja::Environment;

        // Plain string without template markers should be returned as-is
        let mut env = Environment::new();
        env.add_function("tpl", super::tpl);

        let template = r#"{{ tpl("plain text", {}) }}"#;
        let result = env.render_str(template, ()).unwrap();
        assert_eq!(result, "plain text");
    }

    #[test]
    fn test_tpl_complex() {
        use minijinja::Environment;

        let mut env = Environment::new();
        env.add_function("tpl", super::tpl);

        // Test with conditional
        let template =
            r#"{{ tpl("{% if enabled %}yes{% else %}no{% endif %}", {"enabled": true}) }}"#;
        let result = env.render_str(template, ()).unwrap();
        assert_eq!(result, "yes");
    }

    #[test]
    fn test_tpl_recursion_limit() {
        use minijinja::Environment;

        let mut env = Environment::new();
        env.add_function("tpl", super::tpl);

        // Create a deeply nested tpl call that would exceed MAX_TPL_DEPTH
        // Each nested tpl increases depth by 1
        let template = r#"{{ tpl("{{ tpl(\"{{ tpl(\\\"{{ tpl(\\\\\\\"{{ tpl(\\\\\\\\\\\\\\\"{{ tpl(\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\"{{ tpl(\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\"{{ tpl(\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\"{{ tpl(\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\"done\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\", {}) }}\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\", {}) }}\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\", {}) }}\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\", {}) }}\\\\\\\\\\\\\\\", {}) }}\\\\\\\"  , {}) }}\\\\\\\", {}) }}\\\", {}) }}\", {}) }}", {}) }}"#;

        let result = env.render_str(template, ());

        // Should fail with recursion limit error
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.to_string().contains("recursion") || err.to_string().contains("depth"),
            "Expected recursion error, got: {}",
            err
        );
    }

    #[test]
    fn test_tpl_nested_valid() {
        use minijinja::Environment;

        let mut env = Environment::new();
        env.add_function("tpl", super::tpl);

        // 3 levels of nesting should work fine
        let template = r#"{{ tpl("{{ tpl(\"{{ tpl(\\\"level3\\\", {}) }}\", {}) }}", {}) }}"#;
        let result = env.render_str(template, ()).unwrap();
        assert_eq!(result, "level3");
    }

    #[test]
    fn test_truncate_for_error() {
        assert_eq!(truncate_for_error("short", 10), "short");
        assert_eq!(
            truncate_for_error("this is a longer string", 10),
            "this is a ..."
        );
    }

    #[test]
    fn test_lookup_returns_empty() {
        // lookup should return empty dict in template mode
        let result = lookup(
            "v1".to_string(),
            "Secret".to_string(),
            "default".to_string(),
            "my-secret".to_string(),
        );

        // Should be an empty object, not undefined
        assert!(!result.is_undefined());
        // Should be iterable (dict)
        assert!(result.try_iter().is_ok());
    }

    #[test]
    fn test_lookup_in_template() {
        use minijinja::Environment;

        let mut env = Environment::new();
        env.add_function("lookup", super::lookup);

        // Common Helm pattern: check if secret exists
        let template = r#"{% set secret = lookup("v1", "Secret", "default", "my-secret") %}{% if secret %}secret exists{% else %}no secret{% endif %}"#;
        let result = env.render_str(template, ()).unwrap();
        // Empty dict is falsy, so we get "no secret"
        assert_eq!(result, "no secret");
    }

    #[test]
    fn test_lookup_conditional_pattern() {
        use minijinja::Environment;

        let mut env = Environment::new();
        env.add_function("lookup", super::lookup);

        // Recommended pattern: check if lookup result exists before accessing properties
        let template = r#"{% set secret = lookup("v1", "Secret", "ns", "s") %}{% if secret.data is defined %}{{ secret.data.password }}{% else %}generated{% endif %}"#;
        let result = env.render_str(template, ()).unwrap();
        assert_eq!(result, "generated");
    }

    #[test]
    fn test_lookup_safe_pattern() {
        use crate::filters::tojson;
        use minijinja::Environment;

        let mut env = Environment::new();
        env.add_function("lookup", super::lookup);
        env.add_function("get", super::get);
        env.add_filter("tojson", tojson);

        // Safe pattern for strict mode: use get() with default
        let template = r#"{% set secret = lookup("v1", "Secret", "ns", "s") %}{{ get(secret, "data", {}) | tojson }}"#;
        let result = env.render_str(template, ()).unwrap();
        assert_eq!(result, "{}");
    }

    #[test]
    fn test_set_function() {
        use minijinja::Environment;

        let mut env = Environment::new();
        env.add_function("set", super::set);

        let template = r#"{% set d = {"a": 1} %}{{ set(d, "b", 2) }}"#;
        let result = env.render_str(template, ()).unwrap();
        assert!(result.contains("a") && result.contains("b"));
    }

    #[test]
    fn test_unset_function() {
        use minijinja::Environment;

        let mut env = Environment::new();
        env.add_function("unset", super::unset);

        let template = r#"{% set d = {"a": 1, "b": 2} %}{{ unset(d, "a") }}"#;
        let result = env.render_str(template, ()).unwrap();
        assert!(!result.contains("a") && result.contains("b"));
    }

    #[test]
    fn test_dig_function() {
        use minijinja::Environment;

        let mut env = Environment::new();
        env.add_function("dig", super::dig);

        // Test deep access that exists
        let template =
            r#"{% set d = {"a": {"b": {"c": "found"}}} %}{{ dig(d, "a", "b", "c", "default") }}"#;
        let result = env.render_str(template, ()).unwrap();
        assert_eq!(result, "found");

        // Test deep access that doesn't exist (returns default)
        let template2 = r#"{% set d = {"a": {"b": {}}} %}{{ dig(d, "a", "b", "c", "default") }}"#;
        let result2 = env.render_str(template2, ()).unwrap();
        assert_eq!(result2, "default");
    }
}
