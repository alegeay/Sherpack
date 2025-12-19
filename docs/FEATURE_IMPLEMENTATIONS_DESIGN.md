# Design des Implémentations - Rust Idiomatique

Ce document détaille l'architecture et l'implémentation des features manquantes de manière élégante, modulaire et idiomatique en Rust.

---

## Table des Matières

1. [Principes de Design](#principes-de-design)
2. [Feature 1: Files API](#feature-1-files-api)
3. [Feature 2: Subchart Value Scoping](#feature-2-subchart-value-scoping)
4. [Feature 3: NOTES.txt Amélioré](#feature-3-notestxt-amélioré)
5. [Feature 4: sherpack test](#feature-4-sherpack-test)
6. [Feature 5: --atomic Amélioré](#feature-5---atomic-amélioré)
7. [Feature 6: CRDs Directory](#feature-6-crds-directory)
8. [Résumé des Changements](#résumé-des-changements)

---

## Principes de Design

### Patterns Existants à Respecter

```rust
// 1. Builder Pattern (fluent API)
Engine::builder().strict(true).build()
InstallOptions::new("app", "ns").with_wait(Duration::minutes(5))

// 2. Trait-Based Polymorphism
pub trait StorageDriver: Send + Sync { ... }
pub trait RepositoryBackend: Send + Sync { ... }

// 3. Generic Types avec Trait Bounds
pub struct KubeClient<S: StorageDriver> { ... }

// 4. Error Handling avec thiserror
#[derive(Error, Debug)]
pub enum CoreError { ... }
pub type Result<T> = std::result::Result<T, CoreError>;

// 5. Composition over Inheritance
pub struct KubeClient<S> {
    client: kube::Client,
    storage: S,
    engine: Engine,
    diff_engine: DiffEngine,
}
```

### Nouvelles Conventions

```rust
// Newtype Pattern pour type safety
pub struct PackRoot(PathBuf);
pub struct TemplatesDir(PathBuf);

// Extension Traits pour augmenter les types existants
pub trait ValuesExt {
    fn scope_for_subchart(&self, name: &str) -> Self;
}

// Zero-Cost Abstractions via const generics quand approprié
pub struct Files<const SANDBOXED: bool = true> { ... }
```

---

## Feature 1: Files API

### Objectif
Permettre l'accès aux fichiers du pack depuis les templates, avec sécurité sandbox.

### Architecture

```
sherpack-core/
└── src/
    ├── files.rs      # NOUVEAU: Files API
    └── lib.rs        # Re-export Files

sherpack-engine/
└── src/
    ├── functions.rs  # Ajouter files_get, files_glob, files_lines
    └── engine.rs     # Injecter Files dans le contexte
```

### Implémentation

#### `sherpack-core/src/files.rs`

```rust
//! Files API pour accéder aux fichiers du pack depuis les templates
//!
//! Sécurité: Toutes les opérations sont sandboxées au répertoire du pack.

use std::path::{Path, PathBuf};
use std::collections::HashMap;
use std::sync::Arc;
use glob::Pattern;
use crate::error::{CoreError, Result};

/// Trait pour l'accès aux fichiers (permet le mocking)
pub trait FileProvider: Send + Sync {
    /// Lit le contenu d'un fichier
    fn get(&self, path: &str) -> Result<Vec<u8>>;

    /// Vérifie si un fichier existe
    fn exists(&self, path: &str) -> bool;

    /// Liste les fichiers correspondant à un pattern glob
    fn glob(&self, pattern: &str) -> Result<Vec<FileEntry>>;

    /// Lit un fichier ligne par ligne
    fn lines(&self, path: &str) -> Result<Vec<String>>;
}

/// Entrée de fichier retournée par glob
#[derive(Debug, Clone, serde::Serialize)]
pub struct FileEntry {
    /// Chemin relatif au pack
    pub path: String,
    /// Nom du fichier (sans répertoire)
    pub name: String,
    /// Contenu du fichier
    pub content: String,
}

/// Provider de fichiers sandboxé au répertoire du pack
#[derive(Debug, Clone)]
pub struct SandboxedFileProvider {
    /// Répertoire racine du pack
    root: PathBuf,
    /// Cache des fichiers lus (évite les lectures répétées)
    cache: Arc<parking_lot::RwLock<HashMap<PathBuf, Vec<u8>>>>,
}

impl SandboxedFileProvider {
    /// Crée un nouveau provider sandboxé
    pub fn new(pack_root: impl AsRef<Path>) -> Self {
        Self {
            root: pack_root.as_ref().to_path_buf(),
            cache: Arc::new(parking_lot::RwLock::new(HashMap::new())),
        }
    }

    /// Résout et valide un chemin relatif
    ///
    /// Retourne une erreur si le chemin tente d'échapper au sandbox.
    fn resolve_path(&self, relative: &str) -> Result<PathBuf> {
        // Normaliser le chemin (supprimer .., .)
        let requested = Path::new(relative);

        // Empêcher les chemins absolus
        if requested.is_absolute() {
            return Err(CoreError::FileAccess {
                path: relative.to_string(),
                message: "absolute paths are not allowed".to_string(),
            });
        }

        // Construire le chemin complet et le canonicaliser
        let full_path = self.root.join(relative);

        // Vérifier que le chemin résolu est bien dans le sandbox
        // (protège contre les attaques ../../../etc/passwd)
        let canonical = full_path.canonicalize().map_err(|e| {
            CoreError::FileAccess {
                path: relative.to_string(),
                message: format!("failed to resolve path: {}", e),
            }
        })?;

        let canonical_root = self.root.canonicalize().map_err(|e| {
            CoreError::FileAccess {
                path: self.root.display().to_string(),
                message: format!("failed to resolve pack root: {}", e),
            }
        })?;

        if !canonical.starts_with(&canonical_root) {
            return Err(CoreError::FileAccess {
                path: relative.to_string(),
                message: "path escapes pack directory (sandbox violation)".to_string(),
            });
        }

        Ok(canonical)
    }
}

impl FileProvider for SandboxedFileProvider {
    fn get(&self, path: &str) -> Result<Vec<u8>> {
        let resolved = self.resolve_path(path)?;

        // Vérifier le cache d'abord
        {
            let cache = self.cache.read();
            if let Some(content) = cache.get(&resolved) {
                return Ok(content.clone());
            }
        }

        // Lire le fichier
        let content = std::fs::read(&resolved).map_err(|e| {
            CoreError::FileAccess {
                path: path.to_string(),
                message: format!("failed to read file: {}", e),
            }
        })?;

        // Mettre en cache
        {
            let mut cache = self.cache.write();
            cache.insert(resolved, content.clone());
        }

        Ok(content)
    }

    fn exists(&self, path: &str) -> bool {
        self.resolve_path(path)
            .map(|p| p.exists())
            .unwrap_or(false)
    }

    fn glob(&self, pattern: &str) -> Result<Vec<FileEntry>> {
        // Valider le pattern
        let glob_pattern = Pattern::new(pattern).map_err(|e| {
            CoreError::FileAccess {
                path: pattern.to_string(),
                message: format!("invalid glob pattern: {}", e),
            }
        })?;

        let mut entries = Vec::new();

        // Walker récursif
        for entry in walkdir::WalkDir::new(&self.root)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
        {
            let rel_path = entry.path()
                .strip_prefix(&self.root)
                .unwrap_or(entry.path());

            let rel_str = rel_path.to_string_lossy();

            if glob_pattern.matches(&rel_str) {
                // Lire le contenu
                let content = std::fs::read_to_string(entry.path())
                    .unwrap_or_default();

                entries.push(FileEntry {
                    path: rel_str.to_string(),
                    name: entry.file_name().to_string_lossy().to_string(),
                    content,
                });
            }
        }

        // Tri stable pour déterminisme
        entries.sort_by(|a, b| a.path.cmp(&b.path));

        Ok(entries)
    }

    fn lines(&self, path: &str) -> Result<Vec<String>> {
        let content = self.get(path)?;
        let text = String::from_utf8_lossy(&content);
        Ok(text.lines().map(String::from).collect())
    }
}

/// Provider mock pour les tests
#[derive(Debug, Default)]
pub struct MockFileProvider {
    files: HashMap<String, Vec<u8>>,
}

impl MockFileProvider {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_file(mut self, path: &str, content: impl Into<Vec<u8>>) -> Self {
        self.files.insert(path.to_string(), content.into());
        self
    }
}

impl FileProvider for MockFileProvider {
    fn get(&self, path: &str) -> Result<Vec<u8>> {
        self.files.get(path).cloned().ok_or_else(|| {
            CoreError::FileAccess {
                path: path.to_string(),
                message: "file not found".to_string(),
            }
        })
    }

    fn exists(&self, path: &str) -> bool {
        self.files.contains_key(path)
    }

    fn glob(&self, pattern: &str) -> Result<Vec<FileEntry>> {
        let glob = Pattern::new(pattern).map_err(|e| {
            CoreError::FileAccess {
                path: pattern.to_string(),
                message: e.to_string(),
            }
        })?;

        Ok(self.files
            .iter()
            .filter(|(k, _)| glob.matches(k))
            .map(|(path, content)| FileEntry {
                path: path.clone(),
                name: Path::new(path)
                    .file_name()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_default(),
                content: String::from_utf8_lossy(content).to_string(),
            })
            .collect())
    }

    fn lines(&self, path: &str) -> Result<Vec<String>> {
        let content = self.get(path)?;
        Ok(String::from_utf8_lossy(&content)
            .lines()
            .map(String::from)
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_sandbox_prevents_escape() {
        let temp = TempDir::new().unwrap();
        let provider = SandboxedFileProvider::new(temp.path());

        // Tenter d'échapper au sandbox
        let result = provider.get("../../../etc/passwd");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("sandbox"));
    }

    #[test]
    fn test_glob_deterministic_order() {
        let provider = MockFileProvider::new()
            .with_file("config/b.yaml", "b")
            .with_file("config/a.yaml", "a")
            .with_file("config/c.yaml", "c");

        let entries = provider.glob("config/*.yaml").unwrap();
        let paths: Vec<_> = entries.iter().map(|e| &e.path).collect();

        assert_eq!(paths, vec!["config/a.yaml", "config/b.yaml", "config/c.yaml"]);
    }
}
```

#### Intégration dans `sherpack-engine/src/functions.rs`

```rust
use sherpack_core::files::{FileProvider, FileEntry};
use std::sync::Arc;

/// Object wrapper pour exposer Files dans les templates
#[derive(Debug, Clone)]
pub struct FilesObject(Arc<dyn FileProvider>);

impl minijinja::value::Object for FilesObject {
    fn repr(self: &Arc<Self>) -> minijinja::value::ObjectRepr {
        minijinja::value::ObjectRepr::Plain
    }

    fn call_method(
        self: &Arc<Self>,
        _state: &State,
        method: &str,
        args: &[Value],
    ) -> Result<Value, Error> {
        match method {
            "get" => {
                let path = args.first()
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| Error::new(
                        ErrorKind::InvalidOperation,
                        "files.get() requires a path string"
                    ))?;

                let content = self.0.get(path).map_err(|e| {
                    Error::new(ErrorKind::InvalidOperation, e.to_string())
                })?;

                // Retourner comme string (UTF-8 lossy)
                Ok(Value::from(String::from_utf8_lossy(&content).to_string()))
            }
            "get_bytes" => {
                let path = args.first()
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| Error::new(
                        ErrorKind::InvalidOperation,
                        "files.get_bytes() requires a path string"
                    ))?;

                let content = self.0.get(path).map_err(|e| {
                    Error::new(ErrorKind::InvalidOperation, e.to_string())
                })?;

                // Retourner comme bytes (pour b64encode)
                Ok(Value::from(content))
            }
            "exists" => {
                let path = args.first()
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| Error::new(
                        ErrorKind::InvalidOperation,
                        "files.exists() requires a path string"
                    ))?;

                Ok(Value::from(self.0.exists(path)))
            }
            "glob" => {
                let pattern = args.first()
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| Error::new(
                        ErrorKind::InvalidOperation,
                        "files.glob() requires a pattern string"
                    ))?;

                let entries = self.0.glob(pattern).map_err(|e| {
                    Error::new(ErrorKind::InvalidOperation, e.to_string())
                })?;

                Ok(Value::from_serialize(&entries))
            }
            "lines" => {
                let path = args.first()
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| Error::new(
                        ErrorKind::InvalidOperation,
                        "files.lines() requires a path string"
                    ))?;

                let lines = self.0.lines(path).map_err(|e| {
                    Error::new(ErrorKind::InvalidOperation, e.to_string())
                })?;

                Ok(Value::from(lines))
            }
            _ => Err(Error::new(
                ErrorKind::UnknownMethod,
                format!("files object has no method '{}'", method),
            )),
        }
    }
}

impl FilesObject {
    pub fn new(provider: Arc<dyn FileProvider>) -> Self {
        Self(provider)
    }
}
```

#### Usage dans Templates

```jinja2
{# Lire un fichier de configuration #}
data:
  nginx.conf: {{ files.get("config/nginx.conf") | b64encode }}

{# Vérifier l'existence #}
{% if files.exists("config/custom.yaml") %}
  custom: {{ files.get("config/custom.yaml") | toyaml | indent(4) }}
{% endif %}

{# Itérer sur plusieurs fichiers #}
{% for file in files.glob("scripts/*.sh") %}
  {{ file.name }}: {{ file.content | b64encode }}
{% endfor %}

{# Lire ligne par ligne #}
{% for line in files.lines("hosts.txt") %}
  - {{ line }}
{% endfor %}
```

---

## Feature 2: Subchart Value Scoping

### Objectif
Gérer correctement le scoping des values pour les subcharts (dépendances).

### Architecture

```
sherpack-core/
└── src/
    ├── values.rs     # Étendre avec scoping
    ├── context.rs    # SubchartContext
    └── pack.rs       # Gestion des subcharts chargés

sherpack-engine/
└── src/
    └── engine.rs     # Rendu des subcharts isolé
```

### Implémentation

#### Extension de `Values` dans `sherpack-core/src/values.rs`

```rust
impl Values {
    /// Extrait le scope des values pour un subchart
    ///
    /// Pour un subchart "redis", retourne:
    /// - `values.redis.*` comme scope principal
    /// - `values.global.*` accessible via `global.*`
    ///
    /// # Example
    ///
    /// ```yaml
    /// # values.yaml parent
    /// redis:
    ///   auth:
    ///     password: secret
    /// global:
    ///   imageRegistry: gcr.io
    /// ```
    ///
    /// Le subchart redis verra:
    /// ```yaml
    /// auth:
    ///   password: secret
    /// global:
    ///   imageRegistry: gcr.io
    /// ```
    pub fn scope_for_subchart(&self, subchart_name: &str) -> Values {
        let inner = self.as_json();
        let mut scoped = serde_json::Map::new();

        // 1. Extraire les values spécifiques au subchart
        if let Some(obj) = inner.as_object() {
            if let Some(subchart_values) = obj.get(subchart_name) {
                if let Some(subchart_obj) = subchart_values.as_object() {
                    for (k, v) in subchart_obj {
                        scoped.insert(k.clone(), v.clone());
                    }
                }
            }

            // 2. Copier les globals
            if let Some(global) = obj.get("global") {
                scoped.insert("global".to_string(), global.clone());
            }
        }

        Values::from_json(serde_json::Value::Object(scoped))
    }

    /// Vérifie si un subchart est activé
    ///
    /// Cherche `values.<subchart>.enabled` et retourne true par défaut.
    pub fn is_subchart_enabled(&self, subchart_name: &str) -> bool {
        self.get(&format!("{}.enabled", subchart_name))
            .and_then(|v| v.as_bool())
            .unwrap_or(true)  // Enabled par défaut
    }

    /// Annule une valeur (supporte null/~ pour vraiment supprimer)
    pub fn nullify(&mut self, path: &str) -> bool {
        // Implémentation qui gère vraiment null
        let parts: Vec<&str> = path.split('.').collect();
        self.nullify_recursive(&parts)
    }
}
```

#### Nouveau type `SubchartContext` dans `sherpack-core/src/context.rs`

```rust
/// Contexte pour le rendu d'un subchart
#[derive(Debug, Clone)]
pub struct SubchartContext {
    /// Contexte parent (pour accès optionnel)
    parent: Option<Box<TemplateContext>>,

    /// Nom du subchart
    pub name: String,

    /// Alias (si différent du nom)
    pub alias: Option<String>,

    /// Values scopées pour ce subchart
    pub values: serde_json::Value,

    /// Release info (héritée du parent, avec namespace potentiellement différent)
    pub release: ReleaseInfo,

    /// Pack info du subchart
    pub pack: PackInfo,

    /// Capabilities (héritées)
    pub capabilities: Capabilities,
}

impl SubchartContext {
    /// Crée un contexte de subchart à partir du contexte parent
    pub fn from_parent(
        parent: &TemplateContext,
        subchart_name: &str,
        subchart_pack: &PackMetadata,
        alias: Option<String>,
    ) -> Self {
        let values = Values::from_json(parent.values.clone())
            .scope_for_subchart(subchart_name);

        Self {
            parent: Some(Box::new(parent.clone())),
            name: subchart_name.to_string(),
            alias,
            values: values.into_json(),
            release: parent.release.clone(),
            pack: PackInfo::from(subchart_pack),
            capabilities: parent.capabilities.clone(),
        }
    }

    /// Accès au contexte parent (opt-in, non recommandé)
    pub fn parent(&self) -> Option<&TemplateContext> {
        self.parent.as_deref()
    }

    /// Convertit en TemplateContext standard pour le rendu
    pub fn as_template_context(&self) -> TemplateContext {
        TemplateContext {
            values: self.values.clone(),
            release: self.release.clone(),
            pack: self.pack.clone(),
            capabilities: self.capabilities.clone(),
            template: TemplateInfo::default(),
        }
    }
}
```

#### Rendu isolé des subcharts dans `sherpack-engine/src/engine.rs`

```rust
impl Engine {
    /// Rend un pack avec ses subcharts
    pub fn render_pack_with_subcharts(
        &self,
        pack: &LoadedPack,
        subcharts: &[LoadedSubchart],
        context: &TemplateContext,
    ) -> Result<RenderResultWithSubcharts> {
        let mut result = RenderResultWithSubcharts {
            manifests: IndexMap::new(),
            notes: None,
            subchart_manifests: HashMap::new(),
            report: RenderReport::new(),
        };

        // 1. Rendre le pack principal
        let main_result = self.render_pack_collect_errors(pack, context);
        result.manifests = main_result.manifests;
        result.notes = main_result.notes;
        result.report.merge(main_result.report);

        // 2. Rendre chaque subchart avec son contexte isolé
        for subchart in subcharts {
            // Vérifier si le subchart est activé
            let values = Values::from_json(context.values.clone());
            if !values.is_subchart_enabled(&subchart.effective_name()) {
                continue;
            }

            // Créer le contexte isolé
            let subchart_ctx = SubchartContext::from_parent(
                context,
                &subchart.name,
                &subchart.pack.pack.metadata,
                subchart.alias.clone(),
            );

            // Créer un nouvel engine pour isolation des helpers
            // (évite les conflits de noms entre subcharts)
            let subchart_engine = self.create_isolated_engine(&subchart.pack)?;

            let subchart_result = subchart_engine.render_pack_collect_errors(
                &subchart.pack,
                &subchart_ctx.as_template_context(),
            );

            // Préfixer les manifests avec le nom du subchart
            let prefixed: IndexMap<String, String> = subchart_result.manifests
                .into_iter()
                .map(|(name, content)| {
                    (format!("{}/{}", subchart.effective_name(), name), content)
                })
                .collect();

            result.subchart_manifests.insert(
                subchart.effective_name().to_string(),
                prefixed,
            );

            // Collecter les notes des subcharts
            if let Some(notes) = subchart_result.notes {
                result.subchart_notes.push((
                    subchart.effective_name().to_string(),
                    notes,
                ));
            }

            result.report.merge_with_prefix(
                subchart_result.report,
                &subchart.effective_name(),
            );
        }

        Ok(result)
    }

    /// Crée un engine isolé pour un subchart
    ///
    /// Les helpers définis dans le subchart ne polluent pas l'espace global.
    fn create_isolated_engine(&self, pack: &LoadedPack) -> Result<Engine> {
        // Pour l'instant, réutiliser la même config
        // Dans le futur: préfixer les macros/helpers du subchart
        Ok(Engine::new(self.strict_mode))
    }
}

/// Résultat du rendu avec subcharts
#[derive(Debug)]
pub struct RenderResultWithSubcharts {
    /// Manifests du pack principal
    pub manifests: IndexMap<String, String>,

    /// Notes du pack principal
    pub notes: Option<String>,

    /// Manifests par subchart
    pub subchart_manifests: HashMap<String, IndexMap<String, String>>,

    /// Notes des subcharts
    pub subchart_notes: Vec<(String, String)>,

    /// Rapport d'erreurs/succès
    pub report: RenderReport,
}

impl RenderResultWithSubcharts {
    /// Fusionne tous les manifests (principal + subcharts) dans l'ordre
    pub fn all_manifests(&self) -> IndexMap<String, String> {
        let mut all = self.manifests.clone();

        // Ajouter les subcharts dans l'ordre alphabétique (déterminisme)
        let mut subchart_names: Vec<_> = self.subchart_manifests.keys().collect();
        subchart_names.sort();

        for name in subchart_names {
            if let Some(manifests) = self.subchart_manifests.get(name) {
                all.extend(manifests.clone());
            }
        }

        all
    }
}
```

---

## Feature 3: NOTES.txt Amélioré

### Objectif
- Rendre NOTES.txt après l'installation (accès aux resources créées)
- Agréger les notes des subcharts
- Support dans `sherpack template --show-notes`

### Architecture

Le support NOTES.txt existe déjà partiellement (`NOTES_TEMPLATE_PATTERN` dans engine.rs).

### Implémentation

#### Extension dans `sherpack-kube/src/client.rs`

```rust
impl<S: StorageDriver> KubeClient<S> {
    /// Rend les notes post-installation avec informations runtime
    pub async fn render_post_install_notes(
        &self,
        pack: &LoadedPack,
        context: &TemplateContext,
        resources: &[ApplyResult],
    ) -> Result<Option<String>> {
        // Chercher NOTES.txt dans les templates
        let notes_template = pack.template_files()?
            .into_iter()
            .find(|p| p.file_name()
                .map(|n| n.to_string_lossy().to_lowercase().contains("notes"))
                .unwrap_or(false));

        let Some(notes_path) = notes_template else {
            return Ok(None);
        };

        // Enrichir le contexte avec les informations runtime
        let mut enhanced_context = context.clone();

        // Ajouter les resources créées
        enhanced_context.add_runtime_info(RuntimeInfo {
            resources: resources.iter().map(|r| ResourceInfo {
                kind: r.kind.clone(),
                name: r.name.clone(),
                namespace: r.namespace.clone(),
                created: r.created,
            }).collect(),
            // Pourrait inclure: LoadBalancer IPs, NodePorts, etc.
        });

        // Lire et rendre le template
        let template_content = std::fs::read_to_string(&notes_path)?;
        let engine = Engine::new(true);

        engine.render_string(&template_content, &enhanced_context, "NOTES.txt")
            .map(Some)
    }
}

/// Informations runtime injectées dans le contexte NOTES.txt
#[derive(Debug, Clone, Serialize)]
pub struct RuntimeInfo {
    /// Resources créées/mises à jour
    pub resources: Vec<ResourceInfo>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ResourceInfo {
    pub kind: String,
    pub name: String,
    pub namespace: Option<String>,
    pub created: bool,
}
```

#### Affichage CLI dans `sherpack-cli/src/commands/install.rs`

```rust
// Après installation réussie
if let Some(notes) = client.render_post_install_notes(&pack, &context, &results).await? {
    println!();
    println!("{}", "NOTES:".bold().cyan());
    println!("{}", "-".repeat(60));
    println!("{}", notes);
    println!("{}", "-".repeat(60));
}

// Afficher aussi les notes des subcharts
for (subchart_name, subchart_notes) in &render_result.subchart_notes {
    println!();
    println!("{} {}:", "NOTES from".bold().cyan(), subchart_name.bold());
    println!("{}", subchart_notes);
}
```

#### Option CLI pour `sherpack template`

```rust
#[derive(Parser)]
pub struct TemplateArgs {
    /// Show NOTES.txt content
    #[arg(long)]
    show_notes: bool,

    // ... autres args
}

// Dans l'exécution
if args.show_notes {
    if let Some(notes) = result.notes {
        eprintln!();  // stderr pour ne pas polluer le YAML output
        eprintln!("{}", "NOTES:".bold());
        eprintln!("{}", notes);
    }
}
```

---

## Feature 4: sherpack test

### Objectif
Commande `sherpack test` pour exécuter les tests de release avec:
- Exécution parallèle
- Streaming des logs
- Exit codes appropriés pour CI/CD

### Architecture

```
sherpack-kube/
└── src/
    ├── hooks.rs      # HookPhase::Test existe déjà
    └── test.rs       # NOUVEAU: TestRunner

sherpack-cli/
└── src/
    └── commands/
        └── test.rs   # NOUVEAU: commande test
```

### Implémentation

#### `sherpack-kube/src/test.rs`

```rust
//! Test runner pour les releases Sherpack

use std::time::Duration;
use futures::stream::{self, StreamExt};
use tokio::time::timeout;
use kube::{Api, api::LogParams};
use k8s_openapi::api::core::v1::Pod;

use crate::hooks::{Hook, HookPhase, HookExecutor};
use crate::error::{KubeError, Result};

/// Configuration pour l'exécution des tests
#[derive(Debug, Clone)]
pub struct TestConfig {
    /// Timeout global pour tous les tests
    pub timeout: Duration,

    /// Exécuter les tests en parallèle
    pub parallel: bool,

    /// Nombre max de tests parallèles
    pub parallelism: usize,

    /// Streamer les logs en temps réel
    pub stream_logs: bool,

    /// Filtrer par nom de test
    pub filter: Option<String>,

    /// Nettoyer les pods après exécution
    pub cleanup: bool,
}

impl Default for TestConfig {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(300),
            parallel: true,
            parallelism: 4,
            stream_logs: true,
            filter: None,
            cleanup: true,
        }
    }
}

impl TestConfig {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    pub fn with_parallel(mut self, parallel: bool) -> Self {
        self.parallel = parallel;
        self
    }

    pub fn with_filter(mut self, filter: impl Into<String>) -> Self {
        self.filter = Some(filter.into());
        self
    }
}

/// Résultat d'un test individuel
#[derive(Debug, Clone)]
pub struct TestResult {
    pub name: String,
    pub passed: bool,
    pub duration: Duration,
    pub logs: String,
    pub error: Option<String>,
}

/// Résultat global des tests
#[derive(Debug)]
pub struct TestSuiteResult {
    pub passed: usize,
    pub failed: usize,
    pub skipped: usize,
    pub results: Vec<TestResult>,
    pub total_duration: Duration,
}

impl TestSuiteResult {
    pub fn success(&self) -> bool {
        self.failed == 0
    }

    /// Exit code pour CI/CD
    pub fn exit_code(&self) -> i32 {
        if self.success() { 0 } else { 1 }
    }
}

/// Runner de tests
pub struct TestRunner {
    client: kube::Client,
    namespace: String,
    config: TestConfig,
}

impl TestRunner {
    pub fn new(client: kube::Client, namespace: &str, config: TestConfig) -> Self {
        Self {
            client,
            namespace: namespace.to_string(),
            config,
        }
    }

    /// Exécute tous les tests d'une release
    pub async fn run(&self, hooks: Vec<Hook>) -> Result<TestSuiteResult> {
        let start = std::time::Instant::now();

        // Filtrer les hooks de type test
        let test_hooks: Vec<_> = hooks
            .into_iter()
            .filter(|h| h.phases.contains(&HookPhase::Test))
            .filter(|h| {
                self.config.filter.as_ref()
                    .map(|f| h.name.contains(f))
                    .unwrap_or(true)
            })
            .collect();

        if test_hooks.is_empty() {
            return Ok(TestSuiteResult {
                passed: 0,
                failed: 0,
                skipped: 0,
                results: vec![],
                total_duration: start.elapsed(),
            });
        }

        // Exécuter les tests
        let results = if self.config.parallel {
            self.run_parallel(test_hooks).await?
        } else {
            self.run_sequential(test_hooks).await?
        };

        // Compiler les résultats
        let passed = results.iter().filter(|r| r.passed).count();
        let failed = results.iter().filter(|r| !r.passed).count();

        Ok(TestSuiteResult {
            passed,
            failed,
            skipped: 0,
            results,
            total_duration: start.elapsed(),
        })
    }

    async fn run_parallel(&self, hooks: Vec<Hook>) -> Result<Vec<TestResult>> {
        stream::iter(hooks)
            .map(|hook| self.run_single_test(hook))
            .buffer_unordered(self.config.parallelism)
            .collect::<Vec<_>>()
            .await
            .into_iter()
            .collect()
    }

    async fn run_sequential(&self, hooks: Vec<Hook>) -> Result<Vec<TestResult>> {
        let mut results = Vec::with_capacity(hooks.len());
        for hook in hooks {
            results.push(self.run_single_test(hook).await?);
        }
        Ok(results)
    }

    async fn run_single_test(&self, hook: Hook) -> Result<TestResult> {
        let start = std::time::Instant::now();
        let test_name = hook.name.clone();

        // Appliquer le pod de test
        let executor = HookExecutor::new(self.client.clone());

        let result = timeout(
            self.config.timeout,
            executor.execute_hook(&hook, &self.namespace)
        ).await;

        match result {
            Ok(Ok(_)) => {
                // Test réussi, récupérer les logs
                let logs = self.get_pod_logs(&test_name).await.unwrap_or_default();

                if self.config.cleanup {
                    let _ = self.cleanup_test_pod(&test_name).await;
                }

                Ok(TestResult {
                    name: test_name,
                    passed: true,
                    duration: start.elapsed(),
                    logs,
                    error: None,
                })
            }
            Ok(Err(e)) => {
                let logs = self.get_pod_logs(&test_name).await.unwrap_or_default();

                if self.config.cleanup {
                    let _ = self.cleanup_test_pod(&test_name).await;
                }

                Ok(TestResult {
                    name: test_name,
                    passed: false,
                    duration: start.elapsed(),
                    logs,
                    error: Some(e.to_string()),
                })
            }
            Err(_) => {
                // Timeout
                if self.config.cleanup {
                    let _ = self.cleanup_test_pod(&test_name).await;
                }

                Ok(TestResult {
                    name: test_name,
                    passed: false,
                    duration: start.elapsed(),
                    logs: String::new(),
                    error: Some(format!("timeout after {:?}", self.config.timeout)),
                })
            }
        }
    }

    async fn get_pod_logs(&self, pod_name: &str) -> Result<String> {
        let pods: Api<Pod> = Api::namespaced(self.client.clone(), &self.namespace);

        let logs = pods.logs(pod_name, &LogParams::default()).await
            .map_err(|e| KubeError::KubeApi(e.to_string()))?;

        Ok(logs)
    }

    async fn cleanup_test_pod(&self, pod_name: &str) -> Result<()> {
        let pods: Api<Pod> = Api::namespaced(self.client.clone(), &self.namespace);

        pods.delete(pod_name, &Default::default()).await
            .map_err(|e| KubeError::KubeApi(e.to_string()))?;

        Ok(())
    }
}
```

#### Commande CLI `sherpack-cli/src/commands/test.rs`

```rust
use clap::Parser;
use sherpack_kube::test::{TestRunner, TestConfig, TestSuiteResult};

#[derive(Parser)]
pub struct TestArgs {
    /// Release name
    release: String,

    /// Kubernetes namespace
    #[arg(short, long, default_value = "default")]
    namespace: String,

    /// Test timeout
    #[arg(long, default_value = "5m")]
    timeout: humantime::Duration,

    /// Run tests sequentially
    #[arg(long)]
    sequential: bool,

    /// Filter tests by name
    #[arg(long)]
    filter: Option<String>,

    /// Output format (text, json)
    #[arg(long, default_value = "text")]
    output: OutputFormat,

    /// Keep test pods after execution
    #[arg(long)]
    no_cleanup: bool,
}

pub async fn execute(args: TestArgs) -> Result<()> {
    let client = kube::Client::try_default().await?;

    // Récupérer la release
    let storage = SecretsDriver::new(client.clone());
    let release = storage.get_latest(&args.namespace, &args.release).await?;

    let config = TestConfig::new()
        .with_timeout(*args.timeout)
        .with_parallel(!args.sequential)
        .with_filter(args.filter);

    let runner = TestRunner::new(client, &args.namespace, config);
    let result = runner.run(release.hooks).await?;

    // Afficher les résultats
    match args.output {
        OutputFormat::Text => print_text_results(&result),
        OutputFormat::Json => print_json_results(&result)?,
    }

    std::process::exit(result.exit_code());
}

fn print_text_results(result: &TestSuiteResult) {
    println!();
    println!("{}", "TEST RESULTS".bold());
    println!("{}", "=".repeat(60));

    for test in &result.results {
        let status = if test.passed {
            "PASS".green()
        } else {
            "FAIL".red()
        };

        println!(
            "{} {} ({:.2}s)",
            status,
            test.name,
            test.duration.as_secs_f64()
        );

        if !test.passed {
            if let Some(ref error) = test.error {
                println!("  Error: {}", error.red());
            }
            if !test.logs.is_empty() {
                println!("  Logs:");
                for line in test.logs.lines().take(20) {
                    println!("    {}", line);
                }
            }
        }
    }

    println!();
    println!(
        "Passed: {} | Failed: {} | Duration: {:.2}s",
        result.passed.to_string().green(),
        result.failed.to_string().red(),
        result.total_duration.as_secs_f64()
    );
}
```

---

## Feature 5: --atomic Amélioré

### Objectif
- Capturer les logs/events AVANT le rollback
- Timeout séparé pour le rollback
- Meilleure visibilité sur les erreurs

### Implémentation

#### Extension de `UpgradeOptions` dans `sherpack-kube/src/actions.rs`

```rust
#[derive(Debug, Clone)]
pub struct AtomicOptions {
    /// Activer le mode atomic (rollback on failure)
    pub enabled: bool,

    /// Timeout pour l'upgrade
    pub upgrade_timeout: Duration,

    /// Timeout séparé pour le rollback
    pub rollback_timeout: Duration,

    /// Capturer les erreurs avant rollback
    pub capture_errors: bool,

    /// Nombre de lignes de logs à capturer
    pub error_log_lines: usize,
}

impl Default for AtomicOptions {
    fn default() -> Self {
        Self {
            enabled: false,
            upgrade_timeout: Duration::from_secs(300),
            rollback_timeout: Duration::from_secs(300),
            capture_errors: true,
            error_log_lines: 50,
        }
    }
}

impl UpgradeOptions {
    pub fn with_atomic(mut self, options: AtomicOptions) -> Self {
        self.atomic = Some(options);
        self
    }
}
```

#### Capture des erreurs dans `sherpack-kube/src/client.rs`

```rust
/// Informations capturées avant rollback
#[derive(Debug, Clone)]
pub struct PreRollbackDiagnostics {
    /// Pods en échec avec leurs logs
    pub failed_pods: Vec<PodDiagnostic>,

    /// Events Kubernetes pertinents
    pub events: Vec<KubeEvent>,

    /// Temps écoulé avant l'échec
    pub elapsed: Duration,

    /// Raison de l'échec
    pub failure_reason: String,
}

#[derive(Debug, Clone)]
pub struct PodDiagnostic {
    pub name: String,
    pub namespace: String,
    pub status: String,
    pub logs: String,
    pub events: Vec<String>,
}

impl<S: StorageDriver> KubeClient<S> {
    async fn capture_diagnostics(
        &self,
        namespace: &str,
        release_name: &str,
        log_lines: usize,
    ) -> PreRollbackDiagnostics {
        let mut diagnostics = PreRollbackDiagnostics {
            failed_pods: vec![],
            events: vec![],
            elapsed: Duration::ZERO,
            failure_reason: String::new(),
        };

        // Récupérer les pods de la release
        let pods: Api<Pod> = Api::namespaced(self.client.clone(), namespace);
        let selector = format!("app.kubernetes.io/instance={}", release_name);

        if let Ok(pod_list) = pods.list(&ListParams::default().labels(&selector)).await {
            for pod in pod_list {
                let pod_name = pod.metadata.name.unwrap_or_default();
                let status = pod.status
                    .map(|s| s.phase.unwrap_or_default())
                    .unwrap_or_default();

                // Vérifier si le pod est en échec
                if status != "Running" && status != "Succeeded" {
                    // Récupérer les logs
                    let logs = pods.logs(&pod_name, &LogParams {
                        tail_lines: Some(log_lines as i64),
                        ..Default::default()
                    }).await.unwrap_or_default();

                    // Récupérer les events du pod
                    let events = self.get_pod_events(namespace, &pod_name).await
                        .unwrap_or_default();

                    diagnostics.failed_pods.push(PodDiagnostic {
                        name: pod_name,
                        namespace: namespace.to_string(),
                        status,
                        logs,
                        events,
                    });
                }
            }
        }

        // Récupérer les events de namespace
        diagnostics.events = self.get_namespace_events(namespace, release_name).await
            .unwrap_or_default();

        diagnostics
    }

    async fn upgrade_with_atomic(
        &self,
        pack: &LoadedPack,
        context: &TemplateContext,
        options: &UpgradeOptions,
        atomic: &AtomicOptions,
    ) -> Result<UpgradeResult> {
        let start = std::time::Instant::now();

        // Tenter l'upgrade avec timeout
        let upgrade_result = timeout(
            atomic.upgrade_timeout,
            self.upgrade_internal(pack, context, options)
        ).await;

        match upgrade_result {
            Ok(Ok(result)) => Ok(result),
            Ok(Err(e)) | Err(_) => {
                // Échec - capturer les diagnostics AVANT le rollback
                let diagnostics = if atomic.capture_errors {
                    Some(self.capture_diagnostics(
                        &options.namespace,
                        &options.name,
                        atomic.error_log_lines,
                    ).await)
                } else {
                    None
                };

                // Effectuer le rollback avec son propre timeout
                let rollback_result = timeout(
                    atomic.rollback_timeout,
                    self.rollback(&options.namespace, &options.name, None)
                ).await;

                // Construire l'erreur enrichie
                Err(KubeError::AtomicUpgradeFailed {
                    original_error: Box::new(e),
                    diagnostics,
                    rollback_success: rollback_result.is_ok(),
                    elapsed: start.elapsed(),
                })
            }
        }
    }
}
```

#### Affichage CLI enrichi

```rust
fn display_atomic_failure(error: &KubeError) {
    if let KubeError::AtomicUpgradeFailed { diagnostics, rollback_success, .. } = error {
        eprintln!();
        eprintln!("{}", "UPGRADE FAILED - ATOMIC ROLLBACK TRIGGERED".bold().red());
        eprintln!();

        if let Some(diag) = diagnostics {
            // Afficher les pods en échec
            if !diag.failed_pods.is_empty() {
                eprintln!("{}", "Failed Pods:".bold().yellow());
                for pod in &diag.failed_pods {
                    eprintln!("  {} ({})", pod.name.bold(), pod.status.red());

                    if !pod.events.is_empty() {
                        eprintln!("    Events:");
                        for event in pod.events.iter().take(5) {
                            eprintln!("      - {}", event);
                        }
                    }

                    if !pod.logs.is_empty() {
                        eprintln!("    Logs (last {} lines):", pod.logs.lines().count());
                        for line in pod.logs.lines().take(10) {
                            eprintln!("      {}", line);
                        }
                    }
                }
            }
        }

        let status = if *rollback_success {
            "Rollback successful - release restored to previous version".green()
        } else {
            "Rollback FAILED - manual intervention required".red().bold()
        };
        eprintln!();
        eprintln!("{}", status);
    }
}
```

---

## Feature 6: CRDs Directory

### Objectif
Support du répertoire `crds/` avec:
- Installation avant les templates
- Pas de templating par défaut (sécurité)
- Protection contre la suppression

### Architecture

```
sherpack-core/
└── src/
    └── pack.rs       # Ajouter CRD loading

sherpack-kube/
└── src/
    └── crds.rs       # NOUVEAU: CRD installer
```

### Implémentation

#### Extension de `LoadedPack` dans `sherpack-core/src/pack.rs`

```rust
/// Configuration CRD dans Pack.yaml
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CrdConfig {
    /// Installer les CRDs (default: true)
    #[serde(default = "default_true")]
    pub install: bool,

    /// Mettre à jour les CRDs lors d'un upgrade (default: false, dangereux!)
    #[serde(default)]
    pub upgrade: bool,

    /// Conserver les CRDs lors de la désinstallation (default: true)
    #[serde(default = "default_true")]
    pub keep_on_uninstall: bool,

    /// Attendre que les CRDs soient disponibles (default: true)
    #[serde(default = "default_true")]
    pub wait_for_ready: bool,

    /// Activer le templating pour les CRDs (default: false, non recommandé)
    #[serde(default)]
    pub templating: bool,
}

impl Default for CrdConfig {
    fn default() -> Self {
        Self {
            install: true,
            upgrade: false,
            keep_on_uninstall: true,
            wait_for_ready: true,
            templating: false,
        }
    }
}

impl LoadedPack {
    /// Retourne les fichiers CRD si le répertoire crds/ existe
    pub fn crd_files(&self) -> Result<Vec<PathBuf>> {
        let crds_dir = self.root.join("crds");

        if !crds_dir.exists() {
            return Ok(vec![]);
        }

        let mut files = Vec::new();
        for entry in walkdir::WalkDir::new(&crds_dir)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
        {
            let path = entry.path();
            if let Some(ext) = path.extension() {
                if matches!(ext.to_string_lossy().as_ref(), "yaml" | "yml" | "json") {
                    files.push(path.to_path_buf());
                }
            }
        }

        files.sort(); // Déterminisme
        Ok(files)
    }

    /// Charge les CRDs (avec ou sans templating selon config)
    pub fn load_crds(&self, context: Option<&TemplateContext>) -> Result<Vec<String>> {
        let config = self.pack.crds.clone().unwrap_or_default();
        let files = self.crd_files()?;

        let mut crds = Vec::with_capacity(files.len());

        for file in files {
            let content = std::fs::read_to_string(&file)?;

            let processed = if config.templating {
                // Templating activé (rare, non recommandé)
                if let Some(ctx) = context {
                    let engine = sherpack_engine::Engine::strict();
                    engine.render_string(&content, ctx, &file.display().to_string())?
                } else {
                    content
                }
            } else {
                // Pas de templating (par défaut, recommandé)
                content
            };

            crds.push(processed);
        }

        Ok(crds)
    }
}
```

#### `sherpack-kube/src/crds.rs`

```rust
//! CRD installation and management

use k8s_openapi::apiextensions_apiserver::pkg::apis::apiextensions::v1::CustomResourceDefinition;
use kube::{Api, api::PostParams};
use std::time::Duration;

use crate::error::{KubeError, Result};

/// Installateur de CRDs
pub struct CrdInstaller {
    client: kube::Client,
}

impl CrdInstaller {
    pub fn new(client: kube::Client) -> Self {
        Self { client }
    }

    /// Installe les CRDs et attend qu'ils soient disponibles
    pub async fn install(
        &self,
        crd_manifests: &[String],
        wait: bool,
        timeout: Duration,
    ) -> Result<Vec<CrdInstallResult>> {
        let crds_api: Api<CustomResourceDefinition> = Api::all(self.client.clone());
        let mut results = Vec::with_capacity(crd_manifests.len());

        for manifest in crd_manifests {
            // Parser le CRD
            let crd: CustomResourceDefinition = serde_yaml::from_str(manifest)
                .map_err(|e| KubeError::InvalidManifest {
                    message: format!("invalid CRD manifest: {}", e),
                })?;

            let crd_name = crd.metadata.name.clone().unwrap_or_default();

            // Vérifier si le CRD existe déjà
            let existing = crds_api.get_opt(&crd_name).await
                .map_err(|e| KubeError::KubeApi(e.to_string()))?;

            let result = if existing.is_some() {
                // CRD existe - ne pas mettre à jour (sécurité)
                CrdInstallResult {
                    name: crd_name,
                    action: CrdAction::AlreadyExists,
                    ready: true,
                }
            } else {
                // Créer le CRD
                crds_api.create(&PostParams::default(), &crd).await
                    .map_err(|e| KubeError::KubeApi(e.to_string()))?;

                CrdInstallResult {
                    name: crd_name.clone(),
                    action: CrdAction::Created,
                    ready: false,
                }
            };

            results.push(result);
        }

        // Attendre que les CRDs soient prêts
        if wait {
            self.wait_for_ready(&results, timeout).await?;
        }

        Ok(results)
    }

    /// Attend que les CRDs soient établis
    async fn wait_for_ready(
        &self,
        results: &[CrdInstallResult],
        timeout: Duration,
    ) -> Result<()> {
        let crds_api: Api<CustomResourceDefinition> = Api::all(self.client.clone());
        let start = std::time::Instant::now();

        for result in results.iter().filter(|r| r.action == CrdAction::Created) {
            loop {
                if start.elapsed() > timeout {
                    return Err(KubeError::Timeout {
                        operation: format!("waiting for CRD '{}' to be ready", result.name),
                        duration: timeout,
                    });
                }

                if let Ok(crd) = crds_api.get(&result.name).await {
                    if is_crd_established(&crd) {
                        break;
                    }
                }

                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        }

        Ok(())
    }

    /// Vérifie si des CRDs peuvent être supprimés en toute sécurité
    pub async fn can_safely_delete(&self, crd_names: &[String]) -> Result<Vec<CrdDeletionCheck>> {
        let mut checks = Vec::with_capacity(crd_names.len());

        for name in crd_names {
            // Vérifier s'il existe des instances du CRD
            let has_instances = self.count_crd_instances(name).await? > 0;

            checks.push(CrdDeletionCheck {
                name: name.clone(),
                safe_to_delete: !has_instances,
                instance_count: if has_instances {
                    Some(self.count_crd_instances(name).await?)
                } else {
                    None
                },
            });
        }

        Ok(checks)
    }

    async fn count_crd_instances(&self, crd_name: &str) -> Result<usize> {
        // Utiliser l'API discovery pour trouver le GVK
        // puis lister les instances
        // Simplifié ici - implémentation complète nécessaire
        Ok(0)
    }
}

/// Résultat de l'installation d'un CRD
#[derive(Debug, Clone)]
pub struct CrdInstallResult {
    pub name: String,
    pub action: CrdAction,
    pub ready: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum CrdAction {
    Created,
    AlreadyExists,
    Updated,  // Seulement si upgrade: true dans config
}

#[derive(Debug, Clone)]
pub struct CrdDeletionCheck {
    pub name: String,
    pub safe_to_delete: bool,
    pub instance_count: Option<usize>,
}

/// Vérifie si un CRD est établi (prêt à l'emploi)
fn is_crd_established(crd: &CustomResourceDefinition) -> bool {
    crd.status
        .as_ref()
        .and_then(|s| s.conditions.as_ref())
        .map(|conditions| {
            conditions.iter().any(|c| {
                c.type_ == "Established" && c.status == "True"
            })
        })
        .unwrap_or(false)
}
```

#### Intégration dans `KubeClient::install`

```rust
impl<S: StorageDriver> KubeClient<S> {
    pub async fn install(
        &self,
        pack: &LoadedPack,
        context: &TemplateContext,
        options: &InstallOptions,
    ) -> Result<InstallResult> {
        let crd_config = pack.pack.crds.clone().unwrap_or_default();

        // 1. Installer les CRDs d'abord (si présents et activés)
        if crd_config.install {
            let crd_manifests = pack.load_crds(Some(context))?;

            if !crd_manifests.is_empty() {
                let crd_installer = CrdInstaller::new(self.client.clone());

                let crd_results = crd_installer.install(
                    &crd_manifests,
                    crd_config.wait_for_ready,
                    options.timeout,
                ).await?;

                // Reporter les résultats CRD
                for result in &crd_results {
                    match result.action {
                        CrdAction::Created => {
                            tracing::info!("Created CRD: {}", result.name);
                        }
                        CrdAction::AlreadyExists => {
                            tracing::debug!("CRD already exists: {}", result.name);
                        }
                        _ => {}
                    }
                }
            }
        }

        // 2. Continuer avec l'installation normale...
        self.install_internal(pack, context, options).await
    }
}
```

---

## Résumé des Changements

### Nouveaux Fichiers

| Crate | Fichier | Description |
|-------|---------|-------------|
| sherpack-core | `src/files.rs` | Files API avec sandbox |
| sherpack-kube | `src/test.rs` | Test runner |
| sherpack-kube | `src/crds.rs` | CRD installer |
| sherpack-cli | `src/commands/test.rs` | Commande test |

### Fichiers Modifiés

| Crate | Fichier | Modifications |
|-------|---------|---------------|
| sherpack-core | `src/values.rs` | `scope_for_subchart()`, `is_subchart_enabled()` |
| sherpack-core | `src/context.rs` | `SubchartContext` |
| sherpack-core | `src/pack.rs` | `CrdConfig`, `crd_files()`, `load_crds()` |
| sherpack-core | `src/error.rs` | `FileAccess` variant |
| sherpack-engine | `src/engine.rs` | `render_pack_with_subcharts()` |
| sherpack-engine | `src/functions.rs` | `FilesObject` |
| sherpack-kube | `src/client.rs` | Notes post-install, atomic amélioré |
| sherpack-kube | `src/actions.rs` | `AtomicOptions` |
| sherpack-kube | `src/error.rs` | `AtomicUpgradeFailed` variant |

### Dépendances à Ajouter

```toml
# sherpack-core/Cargo.toml
[dependencies]
glob = "0.3"
parking_lot = "0.12"

# sherpack-kube/Cargo.toml
[dependencies]
futures = "0.3"  # Déjà présent probablement
```

### Ordre d'Implémentation Recommandé

1. **Files API** (1-2 jours)
   - Implémentation simple, testable en isolation
   - Bloquant pour migration

2. **Subchart Value Scoping** (2-3 jours)
   - Dépend de la structure existante des Values
   - Bloquant pour migration

3. **NOTES.txt** (0.5 jour)
   - Extension mineure du code existant
   - Quick win pour l'UX

4. **sherpack test** (1-2 jours)
   - Hooks test déjà supportés
   - Bon ROI pour CI/CD

5. **--atomic amélioré** (1-2 jours)
   - Important pour production
   - Peut être fait en parallèle

6. **CRDs directory** (1-2 jours)
   - Nécessaire uniquement pour operators
   - Peut être différé

### Tests à Écrire

```rust
// Chaque feature doit avoir:
// 1. Unit tests pour la logique pure
// 2. Integration tests avec fixtures
// 3. Tests de régression pour les cas edge

#[cfg(test)]
mod tests {
    // Files API
    #[test] fn test_sandbox_escape_prevention() { ... }
    #[test] fn test_glob_pattern_matching() { ... }
    #[test] fn test_file_caching() { ... }

    // Subchart scoping
    #[test] fn test_value_inheritance() { ... }
    #[test] fn test_global_propagation() { ... }
    #[test] fn test_subchart_disabled() { ... }

    // etc.
}
```
