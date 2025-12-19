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

/// Convert value to integer (truncates floats)
pub fn int(value: Value) -> Result<i64, Error> {
    match value.kind() {
        ValueKind::Number => {
            // Try i64 first, then f64 (truncated)
            if let Some(i) = value.as_i64() {
                Ok(i)
            } else if let Ok(f) = f64::try_from(value.clone()) {
                Ok(f as i64)
            } else {
                Err(Error::new(ErrorKind::InvalidOperation, "Cannot convert to int"))
            }
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
            // Try f64 conversion (handles both int and float)
            f64::try_from(value)
                .map_err(|_| Error::new(ErrorKind::InvalidOperation, "Cannot convert to float"))
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

/// Absolute value
pub fn abs(value: Value) -> Result<Value, Error> {
    match value.kind() {
        ValueKind::Number => {
            if let Some(i) = value.as_i64() {
                Ok(Value::from(i.abs()))
            } else if let Ok(f) = f64::try_from(value) {
                Ok(Value::from(f.abs()))
            } else {
                Err(Error::new(ErrorKind::InvalidOperation, "Cannot get absolute value"))
            }
        }
        _ => Err(Error::new(ErrorKind::InvalidOperation, format!("abs requires a number, got {:?}", value.kind()))),
    }
}

// =============================================================================
// Path Functions
// =============================================================================

/// Extract filename from path
/// {{ "/etc/nginx/nginx.conf" | basename }}  →  "nginx.conf"
pub fn basename(path: String) -> String {
    std::path::Path::new(&path)
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default()
}

/// Extract directory from path
/// {{ "/etc/nginx/nginx.conf" | dirname }}  →  "/etc/nginx"
pub fn dirname(path: String) -> String {
    std::path::Path::new(&path)
        .parent()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default()
}

/// Extract file extension (without the dot)
/// {{ "file.tar.gz" | extname }}  →  "gz"
pub fn extname(path: String) -> String {
    std::path::Path::new(&path)
        .extension()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default()
}

/// Clean/normalize path (no filesystem access)
/// {{ "a/b/../c/./d" | cleanpath }}  →  "a/c/d"
pub fn cleanpath(path: String) -> String {
    let is_absolute = path.starts_with('/');
    let mut parts: Vec<&str> = vec![];

    for part in path.split('/') {
        match part {
            "" | "." => continue,
            ".." => { parts.pop(); }
            _ => parts.push(part),
        }
    }

    let result = parts.join("/");
    if is_absolute {
        format!("/{}", result)
    } else if result.is_empty() {
        ".".to_string()
    } else {
        result
    }
}

// =============================================================================
// Regex Functions
// =============================================================================

/// Check if string matches regex pattern
/// {% if name | regex_match("^v[0-9]+") %}matched{% endif %}
pub fn regex_match(value: String, pattern: String) -> Result<bool, Error> {
    regex::Regex::new(&pattern)
        .map(|re| re.is_match(&value))
        .map_err(|e| Error::new(ErrorKind::InvalidOperation, format!("invalid regex '{}': {}", pattern, e)))
}

/// Replace all matches with replacement (supports capture groups: $1, $2, etc.)
/// {{ "v1.2.3" | regex_replace("v([0-9]+)", "version-$1") }}  →  "version-1.2.3"
pub fn regex_replace(value: String, pattern: String, replacement: String) -> Result<String, Error> {
    regex::Regex::new(&pattern)
        .map(|re| re.replace_all(&value, replacement.as_str()).to_string())
        .map_err(|e| Error::new(ErrorKind::InvalidOperation, format!("invalid regex '{}': {}", pattern, e)))
}

/// Find first match, returns empty string if no match
/// {{ "port: 8080" | regex_find("[0-9]+") }}  →  "8080"
pub fn regex_find(value: String, pattern: String) -> Result<String, Error> {
    regex::Regex::new(&pattern)
        .map(|re| re.find(&value).map(|m| m.as_str().to_string()).unwrap_or_default())
        .map_err(|e| Error::new(ErrorKind::InvalidOperation, format!("invalid regex '{}': {}", pattern, e)))
}

/// Find all matches
/// {{ "a1b2c3" | regex_find_all("[0-9]+") }}  →  ["1", "2", "3"]
pub fn regex_find_all(value: String, pattern: String) -> Result<Vec<String>, Error> {
    regex::Regex::new(&pattern)
        .map(|re| re.find_iter(&value).map(|m| m.as_str().to_string()).collect())
        .map_err(|e| Error::new(ErrorKind::InvalidOperation, format!("invalid regex '{}': {}", pattern, e)))
}

// =============================================================================
// Dict Functions
// =============================================================================

/// Get all values from a dict as a list
/// {{ mydict | values }}  →  [value1, value2, ...]
pub fn values(dict: Value) -> Result<Value, Error> {
    match dict.kind() {
        ValueKind::Map => {
            let items: Vec<Value> = dict.try_iter()
                .map_err(|_| Error::new(ErrorKind::InvalidOperation, "cannot iterate dict"))?
                .filter_map(|k| dict.get_item(&k).ok())
                .collect();
            Ok(Value::from(items))
        }
        _ => Err(Error::new(ErrorKind::InvalidOperation, format!("values requires a dict, got {:?}", dict.kind()))),
    }
}

/// Select only specified keys from dict
/// {{ mydict | pick("name", "version") }}
pub fn pick(dict: Value, keys: &[Value]) -> Result<Value, Error> {
    match dict.kind() {
        ValueKind::Map => {
            let mut result = indexmap::IndexMap::new();
            for key in keys {
                if let Some(key_str) = key.as_str() {
                    if let Ok(val) = dict.get_item(key) {
                        result.insert(key_str.to_string(), val);
                    }
                }
            }
            Ok(Value::from_iter(result))
        }
        _ => Err(Error::new(ErrorKind::InvalidOperation, format!("pick requires a dict, got {:?}", dict.kind()))),
    }
}

/// Exclude specified keys from dict
/// {{ mydict | omit("password", "secret") }}
pub fn omit(dict: Value, keys: &[Value]) -> Result<Value, Error> {
    match dict.kind() {
        ValueKind::Map => {
            let exclude: std::collections::HashSet<String> = keys.iter()
                .filter_map(|k| k.as_str().map(|s| s.to_string()))
                .collect();

            let mut result = indexmap::IndexMap::new();
            if let Ok(iter) = dict.try_iter() {
                for key in iter {
                    if let Some(key_str) = key.as_str() {
                        if !exclude.contains(key_str) {
                            if let Ok(val) = dict.get_item(&key) {
                                result.insert(key_str.to_string(), val);
                            }
                        }
                    }
                }
            }
            Ok(Value::from_iter(result))
        }
        _ => Err(Error::new(ErrorKind::InvalidOperation, format!("omit requires a dict, got {:?}", dict.kind()))),
    }
}

// =============================================================================
// List Functions
// =============================================================================

/// Append item to end of list (returns new list)
/// {{ items | append("new") }}
pub fn append(list: Value, item: Value) -> Result<Value, Error> {
    match list.kind() {
        ValueKind::Seq => {
            let mut items: Vec<Value> = list.try_iter()
                .map_err(|_| Error::new(ErrorKind::InvalidOperation, "cannot iterate list"))?
                .collect();
            items.push(item);
            Ok(Value::from(items))
        }
        _ => Err(Error::new(ErrorKind::InvalidOperation, format!("append requires a list, got {:?}", list.kind()))),
    }
}

/// Prepend item to start of list (returns new list)
/// {{ items | prepend("first") }}
pub fn prepend(list: Value, item: Value) -> Result<Value, Error> {
    match list.kind() {
        ValueKind::Seq => {
            let mut items: Vec<Value> = vec![item];
            items.extend(list.try_iter()
                .map_err(|_| Error::new(ErrorKind::InvalidOperation, "cannot iterate list"))?);
            Ok(Value::from(items))
        }
        _ => Err(Error::new(ErrorKind::InvalidOperation, format!("prepend requires a list, got {:?}", list.kind()))),
    }
}

/// Concatenate two lists
/// {{ list1 | concat(list2) }}
pub fn concat(list1: Value, list2: Value) -> Result<Value, Error> {
    match (list1.kind(), list2.kind()) {
        (ValueKind::Seq, ValueKind::Seq) => {
            let mut items: Vec<Value> = list1.try_iter()
                .map_err(|_| Error::new(ErrorKind::InvalidOperation, "cannot iterate first list"))?
                .collect();
            items.extend(list2.try_iter()
                .map_err(|_| Error::new(ErrorKind::InvalidOperation, "cannot iterate second list"))?);
            Ok(Value::from(items))
        }
        _ => Err(Error::new(ErrorKind::InvalidOperation, "concat requires two lists")),
    }
}

/// Remove specified values from list
/// {{ items | without("a", "b") }}
pub fn without(list: Value, exclude: &[Value]) -> Result<Value, Error> {
    match list.kind() {
        ValueKind::Seq => {
            let items: Vec<Value> = list.try_iter()
                .map_err(|_| Error::new(ErrorKind::InvalidOperation, "cannot iterate list"))?
                .filter(|item| !exclude.contains(item))
                .collect();
            Ok(Value::from(items))
        }
        _ => Err(Error::new(ErrorKind::InvalidOperation, format!("without requires a list, got {:?}", list.kind()))),
    }
}

/// Remove empty/falsy values from list
/// {{ ["a", "", null, "b"] | compact }}  →  ["a", "b"]
pub fn compact(list: Value) -> Result<Value, Error> {
    match list.kind() {
        ValueKind::Seq => {
            let items: Vec<Value> = list.try_iter()
                .map_err(|_| Error::new(ErrorKind::InvalidOperation, "cannot iterate list"))?
                .filter(|item| {
                    match item.kind() {
                        ValueKind::Undefined | ValueKind::None => false,
                        ValueKind::String => !item.as_str().unwrap_or("").is_empty(),
                        ValueKind::Seq => item.len().unwrap_or(0) > 0,
                        ValueKind::Map => item.len().unwrap_or(0) > 0,
                        _ => true,
                    }
                })
                .collect();
            Ok(Value::from(items))
        }
        _ => Err(Error::new(ErrorKind::InvalidOperation, format!("compact requires a list, got {:?}", list.kind()))),
    }
}

// =============================================================================
// Math Functions
// =============================================================================

/// Floor: round down to nearest integer
/// {{ 3.7 | floor }}  →  3
pub fn floor(value: Value) -> Result<i64, Error> {
    match value.kind() {
        ValueKind::Number => {
            if let Some(i) = value.as_i64() {
                Ok(i)
            } else if let Ok(f) = f64::try_from(value) {
                Ok(f.floor() as i64)
            } else {
                Err(Error::new(ErrorKind::InvalidOperation, "cannot convert to number"))
            }
        }
        _ => Err(Error::new(ErrorKind::InvalidOperation, format!("floor requires a number, got {:?}", value.kind()))),
    }
}

/// Ceil: round up to nearest integer
/// {{ 3.2 | ceil }}  →  4
pub fn ceil(value: Value) -> Result<i64, Error> {
    match value.kind() {
        ValueKind::Number => {
            if let Some(i) = value.as_i64() {
                Ok(i)
            } else if let Ok(f) = f64::try_from(value) {
                Ok(f.ceil() as i64)
            } else {
                Err(Error::new(ErrorKind::InvalidOperation, "cannot convert to number"))
            }
        }
        _ => Err(Error::new(ErrorKind::InvalidOperation, format!("ceil requires a number, got {:?}", value.kind()))),
    }
}

// =============================================================================
// Crypto Functions
// =============================================================================

/// SHA-1 hash (hex encoded)
/// {{ "hello" | sha1 }}
pub fn sha1sum(value: String) -> String {
    use sha1::{Sha1, Digest};
    let mut hasher = Sha1::new();
    hasher.update(value.as_bytes());
    format!("{:x}", hasher.finalize())
}

/// SHA-512 hash (hex encoded)
/// {{ "hello" | sha512 }}
pub fn sha512sum(value: String) -> String {
    use sha2::{Sha512, Digest};
    let mut hasher = Sha512::new();
    hasher.update(value.as_bytes());
    format!("{:x}", hasher.finalize())
}

/// MD5 hash (hex encoded) - Note: MD5 is cryptographically broken, use only for checksums
/// {{ "hello" | md5 }}
pub fn md5sum(value: String) -> String {
    use md5::{Md5, Digest};
    let mut hasher = Md5::new();
    hasher.update(value.as_bytes());
    format!("{:x}", hasher.finalize())
}

// =============================================================================
// String Functions
// =============================================================================

/// Repeat string N times
/// {{ "-" | repeat(10) }}  →  "----------"
pub fn repeat(value: String, count: usize) -> String {
    value.repeat(count)
}

/// Convert to camelCase
/// {{ "foo_bar_baz" | camelcase }}  →  "fooBarBaz"
/// {{ "foo-bar-baz" | camelcase }}  →  "fooBarBaz"
pub fn camelcase(value: String) -> String {
    let mut result = String::with_capacity(value.len());
    let mut capitalize_next = false;
    let mut first = true;

    for c in value.chars() {
        if c == '_' || c == '-' || c == ' ' {
            capitalize_next = true;
        } else if capitalize_next {
            result.extend(c.to_uppercase());
            capitalize_next = false;
        } else if first {
            result.extend(c.to_lowercase());
            first = false;
        } else {
            result.extend(c.to_lowercase());
        }
    }

    result
}

/// Convert to PascalCase (UpperCamelCase)
/// {{ "foo_bar" | pascalcase }}  →  "FooBar"
pub fn pascalcase(value: String) -> String {
    let mut result = String::with_capacity(value.len());
    let mut capitalize_next = true;

    for c in value.chars() {
        if c == '_' || c == '-' || c == ' ' {
            capitalize_next = true;
        } else if capitalize_next {
            result.extend(c.to_uppercase());
            capitalize_next = false;
        } else {
            result.push(c);
        }
    }

    result
}

/// Substring extraction
/// {{ "hello world" | substr(0, 5) }}  →  "hello"
/// {{ "hello world" | substr(6) }}  →  "world"
pub fn substr(value: String, start: usize, length: Option<usize>) -> String {
    let chars: Vec<char> = value.chars().collect();
    let end = length.map(|l| (start + l).min(chars.len())).unwrap_or(chars.len());
    let start = start.min(chars.len());
    chars[start..end].iter().collect()
}

/// Word wrap at specified width
/// {{ long_text | wrap(80) }}
pub fn wrap(value: String, width: usize) -> String {
    let mut result = String::with_capacity(value.len() + value.len() / width);
    let mut line_len = 0;

    for word in value.split_whitespace() {
        let word_len = word.chars().count();
        if line_len > 0 && line_len + 1 + word_len > width {
            result.push('\n');
            line_len = 0;
        } else if line_len > 0 {
            result.push(' ');
            line_len += 1;
        }
        result.push_str(word);
        line_len += word_len;
    }

    result
}

/// Check if string starts with prefix (function form for Helm compatibility)
/// {{ "hello" | hasprefix("hel") }}  →  true
pub fn hasprefix(value: String, prefix: String) -> bool {
    value.starts_with(&prefix)
}

/// Check if string ends with suffix (function form for Helm compatibility)
/// {{ "hello.txt" | hassuffix(".txt") }}  →  true
pub fn hassuffix(value: String, suffix: String) -> bool {
    value.ends_with(&suffix)
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

    #[test]
    fn test_int_filter() {
        // Integer passthrough
        assert_eq!(int(Value::from(42)).unwrap(), 42);
        // Float truncation
        assert_eq!(int(Value::from(3.7)).unwrap(), 3);
        assert_eq!(int(Value::from(-3.7)).unwrap(), -3);
        // String parsing
        assert_eq!(int(Value::from("123")).unwrap(), 123);
        assert_eq!(int(Value::from("45.9")).unwrap(), 45);
        // Bool conversion
        assert_eq!(int(Value::from(true)).unwrap(), 1);
        assert_eq!(int(Value::from(false)).unwrap(), 0);
    }

    #[test]
    fn test_float_filter() {
        // Float passthrough
        let result = float(Value::from(3.14)).unwrap();
        assert!((result - 3.14).abs() < 0.001);
        // Integer conversion
        let result = float(Value::from(42)).unwrap();
        assert!((result - 42.0).abs() < 0.001);
        // String parsing
        let result = float(Value::from("3.14")).unwrap();
        assert!((result - 3.14).abs() < 0.001);
        // Bool conversion
        assert_eq!(float(Value::from(true)).unwrap(), 1.0);
        assert_eq!(float(Value::from(false)).unwrap(), 0.0);
    }

    #[test]
    fn test_abs_filter() {
        // Positive integer
        let result = abs(Value::from(5)).unwrap();
        assert_eq!(result.as_i64().unwrap(), 5);
        // Negative integer
        let result = abs(Value::from(-5)).unwrap();
        assert_eq!(result.as_i64().unwrap(), 5);
        // Positive float
        let result = abs(Value::from(3.14)).unwrap();
        assert!((f64::try_from(result).unwrap() - 3.14).abs() < 0.001);
        // Negative float
        let result = abs(Value::from(-3.14)).unwrap();
        assert!((f64::try_from(result).unwrap() - 3.14).abs() < 0.001);
        // Zero
        let result = abs(Value::from(0)).unwrap();
        assert_eq!(result.as_i64().unwrap(), 0);
    }

    // =========================================================================
    // Path Function Tests
    // =========================================================================

    #[test]
    fn test_basename() {
        assert_eq!(basename("/etc/nginx/nginx.conf".to_string()), "nginx.conf");
        assert_eq!(basename("file.txt".to_string()), "file.txt");
        assert_eq!(basename("/path/to/dir/".to_string()), "dir"); // Trailing slash is normalized
        assert_eq!(basename("/".to_string()), "");
        assert_eq!(basename("".to_string()), "");
    }

    #[test]
    fn test_dirname() {
        assert_eq!(dirname("/etc/nginx/nginx.conf".to_string()), "/etc/nginx");
        assert_eq!(dirname("file.txt".to_string()), "");
        assert_eq!(dirname("/single".to_string()), "/");
        assert_eq!(dirname("a/b/c".to_string()), "a/b");
    }

    #[test]
    fn test_extname() {
        assert_eq!(extname("file.txt".to_string()), "txt");
        assert_eq!(extname("archive.tar.gz".to_string()), "gz");
        assert_eq!(extname("noext".to_string()), "");
        assert_eq!(extname(".hidden".to_string()), "");
    }

    #[test]
    fn test_cleanpath() {
        assert_eq!(cleanpath("a/b/../c".to_string()), "a/c");
        assert_eq!(cleanpath("a/./b/./c".to_string()), "a/b/c");
        assert_eq!(cleanpath("/a/b/../c".to_string()), "/a/c");
        assert_eq!(cleanpath("../a".to_string()), "a");
        assert_eq!(cleanpath("".to_string()), ".");
    }

    // =========================================================================
    // Regex Function Tests
    // =========================================================================

    #[test]
    fn test_regex_match() {
        assert!(regex_match("v1.2.3".to_string(), r"^v\d+".to_string()).unwrap());
        assert!(!regex_match("1.2.3".to_string(), r"^v\d+".to_string()).unwrap());
        assert!(regex_match("hello@world.com".to_string(), r"@.*\.".to_string()).unwrap());
    }

    #[test]
    fn test_regex_replace() {
        assert_eq!(
            regex_replace("v1.2.3".to_string(), r"v(\d+)".to_string(), "version-$1".to_string()).unwrap(),
            "version-1.2.3"
        );
        assert_eq!(
            regex_replace("foo bar baz".to_string(), r"\s+".to_string(), "-".to_string()).unwrap(),
            "foo-bar-baz"
        );
    }

    #[test]
    fn test_regex_find() {
        assert_eq!(regex_find("port: 8080".to_string(), r"\d+".to_string()).unwrap(), "8080");
        assert_eq!(regex_find("no numbers".to_string(), r"\d+".to_string()).unwrap(), "");
    }

    #[test]
    fn test_regex_find_all() {
        let result = regex_find_all("a1b2c3".to_string(), r"\d+".to_string()).unwrap();
        assert_eq!(result, vec!["1", "2", "3"]);
    }

    // =========================================================================
    // Dict Function Tests
    // =========================================================================

    #[test]
    fn test_values_filter() {
        use minijinja::Environment;
        let mut env = Environment::new();
        env.add_filter("values", values);

        let result = env.render_str(r#"{{ {"a": 1, "b": 2} | values | sort | list }}"#, ()).unwrap();
        assert!(result.contains("1") && result.contains("2"));
    }

    #[test]
    fn test_pick_filter() {
        use minijinja::Environment;
        let mut env = Environment::new();
        env.add_filter("pick", pick);

        let result = env.render_str(r#"{{ {"a": 1, "b": 2, "c": 3} | pick("a", "c") }}"#, ()).unwrap();
        assert!(result.contains("a") && result.contains("c") && !result.contains("b"));
    }

    #[test]
    fn test_omit_filter() {
        use minijinja::Environment;
        let mut env = Environment::new();
        env.add_filter("omit", omit);

        let result = env.render_str(r#"{{ {"a": 1, "b": 2, "c": 3} | omit("b") }}"#, ()).unwrap();
        assert!(result.contains("a") && result.contains("c") && !result.contains(": 2"));
    }

    // =========================================================================
    // List Function Tests
    // =========================================================================

    #[test]
    fn test_append_filter() {
        use minijinja::Environment;
        let mut env = Environment::new();
        env.add_filter("append", append);

        let result = env.render_str(r#"{{ [1, 2] | append(3) }}"#, ()).unwrap();
        assert_eq!(result, "[1, 2, 3]");
    }

    #[test]
    fn test_prepend_filter() {
        use minijinja::Environment;
        let mut env = Environment::new();
        env.add_filter("prepend", prepend);

        let result = env.render_str(r#"{{ [2, 3] | prepend(1) }}"#, ()).unwrap();
        assert_eq!(result, "[1, 2, 3]");
    }

    #[test]
    fn test_concat_filter() {
        use minijinja::Environment;
        let mut env = Environment::new();
        env.add_filter("concat", concat);

        let result = env.render_str(r#"{{ [1, 2] | concat([3, 4]) }}"#, ()).unwrap();
        assert_eq!(result, "[1, 2, 3, 4]");
    }

    #[test]
    fn test_without_filter() {
        use minijinja::Environment;
        let mut env = Environment::new();
        env.add_filter("without", without);

        let result = env.render_str(r#"{{ [1, 2, 3, 2] | without(2) }}"#, ()).unwrap();
        assert_eq!(result, "[1, 3]");
    }

    #[test]
    fn test_compact_filter() {
        use minijinja::Environment;
        let mut env = Environment::new();
        env.add_filter("compact", compact);

        let result = env.render_str(r#"{{ ["a", "", "b"] | compact }}"#, ()).unwrap();
        assert!(result.contains("a") && result.contains("b") && !result.contains(r#""""#));
    }

    // =========================================================================
    // Math Function Tests
    // =========================================================================

    #[test]
    fn test_floor_filter() {
        assert_eq!(floor(Value::from(3.7)).unwrap(), 3);
        assert_eq!(floor(Value::from(3.2)).unwrap(), 3);
        assert_eq!(floor(Value::from(-3.2)).unwrap(), -4);
        assert_eq!(floor(Value::from(5)).unwrap(), 5);
    }

    #[test]
    fn test_ceil_filter() {
        assert_eq!(ceil(Value::from(3.2)).unwrap(), 4);
        assert_eq!(ceil(Value::from(3.7)).unwrap(), 4);
        assert_eq!(ceil(Value::from(-3.7)).unwrap(), -3);
        assert_eq!(ceil(Value::from(5)).unwrap(), 5);
    }

    // =========================================================================
    // Crypto Function Tests
    // =========================================================================

    #[test]
    fn test_sha1sum() {
        // SHA-1 of "hello"
        assert_eq!(sha1sum("hello".to_string()), "aaf4c61ddcc5e8a2dabede0f3b482cd9aea9434d");
    }

    #[test]
    fn test_sha512sum() {
        // SHA-512 of "hello" (first 32 chars)
        let result = sha512sum("hello".to_string());
        assert!(result.starts_with("9b71d224bd62f3785d96d46ad3ea3d73"));
        assert_eq!(result.len(), 128); // SHA-512 produces 128 hex chars
    }

    #[test]
    fn test_md5sum() {
        // MD5 of "hello"
        assert_eq!(md5sum("hello".to_string()), "5d41402abc4b2a76b9719d911017c592");
    }

    // =========================================================================
    // String Function Tests
    // =========================================================================

    #[test]
    fn test_repeat_filter() {
        assert_eq!(repeat("-".to_string(), 5), "-----");
        assert_eq!(repeat("ab".to_string(), 3), "ababab");
        assert_eq!(repeat("x".to_string(), 0), "");
    }

    #[test]
    fn test_camelcase_filter() {
        assert_eq!(camelcase("foo_bar_baz".to_string()), "fooBarBaz");
        assert_eq!(camelcase("foo-bar-baz".to_string()), "fooBarBaz");
        assert_eq!(camelcase("FOO_BAR".to_string()), "fooBar");
        assert_eq!(camelcase("already".to_string()), "already");
    }

    #[test]
    fn test_pascalcase_filter() {
        assert_eq!(pascalcase("foo_bar".to_string()), "FooBar");
        assert_eq!(pascalcase("foo-bar-baz".to_string()), "FooBarBaz");
        assert_eq!(pascalcase("hello".to_string()), "Hello");
    }

    #[test]
    fn test_substr_filter() {
        assert_eq!(substr("hello world".to_string(), 0, Some(5)), "hello");
        assert_eq!(substr("hello world".to_string(), 6, None), "world");
        assert_eq!(substr("hello".to_string(), 10, Some(5)), "");
        assert_eq!(substr("hello".to_string(), 0, Some(100)), "hello");
    }

    #[test]
    fn test_wrap_filter() {
        assert_eq!(wrap("hello world foo bar".to_string(), 10), "hello\nworld foo\nbar");
        assert_eq!(wrap("short".to_string(), 20), "short");
    }

    #[test]
    fn test_hasprefix_filter() {
        assert!(hasprefix("hello world".to_string(), "hello".to_string()));
        assert!(!hasprefix("hello world".to_string(), "world".to_string()));
    }

    #[test]
    fn test_hassuffix_filter() {
        assert!(hassuffix("hello.txt".to_string(), ".txt".to_string()));
        assert!(!hassuffix("hello.txt".to_string(), ".yaml".to_string()));
    }
}
