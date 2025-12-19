//! MiniJinja integration for the Files API
//!
//! This module provides a MiniJinja Object implementation that exposes
//! the Files API to templates.
//!
//! # Usage in Templates
//!
//! ```jinja2
//! {# Read a file as string #}
//! {{ files.get("config/nginx.conf") }}
//!
//! {# Read and encode as base64 #}
//! {{ files.get("config/nginx.conf") | b64encode }}
//!
//! {# Check if file exists #}
//! {% if files.exists("config/custom.yaml") %}
//!   {{ files.get("config/custom.yaml") }}
//! {% endif %}
//!
//! {# Iterate over files matching pattern #}
//! {% for file in files.glob("config/*.yaml") %}
//!   {{ file.name }}: {{ file.content | b64encode }}
//! {% endfor %}
//!
//! {# Read file lines #}
//! {% for line in files.lines("hosts.txt") %}
//!   - {{ line }}
//! {% endfor %}
//! ```

use std::sync::Arc;

use minijinja::value::{Object, ObjectRepr, Value};
use minijinja::{Error, ErrorKind};
use sherpack_core::files::{FileProvider, Files};

/// MiniJinja Object wrapper for the Files API
///
/// This struct implements the `Object` trait to expose file operations
/// to templates via method calls.
#[derive(Debug)]
pub struct FilesObject {
    files: Files,
}

impl FilesObject {
    /// Create a new FilesObject from a Files instance
    pub fn new(files: Files) -> Self {
        Self { files }
    }

    /// Create a new FilesObject from a FileProvider
    pub fn from_provider(provider: impl FileProvider + 'static) -> Self {
        Self {
            files: Files::new(provider),
        }
    }

    /// Create a new FilesObject from an Arc'd FileProvider
    pub fn from_arc_provider(provider: Arc<dyn FileProvider>) -> Self {
        Self {
            files: Files::from_arc(provider),
        }
    }
}

impl Object for FilesObject {
    fn repr(self: &Arc<Self>) -> ObjectRepr {
        ObjectRepr::Plain
    }

    fn call_method(
        self: &Arc<Self>,
        _state: &minijinja::State,
        method: &str,
        args: &[Value],
    ) -> Result<Value, Error> {
        match method {
            "get" => {
                let path = get_path_arg(args, "get")?;
                match self.files.get(&path) {
                    Ok(content) => Ok(Value::from(content)),
                    Err(e) => Err(Error::new(ErrorKind::InvalidOperation, e.to_string())),
                }
            }

            "get_bytes" => {
                let path = get_path_arg(args, "get_bytes")?;
                match self.files.get_bytes(&path) {
                    Ok(bytes) => {
                        // Return as a sequence of integers for b64encode compatibility
                        Ok(Value::from(bytes))
                    }
                    Err(e) => Err(Error::new(ErrorKind::InvalidOperation, e.to_string())),
                }
            }

            "exists" => {
                let path = get_path_arg(args, "exists")?;
                Ok(Value::from(self.files.exists(&path)))
            }

            "glob" => {
                let pattern = get_path_arg(args, "glob")?;
                match self.files.glob(&pattern) {
                    Ok(entries) => {
                        // Convert to MiniJinja-friendly format
                        let values: Vec<Value> = entries
                            .into_iter()
                            .map(|entry| {
                                Value::from_object(FileEntryObject {
                                    path: entry.path,
                                    name: entry.name,
                                    content: entry.content,
                                    size: entry.size,
                                })
                            })
                            .collect();
                        Ok(Value::from(values))
                    }
                    Err(e) => Err(Error::new(ErrorKind::InvalidOperation, e.to_string())),
                }
            }

            "lines" => {
                let path = get_path_arg(args, "lines")?;
                match self.files.lines(&path) {
                    Ok(lines) => Ok(Value::from(lines)),
                    Err(e) => Err(Error::new(ErrorKind::InvalidOperation, e.to_string())),
                }
            }

            _ => Err(Error::new(
                ErrorKind::UnknownMethod,
                format!(
                    "files object has no method '{}'. Available methods: get, get_bytes, exists, glob, lines",
                    method
                ),
            )),
        }
    }
}

/// Helper to extract path argument from method call
fn get_path_arg(args: &[Value], method_name: &str) -> Result<String, Error> {
    args.first()
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| {
            Error::new(
                ErrorKind::InvalidOperation,
                format!("files.{}() requires a path string argument", method_name),
            )
        })
}

/// MiniJinja Object for file entries returned by glob
#[derive(Debug)]
struct FileEntryObject {
    path: String,
    name: String,
    content: String,
    size: usize,
}

impl Object for FileEntryObject {
    fn repr(self: &Arc<Self>) -> ObjectRepr {
        ObjectRepr::Map
    }

    fn get_value(self: &Arc<Self>, key: &Value) -> Option<Value> {
        let key_str = key.as_str()?;
        match key_str {
            "path" => Some(Value::from(self.path.clone())),
            "name" => Some(Value::from(self.name.clone())),
            "content" => Some(Value::from(self.content.clone())),
            "size" => Some(Value::from(self.size)),
            _ => None,
        }
    }

    fn enumerate(self: &Arc<Self>) -> minijinja::value::Enumerator {
        minijinja::value::Enumerator::Str(&["path", "name", "content", "size"])
    }
}

/// Create a MiniJinja Value containing a FilesObject
///
/// This is the main entry point for injecting the files API into templates.
pub fn create_files_value(files: Files) -> Value {
    Value::from_object(FilesObject::new(files))
}

/// Create a MiniJinja Value from a FileProvider
pub fn create_files_value_from_provider(provider: impl FileProvider + 'static) -> Value {
    Value::from_object(FilesObject::from_provider(provider))
}

#[cfg(test)]
mod tests {
    use super::*;
    use minijinja::Environment;
    use sherpack_core::files::MockFileProvider;

    fn create_test_env() -> (Environment<'static>, Value) {
        let provider = MockFileProvider::new()
            .with_text_file("config/app.yaml", "key: value\nother: data")
            .with_text_file("config/db.yaml", "host: localhost")
            .with_text_file("scripts/init.sh", "#!/bin/bash\necho hello");

        let files_value = create_files_value_from_provider(provider);

        let mut env = Environment::new();
        env.add_global("files", files_value.clone());

        (env, files_value)
    }

    #[test]
    fn test_files_get() {
        let (env, _) = create_test_env();

        let result = env
            .render_str(r#"{{ files.get("config/app.yaml") }}"#, ())
            .unwrap();
        assert_eq!(result, "key: value\nother: data");
    }

    #[test]
    fn test_files_exists_true() {
        let (env, _) = create_test_env();

        let result = env
            .render_str(r#"{{ files.exists("config/app.yaml") }}"#, ())
            .unwrap();
        assert_eq!(result, "true");
    }

    #[test]
    fn test_files_exists_false() {
        let (env, _) = create_test_env();

        let result = env
            .render_str(r#"{{ files.exists("nonexistent.txt") }}"#, ())
            .unwrap();
        assert_eq!(result, "false");
    }

    #[test]
    fn test_files_glob() {
        let (env, _) = create_test_env();

        let template = r#"{% for f in files.glob("config/*.yaml") %}{{ f.name }}:{{ f.content | length }},{% endfor %}"#;
        let result = env.render_str(template, ()).unwrap();

        // Files should be in sorted order
        assert!(result.contains("app.yaml:"));
        assert!(result.contains("db.yaml:"));
    }

    #[test]
    fn test_files_glob_attributes() {
        let (env, _) = create_test_env();

        let template = r#"{% for f in files.glob("config/app.yaml") %}path={{ f.path }},name={{ f.name }},size={{ f.size }}{% endfor %}"#;
        let result = env.render_str(template, ()).unwrap();

        assert!(result.contains("path=config/app.yaml"));
        assert!(result.contains("name=app.yaml"));
        assert!(result.contains("size="));
    }

    #[test]
    fn test_files_lines() {
        let (env, _) = create_test_env();

        let template =
            r#"{% for line in files.lines("scripts/init.sh") %}[{{ line }}]{% endfor %}"#;
        let result = env.render_str(template, ()).unwrap();

        assert_eq!(result, "[#!/bin/bash][echo hello]");
    }

    #[test]
    fn test_files_conditional() {
        let (env, _) = create_test_env();

        let template =
            r#"{% if files.exists("config/app.yaml") %}found{% else %}not found{% endif %}"#;
        let result = env.render_str(template, ()).unwrap();
        assert_eq!(result, "found");

        let template2 =
            r#"{% if files.exists("missing.yaml") %}found{% else %}not found{% endif %}"#;
        let result2 = env.render_str(template2, ()).unwrap();
        assert_eq!(result2, "not found");
    }

    #[test]
    fn test_files_get_not_found() {
        let (env, _) = create_test_env();

        let result = env.render_str(r#"{{ files.get("nonexistent.txt") }}"#, ());
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[test]
    fn test_files_unknown_method() {
        let (env, _) = create_test_env();

        let result = env.render_str(r#"{{ files.unknown() }}"#, ());
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("unknown"));
    }

    #[test]
    fn test_files_with_filters() {
        let (env, _) = create_test_env();

        // Test with trim
        let template = r#"{{ files.get("config/app.yaml") | trim }}"#;
        let result = env.render_str(template, ()).unwrap();
        assert_eq!(result, "key: value\nother: data");
    }
}
