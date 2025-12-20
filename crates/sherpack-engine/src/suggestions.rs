//! Fuzzy matching and context-aware suggestions for template errors
//!
//! This module provides intelligent suggestions when template errors occur,
//! using Levenshtein distance for fuzzy matching and context extraction
//! to help users fix common mistakes.

use serde_json::Value as JsonValue;

/// Maximum Levenshtein distance to consider for suggestions
const MAX_SUGGESTION_DISTANCE: usize = 3;

/// All registered filters in the engine
pub const AVAILABLE_FILTERS: &[&str] = &[
    // Custom Sherpack filters
    "toyaml",
    "tojson",
    "tojson_pretty",
    "b64encode",
    "b64decode",
    "quote",
    "squote",
    "nindent",
    "indent",
    "required",
    "empty",
    "haskey",
    "keys",
    "merge",
    "sha256",
    "trunc",
    "trimprefix",
    "trimsuffix",
    "snakecase",
    "kebabcase",
    "tostrings", // Convert list elements to strings
    // Built-in MiniJinja filters
    "default",
    "upper",
    "lower",
    "title",
    "capitalize",
    "replace",
    "trim",
    "join",
    "first",
    "last",
    "length",
    "reverse",
    "sort",
    "unique",
    "map",
    "select",
    "reject",
    "selectattr",
    "rejectattr",
    "batch",
    "slice",
    "dictsort",
    "items",
    "attr",
    "int",
    "float",
    "abs",
    "round",
    "string",
    "list",
    "bool",
    "safe",
    "escape",
    "e",
    "urlencode",
];

/// All registered functions in the engine
pub const AVAILABLE_FUNCTIONS: &[&str] = &[
    // Custom Sherpack functions
    "fail",
    "dict",
    "list",
    "get",
    "coalesce",
    "ternary",
    "uuidv4",
    "tostring",
    "toint",
    "tofloat",
    "now",
    "printf",
    "tpl",     // Dynamic template evaluation
    "tpl_ctx", // Dynamic template with full context
    "lookup",  // K8s resource lookup (returns empty in template mode)
    // Built-in MiniJinja globals
    "range",
    "lipsum",
    "cycler",
    "joiner",
    "namespace",
];

/// Top-level context variables always available in templates
pub const CONTEXT_VARIABLES: &[&str] = &["values", "release", "pack", "capabilities", "template"];

/// Suggestion result with confidence scoring
#[derive(Debug, Clone)]
pub struct Suggestion {
    /// The suggested correction
    pub text: String,
    /// Levenshtein distance (lower = better match)
    pub distance: usize,
    /// Category of suggestion
    pub category: SuggestionCategory,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SuggestionCategory {
    Variable,
    Filter,
    Function,
    Property,
}

/// Calculate Levenshtein distance between two strings
pub fn levenshtein(a: &str, b: &str) -> usize {
    strsim::levenshtein(a, b)
}

/// Find closest matches from a list of candidates
pub fn find_closest_matches(
    input: &str,
    candidates: &[&str],
    max_results: usize,
    category: SuggestionCategory,
) -> Vec<Suggestion> {
    let mut suggestions: Vec<Suggestion> = candidates
        .iter()
        .filter_map(|&candidate| {
            let distance = levenshtein(input, candidate);
            if distance <= MAX_SUGGESTION_DISTANCE && distance > 0 {
                Some(Suggestion {
                    text: candidate.to_string(),
                    distance,
                    category,
                })
            } else {
                None
            }
        })
        .collect();

    // Sort by distance (best matches first)
    suggestions.sort_by_key(|s| s.distance);
    suggestions.truncate(max_results);
    suggestions
}

/// Suggest corrections for an undefined variable
pub fn suggest_undefined_variable(
    variable_name: &str,
    available_variables: &[String],
) -> Option<String> {
    // First check for common typo: "value" instead of "values"
    if variable_name == "value" {
        return Some(
            "Did you mean `values`? The values object is accessed as `values.key`".to_string(),
        );
    }

    // Check top-level context variables
    let context_match = find_closest_matches(
        variable_name,
        CONTEXT_VARIABLES,
        1,
        SuggestionCategory::Variable,
    );

    if let Some(suggestion) = context_match.first() {
        return Some(format!("Did you mean `{}`?", suggestion.text));
    }

    // Check available values
    let candidates: Vec<&str> = available_variables.iter().map(|s| s.as_str()).collect();

    let value_match =
        find_closest_matches(variable_name, &candidates, 3, SuggestionCategory::Variable);

    if !value_match.is_empty() {
        let suggestions: Vec<String> = value_match
            .iter()
            .map(|s| format!("`{}`", s.text))
            .collect();
        Some(format!("Did you mean {}?", suggestions.join(" or ")))
    } else {
        None
    }
}

/// Suggest corrections for an unknown filter
pub fn suggest_unknown_filter(filter_name: &str) -> Option<String> {
    let matches = find_closest_matches(
        filter_name,
        AVAILABLE_FILTERS,
        3,
        SuggestionCategory::Filter,
    );

    if !matches.is_empty() {
        let suggestions: Vec<String> = matches.iter().map(|s| format!("`{}`", s.text)).collect();
        Some(format!(
            "Did you mean {}? Common filters: toyaml, tojson, b64encode, quote, default, indent",
            suggestions.join(" or ")
        ))
    } else {
        Some(format!(
            "Unknown filter `{}`. Common filters: toyaml, tojson, b64encode, quote, default, indent, nindent",
            filter_name
        ))
    }
}

/// Suggest corrections for an unknown function
pub fn suggest_unknown_function(func_name: &str) -> Option<String> {
    let matches = find_closest_matches(
        func_name,
        AVAILABLE_FUNCTIONS,
        3,
        SuggestionCategory::Function,
    );

    if !matches.is_empty() {
        let suggestions: Vec<String> = matches.iter().map(|s| format!("`{}`", s.text)).collect();
        Some(format!("Did you mean {}?", suggestions.join(" or ")))
    } else {
        Some(format!(
            "Unknown function `{}`. Available functions: {}",
            func_name,
            AVAILABLE_FUNCTIONS.join(", ")
        ))
    }
}

/// Extract available keys from a JSON value at a given path
pub fn extract_available_keys(values: &JsonValue, path: &str) -> Vec<String> {
    let parts: Vec<&str> = path.split('.').filter(|s| !s.is_empty()).collect();

    let mut current = values;
    for part in &parts {
        match current.get(part) {
            Some(v) => current = v,
            None => return vec![],
        }
    }

    match current {
        JsonValue::Object(map) => map.keys().cloned().collect(),
        _ => vec![],
    }
}

/// Suggest available properties when accessing undefined key
pub fn suggest_available_properties(
    parent_path: &str,
    attempted_key: &str,
    values: &JsonValue,
) -> Option<String> {
    let available = extract_available_keys(values, parent_path);

    if available.is_empty() {
        return None;
    }

    // Try fuzzy match first
    let candidates: Vec<&str> = available.iter().map(|s| s.as_str()).collect();
    let matches = find_closest_matches(attempted_key, &candidates, 3, SuggestionCategory::Property);

    if !matches.is_empty() {
        let suggestions: Vec<String> = matches
            .iter()
            .map(|s| format!("`{}.{}`", parent_path, s.text))
            .collect();
        Some(format!(
            "Did you mean {}? Available: {}",
            suggestions.join(" or "),
            available.join(", ")
        ))
    } else {
        Some(format!(
            "Key `{}` not found in `{}`. Available keys: {}",
            attempted_key,
            parent_path,
            available.join(", ")
        ))
    }
}

/// Generate a type-specific hint for iteration errors
pub fn suggest_iteration_fix(type_name: &str) -> String {
    match type_name {
        "object" | "map" => {
            "Objects require `| dictsort` to iterate: `{% for key, value in obj | dictsort %}`"
                .to_string()
        }
        "string" => {
            "Strings iterate character by character. Did you mean to split it first?".to_string()
        }
        "null" | "none" => {
            "Value is null/undefined. Check that it exists or use `| default([])` for empty list"
                .to_string()
        }
        _ => format!(
            "Value of type `{}` is not iterable. Use a list or add `| dictsort` for objects",
            type_name
        ),
    }
}

/// Extract variable name from error message
pub fn extract_variable_name(msg: &str) -> Option<String> {
    // Pattern: "undefined variable `foo`" or "variable 'foo' is undefined"
    let patterns = [("`", "`"), ("'", "'"), ("\"", "\"")];

    for (start, end) in patterns {
        if let Some(start_idx) = msg.find(start) {
            let rest = &msg[start_idx + start.len()..];
            if let Some(end_idx) = rest.find(end) {
                return Some(rest[..end_idx].to_string());
            }
        }
    }
    None
}

/// Extract filter name from error message
pub fn extract_filter_name(msg: &str) -> Option<String> {
    extract_variable_name(msg)
}

/// Extract function name from error message
pub fn extract_function_name(msg: &str) -> Option<String> {
    extract_variable_name(msg)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_levenshtein_distance() {
        assert_eq!(levenshtein("value", "values"), 1);
        assert_eq!(levenshtein("toyml", "toyaml"), 1);
        assert_eq!(levenshtein("b64encode", "b64encode"), 0);
        // strsim calculates actual Levenshtein distance = 7
        assert_eq!(levenshtein("something", "completely"), 7);
    }

    #[test]
    fn test_find_closest_matches() {
        let matches =
            find_closest_matches("toyml", AVAILABLE_FILTERS, 3, SuggestionCategory::Filter);
        assert!(!matches.is_empty());
        assert_eq!(matches[0].text, "toyaml");
        assert_eq!(matches[0].distance, 1);
    }

    #[test]
    fn test_suggest_undefined_variable_typo() {
        let suggestion = suggest_undefined_variable("value", &[]);
        assert!(suggestion.is_some());
        assert!(suggestion.unwrap().contains("values"));
    }

    #[test]
    fn test_suggest_unknown_filter() {
        let suggestion = suggest_unknown_filter("toyml");
        assert!(suggestion.is_some());
        assert!(suggestion.unwrap().contains("toyaml"));
    }

    #[test]
    fn test_extract_available_keys() {
        let values = serde_json::json!({
            "image": {
                "repository": "nginx",
                "tag": "latest"
            },
            "replicas": 3
        });

        let keys = extract_available_keys(&values, "image");
        assert!(keys.contains(&"repository".to_string()));
        assert!(keys.contains(&"tag".to_string()));
    }

    #[test]
    fn test_extract_variable_name() {
        assert_eq!(
            extract_variable_name("undefined variable `foo`"),
            Some("foo".to_string())
        );
        assert_eq!(
            extract_variable_name("variable 'bar' is undefined"),
            Some("bar".to_string())
        );
    }
}
