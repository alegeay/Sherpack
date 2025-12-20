# Consolidation Plan

Plan pour stabiliser et nettoyer le codebase avant d'ajouter de nouvelles features.

## Problèmes Identifiés

### 1. Warnings Clippy (50 total)

| Catégorie | Count | Priorité |
|-----------|-------|----------|
| Large Err variants | 24 | HIGH |
| Dead code (sherpack-convert) | 5 | MEDIUM |
| Too many function arguments | 5 | MEDIUM |
| Complex types | 2 | LOW |
| Large enum variants | 2 | MEDIUM |
| Misc (PathBuf, identical blocks) | 3 | LOW |

### 2. Code Mort
- `needs_chunking` function unused
- `colors_enabled` field unused
- `Transformer` fields unused (var_name, index_var, context_var, name)
- `add_warning` method unused

### 3. TODOs dans le Code
- `client.rs`: Use timeout to configure health checker
- `client.rs`: Print detailed diff

### 4. Panics dans le Code de Production
- `schema.rs`: 2 panics (Expected SherpSchema/JsonSchema)
- `credentials.rs`: 2 panics (Expected Basic credentials)

---

## Plan de Consolidation

### Phase 1: Fix Warnings Critiques (1-2h)

#### 1.1 Box Large Error Variants
Les erreurs volumineuses ralentissent le code (allocation sur stack).

```rust
// Avant
enum Error {
    Kube(kube::Error),  // Large
    Io(std::io::Error),
}

// Après
enum Error {
    Kube(Box<kube::Error>),
    Io(std::io::Error),
}
```

#### 1.2 Nettoyer Dead Code (sherpack-convert)
```rust
// Supprimer ou utiliser les champs:
// - BlockType::Range { var_name, index_var }
// - BlockType::With { context_var }
// - BlockType::Define { name }
// - Transformer::seen_unsupported
// - Transformer::root_context_saved
// - Transformer::add_warning()
```

### Phase 2: Refactoring Fonctions (2-3h)

#### 2.1 Builder Pattern pour CLI
Fonctions avec trop d'arguments → Structs/Builders

```rust
// Avant (17 arguments!)
fn do_install(name, namespace, pack, values, set, wait, timeout, ...) {}

// Après
struct InstallConfig {
    name: String,
    namespace: String,
    pack: PathBuf,
    values: Vec<PathBuf>,
    // ...
}

fn do_install(config: InstallConfig) {}
```

#### 2.2 Type Aliases pour Types Complexes
```rust
// Avant
HashMap<String, HashMap<String, Vec<(String, Option<String>)>>>

// Après
type DependencyMap = HashMap<String, HashMap<String, Vec<Dependency>>>;
```

### Phase 3: Éliminer les Panics (1h)

```rust
// Avant
_ => panic!("Expected SherpSchema")

// Après
_ => return Err(SchemaError::InvalidType {
    expected: "SherpSchema",
    got: self.type_name()
})
```

### Phase 4: Améliorer la Robustesse (2-3h)

#### 4.1 Meilleure Gestion d'Erreurs
- Messages d'erreur contextuels
- Suggestions de résolution
- Codes d'erreur pour CI/CD

#### 4.2 Tests Manquants
Vérifier couverture des edge cases:
- Pack vide
- Values invalides
- Templates avec erreurs de syntaxe
- Connexion K8s échouée

#### 4.3 Validation d'Input
- Noms de release (regex)
- Chemins de fichiers
- Valeurs YAML

### Phase 5: Documentation (1-2h)

#### 5.1 Doc Comments Manquants
Ajouter `///` pour toutes les fonctions publiques.

#### 5.2 Examples dans la Doc
```rust
/// Creates a new pack from a directory.
///
/// # Example
/// ```
/// let pack = LoadedPack::load("./mypack")?;
/// ```
pub fn load(path: impl AsRef<Path>) -> Result<Self> {
```

---

## Ordre d'Exécution

1. **Box Error Variants** - Impact immédiat sur performance
2. **Clean Dead Code** - Réduit la confusion
3. **Remove Panics** - Évite crashes en prod
4. **Refactor Arguments** - Meilleure maintenabilité
5. **Add Type Aliases** - Meilleure lisibilité
6. **Add Tests** - Confiance pour futures modifications
7. **Documentation** - Facilite contributions

---

## Métriques de Succès

| Métrique | Avant | Après | Objectif |
|----------|-------|-------|----------|
| Clippy warnings | 50 | 5 | 0 |
| Panics in prod code | 0* | 0 | 0 |
| TODOs | 2 | 2 | 0 |
| Test coverage | 334 tests | 334 tests | 80%+ |

*Note: Les 4 panics identifiés étaient dans du code de test, ce qui est acceptable.

### Warnings restants (5)
Les warnings restants sont cosmétiques et de basse priorité:
- 1x doc list item overindented
- 2x very complex type (type aliases recommandés)
- 1x parameter only used in recursion
- 1x identical if blocks

---

## Commandes Utiles

```bash
# Vérifier les warnings
cargo clippy --workspace 2>&1 | grep "warning:" | wc -l

# Trouver les panics
grep -r "panic!\|unwrap()" crates/ --include="*.rs" | grep -v test

# Couverture de tests
cargo tarpaulin --workspace --out Html

# Doc coverage
cargo doc --workspace --no-deps 2>&1 | grep "warning"
```
