# Comparaison Helm vs Sherpack

Ce document analyse les fonctionnalités de Helm et identifie ce qui manque dans Sherpack.

## Légende

| Symbole | Signification |
|---------|---------------|
| ✅ | Implémenté dans Sherpack |
| ⚠️ | Partiellement implémenté |
| ❌ | Non implémenté |
| 🚫 | Intentionnellement non supporté |

---

## 1. Commandes CLI

### Commandes de Release

| Commande Helm | Sherpack | Status | Notes |
|---------------|----------|--------|-------|
| `helm install` | `sherpack install` | ✅ | Complet avec --wait, --atomic, --dry-run |
| `helm upgrade` | `sherpack upgrade` | ✅ | Avec --install, --reuse-values, --reset-values |
| `helm uninstall` | `sherpack uninstall` | ✅ | Avec --keep-history |
| `helm rollback` | `sherpack rollback` | ✅ | Complet |
| `helm list` | `sherpack list` | ✅ | Avec --all-namespaces |
| `helm status` | `sherpack status` | ✅ | Avec --manifest, --show-values |
| `helm history` | `sherpack history` | ✅ | Complet |
| `helm get manifest` | `sherpack status --manifest` | ✅ | Via flag |
| `helm get values` | `sherpack status --show-values` | ✅ | Via flag |
| `helm get notes` | - | ❌ | **MANQUANT** |
| `helm get hooks` | - | ❌ | **MANQUANT** |
| `helm get metadata` | - | ❌ | **MANQUANT** |
| `helm get all` | - | ❌ | **MANQUANT** |

### Commandes de Chart/Pack

| Commande Helm | Sherpack | Status | Notes |
|---------------|----------|--------|-------|
| `helm create` | `sherpack create` | ✅ | Basique |
| `helm lint` | `sherpack lint` | ✅ | Avec validation schema |
| `helm template` | `sherpack template` | ✅ | Complet |
| `helm package` | `sherpack package` | ✅ | Avec manifest SHA256 |
| `helm show chart` | `sherpack show` | ✅ | |
| `helm show values` | `sherpack show` | ✅ | Via --all |
| `helm show readme` | - | ❌ | **MANQUANT** |
| `helm show crds` | - | ❌ | **MANQUANT** |
| `helm show all` | `sherpack show --all` | ✅ | |
| `helm verify` | `sherpack verify` | ✅ | Minisign au lieu de PGP |
| `helm test` | - | ❌ | **MANQUANT** |

### Commandes de Repository

| Commande Helm | Sherpack | Status | Notes |
|---------------|----------|--------|-------|
| `helm repo add` | `sherpack repo add` | ✅ | HTTP + OCI |
| `helm repo list` | `sherpack repo list` | ✅ | |
| `helm repo update` | `sherpack repo update` | ✅ | |
| `helm repo remove` | `sherpack repo remove` | ✅ | |
| `helm repo index` | - | ❌ | **MANQUANT** (génération d'index.yaml) |
| `helm search repo` | `sherpack search` | ✅ | Avec cache SQLite FTS5 |
| `helm search hub` | - | ❌ | **MANQUANT** (Artifact Hub) |
| `helm pull` | `sherpack pull` | ✅ | |
| `helm push` | `sherpack push` | ✅ | OCI uniquement |

### Commandes de Dépendances

| Commande Helm | Sherpack | Status | Notes |
|---------------|----------|--------|-------|
| `helm dependency list` | `sherpack dependency list` | ✅ | Avec filtrage condition |
| `helm dependency update` | `sherpack dependency update` | ✅ | Avec lock file |
| `helm dependency build` | `sherpack dependency build` | ✅ | Avec vérification intégrité |

### Commandes Utilitaires

| Commande Helm | Sherpack | Status | Notes |
|---------------|----------|--------|-------|
| `helm env` | - | ❌ | **MANQUANT** |
| `helm version` | `sherpack --version` | ✅ | Via Clap |
| `helm completion` | - | ❌ | **MANQUANT** (bash/zsh/fish) |
| `helm plugin` | - | ❌ | **MANQUANT** (système de plugins) |
| `helm registry login` | - | ❌ | **MANQUANT** (auth interactive OCI) |
| `helm registry logout` | - | ❌ | **MANQUANT** |

---

## 2. Objets de Template

### Objets Built-in

| Helm | Sherpack | Status | Notes |
|------|----------|--------|-------|
| `.Values` | `values` | ✅ | Identique |
| `.Release.Name` | `release.name` | ✅ | |
| `.Release.Namespace` | `release.namespace` | ✅ | |
| `.Release.Revision` | `release.revision` | ✅ | |
| `.Release.IsUpgrade` | `release.isUpgrade` | ⚠️ | À vérifier |
| `.Release.IsInstall` | `release.isInstall` | ⚠️ | À vérifier |
| `.Release.Service` | - | ❌ | Toujours "Sherpack" |
| `.Chart.Name` | `pack.name` | ✅ | Renommé |
| `.Chart.Version` | `pack.version` | ✅ | |
| `.Chart.AppVersion` | `pack.appVersion` | ✅ | |
| `.Chart.*` (autres) | `pack.*` | ⚠️ | Partiel |
| `.Capabilities.KubeVersion` | `capabilities.kubeVersion` | ✅ | |
| `.Capabilities.APIVersions` | `capabilities.apiVersions` | ⚠️ | À vérifier |
| `.Capabilities.HelmVersion` | - | 🚫 | N/A |
| `.Template.Name` | - | ❌ | **MANQUANT** |
| `.Template.BasePath` | - | ❌ | **MANQUANT** |
| `.Files` | - | ❌ | **MANQUANT** (critique) |

### Objet `.Files` (MANQUANT)

Helm permet d'accéder aux fichiers du chart :

```go
{{ .Files.Get "config.json" }}
{{ .Files.GetBytes "binary.dat" }}
{{ .Files.Glob "files/*.yaml" }}
{{ .Files.Lines "file.txt" }}
{{ .Files.AsConfig }}
{{ .Files.AsSecrets }}
```

**Impact :** ~30% des charts publics utilisent `.Files`. Sans cette fonctionnalité, ces charts ne peuvent pas être convertis.

**Solution proposée :**
```rust
// Dans sherpack-engine/src/functions.rs
fn files_get(path: &str) -> Result<String>
fn files_glob(pattern: &str) -> Result<Vec<String>>
fn files_as_config() -> Result<Value>
fn files_as_secrets() -> Result<Value>
```

---

## 3. Fonctions de Template

### Fonctions Logiques

| Helm | Sherpack | Status |
|------|----------|--------|
| `and` | `and` | ✅ (natif Jinja2) |
| `or` | `or` | ✅ (natif Jinja2) |
| `not` | `not` | ✅ (natif Jinja2) |
| `eq` | `==` | ✅ (natif Jinja2) |
| `ne` | `!=` | ✅ |
| `lt`, `le`, `gt`, `ge` | `<`, `<=`, `>`, `>=` | ✅ |
| `default` | `default()` | ✅ (filtre) |
| `required` | `required()` | ✅ |
| `empty` | `not x` | ✅ |
| `fail` | `fail()` | ✅ |
| `coalesce` | `x or y or z` | ✅ (natif) |
| `ternary` | `x if cond else y` | ✅ (natif) |

### Fonctions de Chaînes

| Helm | Sherpack | Status |
|------|----------|--------|
| `trim` | `trim` | ✅ |
| `trimPrefix` | `trimPrefix()` | ✅ |
| `trimSuffix` | `trimSuffix()` | ✅ |
| `lower` | `lower` | ✅ |
| `upper` | `upper` | ✅ |
| `title` | `title` | ✅ |
| `camelcase` | `camelcase` | ✅ |
| `snakecase` | `snakecase` | ✅ |
| `kebabcase` | `kebabcase` | ✅ |
| `quote` | `quote` | ✅ |
| `squote` | `squote` | ✅ |
| `indent` | `indent()` | ✅ |
| `nindent` | `nindent()` | ✅ |
| `replace` | `replace()` | ✅ |
| `substr` | `[start:end]` | ✅ (natif) |
| `trunc` | `[:n]` | ✅ (natif) |
| `printf` | `~` ou format | ✅ |
| `wrap` | - | ❌ |
| `wrapWith` | - | ❌ |
| `contains` | `in` | ✅ (natif) |
| `hasPrefix` | `startswith()` | ✅ |
| `hasSuffix` | `endswith()` | ✅ |
| `repeat` | `* n` | ✅ |
| `nospace` | `replace(" ", "")` | ✅ |
| `initials` | - | ❌ |
| `randAlphaNum` | - | 🚫 Non-déterministe |
| `randAlpha` | - | 🚫 |
| `randNumeric` | - | 🚫 |
| `randAscii` | - | 🚫 |
| `plural` | - | ❌ |
| `abbrev` | - | ❌ |
| `abbrevboth` | - | ❌ |

### Fonctions de Conversion de Types

| Helm | Sherpack | Status |
|------|----------|--------|
| `toJson` | `tojson` | ✅ |
| `fromJson` | - | ❌ **MANQUANT** |
| `toYaml` | `toyaml` | ✅ |
| `fromYaml` | - | ❌ **MANQUANT** |
| `toToml` | - | ❌ |
| `fromToml` | - | ❌ |
| `toPrettyJson` | `tojson_pretty` | ✅ |
| `toString` | `tostring` | ✅ |
| `toStrings` | - | ❌ |
| `toDecimal` | - | ❌ |
| `atoi` | `int` | ✅ |
| `int` | `int` | ✅ |
| `int64` | `int` | ✅ |
| `float64` | `float` | ✅ |

### Fonctions de Listes

| Helm | Sherpack | Status |
|------|----------|--------|
| `list` | `list()` ou `[...]` | ✅ |
| `first` | `first` | ✅ |
| `last` | `last` | ✅ |
| `rest` | `[1:]` | ✅ |
| `initial` | `[:-1]` | ✅ |
| `append` | - | ❌ |
| `prepend` | - | ❌ |
| `concat` | `+` | ✅ |
| `reverse` | `reverse` | ✅ |
| `uniq` | `uniq` | ✅ |
| `without` | - | ❌ |
| `has` | `has()` | ✅ |
| `compact` | `compact` | ✅ |
| `index` | `[n]` | ✅ (natif) |
| `slice` | `[start:end]` | ✅ |
| `chunk` | - | ❌ |
| `until` | `range()` | ✅ |
| `untilStep` | `range(start, end, step)` | ✅ |
| `seq` | `range()` | ✅ |
| `sortAlpha` | `sortAlpha` | ✅ |
| `mustAppend` | - | ❌ |
| `mustPrepend` | - | ❌ |

### Fonctions de Dictionnaires

| Helm | Sherpack | Status |
|------|----------|--------|
| `dict` | `dict()` ou `{...}` | ✅ |
| `get` | `get()` | ✅ |
| `set` | - | ❌ (immutable en Jinja2) |
| `unset` | - | ❌ |
| `hasKey` | `has()` | ✅ |
| `pluck` | - | ❌ |
| `dig` | - | ❌ **MANQUANT** |
| `merge` | - | ❌ |
| `mergeOverwrite` | - | ❌ |
| `keys` | `keys` | ✅ |
| `values` | `values` | ✅ |
| `pick` | - | ❌ |
| `omit` | - | ❌ |
| `deepCopy` | - | ❌ |

### Fonctions Mathématiques

| Helm | Sherpack | Status |
|------|----------|--------|
| `add` | `+` | ✅ |
| `sub` | `-` | ✅ |
| `mul` | `*` | ✅ |
| `div` | `/` | ✅ |
| `mod` | `%` | ✅ |
| `max` | `max()` | ✅ |
| `min` | `min()` | ✅ |
| `floor` | `floor` | ⚠️ |
| `ceil` | `ceil` | ⚠️ |
| `round` | `round` | ⚠️ |
| `add1` | `+ 1` | ✅ |
| `len` | `length` | ✅ |

### Fonctions de Date

| Helm | Sherpack | Status |
|------|----------|--------|
| `now` | `now()` | ✅ |
| `date` | `now("%Y-%m-%d")` | ✅ |
| `dateModify` | - | ❌ |
| `dateInZone` | - | ❌ |
| `duration` | - | ❌ |
| `durationRound` | - | ❌ |
| `unixEpoch` | - | ❌ |
| `ago` | - | ❌ |
| `toDate` | - | ❌ |
| `mustToDate` | - | ❌ |

### Fonctions Cryptographiques

| Helm | Sherpack | Status |
|------|----------|--------|
| `sha1sum` | - | ❌ |
| `sha256sum` | `sha256` | ✅ |
| `b64enc` | `b64encode` | ✅ |
| `b64dec` | `b64decode` | ✅ |
| `genCA` | - | 🚫 Non-déterministe |
| `genPrivateKey` | - | 🚫 |
| `genSelfSignedCert` | - | 🚫 |
| `genSignedCert` | - | 🚫 |
| `derivePassword` | - | 🚫 |
| `encryptAES` | - | ❌ |
| `decryptAES` | - | ❌ |
| `htpasswd` | - | ❌ |
| `bcrypt` | - | ❌ |

### Fonctions Kubernetes

| Helm | Sherpack | Status |
|------|----------|--------|
| `lookup` | `{}` (empty dict) | ⚠️ Workaround |
| `.Capabilities.APIVersions.Has` | - | ❌ **MANQUANT** |

### Autres Fonctions

| Helm | Sherpack | Status |
|------|----------|--------|
| `include` | `{% include %}` + macros | ✅ |
| `tpl` | `tpl()` | ⚠️ Partiel |
| `uuidv4` | `uuidv4()` | ✅ |
| `regexMatch` | - | ❌ |
| `regexFind` | - | ❌ |
| `regexFindAll` | - | ❌ |
| `regexReplace` | - | ❌ |
| `regexSplit` | - | ❌ |
| `urlParse` | - | ❌ |
| `urlJoin` | - | ❌ |
| `urlquery` | - | ❌ |
| `osBase` | - | ❌ |
| `osDir` | - | ❌ |
| `osExt` | - | ❌ |
| `osClean` | - | ❌ |
| `osIsAbs` | - | ❌ |
| `semver` | `semver` | ✅ |
| `semverCompare` | `semverCompare()` | ✅ |

---

## 4. Hooks

| Helm | Sherpack | Status |
|------|----------|--------|
| `pre-install` | `pre-install` | ✅ |
| `post-install` | `post-install` | ✅ |
| `pre-upgrade` | `pre-upgrade` | ✅ |
| `post-upgrade` | `post-upgrade` | ✅ |
| `pre-delete` | `pre-delete` | ✅ |
| `post-delete` | `post-delete` | ✅ |
| `pre-rollback` | `pre-rollback` | ✅ |
| `post-rollback` | `post-rollback` | ✅ |
| `test` | `test` | ✅ (défini, mais pas de commande) |
| `helm.sh/hook-weight` | `sherpack.io/hook-weight` | ✅ |
| `helm.sh/hook-delete-policy` | `sherpack.io/hook-delete-policy` | ✅ |
| `helm.sh/resource-policy` | `sherpack.io/resource-policy` | ✅ |

**Note :** Le hook `test` est supporté mais il n'y a pas de commande `sherpack test` pour l'exécuter.

---

## 5. Fonctionnalités Diverses

### Chart/Pack

| Feature | Helm | Sherpack | Status |
|---------|------|----------|--------|
| `Chart.yaml` / `Pack.yaml` | ✅ | ✅ | |
| `values.yaml` | ✅ | ✅ | |
| `values.schema.json` | ✅ | ✅ | JSON Schema + format simplifié |
| `templates/` | ✅ | ✅ | |
| `templates/NOTES.txt` | ✅ | ❌ | **MANQUANT** |
| `crds/` directory | ✅ | ❌ | **MANQUANT** (CRDs non-templated) |
| `charts/` dependencies | ✅ | `packs/` | ✅ |
| `.helmignore` | ✅ | ❌ | **MANQUANT** |
| Library charts | ✅ | `kind: library` | ✅ (défini, pas testé) |
| Subcharts | ✅ | ❌ | **MANQUANT** (scoping values) |

### Repository

| Feature | Helm | Sherpack | Status |
|---------|------|----------|--------|
| HTTP repos (index.yaml) | ✅ | ✅ | |
| OCI registries | ✅ | ✅ | |
| Local file repos | ✅ | ✅ | |
| Repo index generation | ✅ | ❌ | **MANQUANT** (`helm repo index`) |
| Artifact Hub search | ✅ | ❌ | **MANQUANT** |
| Provenance files | ✅ | ❌ | Minisign au lieu de PGP |

### Sécurité

| Feature | Helm | Sherpack | Status |
|---------|------|----------|--------|
| Signature PGP | ✅ | 🚫 | Minisign à la place |
| Signature Minisign | ❌ | ✅ | |
| Integrity verification | ✅ | ✅ | SHA256 manifest |
| Lock files | ❌ | ✅ | **BONUS** Sherpack |
| Diamond conflict detection | ❌ | ✅ | **BONUS** Sherpack |

### Autres

| Feature | Helm | Sherpack | Status |
|---------|------|----------|--------|
| Plugin system | ✅ | ❌ | **MANQUANT** |
| Shell completion | ✅ | ❌ | **MANQUANT** |
| Post-render hooks | ✅ | ❌ | **MANQUANT** |
| JSON Schema validation | ✅ | ✅ | |
| Kubernetes version checks | ✅ | ⚠️ | Partiel |

---

## 6. Résumé des Manques Critiques

### Priorité Haute (bloquant pour migration)

1. **`.Files` API** - ~30% des charts l'utilisent
   - `.Files.Get`, `.Files.Glob`, `.Files.AsConfig`, `.Files.AsSecrets`

2. **`helm test` command** - Tests de release
   - La phase `test` existe mais pas de commande CLI

3. **`templates/NOTES.txt`** - Instructions post-install
   - Affiché après install/upgrade dans Helm

4. **`crds/` directory** - CRDs non-templated
   - Helm les applique avant les autres resources

5. **Subchart value scoping** - Values préfixées par nom du subchart
   - `postgresql.auth.username` → `auth.username` dans le subchart

### Priorité Moyenne

6. **`helm get` subcommands**
   - `helm get notes`, `helm get hooks`, `helm get metadata`

7. **`helm repo index`** - Génération d'index.yaml
   - Nécessaire pour héberger un repo HTTP

8. **`helm search hub`** - Recherche Artifact Hub

9. **`fromJson` / `fromYaml`** - Parsing inline

10. **`.Template.Name` / `.Template.BasePath`**

11. **Fonctions manquantes** : `dig`, `merge`, `pick`, `omit`, `wrap`, `dateModify`, regex functions

### Priorité Basse

12. **Plugin system** - Extension de Sherpack

13. **Shell completion** - bash/zsh/fish

14. **`.helmignore`** équivalent

15. **`helm env`** - Variables d'environnement

---

## 7. Avantages de Sherpack sur Helm

| Feature | Description |
|---------|-------------|
| **Jinja2 syntax** | Plus lisible que Go templates |
| **Error messages** | Messages contextuels avec suggestions |
| **Lock files** | Builds reproductibles (`Pack.lock.yaml`) |
| **Diamond detection** | Erreur explicite sur conflits de version |
| **SQLite cache** | Recherche FTS5 rapide |
| **Condition filtering** | `enabled` + `resolve` + `condition` |
| **Minisign** | Signatures modernes et simples |
| **Schema simplifié** | Alternative au JSON Schema verbeux |
| **Sync waves** | Ordonnancement explicite des resources |
| **Health checks** | Probes HTTP/command intégrés |

---

## 8. Recommandations d'Implémentation

### Phase 1 : Compatibilité Migration (Critique)

1. Implémenter `.Files` API
2. Ajouter commande `sherpack test`
3. Supporter `templates/NOTES.txt`
4. Ajouter support `crds/` directory
5. Implémenter value scoping pour subcharts

### Phase 2 : Parité Fonctionnelle

6. Ajouter `sherpack get notes/hooks/metadata`
7. Ajouter `sherpack repo index`
8. Implémenter `fromJson`/`fromYaml`
9. Ajouter `.Template.Name`/`.Template.BasePath`
10. Compléter les fonctions manquantes

### Phase 3 : Polish

11. Shell completion (clap_complete)
12. Plugin system
13. `.sherpackignore`
14. `sherpack env`
