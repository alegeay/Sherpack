//! Kubernetes-specific template filters
//!
//! These filters extend MiniJinja with Helm-compatible functionality.

use base64::Engine as _;
use minijinja::{Error, ErrorKind, Value, value::ValueKind};
use semver::{Version, VersionReq};

/// Convert a value to YAML format
///
/// Usage: {{ values.config | toyaml }}
pub fn toyaml(value: Value) -> Result<String, Error> {
    // Convert minijinja Value to serde_json::Value
    let json_value: serde_json::Value = serde_json::to_value(&value)
        .map_err(|e| Error::new(ErrorKind::InvalidOperation, e.to_string()))?;

    // Convert to YAML
    let yaml = serde_yaml::to_string(&json_value)
        .map_err(|e| Error::new(ErrorKind::InvalidOperation, e.to_string()))?;

    // Remove trailing newline and leading "---\n" if present
    let yaml = yaml.trim_start_matches("---\n").trim_end();

    Ok(yaml.to_string())
}

/// Convert a value to JSON format
///
/// Usage: {{ values.config | tojson }}
pub fn tojson(value: Value) -> Result<String, Error> {
    let json_value: serde_json::Value = serde_json::to_value(&value)
        .map_err(|e| Error::new(ErrorKind::InvalidOperation, e.to_string()))?;

    serde_json::to_string(&json_value)
        .map_err(|e| Error::new(ErrorKind::InvalidOperation, e.to_string()))
}

/// Convert a value to pretty-printed JSON
///
/// Usage: {{ values.config | tojson_pretty }}
pub fn tojson_pretty(value: Value) -> Result<String, Error> {
    let json_value: serde_json::Value = serde_json::to_value(&value)
        .map_err(|e| Error::new(ErrorKind::InvalidOperation, e.to_string()))?;

    serde_json::to_string_pretty(&json_value)
        .map_err(|e| Error::new(ErrorKind::InvalidOperation, e.to_string()))
}

/// Base64 encode a string
///
/// Usage: {{ secret | b64encode }}
#[must_use]
pub fn b64encode(value: String) -> String {
    base64::engine::general_purpose::STANDARD.encode(value.as_bytes())
}

/// Base64 decode a string
///
/// Usage: {{ encoded | b64decode }}
pub fn b64decode(value: String) -> Result<String, Error> {
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(value.as_bytes())
        .map_err(|e| Error::new(ErrorKind::InvalidOperation, format!("base64 decode error: {}", e)))?;

    String::from_utf8(decoded)
        .map_err(|e| Error::new(ErrorKind::InvalidOperation, format!("UTF-8 decode error: {}", e)))
}

/// Quote a string with double quotes
///
/// Usage: {{ name | quote }}
#[must_use]
pub fn quote(value: Value) -> String {
    let s = if let Some(str_val) = value.as_str() {
        str_val.to_string()
    } else {
        value.to_string()
    };
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

/// Quote a string with single quotes
///
/// Usage: {{ name | squote }}
#[must_use]
pub fn squote(value: Value) -> String {
    let s = if let Some(str_val) = value.as_str() {
        str_val.to_string()
    } else {
        value.to_string()
    };
    format!("'{}'", s.replace('\'', "''"))
}

/// Indent text with a newline prefix (like Helm's nindent)
///
/// Usage: {{ content | nindent(4) }}
#[must_use]
pub fn nindent(value: String, spaces: usize) -> String {
    let line_count = value.lines().count();
    // Pre-allocate: newline + (spaces + avg_line_length) * lines
    let mut result = String::with_capacity(1 + value.len() + spaces * line_count + line_count);
    result.push('\n');

    let indent = " ".repeat(spaces);
    let mut first = true;

    for line in value.lines() {
        if !first {
            result.push('\n');
        }
        first = false;

        if !line.is_empty() {
            result.push_str(&indent);
            result.push_str(line);
        }
    }

    result
}

/// Indent text without newline prefix
///
/// Usage: {{ content | indent(4) }}
pub fn indent(value: String, spaces: usize) -> String {
    let line_count = value.lines().count();
    // Pre-allocate: (spaces + avg_line_length) * lines
    let mut result = String::with_capacity(value.len() + spaces * line_count + line_count);

    let indent_str = " ".repeat(spaces);
    let mut first = true;

    for line in value.lines() {
        if !first {
            result.push('\n');
        }
        first = false;

        if !line.is_empty() {
            result.push_str(&indent_str);
        }
        result.push_str(line);
    }

    result
}

/// Require a value, fail if undefined or empty
///
/// Usage: {{ values.required_field | required("field is required") }}
pub fn required(value: Value, message: Option<String>) -> Result<Value, Error> {
    if value.is_undefined() || value.is_none() {
        let msg = message.unwrap_or_else(|| "required value is missing".to_string());
        Err(Error::new(ErrorKind::InvalidOperation, msg))
    } else if let Some(s) = value.as_str() {
        if s.is_empty() {
            let msg = message.unwrap_or_else(|| "required value is empty".to_string());
            Err(Error::new(ErrorKind::InvalidOperation, msg))
        } else {
            Ok(value)
        }
    } else {
        Ok(value)
    }
}

/// Check if a value is empty
///
/// Usage: {% if values.list | empty %}
pub fn empty(value: Value) -> bool {
    if value.is_undefined() || value.is_none() {
        return true;
    }

    match value.len() {
        Some(len) => len == 0,
        None => {
            if let Some(s) = value.as_str() {
                s.is_empty()
            } else {
                false
            }
        }
    }
}

/// Return the first non-empty value
///
/// Usage: {{ coalesce(values.a, values.b, "default") }}
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

/// Check if a dict has a key
///
/// Usage: {% if values | haskey("foo") %}
pub fn haskey(value: Value, key: String) -> bool {
    value.get_attr(&key).map(|v| !v.is_undefined()).unwrap_or(false)
}

/// Get all keys from a dict
///
/// Usage: {{ values | keys }}
pub fn keys(value: Value) -> Result<Vec<String>, Error> {
    match value.try_iter() {
        Ok(iter) => {
            let keys: Vec<String> = iter
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect();
            Ok(keys)
        }
        Err(_) => Err(Error::new(
            ErrorKind::InvalidOperation,
            "cannot get keys from non-mapping value",
        )),
    }
}

/// Deep merge two dicts
///
/// Usage: {{ dict1 | merge(dict2) }}
pub fn merge(base: Value, overlay: Value) -> Result<Value, Error> {
    let mut base_json: serde_json::Value = serde_json::to_value(&base)
        .map_err(|e| Error::new(ErrorKind::InvalidOperation, e.to_string()))?;
    let overlay_json: serde_json::Value = serde_json::to_value(&overlay)
        .map_err(|e| Error::new(ErrorKind::InvalidOperation, e.to_string()))?;

    deep_merge_json(&mut base_json, &overlay_json);

    Ok(Value::from_serialize(&base_json))
}

fn deep_merge_json(base: &mut serde_json::Value, overlay: &serde_json::Value) {
    match (base, overlay) {
        (serde_json::Value::Object(base_map), serde_json::Value::Object(overlay_map)) => {
            for (key, overlay_value) in overlay_map {
                match base_map.get_mut(key) {
                    Some(base_value) => deep_merge_json(base_value, overlay_value),
                    None => {
                        base_map.insert(key.clone(), overlay_value.clone());
                    }
                }
            }
        }
        (base, overlay) => {
            *base = overlay.clone();
        }
    }
}

/// SHA256 hash of a string
///
/// Usage: {{ value | sha256 }}
pub fn sha256sum(value: String) -> String {
    use sha2::{Sha256, Digest};
    let mut hasher = Sha256::new();
    hasher.update(value.as_bytes());
    format!("{:x}", hasher.finalize())
}

/// Truncate a string to a maximum length
///
/// Usage: {{ name | trunc(63) }}
pub fn trunc(value: String, length: usize) -> String {
    if value.len() <= length {
        value
    } else {
        value.chars().take(length).collect()
    }
}

/// Trim prefix from a string
///
/// Usage: {{ name | trimprefix("v") }}
pub fn trimprefix(value: String, prefix: String) -> String {
    value.strip_prefix(&prefix).unwrap_or(&value).to_string()
}

/// Trim suffix from a string
///
/// Usage: {{ name | trimsuffix(".yaml") }}
pub fn trimsuffix(value: String, suffix: String) -> String {
    value.strip_suffix(&suffix).unwrap_or(&value).to_string()
}

/// Convert to snake_case
///
/// Usage: {{ name | snakecase }}
pub fn snakecase(value: String) -> String {
    // Pre-allocate with extra space for potential underscores
    let mut result = String::with_capacity(value.len() + value.len() / 4);
    let mut prev_upper = false;

    for (i, c) in value.chars().enumerate() {
        if c.is_uppercase() {
            if i > 0 && !prev_upper {
                result.push('_');
            }
            result.push(c.to_lowercase().next().unwrap_or(c));
            prev_upper = true;
        } else if c == '-' || c == ' ' {
            result.push('_');
            prev_upper = false;
        } else {
            result.push(c);
            prev_upper = false;
        }
    }

    result
}

/// Convert to kebab-case
///
/// Usage: {{ name | kebabcase }}
pub fn kebabcase(value: String) -> String {
    snakecase(value).replace('_', "-")
}

/// Convert a list of values to a list of strings
///
/// Usage: {{ list(1, 2, 3) | tostrings }}
/// Result: ["1", "2", "3"]
///
/// This is the Helm-compatible toStrings function.
/// Each element is converted using its string representation.
///
/// ## Sherpack Extensions
///
/// Optional parameters (passed as kwargs):
/// - `prefix`: String to prepend to each element
/// - `suffix`: String to append to each element
/// - `skip_empty`: If true, skip empty/null values (default: false)
///
/// Examples:
/// ```jinja
/// {{ list(80, 443) | tostrings(prefix="port-") }}
/// → ["port-80", "port-443"]
///
/// {{ list(1, 2) | tostrings(suffix="/TCP") }}
/// → ["1/TCP", "2/TCP"]
///
/// {{ list("a", "", "c") | tostrings(skip_empty=true) }}
/// → ["a", "c"]
/// ```
pub fn tostrings(value: Value, kwargs: minijinja::value::Kwargs) -> Result<Vec<String>, Error> {
    // Extract optional parameters using simpler approach
    let prefix: String = kwargs.get("prefix").ok().flatten().unwrap_or_default();
    let suffix: String = kwargs.get("suffix").ok().flatten().unwrap_or_default();
    let skip_empty: bool = kwargs.get("skip_empty").ok().flatten().unwrap_or(false);

    // Ensure no unknown kwargs
    kwargs.assert_all_used()?;

    let has_prefix = !prefix.is_empty();
    let has_suffix = !suffix.is_empty();

    let convert_value = |v: Value| -> Option<String> {
        // Handle null/undefined
        if v.is_undefined() || v.is_none() {
            if skip_empty {
                return None;
            }
            return Some(String::new());
        }

        let s = if let Some(str_val) = v.as_str() {
            str_val.to_string()
        } else {
            v.to_string()
        };

        // Skip empty strings if requested
        if skip_empty && s.is_empty() {
            return None;
        }

        // Apply prefix and suffix if set
        if has_prefix || has_suffix {
            let mut result = String::with_capacity(s.len() + prefix.len() + suffix.len());
            result.push_str(&prefix);
            result.push_str(&s);
            result.push_str(&suffix);
            Some(result)
        } else {
            Some(s)
        }
    };

    match value.try_iter() {
        Ok(iter) => {
            let strings: Vec<String> = iter.filter_map(convert_value).collect();
            Ok(strings)
        }
        Err(_) => {
            // Single value - convert to single-element list
            match convert_value(value) {
                Some(s) => Ok(vec![s]),
                None => Ok(vec![]),
            }
        }
    }
}

/// Compare a version against a semver constraint
///
/// This filter implements Helm's semverCompare function.
/// The constraint can use standard semver operators:
/// - `>=1.0.0` - greater than or equal
/// - `<2.0.0` - less than
/// - `^1.0.0` - compatible with
/// - `~1.0.0` - approximately equivalent
///
/// Usage: {{ version | semver_match(">=1.21.0") }}
pub fn semver_match(version: Value, constraint: String) -> Result<bool, Error> {
    let version_str = version.as_str()
        .ok_or_else(|| Error::new(ErrorKind::InvalidOperation, "version must be a string"))?;

    // Clean up the version string (remove 'v' prefix if present)
    let version_clean = version_str.trim_start_matches('v');

    // Parse the version (handle Kubernetes-style versions like "1.31.0-0")
    let parsed_version = match Version::parse(version_clean) {
        Ok(v) => v,
        Err(_) => {
            // Try to parse as major.minor.patch only
            let parts: Vec<&str> = version_clean.split('-').next()
                .unwrap_or(version_clean)
                .split('.')
                .collect();

            if parts.len() >= 3 {
                let major: u64 = parts[0].parse().unwrap_or(0);
                let minor: u64 = parts[1].parse().unwrap_or(0);
                let patch: u64 = parts[2].parse().unwrap_or(0);
                Version::new(major, minor, patch)
            } else if parts.len() == 2 {
                let major: u64 = parts[0].parse().unwrap_or(0);
                let minor: u64 = parts[1].parse().unwrap_or(0);
                Version::new(major, minor, 0)
            } else {
                return Err(Error::new(ErrorKind::InvalidOperation, format!("Invalid version format: {}", version_str)));
            }
        }
    };

    // Clean up the constraint string (handle Kubernetes-style constraints)
    let constraint_clean = constraint.trim_start_matches(|c: char| c.is_whitespace());

    // Parse the constraint
    let req = VersionReq::parse(constraint_clean)
        .or_else(|_| {
            // Try to handle Kubernetes-style constraints like ">=1.31.0-0"
            let constraint_base = constraint_clean.split('-').next()
                .unwrap_or(constraint_clean);
            VersionReq::parse(constraint_base)
        })
        .map_err(|e| Error::new(ErrorKind::InvalidOperation, format!("Invalid constraint '{}': {}", constraint, e)))?;

    Ok(req.matches(&parsed_version))
}

/// Convert value to integer
pub fn int(value: Value) -> Result<i64, Error> {
    match value.kind() {
        ValueKind::Number => {
            // MiniJinja stores numbers that can be i64
            value.as_i64()
                .ok_or_else(|| Error::new(ErrorKind::InvalidOperation, "Cannot convert to int"))
        }
        ValueKind::String => {
            let s = value.as_str().unwrap_or("");
            s.parse::<i64>()
                .or_else(|_| s.parse::<f64>().map(|f| f as i64))
                .map_err(|_| Error::new(ErrorKind::InvalidOperation, format!("Cannot parse '{}' as int", s)))
        }
        ValueKind::Bool => Ok(if value.is_true() { 1 } else { 0 }),
        _ => Err(Error::new(ErrorKind::InvalidOperation, format!("Cannot convert {:?} to int", value.kind()))),
    }
}

/// Convert value to float
pub fn float(value: Value) -> Result<f64, Error> {
    match value.kind() {
        ValueKind::Number => {
            // Get as i64 and convert to f64
            value.as_i64()
                .map(|n| n as f64)
                .ok_or_else(|| Error::new(ErrorKind::InvalidOperation, "Cannot convert to float"))
        }
        ValueKind::String => {
            let s = value.as_str().unwrap_or("");
            s.parse::<f64>()
                .map_err(|_| Error::new(ErrorKind::InvalidOperation, format!("Cannot parse '{}' as float", s)))
        }
        ValueKind::Bool => Ok(if value.is_true() { 1.0 } else { 0.0 }),
        _ => Err(Error::new(ErrorKind::InvalidOperation, format!("Cannot convert {:?} to float", value.kind()))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_toyaml() {
        let value = Value::from_serialize(&serde_json::json!({
            "name": "test",
            "port": 8080
        }));
        let yaml = toyaml(value).unwrap();
        assert!(yaml.contains("name: test"));
        assert!(yaml.contains("port: 8080"));
    }

    #[test]
    fn test_b64encode_decode() {
        let original = "hello world".to_string();
        let encoded = b64encode(original.clone());
        let decoded = b64decode(encoded).unwrap();
        assert_eq!(original, decoded);
    }

    #[test]
    fn test_quote() {
        assert_eq!(quote(Value::from("test")), "\"test\"");
        assert_eq!(squote(Value::from("test")), "'test'");
    }

    #[test]
    fn test_nindent() {
        let input = "line1\nline2".to_string();
        let result = nindent(input, 4);
        assert_eq!(result, "\n    line1\n    line2");
    }

    #[test]
    fn test_required() {
        assert!(required(Value::from("test"), None).is_ok());
        assert!(required(Value::UNDEFINED, None).is_err());
        assert!(required(Value::from(""), None).is_err());
    }

    #[test]
    fn test_empty() {
        assert!(empty(Value::UNDEFINED));
        assert!(empty(Value::from("")));
        assert!(empty(Value::from_serialize(Vec::<i32>::new())));
        assert!(!empty(Value::from("test")));
    }

    #[test]
    fn test_trunc() {
        assert_eq!(trunc("hello".to_string(), 3), "hel");
        assert_eq!(trunc("hi".to_string(), 10), "hi");
    }

    #[test]
    fn test_snakecase() {
        assert_eq!(snakecase("camelCase".to_string()), "camel_case");
        assert_eq!(snakecase("PascalCase".to_string()), "pascal_case");
    }

    #[test]
    fn test_tostrings_list() {
        use minijinja::Environment;

        let mut env = Environment::new();
        env.add_filter("tostrings", tostrings);

        let result: Vec<String> = env
            .render_str("{{ [1, 2, 3] | tostrings }}", ())
            .unwrap()
            .trim_matches(|c| c == '[' || c == ']')
            .split(", ")
            .map(|s| s.trim_matches('"').to_string())
            .collect();
        assert_eq!(result, vec!["1", "2", "3"]);
    }

    #[test]
    fn test_tostrings_with_prefix() {
        use minijinja::Environment;

        let mut env = Environment::new();
        env.add_filter("tostrings", tostrings);

        let template = r#"{{ [80, 443] | tostrings(prefix="port-") | join(",") }}"#;
        let result = env.render_str(template, ()).unwrap();
        assert_eq!(result, "port-80,port-443");
    }

    #[test]
    fn test_tostrings_with_suffix() {
        use minijinja::Environment;

        let mut env = Environment::new();
        env.add_filter("tostrings", tostrings);

        let template = r#"{{ [1, 2] | tostrings(suffix="/TCP") | join(",") }}"#;
        let result = env.render_str(template, ()).unwrap();
        assert_eq!(result, "1/TCP,2/TCP");
    }

    #[test]
    fn test_tostrings_skip_empty() {
        use minijinja::Environment;

        let mut env = Environment::new();
        env.add_filter("tostrings", tostrings);

        let template = r#"{{ ["a", "", "c"] | tostrings(skip_empty=true) | join(",") }}"#;
        let result = env.render_str(template, ()).unwrap();
        assert_eq!(result, "a,c");
    }

    #[test]
    fn test_tostrings_mixed() {
        use minijinja::Environment;

        let mut env = Environment::new();
        env.add_filter("tostrings", tostrings);

        let template = r#"{{ ["hello", 42, true] | tostrings | join(",") }}"#;
        let result = env.render_str(template, ()).unwrap();
        assert_eq!(result, "hello,42,true");
    }
}
