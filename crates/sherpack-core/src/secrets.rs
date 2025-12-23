//! Secret generation and state management
//!
//! This module provides deterministic secret generation for Sherpack.
//! Unlike Helm's `randAlphaNum` which generates different values on each render,
//! Sherpack generates secrets once and stores them in cluster state.
//!
//! # Example
//!
//! ```jinja2
//! {# In templates #}
//! {{ generate_secret("db-password", 16) }}
//! {{ generate_secret("api-key", 32, "urlsafe") }}
//! ```
//!
//! # How it works
//!
//! 1. First `sherpack install`: generates random secrets, stores in Kubernetes Secret
//! 2. Subsequent operations: reads existing values from state
//! 3. Result: deterministic output, GitOps compatible

use chrono::{DateTime, Utc};
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// =============================================================================
// CHARSET
// =============================================================================

/// Character sets for secret generation
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum SecretCharset {
    /// a-zA-Z0-9 (default)
    #[default]
    Alphanumeric,
    /// a-zA-Z
    Alpha,
    /// 0-9
    Numeric,
    /// 0-9a-f
    Hex,
    /// a-zA-Z0-9+/
    Base64,
    /// a-zA-Z0-9-_ (URL safe)
    UrlSafe,
}

impl SecretCharset {
    /// Get the character set as bytes
    pub const fn chars(&self) -> &'static [u8] {
        match self {
            Self::Alphanumeric => b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789",
            Self::Alpha => b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ",
            Self::Numeric => b"0123456789",
            Self::Hex => b"0123456789abcdef",
            Self::Base64 => b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/",
            Self::UrlSafe => b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_",
        }
    }

    /// Parse charset from string
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "alphanumeric" | "alnum" => Some(Self::Alphanumeric),
            "alpha" => Some(Self::Alpha),
            "numeric" | "num" | "digits" => Some(Self::Numeric),
            "hex" => Some(Self::Hex),
            "base64" => Some(Self::Base64),
            "urlsafe" | "url" => Some(Self::UrlSafe),
            _ => None,
        }
    }
}

// =============================================================================
// SECRET ENTRY
// =============================================================================

/// A generated secret with metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecretEntry {
    /// The secret value
    value: String,

    /// When this secret was first generated
    pub created_at: DateTime<Utc>,

    /// When this secret was last rotated (if ever)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rotated_at: Option<DateTime<Utc>>,

    /// The charset used to generate this secret
    #[serde(default)]
    pub charset: SecretCharset,

    /// The length of the secret
    pub length: usize,
}

impl SecretEntry {
    /// Create a new secret entry
    pub fn new(value: String, charset: SecretCharset, length: usize) -> Self {
        Self {
            value,
            created_at: Utc::now(),
            rotated_at: None,
            charset,
            length,
        }
    }

    /// Get the secret value
    pub fn value(&self) -> &str {
        &self.value
    }

    /// Rotate the secret with a new value
    pub fn rotate(&mut self, new_value: String) {
        self.value = new_value;
        self.rotated_at = Some(Utc::now());
    }
}

// =============================================================================
// SECRET STATE
// =============================================================================

/// State of all generated secrets for a release
///
/// This is persisted to Kubernetes and loaded before each operation.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SecretState {
    /// Schema version for future migrations
    #[serde(default)]
    pub version: u32,

    /// Map of secret name to entry
    #[serde(default)]
    secrets: HashMap<String, SecretEntry>,

    /// Whether any new secrets were generated (not persisted)
    #[serde(skip)]
    dirty: bool,
}

impl SecretState {
    /// Current schema version
    pub const CURRENT_VERSION: u32 = 1;

    /// Create a new empty state
    pub fn new() -> Self {
        Self {
            version: Self::CURRENT_VERSION,
            secrets: HashMap::new(),
            dirty: false,
        }
    }

    /// Get a secret by name
    pub fn get(&self, name: &str) -> Option<&SecretEntry> {
        self.secrets.get(name)
    }

    /// Get a secret value by name
    pub fn get_value(&self, name: &str) -> Option<&str> {
        self.secrets.get(name).map(|e| e.value())
    }

    /// Insert a new secret
    pub fn insert(&mut self, name: String, entry: SecretEntry) {
        self.secrets.insert(name, entry);
        self.dirty = true;
    }

    /// Check if any new secrets were generated
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// Mark as clean (after persisting)
    pub fn mark_clean(&mut self) {
        self.dirty = false;
    }

    /// Get all secret names
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.secrets.keys().map(String::as_str)
    }

    /// Get all secrets
    pub fn iter(&self) -> impl Iterator<Item = (&str, &SecretEntry)> {
        self.secrets.iter().map(|(k, v)| (k.as_str(), v))
    }

    /// Number of secrets
    pub fn len(&self) -> usize {
        self.secrets.len()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.secrets.is_empty()
    }

    /// Rotate a secret
    pub fn rotate(&mut self, name: &str, new_value: String) -> bool {
        if let Some(entry) = self.secrets.get_mut(name) {
            entry.rotate(new_value);
            self.dirty = true;
            true
        } else {
            false
        }
    }
}

// Implement PartialEq manually to ignore dirty flag
impl PartialEq<SecretState> for SecretState {
    fn eq(&self, other: &SecretState) -> bool {
        self.version == other.version && self.secrets == other.secrets
    }
}

impl PartialEq for SecretEntry {
    fn eq(&self, other: &Self) -> bool {
        // Compare only value, charset, and length (not timestamps)
        self.value == other.value && self.charset == other.charset && self.length == other.length
    }
}

// =============================================================================
// SECRET GENERATOR
// =============================================================================

/// Secret generator with state management
///
/// This is the main interface for generating secrets. It maintains state
/// to ensure idempotent generation.
#[derive(Debug)]
pub struct SecretGenerator {
    state: SecretState,
    rng: StdRng,
}

impl SecretGenerator {
    /// Create a new generator with empty state
    pub fn new() -> Self {
        Self {
            state: SecretState::new(),
            rng: StdRng::from_rng(&mut rand::rng()),
        }
    }

    /// Create from existing state (loaded from Kubernetes)
    pub fn with_state(state: SecretState) -> Self {
        Self {
            state,
            rng: StdRng::from_rng(&mut rand::rng()),
        }
    }

    /// Get or generate a secret with default charset
    pub fn get_or_generate(&mut self, name: &str, length: usize) -> String {
        self.get_or_generate_with_charset(name, length, SecretCharset::default())
    }

    /// Get or generate a secret with specific charset
    pub fn get_or_generate_with_charset(
        &mut self,
        name: &str,
        length: usize,
        charset: SecretCharset,
    ) -> String {
        // Return existing secret if present
        if let Some(entry) = self.state.get(name) {
            return entry.value().to_string();
        }

        // Generate new secret
        let value = self.generate_random(length, charset);
        let entry = SecretEntry::new(value.clone(), charset, length);
        self.state.insert(name.to_string(), entry);

        value
    }

    /// Generate a random string (internal)
    fn generate_random(&mut self, length: usize, charset: SecretCharset) -> String {
        let chars = charset.chars();
        (0..length)
            .map(|_| {
                let idx = self.rng.random_range(0..chars.len());
                chars[idx] as char
            })
            .collect()
    }

    /// Get the current state
    pub fn state(&self) -> &SecretState {
        &self.state
    }

    /// Take ownership of the state (consumes the generator)
    pub fn into_state(self) -> SecretState {
        self.state
    }

    /// Check if any new secrets were generated
    pub fn is_dirty(&self) -> bool {
        self.state.is_dirty()
    }

    /// Rotate a secret with a new random value
    pub fn rotate(&mut self, name: &str) -> Option<String> {
        let entry = self.state.get(name)?;
        let new_value = self.generate_random(entry.length, entry.charset);
        self.state.rotate(name, new_value.clone());
        Some(new_value)
    }
}

impl Default for SecretGenerator {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// TESTS
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_charset_chars() {
        assert_eq!(SecretCharset::Numeric.chars(), b"0123456789");
        assert_eq!(SecretCharset::Hex.chars().len(), 16);
        assert_eq!(SecretCharset::Alphanumeric.chars().len(), 62);
    }

    #[test]
    fn test_charset_parse() {
        assert_eq!(SecretCharset::parse("hex"), Some(SecretCharset::Hex));
        assert_eq!(
            SecretCharset::parse("ALPHANUMERIC"),
            Some(SecretCharset::Alphanumeric)
        );
        assert_eq!(SecretCharset::parse("unknown"), None);
    }

    #[test]
    fn test_generator_idempotent() {
        let mut generator = SecretGenerator::new();

        let secret1 = generator.get_or_generate("test", 16);
        let secret2 = generator.get_or_generate("test", 16);

        assert_eq!(secret1, secret2);
        assert_eq!(secret1.len(), 16);
    }

    #[test]
    fn test_generator_different_names() {
        let mut generator = SecretGenerator::new();

        let secret1 = generator.get_or_generate("password1", 16);
        let secret2 = generator.get_or_generate("password2", 16);

        assert_ne!(secret1, secret2);
    }

    #[test]
    fn test_generator_charset() {
        let mut generator = SecretGenerator::new();

        let hex = generator.get_or_generate_with_charset("hex-token", 32, SecretCharset::Hex);
        assert!(hex.chars().all(|c| c.is_ascii_hexdigit()));

        let numeric = generator.get_or_generate_with_charset("pin", 6, SecretCharset::Numeric);
        assert!(numeric.chars().all(|c| c.is_ascii_digit()));
    }

    #[test]
    fn test_state_persistence() {
        let mut generator1 = SecretGenerator::new();
        let secret = generator1.get_or_generate("db-password", 24);

        // Simulate persistence
        let state = generator1.into_state();
        let json = serde_json::to_string(&state).unwrap();

        // Simulate reload
        let loaded_state: SecretState = serde_json::from_str(&json).unwrap();
        let mut generator2 = SecretGenerator::with_state(loaded_state);

        // Should return same secret
        let secret2 = generator2.get_or_generate("db-password", 24);
        assert_eq!(secret, secret2);
        assert!(!generator2.is_dirty()); // Not dirty because secret already existed
    }

    #[test]
    fn test_rotate() {
        let mut generator = SecretGenerator::new();

        let original = generator.get_or_generate("api-key", 32);
        let rotated = generator.rotate("api-key").unwrap();

        assert_ne!(original, rotated);
        assert_eq!(rotated.len(), 32);

        // Getting the secret again should return rotated value
        let current = generator.get_or_generate("api-key", 32);
        assert_eq!(current, rotated);
    }

    #[test]
    fn test_dirty_flag() {
        let mut generator = SecretGenerator::new();
        assert!(!generator.is_dirty());

        generator.get_or_generate("new-secret", 16);
        assert!(generator.is_dirty());

        let mut state = generator.into_state();
        state.mark_clean();
        assert!(!state.is_dirty());
    }
}
