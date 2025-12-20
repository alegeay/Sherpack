# Phase 1: Template Functions Design

## Executive Summary

After deep analysis, **many "missing" functions are already available** through MiniJinja's built-in filters. The real work is:

1. **Documentation** - Show users how Helm patterns translate to Jinja2
2. **Truly missing functions** - Path manipulation, regex, additional crypto
3. **Helm compatibility aliases** - Optional, for migration ease

---

## Part 1: What's Already Available

MiniJinja with `builtins` feature (enabled in Sherpack) provides **46 filters**:

### String Manipulation (ALREADY AVAILABLE)
```jinja2
{{ name | upper }}              {# UPPERCASE #}
{{ name | lower }}              {# lowercase #}
{{ name | title }}              {# Title Case #}
{{ name | capitalize }}         {# Capitalize first #}
{{ text | trim }}               {# Strip whitespace #}
{{ text | replace("a", "b") }}  {# Replace substring #}
```

### List Operations (ALREADY AVAILABLE)
```jinja2
{{ items | first }}             {# First element #}
{{ items | last }}              {# Last element #}
{{ items | reverse }}           {# Reverse list #}
{{ items | sort }}              {# Sort list #}
{{ items | unique }}            {# Remove duplicates #}
{{ items | join(", ") }}        {# Join to string #}
{{ items | length }}            {# Count elements #}
{{ items | min }}               {# Minimum value #}
{{ items | max }}               {# Maximum value #}
{{ items | sum }}               {# Sum of numbers #}
{{ items | slice(0, 5) }}       {# Slice list #}
{{ items | batch(3) }}          {# Chunk into groups #}
{{ items | select("odd") }}     {# Filter by test #}
{{ items | reject("none") }}    {# Reject by test #}
{{ items | map(attribute="name") }}  {# Extract attribute #}
```

### String Splitting (ALREADY AVAILABLE)
```jinja2
{{ "a,b,c" | split(",") }}      {# ["a", "b", "c"] #}
{{ lines | lines }}             {# Split by newlines #}
```

### Math (ALREADY AVAILABLE via operators + filters)
```jinja2
{{ a + b }}                     {# Addition #}
{{ a - b }}                     {# Subtraction #}
{{ a * b }}                     {# Multiplication #}
{{ a / b }}                     {# Division #}
{{ a % b }}                     {# Modulo #}
{{ value | round }}             {# Round to nearest #}
{{ value | round(2) }}          {# Round to 2 decimals #}
{{ value | abs }}               {# Absolute value #}
{{ value | int }}               {# Convert to integer #}
{{ value | float }}             {# Convert to float #}
```

### Dict Operations (ALREADY AVAILABLE)
```jinja2
{{ mydict | dictsort }}         {# Sort dict by key #}
{{ mydict | items }}            {# Get key-value pairs #}
```

### URL Encoding (ALREADY AVAILABLE)
```jinja2
{{ query | urlencode }}         {# URL-encode string #}
```

### Formatting (ALREADY AVAILABLE)
```jinja2
{{ "Hello %s" | format(name) }} {# Printf-style #}
{{ data | pprint }}             {# Pretty print #}
```

---

## Part 2: Helm → Jinja2 Translation Guide

### String Operations

| Helm | Jinja2/Sherpack | Notes |
|------|-----------------|-------|
| `{{ upper .name }}` | `{{ name \| upper }}` | Filter syntax |
| `{{ lower .name }}` | `{{ name \| lower }}` | Filter syntax |
| `{{ title .name }}` | `{{ name \| title }}` | Built-in |
| `{{ trim .name }}` | `{{ name \| trim }}` | Built-in |
| `{{ trimPrefix "v" .ver }}` | `{{ ver \| trimprefix("v") }}` | Custom filter |
| `{{ trimSuffix ".txt" .f }}` | `{{ f \| trimsuffix(".txt") }}` | Custom filter |
| `{{ replace "a" "b" .s }}` | `{{ s \| replace("a", "b") }}` | Built-in |
| `{{ substr 0 5 .s }}` | `{{ s[:5] }}` | Slice syntax |
| `{{ trunc 10 .s }}` | `{{ s \| trunc(10) }}` | Custom filter |
| `{{ nospace .s }}` | `{{ s \| replace(" ", "") }}` | Use replace |
| `{{ cat "a" "b" "c" }}` | `{{ "a" ~ "b" ~ "c" }}` | `~` operator |
| `{{ split "," .s }}` | `{{ s \| split(",") }}` | Built-in |
| `{{ join "," .list }}` | `{{ list \| join(",") }}` | Built-in |

### Boolean Checks

| Helm | Jinja2/Sherpack | Notes |
|------|-----------------|-------|
| `{{ contains "sub" .s }}` | `{{ "sub" in s }}` | `in` operator |
| `{{ hasPrefix "v" .s }}` | `{{ s is startingwith("v") }}` | Test syntax |
| `{{ hasSuffix ".go" .s }}` | `{{ s is endingwith(".go") }}` | Test syntax |
| `{{ empty .val }}` | `{{ val \| empty }}` | Custom filter |
| `{{ default "x" .val }}` | `{{ val \| default("x") }}` | Built-in |

### List Operations

| Helm | Jinja2/Sherpack | Notes |
|------|-----------------|-------|
| `{{ first .list }}` | `{{ list \| first }}` | Built-in |
| `{{ last .list }}` | `{{ list \| last }}` | Built-in |
| `{{ rest .list }}` | `{{ list[1:] }}` | Slice syntax |
| `{{ initial .list }}` | `{{ list[:-1] }}` | Slice syntax |
| `{{ reverse .list }}` | `{{ list \| reverse }}` | Built-in |
| `{{ uniq .list }}` | `{{ list \| unique }}` | Built-in |
| `{{ sortAlpha .list }}` | `{{ list \| sort }}` | Built-in |
| `{{ len .list }}` | `{{ list \| length }}` | Built-in |
| `{{ has "x" .list }}` | `{{ "x" in list }}` | `in` operator |

### Math

| Helm | Jinja2/Sherpack | Notes |
|------|-----------------|-------|
| `{{ add 1 2 }}` | `{{ 1 + 2 }}` | Operators |
| `{{ sub 5 2 }}` | `{{ 5 - 2 }}` | Operators |
| `{{ mul 3 4 }}` | `{{ 3 * 4 }}` | Operators |
| `{{ div 10 2 }}` | `{{ 10 / 2 }}` | Operators |
| `{{ mod 10 3 }}` | `{{ 10 % 3 }}` | Operators |
| `{{ max 1 5 3 }}` | `{{ [1, 5, 3] \| max }}` | Filter on list |
| `{{ min 1 5 3 }}` | `{{ [1, 5, 3] \| min }}` | Filter on list |
| `{{ round 3.7 }}` | `{{ 3.7 \| round }}` | Built-in |
| `{{ floor 3.7 }}` | `{{ 3.7 \| int }}` | Truncates |
| `{{ ceil 3.2 }}` | `{{ (3.2 + 0.9999) \| int }}` | Workaround* |

*Note: True `ceil` filter should be added.

### Dict Operations

| Helm | Jinja2/Sherpack | Notes |
|------|-----------------|-------|
| `{{ keys .dict }}` | `{{ dict \| keys }}` | Custom filter |
| `{{ values .dict }}` | Need to add | **MISSING** |
| `{{ hasKey .dict "k" }}` | `{{ dict \| haskey("k") }}` | Custom filter |
| `{{ merge .d1 .d2 }}` | `{{ d1 \| merge(d2) }}` | Custom filter |
| `{{ pick .d "a" "b" }}` | Need to add | **MISSING** |
| `{{ omit .d "a" "b" }}` | Need to add | **MISSING** |
| `{{ dig "a" "b" .d }}` | `{{ d.a.b }}` or `get(d, "a.b")` | Dot notation |

---

## Part 3: Truly Missing Functions

### Category 1: Path Manipulation (HIGH PRIORITY)

No equivalent in MiniJinja. Essential for Kubernetes manifests.

```rust
// filters.rs additions

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

/// Extract file extension
/// {{ "file.tar.gz" | extname }}  →  "gz"
pub fn extname(path: String) -> String {
    std::path::Path::new(&path)
        .extension()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default()
}

/// Clean/normalize path
/// {{ "a/b/../c" | cleanpath }}  →  "a/c"
pub fn cleanpath(path: String) -> String {
    // Use a simple normalization (no filesystem access)
    let mut parts: Vec<&str> = vec![];
    for part in path.split('/') {
        match part {
            "" | "." => continue,
            ".." => { parts.pop(); }
            _ => parts.push(part),
        }
    }
    let result = parts.join("/");
    if path.starts_with('/') {
        format!("/{}", result)
    } else {
        result
    }
}
```

### Category 2: Regex (HIGH PRIORITY)

Essential for advanced string manipulation.

```rust
use regex::Regex;

/// Check if string matches regex pattern
/// {% if name | regex_match("^v[0-9]+") %}
pub fn regex_match(value: String, pattern: String) -> Result<bool, Error> {
    let re = Regex::new(&pattern).map_err(|e| {
        Error::new(ErrorKind::InvalidOperation, format!("invalid regex: {}", e))
    })?;
    Ok(re.is_match(&value))
}

/// Replace all matches with replacement
/// {{ version | regex_replace("v([0-9]+)", "version-$1") }}
pub fn regex_replace(
    value: String,
    pattern: String,
    replacement: String
) -> Result<String, Error> {
    let re = Regex::new(&pattern).map_err(|e| {
        Error::new(ErrorKind::InvalidOperation, format!("invalid regex: {}", e))
    })?;
    Ok(re.replace_all(&value, replacement.as_str()).to_string())
}

/// Find first match
/// {{ text | regex_find("[0-9]+") }}  →  "123" or ""
pub fn regex_find(value: String, pattern: String) -> Result<String, Error> {
    let re = Regex::new(&pattern).map_err(|e| {
        Error::new(ErrorKind::InvalidOperation, format!("invalid regex: {}", e))
    })?;
    Ok(re.find(&value)
        .map(|m| m.as_str().to_string())
        .unwrap_or_default())
}

/// Find all matches
/// {{ text | regex_find_all("[0-9]+") }}  →  ["123", "456"]
pub fn regex_find_all(value: String, pattern: String) -> Result<Vec<String>, Error> {
    let re = Regex::new(&pattern).map_err(|e| {
        Error::new(ErrorKind::InvalidOperation, format!("invalid regex: {}", e))
    })?;
    Ok(re.find_iter(&value)
        .map(|m| m.as_str().to_string())
        .collect())
}
```

### Category 3: Dict Utilities (MEDIUM PRIORITY)

```rust
/// Get dict values as list
/// {{ mydict | values }}  →  [value1, value2, ...]
pub fn values(dict: Value) -> Result<Vec<Value>, Error> {
    match dict.kind() {
        ValueKind::Map => {
            Ok(dict.try_iter()?
                .filter_map(|k| dict.get_item(&k).ok())
                .collect())
        }
        _ => Err(Error::new(
            ErrorKind::InvalidOperation,
            "values() requires a dict"
        ))
    }
}

/// Pick only specified keys from dict
/// {{ mydict | pick("name", "version") }}
pub fn pick(dict: Value, keys: Rest<String>) -> Result<Value, Error> {
    let mut result = std::collections::BTreeMap::new();
    for key in keys.iter() {
        if let Ok(val) = dict.get_item(&Value::from(key.clone())) {
            if !val.is_undefined() {
                result.insert(key.clone(), val);
            }
        }
    }
    Ok(Value::from_iter(result))
}

/// Omit specified keys from dict
/// {{ mydict | omit("password", "secret") }}
pub fn omit(dict: Value, keys: Rest<String>) -> Result<Value, Error> {
    let exclude: std::collections::HashSet<_> = keys.iter().collect();
    let mut result = std::collections::BTreeMap::new();

    if let Ok(iter) = dict.try_iter() {
        for key in iter {
            let key_str = key.to_string();
            if !exclude.contains(&key_str) {
                if let Ok(val) = dict.get_item(&key) {
                    result.insert(key_str, val);
                }
            }
        }
    }
    Ok(Value::from_iter(result))
}
```

### Category 4: List Utilities (MEDIUM PRIORITY)

```rust
/// Append item to list (returns new list)
/// {{ items | append("new") }}
pub fn append(list: Value, item: Value) -> Result<Value, Error> {
    let mut vec: Vec<Value> = list.try_iter()?
        .collect();
    vec.push(item);
    Ok(Value::from(vec))
}

/// Prepend item to list (returns new list)
/// {{ items | prepend("first") }}
pub fn prepend(list: Value, item: Value) -> Result<Value, Error> {
    let mut vec: Vec<Value> = vec![item];
    vec.extend(list.try_iter()?);
    Ok(Value::from(vec))
}

/// Concatenate two lists
/// {{ list1 | concat(list2) }}
pub fn concat(list1: Value, list2: Value) -> Result<Value, Error> {
    let mut vec: Vec<Value> = list1.try_iter()?.collect();
    vec.extend(list2.try_iter()?);
    Ok(Value::from(vec))
}

/// Remove items from list
/// {{ items | without("a", "b") }}
pub fn without(list: Value, exclude: Rest<Value>) -> Result<Value, Error> {
    let exclude_set: Vec<_> = exclude.iter().cloned().collect();
    let result: Vec<Value> = list.try_iter()?
        .filter(|item| !exclude_set.contains(item))
        .collect();
    Ok(Value::from(result))
}

/// Remove null/empty values
/// {{ items | compact }}
pub fn compact(list: Value) -> Result<Value, Error> {
    let result: Vec<Value> = list.try_iter()?
        .filter(|item| {
            !item.is_none() &&
            !item.is_undefined() &&
            !(item.as_str().map(|s| s.is_empty()).unwrap_or(false))
        })
        .collect();
    Ok(Value::from(result))
}
```

### Category 5: Additional Math (LOW PRIORITY)

```rust
/// Floor - round down to integer
/// {{ 3.7 | floor }}  →  3
pub fn floor(value: f64) -> i64 {
    value.floor() as i64
}

/// Ceil - round up to integer
/// {{ 3.2 | ceil }}  →  4
pub fn ceil(value: f64) -> i64 {
    value.ceil() as i64
}
```

### Category 6: Additional Crypto (LOW PRIORITY)

```rust
use sha1::Sha1;
use sha2::Sha512;
use md5::Md5;

/// SHA1 hash (160-bit)
pub fn sha1sum(value: String) -> String {
    use sha1::Digest;
    let mut hasher = Sha1::new();
    hasher.update(value.as_bytes());
    format!("{:x}", hasher.finalize())
}

/// SHA512 hash (512-bit)
pub fn sha512sum(value: String) -> String {
    use sha2::Digest;
    let mut hasher = Sha512::new();
    hasher.update(value.as_bytes());
    format!("{:x}", hasher.finalize())
}

/// MD5 hash (deprecated, but Helm has it)
pub fn md5sum(value: String) -> String {
    use md5::Digest;
    let mut hasher = Md5::new();
    hasher.update(value.as_bytes());
    format!("{:x}", hasher.finalize())
}
```

---

## Part 4: Implementation Plan

### Step 1: Add Dependencies (Cargo.toml)

```toml
[dependencies]
regex = "1"      # For regex functions
sha1 = "0.10"    # For sha1sum
md-5 = "0.10"    # For md5sum (package name differs)
# sha2 already in workspace for sha256
```

### Step 2: Implementation Order

| Priority | Functions | Est. Time |
|----------|-----------|-----------|
| 1 | `basename`, `dirname`, `extname`, `cleanpath` | 30 min |
| 2 | `regex_match`, `regex_replace`, `regex_find`, `regex_find_all` | 45 min |
| 3 | `values`, `pick`, `omit` | 30 min |
| 4 | `append`, `prepend`, `concat`, `without`, `compact` | 30 min |
| 5 | `floor`, `ceil` | 10 min |
| 6 | `sha1sum`, `sha512sum`, `md5sum` | 15 min |

**Total: ~3 hours**

### Step 3: Test Requirements

Each filter needs:
1. **Basic test** - Normal usage
2. **Empty input test** - Empty string/list behavior
3. **Type error test** - Wrong type handling
4. **Edge cases** - Unicode, special characters, null

Example test structure:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basename_basic() {
        assert_eq!(basename("/etc/nginx/nginx.conf".into()), "nginx.conf");
        assert_eq!(basename("file.txt".into()), "file.txt");
    }

    #[test]
    fn test_basename_edge_cases() {
        assert_eq!(basename("".into()), "");
        assert_eq!(basename("/".into()), "");
        assert_eq!(basename("/etc/".into()), "");
        assert_eq!(basename("no-extension".into()), "no-extension");
    }

    #[test]
    fn test_basename_unicode() {
        assert_eq!(basename("/données/fichier.txt".into()), "fichier.txt");
    }
}
```

### Step 4: Register in Engine

```rust
// In create_environment()

// Path filters
env.add_filter("basename", filters::basename);
env.add_filter("dirname", filters::dirname);
env.add_filter("extname", filters::extname);
env.add_filter("cleanpath", filters::cleanpath);

// Regex filters
env.add_filter("regex_match", filters::regex_match);
env.add_filter("regex_replace", filters::regex_replace);
env.add_filter("regex_find", filters::regex_find);
env.add_filter("regex_find_all", filters::regex_find_all);

// Dict filters
env.add_filter("values", filters::values);
env.add_filter("pick", filters::pick);
env.add_filter("omit", filters::omit);

// List filters
env.add_filter("append", filters::append);
env.add_filter("prepend", filters::prepend);
env.add_filter("concat", filters::concat);
env.add_filter("without", filters::without);
env.add_filter("compact", filters::compact);

// Math filters
env.add_filter("floor", filters::floor);
env.add_filter("ceil", filters::ceil);

// Crypto filters
env.add_filter("sha1", filters::sha1sum);
env.add_filter("sha512", filters::sha512sum);
env.add_filter("md5", filters::md5sum);
```

---

## Part 5: Documentation Update

Create/update user-facing documentation:

1. **Template Reference** - All available filters with examples
2. **Helm Migration Guide** - Translation table (Part 2 above)
3. **Jinja2 Primer** - For users new to Jinja2 syntax

Key message: **Jinja2 is often MORE readable than Helm templates**

```
# Helm (Go templates) - function-first, harder to read
{{ trimPrefix "v" (lower .Values.version) }}

# Sherpack (Jinja2) - pipeline style, reads left-to-right
{{ values.version | lower | trimprefix("v") }}
```

---

## Summary

### What We DON'T Need to Add (Already in MiniJinja)
- `upper`, `lower`, `title`, `capitalize`
- `first`, `last`, `reverse`, `sort`, `unique`
- `replace`, `split`, `join`, `trim`
- `min`, `max`, `sum`, `round`, `abs`
- `urlencode`, `default`, `length`
- Math operators: `+`, `-`, `*`, `/`, `%`
- Boolean operators: `in`, `is`, `and`, `or`, `not`

### What We NEED to Add
1. **Path:** `basename`, `dirname`, `extname`, `cleanpath`
2. **Regex:** `regex_match`, `regex_replace`, `regex_find`, `regex_find_all`
3. **Dict:** `values`, `pick`, `omit`
4. **List:** `append`, `prepend`, `concat`, `without`, `compact`
5. **Math:** `floor`, `ceil`
6. **Crypto:** `sha1`, `sha512`, `md5`

### Total: ~20 new filters (not 60+)

The "gap" is much smaller than initially thought because MiniJinja's builtins cover most needs.
