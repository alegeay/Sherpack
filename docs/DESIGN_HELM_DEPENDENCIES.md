# Design: Helm Chart Dependencies & Static Enable/Disable

## Executive Summary

Cette proposition résout deux problèmes fondamentaux:

1. **Dépendances Helm natives** - Pouvoir dépendre directement de charts Helm sans conversion manuelle préalable
2. **Résolution conditionnelle** - Ne pas résoudre/télécharger les dépendances désactivées (crucial pour environnements air-gapped)

---

## Problème Actuel

### Helm : La Douleur

```yaml
# Helm Chart.yaml
dependencies:
  - name: postgresql
    version: "12.x.x"
    repository: https://charts.bitnami.com
    condition: postgresql.enabled  # Évaluée UNIQUEMENT au template time
```

**Comportement Helm:**
1. `helm dependency update` → Télécharge TOUTES les dépendances
2. Même si `postgresql.enabled=false`, la dépendance est téléchargée
3. En environnement air-gapped, on doit mirror TOUT le chart ecosystem

### Sherpack Actuel

```yaml
# Pack.yaml
dependencies:
  - name: redis
    version: "^7.0"
    repository: https://sherpack-repo.example.com  # DOIT être un repo Sherpack
    condition: redis.enabled
```

**Limitations:**
1. Ne supporte pas les repositories Helm natifs
2. `condition` n'est pas évaluée à la résolution (comme Helm)
3. Pas de moyen de désactiver statiquement une dépendance

---

## Design Proposé

### 1. Schéma Étendu des Dépendances

```yaml
# Pack.yaml
apiVersion: sherpack/v1
metadata:
  name: my-app
  version: 1.0.0

dependencies:
  # Dépendance Sherpack standard
  - name: my-lib
    version: "^1.0"
    repository: https://sherpack.example.com

  # Dépendance Helm avec conversion automatique
  - name: postgresql
    version: "12.x.x"
    repository: https://charts.bitnami.com
    type: helm                    # NOUVEAU: helm | sherpack | auto
    enabled: true                 # NOUVEAU: désactivation statique
    condition: postgresql.enabled # Runtime condition (existant)

  # Dépendance désactivée statiquement (jamais résolue)
  - name: redis
    version: "^7.0"
    repository: https://charts.bitnami.com
    type: helm
    enabled: false                # NE SERA JAMAIS téléchargée

  # Contrôle fin de résolution
  - name: monitoring
    version: "^2.0"
    repository: oci://registry.example.com/charts
    type: helm
    enabled: true
    condition: monitoring.enabled
    resolve: when-enabled         # NOUVEAU: always | when-enabled | never
```

### 2. Nouveaux Champs

| Champ | Type | Default | Description |
|-------|------|---------|-------------|
| `type` | `helm \| sherpack \| auto` | `auto` | Format de la dépendance |
| `enabled` | `bool` | `true` | Désactivation statique (lock-time) |
| `resolve` | `always \| when-enabled \| never` | `when-enabled` | Quand résoudre/télécharger |

### 3. Matrice de Comportement

```
┌──────────┬───────────────┬────────────────────────────────────────┐
│ enabled  │ resolve       │ Comportement                           │
├──────────┼───────────────┼────────────────────────────────────────┤
│ false    │ (ignoré)      │ Jamais résolu, jamais téléchargé       │
│ true     │ always        │ Toujours résolu (pour vendoring/cache) │
│ true     │ when-enabled  │ Résolu seulement si condition=true     │
│ true     │ never         │ Jamais résolu (doit être local)        │
└──────────┴───────────────┴────────────────────────────────────────┘
```

---

## Architecture d'Implémentation

### Phase 1: Pipeline de Résolution

```
Pack.yaml
    │
    ▼
┌─────────────────────────────────────────────────────────────┐
│  1. PARSE                                                    │
│     - Lire dependencies[]                                    │
│     - Valider schéma                                         │
└─────────────────────────────────────────────────────────────┘
    │
    ▼
┌─────────────────────────────────────────────────────────────┐
│  2. FILTER (enabled=false)                                   │
│     - Exclure dépendances avec enabled: false                │
│     - Log: "Skipping disabled dependency: redis"             │
└─────────────────────────────────────────────────────────────┘
    │
    ▼
┌─────────────────────────────────────────────────────────────┐
│  3. EVALUATE CONDITIONS (resolve=when-enabled)               │
│     - Charger values.yaml                                    │
│     - Évaluer conditions simples (pas de templating)         │
│     - Exclure si condition=false ET resolve=when-enabled     │
└─────────────────────────────────────────────────────────────┘
    │
    ▼
┌─────────────────────────────────────────────────────────────┐
│  4. DETECT TYPE (type=auto)                                  │
│     - Fetch repo index                                       │
│     - Si index.yaml contient apiVersion: v1 → Helm           │
│     - Si index.yaml contient apiVersion: sherpack/v1 → Pack  │
└─────────────────────────────────────────────────────────────┘
    │
    ▼
┌─────────────────────────────────────────────────────────────┐
│  5. RESOLVE                                                  │
│     - Semver matching                                        │
│     - Diamond conflict detection                             │
│     - Transitive dependencies                                │
└─────────────────────────────────────────────────────────────┘
    │
    ▼
┌─────────────────────────────────────────────────────────────┐
│  6. DOWNLOAD                                                 │
│     - Télécharger archives                                   │
│     - Vérifier SHA256                                        │
│     - Stocker dans cache: ~/.sherpack/cache/                 │
└─────────────────────────────────────────────────────────────┘
    │
    ▼
┌─────────────────────────────────────────────────────────────┐
│  7. CONVERT (si type=helm)                                   │
│     - Utiliser sherpack-convert                              │
│     - Go templates → Jinja2                                  │
│     - Chart.yaml → Pack.yaml                                 │
└─────────────────────────────────────────────────────────────┘
    │
    ▼
┌─────────────────────────────────────────────────────────────┐
│  8. INSTALL                                                  │
│     - Copier dans packs/<name>/                              │
│     - Générer Pack.lock.yaml                                 │
└─────────────────────────────────────────────────────────────┘
```

### Phase 2: Structure du Cache

```
~/.sherpack/
├── cache/
│   ├── helm/                           # Charts Helm originaux
│   │   └── bitnami/
│   │       └── postgresql-12.1.5.tgz
│   │
│   └── converted/                      # Packs convertis
│       └── bitnami/
│           └── postgresql-12.1.5/
│               ├── Pack.yaml
│               ├── values.yaml
│               └── templates/
│
└── repos/                              # Index des repos
    └── bitnami.yaml
```

### Phase 3: Lock File Étendu

```yaml
# Pack.lock.yaml
version: 2                              # Nouvelle version de lock
generated: 2025-01-15T10:30:00Z
packYamlDigest: sha256:abc123...
valuesDigest: sha256:def456...          # NOUVEAU: pour condition evaluation

dependencies:
  - name: postgresql
    version: 12.1.5
    repository: https://charts.bitnami.com
    digest: sha256:original-helm-chart...

    # NOUVEAU: Métadonnées de conversion
    source:
      type: helm
      originalVersion: 12.1.5
      chartDigest: sha256:original...
      convertedDigest: sha256:converted...
      converterVersion: 0.1.0

    # NOUVEAU: État de résolution
    resolution:
      enabled: true
      conditionEvaluated: true          # La condition était-elle vraie?
      conditionExpression: "postgresql.enabled"

  - name: redis
    version: 7.2.4
    repository: https://charts.bitnami.com

    source:
      type: helm
      originalVersion: 7.2.4

    resolution:
      enabled: false                    # Désactivé statiquement
      skipped: true                     # Non téléchargé
      reason: "Disabled in Pack.yaml"
```

---

## Évaluation des Conditions

### Conditions Simples (Recommandé)

```yaml
# Évaluation statique possible
condition: postgresql.enabled           # → values.postgresql.enabled
condition: features.monitoring          # → values.features.monitoring
condition: global.database.external     # → values.global.database.external
```

**Algorithme:**
```rust
fn evaluate_condition(condition: &str, values: &Value) -> bool {
    // Parse: "foo.bar.baz" → ["foo", "bar", "baz"]
    let path: Vec<&str> = condition.split('.').collect();

    // Navigate values
    let mut current = values;
    for part in path {
        current = current.get(part)?;
    }

    // Coerce to bool
    match current {
        Value::Bool(b) => *b,
        Value::Null => false,
        Value::String(s) => !s.is_empty() && s != "false",
        Value::Number(n) => n.as_f64().unwrap_or(0.0) != 0.0,
        _ => true,  // Objects/Arrays are truthy
    }
}
```

### Conditions Complexes (Non Supportées à la Résolution)

```yaml
# Ces conditions NE PEUVENT PAS être évaluées à la résolution
condition: "{{ .Values.db.type == 'postgresql' }}"  # Template expression
condition: "and .Values.a .Values.b"                 # Go template logic
```

**Comportement:** Si la condition contient `{{` ou des fonctions Go template, elle est considérée comme `true` à la résolution et évaluée uniquement au template time.

---

## API Rust

### Nouveaux Types

```rust
// crates/sherpack-core/src/pack.rs

/// Dependency type
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum DependencyType {
    #[default]
    Auto,
    Helm,
    Sherpack,
}

/// When to resolve a dependency
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum ResolvePolicy {
    /// Always resolve (for vendoring)
    Always,
    /// Only resolve if condition evaluates to true
    #[default]
    WhenEnabled,
    /// Never resolve (must be local)
    Never,
}

/// Pack dependency (extended)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Dependency {
    pub name: String,
    pub version: String,
    pub repository: String,

    // Type detection
    #[serde(default)]
    pub r#type: DependencyType,

    // Static enable/disable
    #[serde(default = "default_true")]
    pub enabled: bool,

    // Runtime condition
    #[serde(default)]
    pub condition: Option<String>,

    // Resolution policy
    #[serde(default)]
    pub resolve: ResolvePolicy,

    // Existing fields
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub alias: Option<String>,
}
```

### Resolver Modifié

```rust
// crates/sherpack-repo/src/dependency.rs

pub struct ResolutionContext {
    /// Values for condition evaluation
    pub values: serde_json::Value,
    /// Whether to skip disabled dependencies
    pub skip_disabled: bool,
    /// Whether to evaluate conditions
    pub evaluate_conditions: bool,
}

impl DependencyResolver {
    /// Filter dependencies based on enabled flag and conditions
    pub fn filter_resolvable(
        &self,
        deps: &[Dependency],
        ctx: &ResolutionContext,
    ) -> Vec<FilteredDependency> {
        deps.iter()
            .map(|dep| {
                // Check static enabled flag
                if !dep.enabled {
                    return FilteredDependency {
                        dep: dep.clone(),
                        should_resolve: false,
                        reason: SkipReason::StaticDisabled,
                    };
                }

                // Check resolve policy
                match dep.resolve {
                    ResolvePolicy::Never => FilteredDependency {
                        dep: dep.clone(),
                        should_resolve: false,
                        reason: SkipReason::PolicyNever,
                    },
                    ResolvePolicy::Always => FilteredDependency {
                        dep: dep.clone(),
                        should_resolve: true,
                        reason: SkipReason::None,
                    },
                    ResolvePolicy::WhenEnabled => {
                        // Evaluate condition
                        let enabled = dep.condition
                            .as_ref()
                            .map(|c| evaluate_simple_condition(c, &ctx.values))
                            .unwrap_or(true);

                        FilteredDependency {
                            dep: dep.clone(),
                            should_resolve: enabled,
                            reason: if enabled {
                                SkipReason::None
                            } else {
                                SkipReason::ConditionFalse
                            },
                        }
                    }
                }
            })
            .collect()
    }
}
```

### Helm Converter Integration

```rust
// crates/sherpack-repo/src/helm.rs (nouveau fichier)

use sherpack_convert::{Converter, ConvertOptions};

/// Download and convert a Helm chart
pub async fn fetch_helm_chart(
    repo_url: &str,
    name: &str,
    version: &str,
    cache_dir: &Path,
) -> Result<PathBuf> {
    // 1. Check if already converted in cache
    let converted_path = cache_dir
        .join("converted")
        .join(repo_name(repo_url))
        .join(format!("{}-{}", name, version));

    if converted_path.exists() {
        return Ok(converted_path);
    }

    // 2. Download original Helm chart
    let helm_cache = cache_dir.join("helm").join(repo_name(repo_url));
    let chart_archive = download_helm_chart(repo_url, name, version, &helm_cache).await?;

    // 3. Extract
    let extract_dir = tempdir()?;
    extract_tgz(&chart_archive, extract_dir.path())?;

    // 4. Convert to Sherpack
    let converter = Converter::new(ConvertOptions::default());
    let chart_dir = find_chart_dir(extract_dir.path())?;
    converter.convert(&chart_dir, &converted_path)?;

    // 5. Add conversion metadata
    add_conversion_metadata(&converted_path, name, version, repo_url)?;

    Ok(converted_path)
}
```

---

## CLI Commands

### Commandes Modifiées

```bash
# Résoudre seulement les dépendances activées (défaut)
sherpack dependency update ./mypack

# Forcer la résolution de TOUTES les dépendances (pour vendoring)
sherpack dependency update ./mypack --all

# Voir ce qui serait résolu
sherpack dependency update ./mypack --dry-run

# Spécifier les values pour l'évaluation des conditions
sherpack dependency update ./mypack --values prod-values.yaml

# Construire les dépendances (télécharger dans packs/)
sherpack dependency build ./mypack

# Vérifier la cohérence du lock file
sherpack dependency verify ./mypack
```

### Nouvelles Commandes

```bash
# Vendoriser pour air-gap (tout inclure)
sherpack dependency vendor ./mypack --output ./vendored/
# Résultat:
# ./vendored/
# ├── Pack.yaml
# ├── values.yaml
# ├── templates/
# └── packs/
#     ├── postgresql/
#     └── redis/

# Afficher l'état des dépendances
sherpack dependency status ./mypack
# Output:
# ┌────────────┬─────────┬──────────┬───────────────────┐
# │ Name       │ Version │ Status   │ Reason            │
# ├────────────┼─────────┼──────────┼───────────────────┤
# │ postgresql │ 12.1.5  │ resolved │                   │
# │ redis      │ -       │ skipped  │ enabled: false    │
# │ monitoring │ -       │ skipped  │ condition: false  │
# └────────────┴─────────┴──────────┴───────────────────┘
```

---

## Cas d'Usage: Environnement Air-Gapped

### Workflow de Préparation

```bash
# Sur machine connectée:

# 1. Créer un pack avec dépendances Helm
cat > myapp/Pack.yaml << 'EOF'
apiVersion: sherpack/v1
metadata:
  name: myapp
  version: 1.0.0
dependencies:
  - name: postgresql
    version: "12.x.x"
    repository: https://charts.bitnami.com
    type: helm
    condition: database.type == "postgresql"
  - name: mysql
    version: "9.x.x"
    repository: https://charts.bitnami.com
    type: helm
    condition: database.type == "mysql"
EOF

# 2. Résoudre avec values de production (postgresql activé)
sherpack dependency update myapp/ --values prod-values.yaml
# → postgresql résolu, mysql ignoré

# 3. Vendoriser (tout inclure pour avoir le choix)
sherpack dependency vendor myapp/ --all --output ./airgap-bundle/
```

### Workflow en Air-Gap

```bash
# Sur machine isolée:

# 1. Copier le bundle
scp -r airgap-bundle/ isolated-server:/apps/myapp/

# 2. Déployer (les dépendances sont déjà locales)
sherpack install myapp ./airgap-bundle/ \
  --values prod-values.yaml \
  --set database.type=postgresql

# → Utilise packs/postgresql/ local
# → Ignore mysql car condition=false
```

---

## Backward Compatibility

### Migration Automatique

Les `Pack.yaml` existants restent valides:

```yaml
# Ancien format (toujours supporté)
dependencies:
  - name: redis
    version: "^7.0"
    repository: https://repo.example.com
    condition: redis.enabled
```

Est équivalent à:

```yaml
# Nouveau format (explicite)
dependencies:
  - name: redis
    version: "^7.0"
    repository: https://repo.example.com
    type: auto
    enabled: true
    condition: redis.enabled
    resolve: when-enabled
```

### Lock File Upgrade

```yaml
# Version 1 (ancien)
version: 1
dependencies:
  - name: redis
    version: 7.2.4
    ...

# Version 2 (nouveau) - généré automatiquement lors du update
version: 2
dependencies:
  - name: redis
    version: 7.2.4
    source:
      type: sherpack
    resolution:
      enabled: true
    ...
```

---

## Phases d'Implémentation

### Phase 1: Foundation (1-2 jours)
- [ ] Étendre `Dependency` struct avec `type`, `enabled`, `resolve`
- [ ] Ajouter `DependencyType` et `ResolvePolicy` enums
- [ ] Tests unitaires pour parsing

### Phase 2: Filtering (1 jour)
- [ ] Implémenter `filter_resolvable()` dans resolver
- [ ] Évaluation de conditions simples
- [ ] Tests d'intégration

### Phase 3: Helm Support (2-3 jours)
- [ ] Auto-détection du type (Helm vs Sherpack)
- [ ] Intégration avec sherpack-convert
- [ ] Cache de conversion
- [ ] Tests avec vrais charts Helm

### Phase 4: Lock File v2 (1 jour)
- [ ] Nouveau format avec métadonnées source/resolution
- [ ] Migration automatique v1 → v2
- [ ] Commande `dependency verify`

### Phase 5: CLI & Polish (1 jour)
- [ ] Nouvelles options CLI (`--all`, `--dry-run`, `--values`)
- [ ] Commande `dependency vendor`
- [ ] Commande `dependency status`
- [ ] Documentation

---

## Risques et Mitigations

| Risque | Impact | Mitigation |
|--------|--------|------------|
| Conversion Helm imparfaite | Moyen | Mode `--keep-helm` pour garder l'original |
| Conditions complexes non évaluables | Faible | Fallback sur `true`, warning affiché |
| Lock file incompatible | Faible | Migration automatique, version explicite |
| Performance (conversion) | Faible | Cache agressif, conversion une seule fois |

---

## Conclusion

Cette implémentation apporte:

1. **Flexibilité** - Support natif de Helm + Sherpack
2. **Efficacité** - Ne télécharge que ce qui est nécessaire
3. **Air-gap friendly** - Vendoring complet avec contrôle fin
4. **Backward compatible** - Migration transparente

L'architecture proposée est modulaire et permet une implémentation incrémentale sans breaking changes.
