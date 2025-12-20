//! Secret generation integration for MiniJinja templates
//!
//! This module provides the `generate_secret()` template function that generates
//! deterministic, stateful secrets for Kubernetes deployments.
//!
//! # Usage in Templates
//!
//! ```jinja2
//! # Generate a 16-char alphanumeric secret
//! password: {{ generate_secret("db-password", 16) }}
//!
//! # Generate a 32-char hex secret
//! token: {{ generate_secret("api-token", 32, "hex") }}
//!
//! # Supported charsets: alphanumeric, alpha, numeric, hex, base64, urlsafe
//! ```
//!
//! # How It Works
//!
//! Unlike Helm's `randAlphaNum` which generates different values on each render:
//!
//! 1. **First install**: Secrets are generated randomly and stored in cluster state
//! 2. **Subsequent renders**: Same values are returned from state
//! 3. **Result**: Deterministic output, GitOps compatible
//!
//! # Integration
//!
//! ```rust,no_run
//! use sherpack_engine::secrets::SecretFunctionState;
//! use sherpack_core::SecretState;
//! use minijinja::Environment;
//!
//! // Create from existing state (loaded from K8s)
//! let existing_state = SecretState::new();
//! let secret_fn = SecretFunctionState::with_state(existing_state);
//!
//! // Register with MiniJinja environment
//! let mut env = Environment::new();
//! secret_fn.register(&mut env);
//!
//! // After rendering, extract state for persistence
//! let state = secret_fn.take_state();
//! if state.is_dirty() {
//!     // Persist to Kubernetes
//! }
//! ```

use minijinja::{Environment, Error, ErrorKind};
use sherpack_core::{SecretCharset, SecretGenerator, SecretState};
use std::sync::Arc;

/// Wrapper around SecretGenerator for MiniJinja integration
///
/// Uses `Arc<Mutex<>>` to provide interior mutability needed by MiniJinja
/// functions which capture state but need to mutate it.
///
/// Note: Uses `Arc<Mutex<>>` for thread-safety compatibility with MiniJinja's
/// `Send + Sync` requirements for global functions.
#[derive(Debug, Clone)]
pub struct SecretFunctionState {
    generator: Arc<std::sync::Mutex<SecretGenerator>>,
}

impl Default for SecretFunctionState {
    fn default() -> Self {
        Self::new()
    }
}

impl SecretFunctionState {
    /// Create a new state with empty generator
    pub fn new() -> Self {
        Self {
            generator: Arc::new(std::sync::Mutex::new(SecretGenerator::new())),
        }
    }

    /// Create from existing state (loaded from Kubernetes)
    pub fn with_state(state: SecretState) -> Self {
        Self {
            generator: Arc::new(std::sync::Mutex::new(SecretGenerator::with_state(state))),
        }
    }

    /// Check if any new secrets were generated
    pub fn is_dirty(&self) -> bool {
        self.generator.lock().unwrap().is_dirty()
    }

    /// Take ownership of the state (consumes internal generator)
    ///
    /// Note: This replaces the generator with a new empty one. Use this
    /// only when you're done rendering and want to extract the state.
    pub fn take_state(&self) -> SecretState {
        let mut generator = self.generator.lock().unwrap();
        let state = std::mem::take(&mut *generator);
        state.into_state()
    }

    /// Register the `generate_secret` function on a MiniJinja environment
    ///
    /// # Arguments accepted by the function
    ///
    /// - `name` (required): Unique identifier for this secret
    /// - `length` (required): Length of the secret in characters
    /// - `charset` (optional): One of: alphanumeric, alpha, numeric, hex, base64, urlsafe
    ///
    /// # Example
    ///
    /// ```jinja2
    /// {{ generate_secret("my-password", 24) }}
    /// {{ generate_secret("hex-token", 32, "hex") }}
    /// ```
    pub fn register(&self, env: &mut Environment<'static>) {
        let generator = Arc::clone(&self.generator);

        env.add_function(
            "generate_secret",
            move |name: String, length: i64, charset: Option<String>| -> Result<String, Error> {
                // Validate name
                if name.is_empty() {
                    return Err(Error::new(
                        ErrorKind::InvalidOperation,
                        "generate_secret: name cannot be empty",
                    ));
                }

                // Validate length
                if length < 1 {
                    return Err(Error::new(
                        ErrorKind::InvalidOperation,
                        format!("generate_secret: length must be positive, got {}", length),
                    ));
                }

                if length > 4096 {
                    return Err(Error::new(
                        ErrorKind::InvalidOperation,
                        format!("generate_secret: length {} exceeds maximum of 4096", length),
                    ));
                }

                // Parse optional charset
                let charset = match charset {
                    Some(ref charset_str) => {
                        SecretCharset::parse(charset_str).ok_or_else(|| {
                            Error::new(
                                ErrorKind::InvalidOperation,
                                format!(
                                    "generate_secret: unknown charset '{}'. Valid options: \
                                 alphanumeric, alpha, numeric, hex, base64, urlsafe",
                                    charset_str
                                ),
                            )
                        })?
                    }
                    None => SecretCharset::default(),
                };

                // Generate or retrieve the secret
                let mut secret_gen = generator.lock().unwrap();
                let secret =
                    secret_gen.get_or_generate_with_charset(&name, length as usize, charset);

                Ok(secret)
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_secret_basic() {
        let mut env = Environment::new();
        let state = SecretFunctionState::new();
        state.register(&mut env);

        let template = r#"{{ generate_secret("test-password", 16) }}"#;
        let result = env.render_str(template, ()).unwrap();

        assert_eq!(result.len(), 16);
        assert!(result.chars().all(|c| c.is_ascii_alphanumeric()));
    }

    #[test]
    fn test_generate_secret_idempotent() {
        let mut env = Environment::new();
        let state = SecretFunctionState::new();
        state.register(&mut env);

        // Render twice with same name
        let template1 = r#"{{ generate_secret("db-password", 20) }}"#;
        let result1 = env.render_str(template1, ()).unwrap();
        let result2 = env.render_str(template1, ()).unwrap();

        // Should return the same value
        assert_eq!(result1, result2);
    }

    #[test]
    fn test_generate_secret_different_names() {
        let mut env = Environment::new();
        let state = SecretFunctionState::new();
        state.register(&mut env);

        let template =
            r#"{{ generate_secret("password1", 16) }}-{{ generate_secret("password2", 16) }}"#;
        let result = env.render_str(template, ()).unwrap();

        let parts: Vec<&str> = result.split('-').collect();
        assert_eq!(parts.len(), 2);
        assert_ne!(parts[0], parts[1]);
    }

    #[test]
    fn test_generate_secret_hex_charset() {
        let mut env = Environment::new();
        let state = SecretFunctionState::new();
        state.register(&mut env);

        let template = r#"{{ generate_secret("hex-token", 32, "hex") }}"#;
        let result = env.render_str(template, ()).unwrap();

        assert_eq!(result.len(), 32);
        assert!(result.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_generate_secret_numeric_charset() {
        let mut env = Environment::new();
        let state = SecretFunctionState::new();
        state.register(&mut env);

        let template = r#"{{ generate_secret("pin", 6, "numeric") }}"#;
        let result = env.render_str(template, ()).unwrap();

        assert_eq!(result.len(), 6);
        assert!(result.chars().all(|c| c.is_ascii_digit()));
    }

    #[test]
    fn test_generate_secret_invalid_charset() {
        let mut env = Environment::new();
        let state = SecretFunctionState::new();
        state.register(&mut env);

        let template = r#"{{ generate_secret("test", 16, "invalid") }}"#;
        let result = env.render_str(template, ());

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("unknown charset"));
    }

    #[test]
    fn test_generate_secret_missing_args() {
        let mut env = Environment::new();
        let state = SecretFunctionState::new();
        state.register(&mut env);

        // Missing length argument - MiniJinja handles this with its own error
        let template = r#"{{ generate_secret("only-name") }}"#;
        let result = env.render_str(template, ());

        assert!(result.is_err());
    }

    #[test]
    fn test_generate_secret_invalid_length() {
        let mut env = Environment::new();
        let state = SecretFunctionState::new();
        state.register(&mut env);

        // Zero length
        let result = env.render_str(r#"{{ generate_secret("test", 0) }}"#, ());
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("must be positive"));

        // Negative length - need fresh env because function is already registered
        let mut env2 = Environment::new();
        let state2 = SecretFunctionState::new();
        state2.register(&mut env2);
        let result = env2.render_str(r#"{{ generate_secret("test", -5) }}"#, ());
        assert!(result.is_err());

        // Too long
        let mut env3 = Environment::new();
        let state3 = SecretFunctionState::new();
        state3.register(&mut env3);
        let result = env3.render_str(r#"{{ generate_secret("test", 10000) }}"#, ());
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("exceeds maximum"));
    }

    #[test]
    fn test_state_is_dirty() {
        let state = SecretFunctionState::new();
        assert!(!state.is_dirty());

        let mut env = Environment::new();
        state.register(&mut env);

        env.render_str(r#"{{ generate_secret("new-secret", 16) }}"#, ())
            .unwrap();

        assert!(state.is_dirty());
    }

    #[test]
    fn test_state_persistence() {
        // First "install" - generate secrets
        let state1 = SecretFunctionState::new();
        let mut env1 = Environment::new();
        state1.register(&mut env1);

        let secret = env1
            .render_str(r#"{{ generate_secret("db-password", 24) }}"#, ())
            .unwrap();

        // Simulate persisting state
        let persisted = state1.take_state();
        let json = serde_json::to_string(&persisted).unwrap();

        // "Upgrade" - load existing state
        let loaded: SecretState = serde_json::from_str(&json).unwrap();
        let state2 = SecretFunctionState::with_state(loaded);
        let mut env2 = Environment::new();
        state2.register(&mut env2);

        let secret2 = env2
            .render_str(r#"{{ generate_secret("db-password", 24) }}"#, ())
            .unwrap();

        // Should return same value
        assert_eq!(secret, secret2);
        // Should NOT be dirty (secret already existed)
        assert!(!state2.is_dirty());
    }

    #[test]
    fn test_multiple_secrets_in_template() {
        let state = SecretFunctionState::new();
        let mut env = Environment::new();
        state.register(&mut env);

        let template = r#"
postgres-password: {{ generate_secret("postgres-password", 24) }}
replication-password: {{ generate_secret("replication-password", 24) }}
api-key: {{ generate_secret("api-key", 32, "hex") }}
"#;

        let result = env.render_str(template, ()).unwrap();

        // Verify we got different values for each
        let lines: Vec<&str> = result.lines().filter(|l| !l.is_empty()).collect();
        assert_eq!(lines.len(), 3);

        let postgres_pw = lines[0].split(": ").nth(1).unwrap();
        let repl_pw = lines[1].split(": ").nth(1).unwrap();
        let api_key = lines[2].split(": ").nth(1).unwrap();

        assert_ne!(postgres_pw, repl_pw);
        assert_ne!(postgres_pw, api_key);
        assert_eq!(postgres_pw.len(), 24);
        assert_eq!(repl_pw.len(), 24);
        assert_eq!(api_key.len(), 32);
    }
}
