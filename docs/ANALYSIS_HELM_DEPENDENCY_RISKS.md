# Analyse des Risques: Helm Charts comme Dépendances Sherpack

## TL;DR

Il y a **8 problèmes majeurs** et **6 problèmes mineurs** à résoudre. Le plus critique est le **scoping des values pour les subcharts** qui nécessite une implémentation spécifique.

---

## Problèmes CRITIQUES (Bloquants)

### 1. Scoping des Values pour Subcharts ⚠️ CRITIQUE

**Comment Helm fonctionne:**
```yaml
# Parent values.yaml
postgresql:           # ← Préfixe = nom du subchart
  auth:
    username: myuser
  primary:
    persistence:
      size: 10Gi

global:               # ← Passé à TOUS les subcharts
  storageClass: fast
```

Le subchart `postgresql` reçoit:
```yaml
# Ce que voit le subchart
auth:                 # ← Préfixe STRIPPÉ automatiquement
  username: myuser
primary:
  persistence:
    size: 10Gi

global:               # ← Passé tel quel
  storageClass: fast
```

**Problème Sherpack:**
Actuellement, Sherpack ne fait PAS ce scoping. Si on convertit un chart Helm qui attend `{{ .Values.auth.username }}`, ça devient `{{ values.auth.username }}` mais les values passées sont `{ postgresql: { auth: { username: ... } } }`.

**Solution Requise:**
```rust
// Au moment du rendu, scoper les values pour chaque dépendance
fn scope_values_for_dependency(
    parent_values: &Value,
    dep_name: &str,
    dep_alias: Option<&str>,
) -> Value {
    let key = dep_alias.unwrap_or(dep_name);
    let mut scoped = parent_values.get(key).cloned().unwrap_or(Value::Object(Map::new()));

    // Merger les globals
    if let Some(global) = parent_values.get("global") {
        scoped["global"] = global.clone();
    }

    scoped
}
```

**Impact:** Sans cette implémentation, AUCUN chart Helm converti ne fonctionnera correctement comme dépendance.

---

### 2. Accès aux Macros Cross-Charts ⚠️ CRITIQUE

**Comment Helm fonctionne:**
```yaml
# Dans parent/templates/deployment.yaml
metadata:
  labels:
    {{- include "postgresql.labels" . | nindent 4 }}
```

Le parent peut appeler les templates définis dans ses subcharts.

**Comment Sherpack convertit:**
```jinja2
{# Conversion actuelle #}
metadata:
  labels:
    {{ labels() | nindent(4) }}
```

**Problème:**
- `labels()` n'est pas importé depuis le subchart
- Jinja2 requiert des imports explicites: `{% from "postgresql/_helpers.tpl" import labels %}`
- Le namespace n'est pas préservé (collision possible entre `myapp.labels` et `postgresql.labels`)

**Solution Requise:**
```jinja2
{# Génération d'un fichier _imports.tpl automatique #}
{% from "packs/postgresql/templates/_helpers.tpl" import labels as postgresql_labels %}
{% from "packs/redis/templates/_helpers.tpl" import labels as redis_labels %}

{# Utilisation #}
{{ postgresql_labels() }}
```

**Ou alternative - namespace automatique:**
```rust
// Lors de la conversion, préfixer tous les appels cross-chart
fn transform_include_cross_chart(&self, name: &str, source_chart: &str) -> String {
    if name.starts_with(&format!("{}.", source_chart)) {
        // Same chart - strip prefix
        let macro_name = name.replace(".", "_");
        format!("{}()", macro_name)
    } else {
        // Cross-chart - keep namespace
        let full_name = name.replace(".", "_");
        format!("{}()", full_name)
    }
}
```

---

### 3. Library Charts ⚠️ CRITIQUE

**Comment Helm fonctionne:**
```yaml
# Chart.yaml
type: library  # Pas de templates installables, juste des helpers
```

Beaucoup de charts Bitnami dépendent de `bitnami/common` (library chart).

```yaml
# bitnami/postgresql/Chart.yaml
dependencies:
  - name: common
    version: "2.x.x"
    repository: https://charts.bitnami.com
```

**Problème:**
- Les library charts ne s'installent pas (pas de manifests)
- Ils fournissent UNIQUEMENT des templates réutilisables
- La conversion doit traiter différemment `type: library` vs `type: application`

**Solution Requise:**
```rust
enum PackKind {
    Application,  // Génère des manifests
    Library,      // Seulement des macros/helpers
}

// Lors du build des dépendances
fn build_dependency(dep: &ResolvedDependency) -> Result<()> {
    match dep.pack_kind {
        PackKind::Library => {
            // Ne PAS inclure dans le rendu final
            // Juste rendre disponible pour les imports
            copy_to_libs_dir(dep)?;
        }
        PackKind::Application => {
            // Comportement normal
            copy_to_packs_dir(dep)?;
        }
    }
}
```

---

### 4. Dépendances Transitives Helm ⚠️ CRITIQUE

**Scénario:**
```
my-sherpack-app (Sherpack)
└── postgresql (Helm, converti)
    └── common (Helm library)
        └── (autres dépendances possibles)
```

**Problème:**
- Si on convertit `postgresql`, il a des dépendances Helm
- Ces dépendances ne sont pas dans `packs/` du chart converti
- La résolution doit être RÉCURSIVE

**Solution Requise:**
```rust
async fn resolve_helm_dependency(dep: &HelmDependency) -> Result<ConvertedPack> {
    // 1. Télécharger le chart Helm
    let chart = download_helm_chart(dep).await?;

    // 2. Parser Chart.yaml pour trouver les dépendances
    let helm_deps = parse_helm_dependencies(&chart)?;

    // 3. Résoudre récursivement
    let mut converted_deps = Vec::new();
    for helm_dep in helm_deps {
        let converted = resolve_helm_dependency(&helm_dep).await?;  // RÉCURSIF
        converted_deps.push(converted);
    }

    // 4. Convertir ce chart
    let converted = convert_chart(&chart, &converted_deps)?;

    Ok(converted)
}
```

---

## Problèmes MAJEURS (Importants)

### 5. Files API Non Supporté ⚠️ MAJEUR

**Ce que fait Helm:**
```yaml
data:
  config.json: |
    {{ .Files.Get "files/config.json" | indent 4 }}

  {{- range $path, $bytes := .Files.Glob "files/*.yaml" }}
  {{ $path }}: |
    {{ $bytes | indent 4 }}
  {{- end }}
```

**Problème:**
La conversion actuelle émet un warning mais le code ne fonctionne pas:
```jinja2
{# Conversion actuelle - CASSÉ #}
data:
  config.json: |
    {# UNSUPPORTED: .Files.Get - Embed file content in values.yaml #}
```

**Charts affectés:** ~30% des charts publics utilisent `.Files`

**Solutions possibles:**

A) **Extraction au moment de la conversion:**
```rust
fn convert_files_usage(chart_path: &Path, template: &str) -> String {
    // Parser les appels à .Files.Get
    // Extraire le contenu des fichiers référencés
    // Injecter directement dans le template converti
}
```

B) **Implémenter Files dans Sherpack:**
```rust
// Dans sherpack-engine/src/functions.rs
fn files_get(path: &str) -> Result<String> {
    // Lire depuis pack/files/
}
```

C) **Pré-processeur au build:**
```yaml
# Pack.yaml extension
files:
  inline: true  # Inline tous les fichiers au packaging
```

---

### 6. Lookup Function ⚠️ MAJEUR

**Ce que fait Helm:**
```yaml
{{- $secret := lookup "v1" "Secret" .Release.Namespace "my-secret" }}
{{- if $secret }}
password: {{ $secret.data.password }}
{{- else }}
password: {{ randAlphaNum 32 | b64enc }}
{{- end }}
```

**Comportement actuel:**
- Conversion: `lookup` → `{}` (dict vide)
- Fonctionne en mode template (comme `helm template`)
- MAIS le chart attend peut-être une vraie valeur

**Problème profond:**
- `lookup` interroge le cluster AU MOMENT DU RENDU
- C'est anti-GitOps par nature
- Mais beaucoup de charts l'utilisent pour des migrations/upgrades

**Solution:**
```yaml
# Ajouter un mode "cluster-aware" optionnel
engine:
  lookup: disabled  # (default) Retourne {}
  lookup: enabled   # Interroge le cluster (non-GitOps)
  lookup: cached    # Utilise un snapshot du cluster
```

---

### 7. tpl Function (Dynamic Templating) ⚠️ MAJEUR

**Ce que fait Helm:**
```yaml
# values.yaml
customAnnotations: |
  checksum/config: {{ include "mychart.configChecksum" . }}

# template
annotations:
  {{ tpl .Values.customAnnotations . | nindent 4 }}
```

`tpl` permet de templater une STRING qui contient du Go template.

**Conversion actuelle:**
```jinja2
annotations:
  {{ tpl(values.customAnnotations) | nindent(4) }}
```

**Problème:**
- Le contenu de `customAnnotations` est TOUJOURS du Go template
- Même après conversion, les values peuvent contenir `{{ .Values.x }}`
- Il faudrait convertir AUSSI les values, ou interpréter le Go template

**Solution:**
```rust
// Option A: Implémenter tpl avec un mini-parser Go template
fn tpl(template_str: &str, context: &Value) -> Result<String> {
    // Parser le Go template dans la string
    // Le convertir en Jinja2 à la volée
    // Le rendre
}

// Option B: Détecter et avertir
fn convert_tpl_usage(expr: &str) -> String {
    format!("{{# WARNING: tpl() may contain Go template syntax #}}{}", expr)
}
```

---

### 8. Hooks Annotation Translation ⚠️ MAJEUR

**Helm:**
```yaml
annotations:
  "helm.sh/hook": pre-install,pre-upgrade
  "helm.sh/hook-weight": "5"
  "helm.sh/hook-delete-policy": before-hook-creation
  "helm.sh/resource-policy": keep
```

**Sherpack:**
```yaml
annotations:
  "sherpack.io/hook": pre-install,pre-upgrade
  "sherpack.io/hook-weight": "5"
  "sherpack.io/hook-delete-policy": before-hook-creation
  "sherpack.io/resource-policy": keep
```

**Problème:**
- La conversion doit traduire les annotations
- Les behaviours doivent être IDENTIQUES
- `helm.sh/resource-policy: keep` est critique (ne pas supprimer au uninstall)

**Solution:**
```rust
fn translate_helm_annotations(template: &str) -> String {
    template
        .replace("helm.sh/hook", "sherpack.io/hook")
        .replace("helm.sh/hook-weight", "sherpack.io/hook-weight")
        .replace("helm.sh/hook-delete-policy", "sherpack.io/hook-delete-policy")
        .replace("helm.sh/resource-policy", "sherpack.io/resource-policy")
}
```

---

## Problèmes MINEURS (Gérables)

### 9. CRDs Directory

**Helm:** `crds/` directory avec traitement spécial (jamais upgradé)

**Solution:** Convertir `crds/` en templates normaux avec annotation `sherpack.io/crd-policy: create-only`

### 10. .helmignore

**Helm:** `.helmignore` contrôle le packaging

**Solution:** Respecter `.helmignore` lors de la conversion, créer équivalent `.sherpackignore`

### 11. Chart.yaml vs Pack.yaml

**Différences de schéma**

**Solution:** Mapper les champs lors de la conversion
```rust
fn convert_chart_yaml(chart: &HelmChart) -> Pack {
    Pack {
        api_version: "sherpack/v1".to_string(),
        metadata: PackMetadata {
            name: chart.name.clone(),
            version: chart.version.clone(),
            app_version: chart.app_version.clone(),
            // ... autres champs
        },
        // ...
    }
}
```

### 12. values.schema.json → values.schema.yaml

**Solution:** Conversion automatique JSON Schema → YAML format

### 13. Notes.txt

**Solution:** Renommer et convertir comme un template normal

### 14. Test Hooks

**Solution:** Convertir `helm.sh/hook: test` → `sherpack.io/hook: test`

---

## Matrice de Compatibilité

| Feature | Status | Impact si non résolu | Priorité |
|---------|--------|---------------------|----------|
| Value scoping | ❌ Non implémenté | Charts subcharts cassés | P0 |
| Cross-chart includes | ❌ Non implémenté | Erreurs de rendu | P0 |
| Library charts | ❌ Non implémenté | Bitnami charts cassés | P0 |
| Transitive deps | ⚠️ Partiel | Dépendances manquantes | P0 |
| Files API | ❌ Non implémenté | ~30% charts cassés | P1 |
| Lookup function | ✅ Workaround ({}) | Comportement différent | P2 |
| tpl function | ⚠️ Partiel | Dynamic values cassées | P1 |
| Hook annotations | ❌ Non traduit | Hooks non exécutés | P1 |
| CRDs | ⚠️ Partiel | Comportement différent | P2 |
| Schema conversion | ❌ Non implémenté | Pas de validation | P3 |

---

## Recommandation: Approche en 2 Phases

### Phase 1: Compatibilité de Base (Pré-requis)

Avant de permettre les dépendances Helm, implémenter:

1. **Value scoping** pour subcharts
2. **Cross-chart macro access** avec imports automatiques
3. **Library chart support**
4. **Recursive dependency resolution**
5. **Hook annotation translation**

**Effort estimé:** 3-5 jours de développement

### Phase 2: Compatibilité Avancée

Après stabilisation de Phase 1:

1. **Files API** (au moins `.Files.Get`)
2. **tpl function** améliorée
3. **Schema conversion**
4. **CRD handling**

**Effort estimé:** 2-3 jours de développement

---

## Alternative: Mode Hybride

Au lieu de convertir les charts Helm, les exécuter "nativement":

```yaml
dependencies:
  - name: postgresql
    version: "12.x.x"
    repository: https://charts.bitnami.com
    type: helm
    mode: native  # NE PAS convertir, utiliser helm template
```

**Avantages:**
- Compatibilité 100%
- Pas de bugs de conversion

**Inconvénients:**
- Nécessite `helm` installé
- Deux moteurs de template
- Complexité accrue

**Implémentation:**
```rust
fn render_dependency(dep: &Dependency, values: &Value) -> Result<Vec<Manifest>> {
    match dep.mode {
        DependencyMode::Convert => {
            // Utiliser sherpack-engine
            let pack = load_converted_pack(dep)?;
            engine.render_pack(&pack, values)
        }
        DependencyMode::Native => {
            // Appeler helm template
            let output = Command::new("helm")
                .args(["template", &dep.name, &dep.path])
                .args(["--values", "-"])
                .stdin(Stdio::piped())
                .output()?;
            parse_manifests(&output.stdout)
        }
    }
}
```

---

## Conclusion

**Peut-on avoir des dépendances Helm dans Sherpack ?**

**Oui, MAIS** il faut d'abord implémenter:

1. ✅ Scoping des values (CRITIQUE)
2. ✅ Accès aux macros cross-chart (CRITIQUE)
3. ✅ Support des library charts (CRITIQUE)
4. ✅ Résolution récursive (CRITIQUE)
5. ✅ Translation des annotations hooks (IMPORTANT)

Sans ces implémentations, les charts Helm convertis ne fonctionneront pas correctement comme dépendances.

Le mode hybride (native) est une alternative viable si la conversion s'avère trop complexe.
