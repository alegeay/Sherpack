# Plan de Correction du Convertisseur Helm → Sherpack

## Vue d'ensemble

Trois bugs ont été identifiés lors de la conversion du chart `ingress-nginx` :

1. **Itération sur dictionnaires** - Génère `for v in dict` au lieu de `for k, v in dict`
2. **Scope des variables dans les macros** - Variables non préfixées par `values.`
3. **Filtre `default`** - Sémantique différente entre Helm/Sprig et Jinja2

## Architecture proposée

```
sherpack-convert/
├── src/
│   ├── type_inference.rs   # NOUVEAU: Inférence de types depuis values.yaml
│   ├── transformer.rs      # Modifié: Utilise TypeContext
│   ├── converter.rs        # Modifié: Construit TypeContext
│   └── ...
```

---

## Bug 1: Itération sur Dictionnaires

### Problème

```go
// Helm (Go template)
{{- range $key, $value := .Values.controller.containerPort }}
```

Génère actuellement :
```jinja
{%- for value in values.controller.containerPort %}{#- key = loop.index0 #}
```

Devrait générer :
```jinja
{%- for key, value in values.controller.containerPort | dictsort %}
```

### Solution : Inférence de Types

Créer un module `type_inference.rs` qui analyse `values.yaml` pour déterminer les types :

```rust
// crates/sherpack-convert/src/type_inference.rs

use serde_yaml::Value;
use std::collections::HashMap;

/// Types inférés depuis values.yaml
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InferredType {
    /// Scalaire (string, int, bool, null)
    Scalar,
    /// Liste/Array
    List,
    /// Dictionnaire/Map
    Dict,
    /// Type inconnu (chemin non trouvé)
    Unknown,
}

/// Contexte de types pour le transformer
#[derive(Debug, Default)]
pub struct TypeContext {
    /// Map de chemins vers leurs types inférés
    /// Ex: "controller.containerPort" -> Dict
    types: HashMap<String, InferredType>,
}

impl TypeContext {
    /// Construit le contexte depuis values.yaml
    pub fn from_values(values: &Value) -> Self {
        let mut ctx = Self::default();
        ctx.collect_types("", values);
        ctx
    }

    fn collect_types(&mut self, prefix: &str, value: &Value) {
        match value {
            Value::Mapping(map) => {
                // Ce niveau est un dictionnaire
                if !prefix.is_empty() {
                    self.types.insert(prefix.to_string(), InferredType::Dict);
                }
                // Récursion sur les enfants
                for (k, v) in map {
                    if let Value::String(key) = k {
                        let path = if prefix.is_empty() {
                            key.clone()
                        } else {
                            format!("{}.{}", prefix, key)
                        };
                        self.collect_types(&path, v);
                    }
                }
            }
            Value::Sequence(_) => {
                if !prefix.is_empty() {
                    self.types.insert(prefix.to_string(), InferredType::List);
                }
            }
            _ => {
                if !prefix.is_empty() {
                    self.types.insert(prefix.to_string(), InferredType::Scalar);
                }
            }
        }
    }

    /// Récupère le type d'un chemin de valeur
    /// Ex: "values.controller.containerPort" -> Some(Dict)
    pub fn get_type(&self, path: &str) -> InferredType {
        // Normalise le chemin (retire "values." si présent)
        let normalized = path
            .strip_prefix("values.")
            .or_else(|| path.strip_prefix(".Values."))
            .unwrap_or(path);

        self.types
            .get(normalized)
            .cloned()
            .unwrap_or(InferredType::Unknown)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dict_detection() {
        let yaml = r#"
controller:
  containerPort:
    http: 80
    https: 443
  replicas: 1
  labels:
    - app
    - version
"#;
        let values: Value = serde_yaml::from_str(yaml).unwrap();
        let ctx = TypeContext::from_values(&values);

        assert_eq!(ctx.get_type("controller.containerPort"), InferredType::Dict);
        assert_eq!(ctx.get_type("controller.replicas"), InferredType::Scalar);
        assert_eq!(ctx.get_type("controller.labels"), InferredType::List);
    }
}
```

### Modification du Transformer

```rust
// Dans transformer.rs

pub struct Transformer {
    block_stack: Vec<BlockType>,
    warnings: Vec<TransformWarning>,
    chart_prefix: Option<String>,
    context_var: Option<String>,
    type_context: Option<TypeContext>,  // NOUVEAU
}

impl Transformer {
    /// Ajoute le contexte de types pour une meilleure conversion
    pub fn with_type_context(mut self, ctx: TypeContext) -> Self {
        self.type_context = Some(ctx);
        self
    }

    // Dans transform_action, cas Range:
    fn transform_range(&mut self, vars: &Option<RangeVars>, pipeline: &Pipeline, trim_left: &str) -> String {
        let collection = self.transform_pipeline(pipeline);

        // Détermine si c'est un dictionnaire
        let is_dict = self.type_context
            .as_ref()
            .map(|ctx| ctx.get_type(&collection) == InferredType::Dict)
            .unwrap_or(false);

        match vars {
            Some(RangeVars { index_var: Some(key), value_var }) if is_dict => {
                // Itération sur dictionnaire avec clé explicite
                format!(
                    "{{%{} for {}, {} in {} | dictsort %}}",
                    trim_left, key, value_var, collection
                )
            }
            Some(RangeVars { index_var: Some(idx), value_var }) => {
                // Itération sur liste avec index (comportement actuel)
                format!(
                    "{{%{} for {} in {} %}}{{#- {} = loop.index0 #}}",
                    trim_left, value_var, collection, idx
                )
            }
            Some(RangeVars { index_var: None, value_var }) => {
                format!("{{%{} for {} in {} %}}", trim_left, value_var, collection)
            }
            None => {
                // Génère un nom de variable basé sur le nom de la collection
                let var_name = self.infer_loop_var_name(&collection);
                format!("{{%{} for {} in {} %}}", trim_left, var_name, collection)
            }
        }
    }
}
```

---

## Bug 2: Scope des Variables dans les Macros

### Problème

```go
// Helm
{{- define "chart.image" -}}
{{- if .Values.controller.image.chroot -}}
{{- .Values.controller.image.image -}}-chroot
{{- end -}}
{{- end -}}
```

Génère actuellement :
```jinja
{%- macro image() %}
{%- if chroot %}
{{- image -}}-chroot  {# BUG: 'image' non défini #}
{%- endif %}
{%- endmacro %}
```

### Solution : Tracking du Contexte

Le problème vient du fait que dans un `define`, le contexte `.` est passé explicitement.
Nous devons tracker quand nous sommes dans un `define` et comment le contexte est utilisé.

```rust
// Dans transformer.rs

/// Contexte de transformation pour les blocs
#[derive(Debug, Clone)]
struct BlockContext {
    block_type: BlockType,
    /// Pour Define: le chemin du contexte passé (ex: "values.controller.image")
    context_path: Option<String>,
}

pub struct Transformer {
    block_stack: Vec<BlockContext>,  // Modifié: contient plus d'info
    // ...
    /// Pile de contextes pour résoudre les variables relatives
    context_stack: Vec<String>,
}

impl Transformer {
    fn transform_field(&mut self, field: &Field) -> String {
        let path = self.transform_field_path(&field.path);

        // Si nous sommes dans un define et que le chemin commence par ".",
        // c'est relatif au contexte passé
        if self.in_define_block() && path.starts_with('.') {
            if let Some(ctx_path) = self.current_context_path() {
                // ".image" dans le contexte "values.controller.image"
                // devient "values.controller.image.image"
                return format!("{}{}", ctx_path, path);
            }
        }

        path
    }

    /// Détecte le contexte passé à un template/include
    fn detect_context_for_template(&self, args: &[Argument]) -> Option<String> {
        // {{ template "name" .Values.controller.image }}
        // Le dernier argument est souvent le contexte
        args.last().and_then(|arg| {
            if let Argument::Field(field) = arg {
                Some(self.transform_field_path(&field.path))
            } else {
                None
            }
        })
    }
}
```

### Alternative Plus Simple : Post-Processing

Une approche plus pragmatique est de détecter les variables non définies après conversion
et d'ajouter automatiquement le préfixe `values.` :

```rust
// Dans converter.rs

fn post_process_macro(content: &str, values: &Value) -> String {
    // Regex pour trouver les variables Jinja2 non préfixées
    let var_pattern = Regex::new(r"\{\{-?\s*([a-zA-Z_][a-zA-Z0-9_]*)\s*").unwrap();

    let mut result = content.to_string();

    for cap in var_pattern.captures_iter(content) {
        let var_name = &cap[1];

        // Skip les mots-clés Jinja2
        if ["if", "else", "endif", "for", "endfor", "set", "macro", "endmacro"]
            .contains(&var_name)
        {
            continue;
        }

        // Cherche dans values.yaml si cette variable existe quelque part
        if let Some(full_path) = find_value_path(values, var_name) {
            result = result.replace(
                &format!("{{{{ {} }}}}", var_name),
                &format!("{{{{ {} }}}}", full_path),
            );
        }
    }

    result
}
```

---

## Bug 3: Filtre `default` - Sémantique Différente

### Problème

```go
// Helm - default gère: nil, "", undefined
{{ .Values.namespaceOverride | default .Release.Namespace }}
```

```jinja
{# Jinja2 - default ne gère que: undefined #}
{{ values.namespaceOverride | default(release.namespace) }}
{# Si namespaceOverride = "", le résultat est "" et non release.namespace #}
```

### Solution 1 : Transformation Intelligente

Détecter le pattern `| default(x)` et le transformer en `or x` :

```rust
// Dans transformer.rs, lors du traitement des filtres

fn transform_filter(&mut self, filter: &str, args: &[Argument]) -> String {
    match filter {
        "default" => {
            // Helm's default = Jinja2's `or`
            if let Some(default_val) = args.first() {
                let val = self.transform_argument(default_val);
                // Utilise `or` au lieu de `| default()` pour matcher la sémantique Helm
                return format!("or {}", val);
            }
            "or none".to_string()
        }
        // ... autres filtres
        _ => self.transform_generic_filter(filter, args),
    }
}

// Dans le cas des pipelines:
fn transform_pipeline(&mut self, pipeline: &Pipeline) -> String {
    let mut result = String::new();

    for (i, cmd) in pipeline.commands.iter().enumerate() {
        let transformed = self.transform_command(cmd);

        if i == 0 {
            result = transformed;
        } else if transformed.starts_with("or ") {
            // Cas spécial: `or` devient un opérateur, pas un filtre
            result = format!("({} {})", result, transformed);
        } else {
            result = format!("{} | {}", result, transformed);
        }
    }

    result
}
```

### Solution 2 : Filtre Custom dans sherpack-engine

Ajouter un filtre `helm_default` dans le moteur qui émule le comportement Helm :

```rust
// Dans sherpack-engine/src/filters.rs

/// Filtre default compatible Helm (gère nil, "", undefined)
fn helm_default(value: Value, default: Value) -> Value {
    match &value {
        Value::Undefined => default,
        Value::None => default,
        Value::String(s) if s.is_empty() => default,
        _ => value,
    }
}

// Puis dans le transformer:
fn transform_filter(&mut self, filter: &str, args: &[Argument]) -> String {
    match filter {
        "default" => {
            // Utilise notre filtre helm_default pour la compatibilité
            let arg = args.first()
                .map(|a| self.transform_argument(a))
                .unwrap_or_else(|| "none".to_string());
            format!("helm_default({})", arg)
        }
        _ => // ...
    }
}
```

### Recommandation

**Solution 1** (transformation en `or`) est préférable car :
- Zero runtime overhead
- Code Jinja2 idiomatique
- Pas de dépendance sur des filtres custom

---

## Plan d'Implémentation

### Phase 1 : Infrastructure (PR #1)

1. Créer `type_inference.rs` avec `TypeContext`
2. Ajouter tests unitaires
3. Intégrer dans `Converter::convert()`

```rust
// converter.rs
pub fn convert(&self, chart_path: &Path, output_path: &Path) -> Result<ConversionResult> {
    // ...

    // Charger et analyser values.yaml
    let values_path = chart_path.join("values.yaml");
    let type_context = if values_path.exists() {
        let content = fs::read_to_string(&values_path)?;
        let values: serde_yaml::Value = serde_yaml::from_str(&content)?;
        Some(TypeContext::from_values(&values))
    } else {
        None
    };

    // Passer au transformer
    let mut transformer = Transformer::new()
        .with_chart_prefix(&chart_name);
    if let Some(ctx) = type_context {
        transformer = transformer.with_type_context(ctx);
    }

    // ...
}
```

### Phase 2 : Fix Itération Dictionnaires (PR #2)

1. Modifier `transform_action` pour le cas `Range`
2. Utiliser `TypeContext` pour détecter les dicts
3. Générer `for k, v in dict | dictsort`
4. Ajouter warning si type inconnu avec heuristique

### Phase 3 : Fix Filtre Default (PR #3)

1. Transformer `| default(x)` en `or x`
2. Gérer les cas complexes : `| default "" | default "fallback"`
3. Ajouter tests de régression

### Phase 4 : Fix Scope Macros (PR #4)

1. Option A : Tracking du contexte dans les defines
2. Option B : Post-processing avec détection de variables
3. Ajouter warning pour variables potentiellement mal scopées

---

## Tests de Validation

```rust
#[test]
fn test_dict_iteration() {
    let values = r#"
controller:
  containerPort:
    http: 80
    https: 443
"#;
    let ctx = TypeContext::from_values(&serde_yaml::from_str(values).unwrap());
    let mut t = Transformer::new().with_type_context(ctx);

    assert_eq!(
        t.transform("{{- range $key, $value := .Values.controller.containerPort }}"),
        "{%- for key, value in values.controller.containerPort | dictsort %}"
    );
}

#[test]
fn test_default_to_or() {
    assert_eq!(
        transform("{{ .Values.x | default .Values.y }}"),
        "{{ (values.x or values.y) }}"
    );
}

#[test]
fn test_default_chain() {
    assert_eq!(
        transform("{{ .Values.a | default .Values.b | default \"c\" }}"),
        "{{ (values.a or values.b or \"c\") }}"
    );
}
```

---

## Résumé des Changements

| Fichier | Modification |
|---------|--------------|
| `type_inference.rs` | **NOUVEAU** - Inférence de types depuis values.yaml |
| `transformer.rs` | Ajout `TypeContext`, fix range/default |
| `converter.rs` | Construit et passe `TypeContext` |
| `lib.rs` | Export du nouveau module |

**Estimation** : ~400 lignes de code, 3-4 PRs

**Compatibilité** : 100% rétrocompatible, améliorations silencieuses

---

## Statut d'Implémentation (v0.1.1)

### Terminé :

1. **`type_inference.rs`** - Module complet avec:
   - `TypeContext::from_yaml()` pour charger les types depuis values.yaml
   - `TypeHeuristics` pour deviner les types par convention de nommage
   - Support des chemins normalisés (`.Values.x`, `values.x`, `x`)
   - 12 tests unitaires

2. **Fix Itération Dictionnaires** - Génère maintenant:
   ```jinja
   {%- for key, value in values.controller.containerPort | dictsort %}
   ```
   Au lieu de:
   ```jinja
   {%- for value in values.controller.containerPort %}{#- key = loop.index0 #}
   ```

3. **Fix Filtre `default`** - Transforme en `or`:
   ```jinja
   {{ (values.x or "fallback") }}
   ```
   Au lieu de:
   ```jinja
   {{ values.x | default("fallback") }}
   ```

### Restant (v0.2.0) :

1. **Scope des Variables dans les Macros** - Les macros avec contexte implicite ne sont pas correctement converties:
   ```jinja
   {# Helm: {{ .image }} dans define avec contexte .Values.controller.image #}
   {# Actuel: {{ image }} - variable non définie #}
   {# Attendu: {{ values.controller.image.image }} #}
   ```

2. **Whitespace dans les Macros** - Indentation excessive dans le output des macros
