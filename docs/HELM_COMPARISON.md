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
| `helm test` | `sherpack test` | ✅ | `commands/test.rs`, exécute les hooks `test` du release stocké |

### Commandes de Repository

| Commande Helm | Sherpack | Status | Notes |
|---------------|----------|--------|-------|
| `helm repo add` | `sherpack repo add` | ✅ | HTTP + OCI |
| `helm repo list` | `sherpack repo list` | ✅ | |
| `helm repo update` | `sherpack repo update` | ✅ | |
| `helm repo remove` | `sherpack repo remove` | ✅ | |
| `helm repo index` | `sherpack repo index` | ✅ | `commands/repo.rs::index`, supporte `--url` et `--merge` |
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
| `helm completion` | `sherpack completion` | ✅ | bash/zsh/fish/powershell/elvish via `clap_complete` |
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
| `.Release.IsUpgrade` | `release.isUpgrade` | ✅ | `release.rs:94` |
| `.Release.IsInstall` | `release.isInstall` | ✅ | `release.rs:91` |
| `.Release.Service` | - | ❌ | Toujours "Sherpack" |
| `.Chart.Name` | `pack.name` | ✅ | Renommé |
| `.Chart.Version` | `pack.version` | ✅ | |
| `.Chart.AppVersion` | `pack.appVersion` | ✅ | |
| `.Chart.*` (autres) | `pack.*` | ⚠️ | Partiel |
| `.Capabilities.KubeVersion` | `capabilities.kubeVersion` | ✅ | |
| `.Capabilities.APIVersions` | `capabilities.apiVersions` | ✅ | `context.rs:61` |
| `.Capabilities.HelmVersion` | - | 🚫 | N/A |
| `.Template.Name` | - | ❌ | **MANQUANT** |
| `.Template.BasePath` | - | ❌ | **MANQUANT** |
| `.Files` | `files` | ✅ | `engine.rs:411`, `files_object.rs` |

### Objet `.Files` (Implémenté)

Sherpack expose `files` aux templates via `SandboxedFileProvider` (sandbox restreint à la racine du pack) :

```jinja
{{ files.Get("config.json") }}
{{ files.Glob("files/*.yaml") }}
{{ files.AsConfig() }}
{{ files.AsSecrets() }}
{{ files.Lines("file.txt") }}
```

Le converter Helm transforme automatiquement `.Files.Get`, `.Files.Glob`, etc. vers cette API.

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
| `fromJson` | `fromjson` | ✅ | filtre + fonction (`filters.rs`) |
| `toYaml` | `toyaml` | ✅ |
| `fromYaml` | `fromyaml` | ✅ | filtre + fonction (`filters.rs`) |
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
| `dig` | `dig()` | ✅ | `functions.rs:150`, `dig(d, "a", "b", default)` |
| `merge` | `merge` | ✅ | filtre dict (`filters.rs::merge`) |
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
| `lookup` | `lookup()` | ✅ | Cluster-aware en mode install/upgrade ; `{}` en `sherpack template`. Voir [docs/LOOKUP.md](LOOKUP.md) |
| `.Capabilities.APIVersions.Has` | `"x" in capabilities.apiVersions` | ⚠️ | Méthode `.Has` absente, mais le pattern Jinja2 natif `in` fonctionne (`apiVersions: Vec<String>` exposé) |

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
| `templates/NOTES.txt` | ✅ | ✅ | `engine.rs:20`, exposé via `status` |
| `crds/` directory | ✅ | ⚠️ | Détection présente (`crd/detection.rs`), wiring full pas vérifié |
| `charts/` dependencies | ✅ | `packs/` | ✅ |
| `.helmignore` | ✅ | ⚠️ | Converter renomme en `.sherpackignore` mais pas honoré au `package` |
| Library charts | ✅ | `kind: library` | ✅ (défini, pas testé) |
| Subcharts | ✅ | ✅ | Value scoping + globals (`pack_renderer.rs:339`, `Values::for_subchart_json`) |

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

### Priorité Haute (bloquant pour migration de charts Helm)

_(plus de bloquants — tous les manques critiques précédents sont fermés)_

### Priorité Moyenne

2. **`helm get` subcommands**
   - `helm get notes`, `helm get hooks`, `helm get metadata`

3. **`helm search hub`** - Recherche Artifact Hub

4. **`.Template.Name` / `.Template.BasePath`**

5. **Fonctions manquantes** : `dateModify`, `dateInZone`, `fromToml`/`toToml`, `htpasswd`, `bcrypt`, `urlParse`/`urlJoin`, `osBase`/`osDir`/`osExt`, `pluck`, `mergeOverwrite`

### Priorité Basse

6. **Plugin system** - Extension de Sherpack

7. **`.helmignore` honoré au packaging** - Converter le renomme déjà mais le `sherpack package` ne le lit pas

8. **`helm env`** - Variables d'environnement

### Déjà implémenté (corrections vs anciennes versions de ce doc)

- ✅ `.Files` API (`files_object.rs`)
- ✅ `templates/NOTES.txt` (`engine.rs:20`)
- ✅ Subchart value scoping + globals (`pack_renderer.rs:339`)
- ✅ `dig`, `pick`, `omit`, `set`, `unset`, `values` (dict functions)
- ✅ `regex_match`, `regex_replace`, `regex_find`, `regex_find_all`
- ✅ `basename`, `dirname`, `extname`, `cleanpath`
- ✅ `sha1`, `sha256`, `sha512`, `md5`
- ✅ `floor`, `ceil`, `abs`
- ✅ `fromJson` / `fromYaml` (filtres + fonctions globales, `filters.rs`)
- ✅ `sherpack repo index` (génération d'index.yaml, supporte `--url` et `--merge`)
- ✅ `sherpack test` (exécute les hooks `test` du release stocké)
- ✅ `sherpack completion <shell>` (bash/zsh/fish/powershell/elvish)
- ✅ `lookup()` réel en install/upgrade (`engine/cluster_reader.rs`, `kube/lookup.rs`) — Helm-compat : 4-arg, swallow d'erreurs, cache intra-render, timeout configurable (`SHERPACK_LOOKUP_TIMEOUT_SECS`, défaut 5s), warning aggregé sur résultats non-vides ; converter Helm préserve l'appel. Doc utilisateur : [LOOKUP.md](LOOKUP.md)

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
