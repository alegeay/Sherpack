//! Template functions (global functions available in templates)

use minijinja::{Error, ErrorKind, Value};

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
    if args.len() % 2 != 0 {
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

    Ok(Value::from_serialize(&serde_json::Value::Object(map)))
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
        s.parse::<i64>()
            .map_err(|_| Error::new(ErrorKind::InvalidOperation, format!("cannot convert '{}' to int", s)))
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
        s.parse::<f64>()
            .map_err(|_| Error::new(ErrorKind::InvalidOperation, format!("cannot convert '{}' to float", s)))
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
pub fn printf(format: String, args: Vec<Value>) -> Result<String, Error> {
    let mut result = format;
    let mut arg_idx = 0;

    // Simple replacement - supports %s, %d, %v
    while let Some(pos) = result.find('%') {
        if pos + 1 >= result.len() {
            break;
        }

        let format_char = match result.chars().nth(pos + 1) {
            Some(c) => c,
            None => break, // Malformed format string, stop processing
        };

        if format_char == '%' {
            result = result.replacen("%%", "%", 1);
            continue;
        }

        if arg_idx >= args.len() {
            return Err(Error::new(
                ErrorKind::InvalidOperation,
                "not enough arguments for format string",
            ));
        }

        let replacement = match format_char {
            's' | 'v' => args[arg_idx].to_string(),
            'd' => {
                if let Some(n) = args[arg_idx].as_i64() {
                    n.to_string()
                } else {
                    args[arg_idx].to_string()
                }
            }
            'f' => {
                if let Some(n) = args[arg_idx].as_i64() {
                    (n as f64).to_string()
                } else {
                    args[arg_idx].to_string()
                }
            }
            _ => args[arg_idx].to_string(),
        };

        result = result.replacen(&format!("%{}", format_char), &replacement, 1);
        arg_idx += 1;
    }

    Ok(result)
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
}
