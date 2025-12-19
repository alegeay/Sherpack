//! Secure credential management with redirect protection
//!
//! Key security features:
//! - Credentials scoped to specific URL prefixes
//! - NEVER sends credentials after cross-origin redirect
//! - Support for environment variables (CI/CD friendly)
//! - Optional Docker credential helper integration

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use url::Url;

use crate::error::{RepoError, Result};

/// Credential types supported
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Credentials {
    /// Basic authentication (username/password)
    Basic { username: String, password: String },

    /// Bearer token authentication
    Bearer { token: String },

    /// Environment variable references (CI/CD friendly)
    Env {
        username_var: String,
        password_var: String,
    },

    /// Docker config.json reference
    DockerConfig { path: Option<PathBuf> },
}

impl Credentials {
    /// Create basic auth credentials
    pub fn basic(username: impl Into<String>, password: impl Into<String>) -> Self {
        Credentials::Basic {
            username: username.into(),
            password: password.into(),
        }
    }

    /// Create bearer token credentials
    pub fn bearer(token: impl Into<String>) -> Self {
        Credentials::Bearer {
            token: token.into(),
        }
    }

    /// Create environment variable credentials
    pub fn from_env(username_var: impl Into<String>, password_var: impl Into<String>) -> Self {
        Credentials::Env {
            username_var: username_var.into(),
            password_var: password_var.into(),
        }
    }

    /// Resolve credentials to actual values
    pub fn resolve(&self) -> Result<ResolvedCredentials> {
        match self {
            Credentials::Basic { username, password } => Ok(ResolvedCredentials::Basic {
                username: username.clone(),
                password: password.clone(),
            }),
            Credentials::Bearer { token } => Ok(ResolvedCredentials::Bearer {
                token: token.clone(),
            }),
            Credentials::Env {
                username_var,
                password_var,
            } => {
                let username = std::env::var(username_var).map_err(|_| RepoError::AuthFailed {
                    message: format!("Environment variable {} not set", username_var),
                })?;
                let password = std::env::var(password_var).map_err(|_| RepoError::AuthFailed {
                    message: format!("Environment variable {} not set", password_var),
                })?;
                Ok(ResolvedCredentials::Basic { username, password })
            }
            Credentials::DockerConfig { path } => {
                let docker_config = load_docker_config(path.as_deref())?;
                Ok(ResolvedCredentials::DockerAuth(docker_config))
            }
        }
    }
}

/// Resolved credentials ready for use
#[derive(Debug, Clone)]
pub enum ResolvedCredentials {
    Basic { username: String, password: String },
    Bearer { token: String },
    DockerAuth(DockerConfig),
}

impl ResolvedCredentials {
    /// Get authorization header value for a URL
    pub fn auth_header(&self, url: &str) -> Option<String> {
        match self {
            ResolvedCredentials::Basic { username, password } => {
                let encoded = base64::Engine::encode(
                    &base64::engine::general_purpose::STANDARD,
                    format!("{}:{}", username, password),
                );
                Some(format!("Basic {}", encoded))
            }
            ResolvedCredentials::Bearer { token } => Some(format!("Bearer {}", token)),
            ResolvedCredentials::DockerAuth(config) => config.auth_for_url(url),
        }
    }
}

/// Docker config.json format
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DockerConfig {
    #[serde(default)]
    pub auths: HashMap<String, DockerAuth>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DockerAuth {
    #[serde(default)]
    pub auth: Option<String>,
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub password: Option<String>,
}

impl DockerConfig {
    /// Get auth header for a URL
    pub fn auth_for_url(&self, url: &str) -> Option<String> {
        let parsed = Url::parse(url).ok()?;
        let host = parsed.host_str()?;

        // Try exact match first, then registry variations
        let candidates = [
            host.to_string(),
            format!("https://{}", host),
            format!("http://{}", host),
        ];

        for candidate in &candidates {
            if let Some(auth) = self.auths.get(candidate) {
                if let Some(encoded) = &auth.auth {
                    return Some(format!("Basic {}", encoded));
                }
                if let (Some(u), Some(p)) = (&auth.username, &auth.password) {
                    let encoded = base64::Engine::encode(
                        &base64::engine::general_purpose::STANDARD,
                        format!("{}:{}", u, p),
                    );
                    return Some(format!("Basic {}", encoded));
                }
            }
        }
        None
    }
}

/// Load Docker config from default or specified path
fn load_docker_config(path: Option<&Path>) -> Result<DockerConfig> {
    let config_path = match path {
        Some(p) => p.to_path_buf(),
        None => {
            let home = dirs::home_dir().ok_or_else(|| RepoError::AuthFailed {
                message: "Could not determine home directory".to_string(),
            })?;
            home.join(".docker").join("config.json")
        }
    };

    if !config_path.exists() {
        return Ok(DockerConfig::default());
    }

    let content = std::fs::read_to_string(&config_path)?;
    let config: DockerConfig = serde_json::from_str(&content)?;
    Ok(config)
}

/// Credential store - manages credentials scoped to repositories
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CredentialStore {
    /// Credentials by repository name
    #[serde(default)]
    credentials: HashMap<String, Credentials>,
}

impl CredentialStore {
    /// Load credential store from default location
    pub fn load() -> Result<Self> {
        let path = Self::default_path()?;
        if path.exists() {
            Self::load_from(&path)
        } else {
            Ok(Self::default())
        }
    }

    /// Load from specific path
    pub fn load_from(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let store: Self = serde_yaml::from_str(&content)?;
        Ok(store)
    }

    /// Save to default location
    pub fn save(&self) -> Result<()> {
        let path = Self::default_path()?;
        self.save_to(&path)
    }

    /// Save to specific path
    pub fn save_to(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        // Set restrictive permissions on Unix
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            let content = serde_yaml::to_string(self)?;
            let mut options = std::fs::OpenOptions::new();
            options.write(true).create(true).truncate(true).mode(0o600);
            std::io::Write::write_all(&mut options.open(path)?, content.as_bytes())?;
            Ok(())
        }

        #[cfg(not(unix))]
        {
            let content = serde_yaml::to_string(self)?;
            std::fs::write(path, content)?;
            Ok(())
        }
    }

    /// Get default credential store path
    pub fn default_path() -> Result<PathBuf> {
        let config_dir = dirs::config_dir().ok_or_else(|| RepoError::InvalidConfig {
            message: "Could not determine config directory".to_string(),
        })?;
        Ok(config_dir.join("sherpack").join("credentials.yaml"))
    }

    /// Store credentials for a repository
    pub fn set(&mut self, repo_name: &str, credentials: Credentials) {
        self.credentials.insert(repo_name.to_string(), credentials);
    }

    /// Get credentials for a repository
    pub fn get(&self, repo_name: &str) -> Option<&Credentials> {
        self.credentials.get(repo_name)
    }

    /// Remove credentials for a repository
    pub fn remove(&mut self, repo_name: &str) -> Option<Credentials> {
        self.credentials.remove(repo_name)
    }

    /// Check if credentials exist for a repository
    pub fn has(&self, repo_name: &str) -> bool {
        self.credentials.contains_key(repo_name)
    }
}

/// Scoped credentials - maps URL prefixes to credentials
/// SECURITY: Never sends credentials to URLs outside the scope
#[derive(Debug, Clone, Default)]
pub struct ScopedCredentials {
    scopes: HashMap<String, ResolvedCredentials>,
}

impl ScopedCredentials {
    /// Add credentials for a URL scope
    pub fn add(&mut self, url_prefix: &str, credentials: ResolvedCredentials) {
        // Normalize the prefix
        let prefix = url_prefix.trim_end_matches('/').to_string();
        self.scopes.insert(prefix, credentials);
    }

    /// Get credentials for a URL (by longest matching prefix)
    pub fn for_url(&self, url: &str) -> Option<&ResolvedCredentials> {
        self.scopes
            .iter()
            .filter(|(prefix, _)| url.starts_with(prefix.as_str()))
            .max_by_key(|(prefix, _)| prefix.len())
            .map(|(_, creds)| creds)
    }

    /// Check if two URLs are same-origin (for redirect safety)
    pub fn same_origin(url1: &str, url2: &str) -> bool {
        let parse1 = Url::parse(url1);
        let parse2 = Url::parse(url2);

        match (parse1, parse2) {
            (Ok(u1), Ok(u2)) => {
                u1.scheme() == u2.scheme()
                    && u1.host() == u2.host()
                    && u1.port_or_known_default() == u2.port_or_known_default()
            }
            _ => false,
        }
    }
}

/// Secure HTTP client wrapper with redirect protection
pub struct SecureHttpClient {
    client: reqwest::Client,
    credentials: ScopedCredentials,
}

impl SecureHttpClient {
    /// Create a new secure HTTP client
    pub fn new(credentials: ScopedCredentials) -> Result<Self> {
        let client = reqwest::Client::builder()
            // CRITICAL: Disable automatic redirect following
            // We handle redirects manually to prevent credential leaks
            .redirect(reqwest::redirect::Policy::none())
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .map_err(|e| RepoError::NetworkError {
                message: e.to_string(),
            })?;

        Ok(Self {
            client,
            credentials,
        })
    }

    /// Create without credentials (public repos)
    pub fn public() -> Result<Self> {
        Self::new(ScopedCredentials::default())
    }

    /// Fetch a URL with secure redirect handling
    ///
    /// SECURITY: Credentials are NEVER sent after cross-origin redirects
    pub async fn get(&self, url: &str) -> Result<reqwest::Response> {
        self.get_with_redirects(url, 10).await
    }

    async fn get_with_redirects(&self, url: &str, max_redirects: u32) -> Result<reqwest::Response> {
        let mut current_url = url.to_string();
        let mut redirects = 0;
        let original_url = url.to_string();

        loop {
            // Build request
            let mut request = self.client.get(&current_url);

            // Add auth ONLY if same origin as original URL
            if ScopedCredentials::same_origin(&original_url, &current_url) {
                if let Some(creds) = self.credentials.for_url(&current_url) {
                    if let Some(auth) = creds.auth_header(&current_url) {
                        request = request.header("Authorization", auth);
                    }
                }
            } else {
                // Cross-origin redirect - DO NOT send credentials
                tracing::warn!(
                    "Cross-origin redirect from {} to {} - credentials not forwarded",
                    original_url,
                    current_url
                );
            }

            let response = request.send().await?;
            let status = response.status();

            // Handle redirects
            if status.is_redirection() {
                redirects += 1;
                if redirects > max_redirects {
                    return Err(RepoError::NetworkError {
                        message: format!("Too many redirects (max {})", max_redirects),
                    });
                }

                let location = response
                    .headers()
                    .get("Location")
                    .and_then(|v| v.to_str().ok())
                    .ok_or_else(|| RepoError::NetworkError {
                        message: "Redirect without Location header".to_string(),
                    })?;

                // Resolve relative URLs
                let base = Url::parse(&current_url)?;
                let redirect_url = base.join(location)?;
                current_url = redirect_url.to_string();

                continue;
            }

            // Handle rate limiting
            if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
                let retry_after = response
                    .headers()
                    .get("Retry-After")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(60);

                return Err(RepoError::RateLimited { retry_after });
            }

            // Handle auth errors
            if status == reqwest::StatusCode::UNAUTHORIZED {
                return Err(RepoError::AuthRequired { url: current_url });
            }
            if status == reqwest::StatusCode::FORBIDDEN {
                return Err(RepoError::AuthFailed {
                    message: format!("Access denied to {}", current_url),
                });
            }

            // Handle other errors
            if !status.is_success() {
                return Err(RepoError::HttpError {
                    status: status.as_u16(),
                    message: format!("Request to {} failed", current_url),
                });
            }

            return Ok(response);
        }
    }

    /// Fetch bytes from URL
    pub async fn get_bytes(&self, url: &str) -> Result<Vec<u8>> {
        let response = self.get(url).await?;
        let bytes = response.bytes().await.map_err(|e| RepoError::NetworkError {
            message: e.to_string(),
        })?;
        Ok(bytes.to_vec())
    }

    /// Fetch text from URL
    pub async fn get_text(&self, url: &str) -> Result<String> {
        let response = self.get(url).await?;
        let text = response.text().await.map_err(|e| RepoError::NetworkError {
            message: e.to_string(),
        })?;
        Ok(text)
    }

    /// Fetch with ETag caching
    pub async fn get_cached(
        &self,
        url: &str,
        etag: Option<&str>,
    ) -> Result<CachedResponse> {
        let mut current_url = url.to_string();
        let mut redirects = 0;
        let original_url = url.to_string();

        loop {
            let mut request = self.client.get(&current_url);

            // Add ETag for conditional request
            if let Some(etag) = etag {
                request = request.header("If-None-Match", etag);
            }

            // Add auth if same origin
            if ScopedCredentials::same_origin(&original_url, &current_url) {
                if let Some(creds) = self.credentials.for_url(&current_url) {
                    if let Some(auth) = creds.auth_header(&current_url) {
                        request = request.header("Authorization", auth);
                    }
                }
            }

            let response = request.send().await?;
            let status = response.status();

            // Handle redirects
            if status.is_redirection() {
                redirects += 1;
                if redirects > 10 {
                    return Err(RepoError::NetworkError {
                        message: "Too many redirects".to_string(),
                    });
                }
                let location = response
                    .headers()
                    .get("Location")
                    .and_then(|v| v.to_str().ok())
                    .ok_or_else(|| RepoError::NetworkError {
                        message: "Redirect without Location header".to_string(),
                    })?;
                let base = Url::parse(&current_url)?;
                let redirect_url = base.join(location)?;
                current_url = redirect_url.to_string();
                continue;
            }

            // Not modified - use cache
            if status == reqwest::StatusCode::NOT_MODIFIED {
                return Ok(CachedResponse::NotModified);
            }

            if !status.is_success() {
                return Err(RepoError::HttpError {
                    status: status.as_u16(),
                    message: format!("Request to {} failed", current_url),
                });
            }

            // Get new ETag
            let new_etag = response
                .headers()
                .get("ETag")
                .and_then(|v| v.to_str().ok())
                .map(String::from);

            let bytes = response.bytes().await.map_err(|e| RepoError::NetworkError {
                message: e.to_string(),
            })?;

            return Ok(CachedResponse::Fresh {
                data: bytes.to_vec(),
                etag: new_etag,
            });
        }
    }
}

/// Response from a cached request
#[derive(Debug)]
pub enum CachedResponse {
    /// Content hasn't changed, use cached version
    NotModified,
    /// Fresh content with optional new ETag
    Fresh { data: Vec<u8>, etag: Option<String> },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_same_origin() {
        assert!(ScopedCredentials::same_origin(
            "https://example.com/foo",
            "https://example.com/bar"
        ));
        assert!(ScopedCredentials::same_origin(
            "https://example.com:443/foo",
            "https://example.com/bar"
        ));
        assert!(!ScopedCredentials::same_origin(
            "https://example.com/foo",
            "https://other.com/bar"
        ));
        assert!(!ScopedCredentials::same_origin(
            "https://example.com/foo",
            "http://example.com/bar"
        ));
        assert!(!ScopedCredentials::same_origin(
            "https://example.com/foo",
            "https://example.com:8443/bar"
        ));
    }

    #[test]
    fn test_scoped_credentials() {
        let mut scoped = ScopedCredentials::default();
        scoped.add(
            "https://private.example.com",
            ResolvedCredentials::Bearer {
                token: "secret".to_string(),
            },
        );

        // Matches prefix
        assert!(scoped
            .for_url("https://private.example.com/index.yaml")
            .is_some());
        assert!(scoped
            .for_url("https://private.example.com/charts/nginx.tgz")
            .is_some());

        // Doesn't match different host
        assert!(scoped
            .for_url("https://public.example.com/index.yaml")
            .is_none());
    }

    #[test]
    fn test_credentials_resolve() {
        let basic = Credentials::basic("user", "pass");
        let resolved = basic.resolve().unwrap();

        match resolved {
            ResolvedCredentials::Basic { username, password } => {
                assert_eq!(username, "user");
                assert_eq!(password, "pass");
            }
            _ => panic!("Expected Basic credentials"),
        }
    }

    #[test]
    fn test_env_credentials() {
        // SAFETY: Test runs in single thread, no concurrent access to env vars
        unsafe {
            std::env::set_var("TEST_USER_VAR", "testuser");
            std::env::set_var("TEST_PASS_VAR", "testpass");
        }

        let env_creds = Credentials::from_env("TEST_USER_VAR", "TEST_PASS_VAR");
        let resolved = env_creds.resolve().unwrap();

        match resolved {
            ResolvedCredentials::Basic { username, password } => {
                assert_eq!(username, "testuser");
                assert_eq!(password, "testpass");
            }
            _ => panic!("Expected Basic credentials"),
        }

        // SAFETY: Test runs in single thread, no concurrent access to env vars
        unsafe {
            std::env::remove_var("TEST_USER_VAR");
            std::env::remove_var("TEST_PASS_VAR");
        }
    }

    #[test]
    fn test_credential_store() {
        let mut store = CredentialStore::default();

        store.set("bitnami", Credentials::basic("user", "pass"));
        assert!(store.has("bitnami"));
        assert!(!store.has("other"));

        let creds = store.get("bitnami").unwrap();
        assert!(matches!(creds, Credentials::Basic { .. }));

        store.remove("bitnami");
        assert!(!store.has("bitnami"));
    }
}
