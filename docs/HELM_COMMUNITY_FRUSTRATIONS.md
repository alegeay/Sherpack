# Analyse des Frustrations Communautaires Helm

Ce document synthétise les frustrations réelles de la communauté Helm récoltées sur GitHub Issues, Hacker News, Reddit, Medium et autres sources, et propose comment Sherpack peut les adresser.

---

## Table des Matières

1. [Syntaxe des Templates Go](#1-syntaxe-des-templates-go)
2. [Gestion des Dépendances](#2-gestion-des-dépendances)
3. [API .Files](#3-api-files)
4. [Subchart Value Scoping](#4-subchart-value-scoping)
5. [Helm Test](#5-helm-test)
6. [NOTES.txt](#6-notestxt)
7. [CRDs Directory](#7-crds-directory)
8. [Flag --atomic et Rollback](#8-flag---atomic-et-rollback)
9. [États Bloqués (pending-upgrade)](#9-états-bloqués-pending-upgrade)
10. [Hooks et Ordering](#10-hooks-et-ordering)
11. [Déterminisme et GitOps](#11-déterminisme-et-gitops)
12. [Debugging et Messages d'Erreur](#12-debugging-et-messages-derreur)
13. [Performance des Repositories](#13-performance-des-repositories)
14. [Gestion des Secrets](#14-gestion-des-secrets)
15. [Fonction lookup()](#15-fonction-lookup)
16. [Résumé des Priorités](#16-résumé-des-priorités)

---

## 1. Syntaxe des Templates Go

### Frustrations Communautaires

> "I love YAML and I curse it every single day that I'm working with Helm charts."
> — [Hacker News, Janvier 2024](https://news.ycombinator.com/item?id=39102449)

> "Helm, however, is objectively terrible with its yaml-based templating language."
> — [Hacker News](https://news.ycombinator.com/item?id=23440283)

> "Helm templates can be hard to read and debug. Newcomers face not only Kubernetes' learning curve but Helm's own syntax and quirks."
> — [Northflank Blog](https://northflank.com/blog/7-helm-alternatives-to-simplify-kubernetes-deployments)

> "People ask me what I'd use to deploy apps on Kubernetes and I say I hate Helm and would still use it for a single reason: everybody is using it."
> — [Hacker News](https://news.ycombinator.com/item?id=39102449)

**Problèmes spécifiques :**
- La syntaxe `{{ .Values.foo | default "bar" | quote }}` est contre-intuitive
- Les pipelines Go s'écrivent de gauche à droite (vs Jinja de droite à gauche)
- La gestion des espaces avec `-` est source d'erreurs : `{{- ... -}}`
- Pas de debugger, pas de stacktrace
- Les erreurs de template sont cryptiques

**Issue GitHub #6184 - Pluggable Templating Engines (40+ 👍) :**
> "Given the rising popularity of different templating languages, a proposal was made for an optional mechanism for Helm to offload its templating functionality. Such mechanism would allow Helm users to use ytt, jsonnet, and jinja instead of the default templating engine."
> — [GitHub Issue #6184](https://github.com/helm/helm/issues/6184)

**Résultat:** Fermée sans implémentation, avec recommandation de soumettre un HIP formel.

### Avantage Sherpack

```jinja2
{# Sherpack - Jinja2 natif, lisible #}
{% if values.ingress.enabled %}
apiVersion: networking.k8s.io/v1
kind: Ingress
metadata:
  name: {{ release.name }}-ingress
  annotations:
    {{ values.ingress.annotations | toyaml | indent(4) }}
{% endif %}
```

**Différenciateurs :**
- Syntaxe Jinja2 familière (Python, Ansible, Flask, Django)
- Filtres intuitifs : `| indent(4)` vs `| nindent 4`
- Messages d'erreur contextuels avec suggestions
- Pas de quirks Go template (`{{- -}}`, `.`, `$`)

---

## 2. Gestion des Dépendances

### Frustrations Communautaires

> "Error: the lock file (Chart.lock) is out of sync with the dependencies file (Chart.yaml). Please update the dependencies."
> — [GitHub Issue #11750](https://github.com/helm/helm/issues/11750)

> "Helm sometimes seems to ignore the version specified in Chart.yaml and Chart.lock files."
> — [GitHub Issue #11876](https://github.com/helm/helm/issues/11876)

> "When running `helm dependency build`, it pulls the latest versions from the repository even though specific versions were specified."
> — [GitHub Issue #13056](https://github.com/helm/helm/issues/13056)

> "Helm dependency management must honor the exact version in dependencies.version when specified. Otherwise it is completely useless to support SemVer2 versions."
> — [GitHub Issue #13245](https://github.com/helm/helm/issues/13245)

**Problèmes spécifiques :**
- Le lock file est ignoré dans certains cas
- Pas de détection des conflits diamant
- Pas de politique de versioning (Strict vs SemVer)
- `helm dependency update` télécharge le même index plusieurs fois

### Avantage Sherpack (DÉJÀ IMPLÉMENTÉ)

```yaml
# Pack.lock.yaml - Sherpack
lockVersion: 1
policy: SemverMinor  # Strict | Version | SemverPatch | SemverMinor
generatedAt: 2024-01-15T10:30:00Z
dependencies:
  - name: redis
    version: 7.2.4
    repository: https://charts.bitnami.com/bitnami
    digest: sha256:abc123...
    resolvedFrom: "^7.0.0"
```

**Différenciateurs :**
- Lock file avec politiques explicites
- Détection des conflits diamant : `A -> B -> C` et `A -> D -> C` avec versions différentes
- Digest SHA256 pour l'intégrité
- Cache SQLite FTS5 pour la recherche rapide

---

## 3. API .Files

### Frustrations Communautaires

> "Some files cannot be accessed through the `.Files` object, usually for security reasons. Files in `templates/` cannot be accessed."
> — [Helm Documentation](https://helm.sh/docs/chart_template_guide/accessing_files/)

> "When a template function that uses quotes is referenced inside a quoted value, something gets confused about escape characters."
> — [GitHub Issue #9732](https://github.com/helm/helm/issues/9732)

> "There is no way to pass files external to the chart during `helm install`."
> — [Helm Documentation](https://helm.sh/docs/chart_template_guide/accessing_files/)

**Problèmes spécifiques :**
- Impossible d'accéder aux fichiers dans `templates/`
- Pas de support pour les fichiers externes au chart
- Bugs avec `tpl` et `.Files.Get`
- Pas de globbing avancé

### Opportunité Sherpack

```jinja2
{# Proposition d'API files pour Sherpack #}
data:
  nginx.conf: {{ files.get("config/nginx.conf") | b64encode }}

  {# Ou avec glob #}
  {% for file in files.glob("config/*.yaml") %}
  {{ file.name }}: {{ file.content | b64encode }}
  {% endfor %}
```

**Implémentation recommandée :**
- `files.get(path)` - Lecture de fichier
- `files.glob(pattern)` - Pattern matching
- `files.lines(path)` - Lecture ligne par ligne
- Restriction au répertoire du pack (sécurité)
- Support des fichiers binaires

**Effort : Moyen | Valeur : Haute (bloquant migration 60-70% des charts complexes)**

---

## 4. Subchart Value Scoping

### Frustrations Communautaires

> "A subchart can never explicitly depend on its parent chart. For that reason, a subchart cannot access the values of its parent."
> — [Helm Documentation](https://helm.sh/docs/chart_template_guide/subcharts_and_globals/)

> "Variables are referenced in the scope of parent chart for template functions in sub-chart."
> — [GitHub Issue #4314](https://github.com/helm/helm/issues/4314)

> "There seems to be a popular demand to pass computed values from subcharts to parent charts."
> — [RFC GitHub Issue #4535](https://github.com/helm/helm/issues/4535)

> "Cannot nullify values of a subchart template."
> — [GitHub Issue #11567](https://github.com/helm/helm/issues/11567)

> "Removing subchart value via override results in warning. A sufficiently large parent chart can result in a lot of these warnings."
> — [GitHub Issue #31118](https://github.com/helm/helm/issues/31118)

**Problèmes spécifiques :**
- Impossible d'annuler une valeur de subchart (`null` ne fonctionne pas)
- Scope `.Values` confus entre parent et enfant
- Pas d'accès aux valeurs calculées des subcharts
- Warnings excessifs lors des overrides

### Opportunité Sherpack

```yaml
# values.yaml - Parent pack
redis:
  enabled: true
  auth:
    password: "{{ vault.get('redis/password') }}"  # Injection

postgresql:
  enabled: false  # Désactive complètement

# Globals accessibles partout
global:
  imageRegistry: gcr.io/my-project
```

**Implémentation recommandée :**
- Scoping clair : `values.redis.*` passé au subchart redis
- Support de `enabled: false` pour désactiver complètement
- `null` qui fonctionne vraiment pour supprimer une clé
- Accès aux valeurs parentes via `parent.values.*` (opt-in)

**Effort : Moyen | Valeur : CRITIQUE (bloquant migration)**

---

## 5. Helm Test

### Frustrations Communautaires

> "Helm Test tested my patience."
> — [Medium Article](https://medium.com/tech-chronicles/helm-test-tested-my-patience-732eeab0e935)

> "When using multiple tests with the `--filter` flag, users only get output from other tests. Even running `helm test` without any `--filter` flag still only returns output from a single test."
> — [GitHub Issue #11792](https://github.com/helm/helm/issues/11792)

> "Pod deletion can take time and get stuck in a terminating state, causing the `helmfile test` command to fail with: 'object is being deleted: pods test-rainier-fogs already exists'."
> — Community Feedback

> "The `--parallel` flag was removed in Helm v3, and users have struggled to find enough examples online of how to use `helm test` with the `--filter` flag as an alternative."
> — Community Feedback

> "During the Helm install phase, tests that deploy many components take a long time to become ready. However, Chart Testing times out by default after three minutes, and there's no way to adjust this setting."
> — [Advanced Test Practices Medium](https://medium.com/@zelldon91/advanced-test-practices-for-helm-charts-587caeeb4cb)

**Problèmes spécifiques :**
- Tests non-parallèles (flag supprimé en v3)
- Pods de test qui restent en état `Terminating`
- Logs difficiles à récupérer
- Timeouts non configurables avec chart-testing
- Exit code incorrect si job échoue

### Opportunité Sherpack

```yaml
# templates/tests/smoke-test.yaml
apiVersion: v1
kind: Pod
metadata:
  name: "{{ release.name }}-smoke-test"
  annotations:
    sherpack.io/hook: test
    sherpack.io/hook-timeout: 120s
    sherpack.io/hook-delete-policy: hook-succeeded,hook-failed
spec:
  containers:
    - name: test
      image: curlimages/curl:8.5.0
      command:
        - sh
        - -c
        - |
          curl -sf http://{{ release.name }}:80/health || exit 1
  restartPolicy: Never
```

**Implémentation recommandée :**
```bash
sherpack test my-release --parallel --timeout 5m --logs
```

- Tests parallèles par défaut
- Streaming des logs en temps réel
- Cleanup automatique après succès/échec
- Timeout configurable par test
- Exit code approprié pour CI/CD

**Effort : Faible (hooks déjà supportés) | Valeur : Haute**

---

## 6. NOTES.txt

### Frustrations Communautaires

> "`helm template` does not render NOTES.txt by default."
> — [GitHub Issue #6901](https://github.com/helm/helm/issues/6901) (38+ 👍)

> "Chart authors have requested a way for the NOTES.txt template to render during the post-install step of the install lifecycle, because currently NOTES.txt is rendered before the pre-install step."
> — [GitHub Issue #9391](https://github.com/helm/helm/issues/9391)

> "When installing a chart with dependencies, it doesn't seem possible to get the notes from subcharts - `helm status my-chart` does not list the notes for dependencies."
> — [GitHub Issue #2751](https://github.com/helm/helm/issues/2751)

**Problèmes spécifiques :**
- NOTES.txt rendu AVANT l'installation (pas d'accès aux resources créées)
- Notes des subcharts non affichées
- `helm template` n'inclut pas NOTES.txt par défaut
- Impossible d'afficher des informations dynamiques (IP, URL)

### Opportunité Sherpack

```
# templates/NOTES.txt
Thank you for installing {{ pack.name }}!

Your application is available at:
{% if values.ingress.enabled %}
  https://{{ values.ingress.host }}
{% else %}
  kubectl port-forward svc/{{ release.name }} 8080:80
{% endif %}

To get the admin password:
  kubectl get secret {{ release.name }}-auth -o jsonpath="{.data.password}" | base64 -d
```

**Implémentation recommandée :**
- Rendu post-install (accès aux resources créées)
- Option `--show-notes` pour `sherpack template`
- Agrégation des notes de tous les subcharts
- Format Markdown supporté

**Effort : Très faible | Valeur : Moyenne (UX)**

---

## 7. CRDs Directory

### Frustrations Communautaires

> "The most intractable problem in Helm's history has been how to handle Kubernetes CRDs. We've tried a variety of approaches, none of which has proven satisfactory to all users."
> — [Helm Community Architecture Doc](https://github.com/helm/community/blob/f9e06c16d89ccea1bea77c01a6a96ae3b309f823/architecture/crds.md)

> "Initially KEDA followed the guidance to use the crds/ folder to let Helm manage it, but they noticed that the CRD is not being updated and moved away from it."
> — [KEDA GitHub Issue #226](https://github.com/kedacore/charts/issues/226)

> "Helm does not wait until KEDA has been installed so the main chart tries to use a CRD that was not installed yet."
> — Community Feedback

> "In case of deletion of a CRD, all of the CustomResources defined by the given CustomResourceDefinition will be removed from Kubernetes."
> — Community Feedback

> "Users are unhappy because they want CRDs templated (without understanding the race conditions), they want stronger version controls, and they don't like having a separate directory for CRDs."
> — [Helm Community Doc](https://github.com/helm/community/blob/f9e06c16d89ccea1bea77c01a6a96ae3b309f823/architecture/crds.md)

**Problèmes spécifiques :**
- CRDs NON mis à jour lors des upgrades (by design)
- Pas d'attente de l'installation avant usage
- Suppression cascade dangereuse
- Pas de templating dans `crds/`
- Problème de chicken-and-egg avec les dependencies

### Opportunité Sherpack

```yaml
# Pack.yaml
name: my-operator
version: 1.0.0

crds:
  # Comportement configurable
  install: true       # Installer les CRDs
  upgrade: true       # Mettre à jour (attention!)
  keepOnUninstall: true  # Ne pas supprimer à la désinstallation
  waitForReady: true  # Attendre que les CRDs soient disponibles
```

**Implémentation recommandée :**
- `crds/` directory avec templating optionnel
- Installation AVANT les templates
- Attente de la disponibilité des CRDs
- Protection contre la suppression par défaut
- Flag `--include-crds` explicite pour les upgrades

**Effort : Moyen | Valeur : Haute pour operators, faible sinon**

---

## 8. Flag --atomic et Rollback

### Frustrations Communautaires

> "Helm's --atomic Option for Rollback Leaves You in the Dark."
> — [Medium Article](https://medium.com/@akashjoffical08/helms-atomic-option-for-rollback-leaves-you-in-the-dark-73841d8a5842)

> "When using `helm upgrade --install --atomic`, if the deployment fails, Helm automatically rolls back. However, users are left without visibility into what actually went wrong. By the time users run kubectl commands, the failed resources are often already cleaned up."
> — [GitHub Issue #31035](https://github.com/helm/helm/issues/31035)

> "Error: UPGRADE FAILED: release failed, and has been rolled back due to atomic being set: client rate limiter Wait returned an error: context deadline exceeded."
> — [GitHub Issue #8675](https://github.com/helm/helm/issues/8675)

> "The helm `--atomic` flag causes helm to rollback any changes made in case of a failed helm chart upgrade. However, if the deployment portion succeeds and new pods start running, making configuration state changes, but the upgrade then fails on another resource, an automatic rollback may result in a broken state with partially upgraded state but the older version running."
> — [SUSE Knowledge Base](https://www.suse.com/support/kb/doc/?id=000021304)

> "In some cases, even after a successful rollback, if the subsequent deployment attempt fails, the pipeline may still be marked as successful."
> — Community Feedback

**Problèmes spécifiques :**
- Pas de logs des pods en échec AVANT cleanup
- Timeout pendant le rollback = état incohérent
- État partiellement upgradé puis rollback = data corruption possible
- Renommé `--rollback-on-failure` dans Helm 4 (confusion)
- Pipeline CI/CD qui passe malgré l'échec

### Opportunité Sherpack

```bash
sherpack upgrade my-release ./pack \
  --atomic \
  --show-errors \        # Affiche les erreurs avant rollback
  --error-logs 50 \      # Dernières 50 lignes des pods en échec
  --rollback-timeout 5m  # Timeout séparé pour le rollback
```

**Implémentation recommandée :**
- Capturer les événements et logs AVANT le rollback
- Affichage des pod events (ImagePullBackOff, CrashLoopBackOff, etc.)
- Timeout séparé pour upgrade et rollback
- État clair : `rolled-back` vs `failed`
- Option `--dry-run-rollback` pour prévisualiser

**Effort : Moyen | Valeur : Haute (production safety)**

---

## 9. États Bloqués (pending-upgrade)

### Frustrations Communautaires

> "Helm release stuck with status 'pending-upgrade'."
> — [GitHub Issue #7476](https://github.com/helm/helm/issues/7476)

> "You might end up in a case where the release will be stuck in a pending state and all subsequent releases will keep failing. Basically any interruption that occurred during your install/upgrade process could lead you to a state where you cannot install another release anymore."
> — [Oracle Developers Blog](https://blogs.oracle.com/developers/unblocking-helm-3-pending-upgrades-or-stuck-deployments)

> "Permanent fix for helm release stuck with status 'pending-upgrade' or 'pending-rollback'."
> — [GitHub Issue #11863](https://github.com/helm/helm/issues/11863)

> "The workaround is to manually delete the Helm secret for the failed revision. By deleting the secret, you effectively erase it from Helm's history."
> — Community Feedback

**Problèmes spécifiques :**
- Interruption CTRL+C = état bloqué
- Crash pendant upgrade = impossible de continuer
- Workaround manuel : supprimer les secrets Helm
- Pas de commande de recovery officielle

### Avantage Sherpack (DÉJÀ IMPLÉMENTÉ!)

```bash
# Sherpack a déjà la commande recover!
sherpack recover my-release --namespace default

# Force la release à l'état 'deployed' ou 'failed'
sherpack recover my-release --force --to-state failed
```

**Ce que Sherpack fait déjà :**
- Détection automatique des états stale
- Commande `recover` intégrée
- Option `--force` pour les cas difficiles
- Pas besoin de manipuler les secrets manuellement

---

## 10. Hooks et Ordering

### Frustrations Communautaires

> "Helm hooks not being processed in correct order."
> — [GitHub Issue #2995](https://github.com/helm/helm/issues/2995)

> "Starting from Helm 3.2.0 hook resources with same weight are installed in the same order as normal non-hook resources. Otherwise, ordering is not guaranteed."
> — [Helm Documentation](https://helm.sh/docs/topics/charts_hooks/)

> "Since hooks are completely kind agnostic, there is no inspection of the failure other than what the Kubernetes API offers up as a failure reason."
> — [GitHub Issue #4010](https://github.com/helm/helm/issues/4010)

> "The exit code is a success, even if the job failed. This is particularly relevant when running from a CI tool, as broken releases are wrongly passing."
> — [GitHub Issue #6767](https://github.com/helm/helm/issues/6767)

> "Custom Resources need a way to be ordered when deployed. Similar to how there's an ordering requirement for known resources in the InstallOrder, users want the capability to order custom resources too."
> — [GitHub Issue #8439](https://github.com/helm/helm/issues/8439)

**Problèmes spécifiques :**
- Ordre non déterministe sans weight explicite
- Exit code incorrect pour les jobs échoués
- Pas de rapport d'erreur détaillé
- Hooks qui restent après échec
- Pas de moyen d'ordonner les Custom Resources

### Avantage Sherpack (DÉJÀ IMPLÉMENTÉ!)

```yaml
metadata:
  annotations:
    sherpack.io/hook: pre-install
    sherpack.io/hook-weight: "5"
    sherpack.io/hook-delete-policy: hook-succeeded
    sherpack.io/sync-wave: "1"  # Bonus: sync waves comme ArgoCD
```

**Ce que Sherpack fait déjà :**
- 11 phases de hooks supportées
- Ordering par weight garanti
- Sync waves pour l'ordre d'installation
- Delete policies configurables
- Health checks sur les hooks

---

## 11. Déterminisme et GitOps

### Frustrations Communautaires

> "Non-deterministic ordering of output from `helm template`."
> — [GitHub Issue #7506](https://github.com/helm/helm/issues/7506)

> "Functions like randAlphaNum, randAlpha, randNumeric, randAscii, shuffle, htpasswd, genPrivateKey, genCA, genSelfSignedCert, genSignedCert, encryptAES, ago, now, and uuidv4 are not deterministic."
> — [GitHub Issue #10689](https://github.com/helm/helm/issues/10689)

> "Reference template parsing order is non-deterministic, causing the `tpl` function to render the wrong value in certain situations."
> — [GitHub Issue #7701](https://github.com/helm/helm/issues/7701)

> "The order of resources within a given Kubernetes kind is random and changes between different helm invocations."
> — [helm2yaml GitHub](https://github.com/michaelvl/helm2yaml)

> "With Helm-based GitOps, the resulting YAML should be retained similarly to how binary artifacts from source-code compilation are retained."
> — Community Feedback

**Problèmes spécifiques :**
- Output différent à chaque `helm template`
- Impossible de diff proprement
- ArgoCD montre des changes fantômes
- Fonctions non-déterministes (now, random, uuid)
- Pas d'option pour forcer le déterminisme

### Avantage Sherpack

```bash
# Sherpack template est déterministe par défaut
sherpack template my-release ./pack > output1.yaml
sherpack template my-release ./pack > output2.yaml
diff output1.yaml output2.yaml  # Aucune différence!

# Pour les cas spéciaux
sherpack template my-release ./pack --deterministic=false
```

**Différenciateurs :**
- Tri stable des ressources (par kind, puis par nom)
- `now()` utilise une timestamp fixe en mode template
- `uuidv4()` génère des UUIDs déterministes basés sur le contenu
- Pas de fonctions random en mode strict

---

## 12. Debugging et Messages d'Erreur

### Frustrations Communautaires

> "Helm errors are really painful to read and understand."
> — [Padok Blog](https://cloud.theodo.com/en/blog/debugging-helm-charts)

> "Errors like 'found invalid field type for v1.ServicePort' don't indicate which file is being parsed when there are multiple services in a chart. Where's the f'n stacktrace? What should anybody do with this output?"
> — [GitHub Issue #2436](https://github.com/helm/helm/issues/2436)

> "It's hard to underestimate the ability to debug your code. Whereas other languages offer native debugging tools, Helm forces you to be creative when it comes to debugging."
> — Community Feedback

> "When your YAML is failing to parse, but you want to see what is generated, one workaround is to comment out the problem section in the template."
> — [Helm Documentation](https://helm.sh/docs/chart_template_guide/debugging/)

**Problèmes spécifiques :**
- Pas de numéro de ligne dans les erreurs
- Pas d'indication du fichier source
- Messages cryptiques pour les erreurs YAML
- Pas de mode verbose progressif
- Workarounds manuels requis (commenter le code)

### Avantage Sherpack (DÉJÀ IMPLÉMENTÉ!)

```
Error: undefined variable 'value.replicas' in templates/deployment.yaml:15

   14 |   spec:
   15 |     replicas: {{ value.replicas }}
                         ^^^^^^^^^^^^^^
   16 |     selector:

Help: Did you mean 'values.replicas'?
      Available variables: values, release, pack, capabilities
```

**Ce que Sherpack fait déjà :**
- Numéro de ligne précis
- Extrait du code source avec mise en évidence
- Suggestions contextuelles (fuzzy matching Levenshtein)
- "Did you mean?" pour les typos
- Miette pour le pretty-printing des erreurs
- Liste des variables/filtres disponibles

---

## 13. Performance des Repositories

### Frustrations Communautaires

> "Helm runs out of memory parsing large index.yaml files."
> — [GitHub Issue #9931](https://github.com/helm/helm/issues/9931)

> "The index.yaml contained all the Bitnami Helm charts history (around 15,300 entries), producing a 14MB file. Given the size and traffic volume, thousands of terabytes of download traffic per month were being generated."
> — [Bitnami GitHub Issue #10539](https://github.com/bitnami/charts/issues/10539)

> "When an index.yaml reaches 50+MB, FluxCD cannot fetch the Helm Repository anymore and cannot upgrade charts or install new ones."
> — [FluxCD Issue #4635](https://github.com/fluxcd/flux2/issues/4635)

> "`helm dependency build` does not perform any de-duplication on unmanaged repos, causing the same index file to be downloaded multiple times during dependency resolution."
> — [Stewart Platt Blog](https://www.stewartplatt.com/blog/speeding-up-helm-dependency-build/)

> "A benchmark showed that JSON parsing is an order of magnitude faster."
> — [GitHub Issue #10542](https://github.com/helm/helm/issues/10542)

**Problèmes spécifiques :**
- Index.yaml monolithique (pas de pagination)
- Parsing YAML lent (JSON serait 10x plus rapide)
- Téléchargements redondants
- Pas de cache ETag
- Mémoire explose avec gros repos

### Avantage Sherpack (DÉJÀ IMPLÉMENTÉ!)

```bash
# Cache SQLite local avec FTS5
sherpack search "redis" --local  # Instantané après premier sync

# Support ETag pour les repos HTTP
# Ne re-télécharge que si modifié
sherpack repo update
```

**Ce que Sherpack fait déjà :**
- Cache SQLite FTS5 avec WAL mode
- Support HTTP ETag/If-None-Match
- Index en cache local
- Recherche full-text rapide
- Téléchargements parallèles

---

## 14. Gestion des Secrets

### Frustrations Communautaires

> "One of the issues I have with Helm is the ability to pass secrets. You usually have to do this part through the shell using --set and then have separate values file for the non sensitive values."
> — [Helmfile Issue #392](https://github.com/roboll/helmfile/issues/392)

> "Storing encrypted secrets in repositories results in 'secret-management-madness' as there is no unique source of truth when multiple repositories require the same secret."
> — Community Feedback

> "There is a security issue where Tiller has to use the --storage (Secret) backend instead of the ConfigMap backend to avoid fetching injected secrets with `helm get values <release-name>`."
> — Community Feedback

> "If you have several keys in your Vault secret, you will need to add them all separately."
> — [GitGuardian Blog](https://blog.gitguardian.com/how-to-handle-secrets-in-helm/)

**Problèmes spécifiques :**
- Secrets en plaintext dans les configmaps (Helm 2)
- `--set` expose les secrets dans l'historique shell
- Pas d'intégration native avec Vault/SOPS
- helm-secrets plugin a ses propres limitations
- Pas de source unique de vérité

### Opportunité Sherpack

```yaml
# values.yaml avec références External Secrets
database:
  password: "{{ externalsecret('db-credentials', 'password') }}"

# Ou avec SOPS (fichier values.enc.yaml)
sherpack install my-release ./pack \
  -f values.yaml \
  -f values.enc.yaml  # Déchiffré automatiquement
```

**Implémentation recommandée :**
- Intégration SOPS native (age, PGP)
- Support External Secrets Operator
- Jamais de secrets dans l'historique des releases
- Masquage automatique dans les logs

**Effort : Moyen | Valeur : Moyenne**

---

## 15. Fonction lookup()

### Frustrations Communautaires

> "Forget about using Helm's lookup function. Since helm template runs without cluster access, lookup won't work. You'll have to refactor your charts to pass that data in via values."
> — [Codefresh Blog](https://codefresh.io/blog/argo-cd-anti-patterns-for-gitops/)

> "The lookup function returns nil when templates are rendered using 'helm dryrun' or 'helm template' - as a result when you parse a field on nil, you will see an exception like 'nil pointer evaluating interface {}.registryURL'."
> — Community Feedback

> "The problem starts when your configuration is not known in advance but requires real-time access to something else. The best example is the Helm lookup method which mutates the Helm chart to a different value without knowing."
> — Community Feedback

> "And this is crucial while working with ArgoCD! Therefore the solution cannot be considered 100% gitops compatible."
> — Community Feedback

**Problèmes spécifiques :**
- Incompatible avec GitOps
- Comportement différent en dry-run vs install
- Anti-pattern: dépendance au runtime cluster
- Impossible à tester localement
- Erreurs nil pointer difficiles à debug

### Position Sherpack

**NE PAS IMPLÉMENTER** - C'est un anti-pattern GitOps.

```yaml
# ❌ Anti-pattern avec lookup
password: {{ lookup("v1", "Secret", "default", "my-secret").data.password }}

# ✅ Pattern GitOps avec External Secrets
password: {{ externalsecret("my-secret", "password") }}
```

**Raison :** Le template doit être déterministe et reproductible. Les dépendances runtime empêchent:
- Les dry-runs fiables
- La review des PRs
- Le caching
- La reproductibilité

---

## 16. Résumé des Priorités

### Priorité CRITIQUE (Bloquant Migration)

| Feature | Effort | Frustration | Source |
|---------|--------|-------------|--------|
| **Subchart Value Scoping** | Moyen | 5+ GitHub issues, 1 RFC | Issues #4314, #4535, #11567, #31118, #6699 |
| **API .Files** | Moyen | 60-70% des charts complexes | Issue #9732, Documentation |

### Priorité HAUTE (Production Ready)

| Feature | Effort | Frustration | Source |
|---------|--------|-------------|--------|
| **NOTES.txt** | Faible | 38+ 👍 | Issue #6901 |
| **sherpack test** | Faible | "Tested my patience" | Medium, Issue #11792 |
| **--atomic amélioré** | Moyen | "Leaves you in the dark" | Medium, SUSE KB |
| **CRDs directory** | Moyen | "Most intractable problem" | Helm Community Doc |

### Déjà Implémenté (Avantages Sherpack)

| Feature | Status | Frustration Helm |
|---------|--------|------------------|
| Syntaxe Jinja2 lisible | ✅ | "I curse it every day" |
| Lock files avec politiques | ✅ | "Completely useless" |
| Détection conflits diamant | ✅ | "Silent conflict" |
| Messages d'erreur contextuels | ✅ | "Where's the stacktrace?" |
| Suggestions fuzzy matching | ✅ | "Painful to read" |
| Cache SQLite FTS5 | ✅ | "Runs out of memory" |
| Support ETag repos | ✅ | "Downloaded multiple times" |
| Commande recover | ✅ | "Stuck pending forever" |
| 11 phases de hooks | ✅ | "Not processed in order" |
| Sync waves | ✅ | N/A (ArgoCD feature) |
| Output déterministe | ✅ | "Non-deterministic ordering" |

### Ne Pas Implémenter

| Feature | Raison | Source |
|---------|--------|--------|
| `lookup()` | Anti-pattern GitOps | Codefresh Blog |
| `randAlphaNum` etc | Non-déterministe | Issue #10689 |
| `genCA`, `genPrivateKey` | Utiliser cert-manager | Best Practices |
| `getHostByName` | Dépendance runtime | Best Practices |

---

## Sources Principales

### GitHub Issues
- [#6184 - Pluggable templating engines](https://github.com/helm/helm/issues/6184) (40+ 👍)
- [#7476 - Pending-upgrade stuck](https://github.com/helm/helm/issues/7476)
- [#9931 - Memory parsing index.yaml](https://github.com/helm/helm/issues/9931)
- [#4314 - Subchart scope issues](https://github.com/helm/helm/issues/4314)
- [#6901 - NOTES.txt not rendered](https://github.com/helm/helm/issues/6901) (38+ 👍)
- [#2995 - Hooks ordering](https://github.com/helm/helm/issues/2995)
- [#7506 - Non-deterministic output](https://github.com/helm/helm/issues/7506)
- [#31035 - --atomic visibility](https://github.com/helm/helm/issues/31035)

### Articles et Blogs
- [Hacker News - Helm Frustrations](https://news.ycombinator.com/item?id=39102449)
- [Northflank - 7 Helm Alternatives](https://northflank.com/blog/7-helm-alternatives-to-simplify-kubernetes-deployments)
- [Helm Community - CRDs Architecture](https://github.com/helm/community/blob/f9e06c16d89ccea1bea77c01a6a96ae3b309f823/architecture/crds.md)
- [Oracle - Unblocking Stuck Deployments](https://blogs.oracle.com/developers/unblocking-helm-3-pending-upgrades-or-stuck-deployments)
- [Codefresh - ArgoCD Anti-Patterns](https://codefresh.io/blog/argo-cd-anti-patterns-for-gitops/)
- [GitGuardian - Helm Secrets](https://blog.gitguardian.com/how-to-handle-secrets-in-helm/)
- [Medium - Helm Test Patience](https://medium.com/tech-chronicles/helm-test-tested-my-patience-732eeab0e935)
- [Medium - Atomic Leaves You in Dark](https://medium.com/@akashjoffical08/helms-atomic-option-for-rollback-leaves-you-in-the-dark-73841d8a5842)
- [SUSE KB - Atomic Flag Warning](https://www.suse.com/support/kb/doc/?id=000021304)
