---
id: crd-handling
title: Gestion des CRD
sidebar_position: 6
---

# Gestion des CRD

Sherpack fournit une gestion sophistiquée des CustomResourceDefinitions (CRD) qui résout les limitations majeures de Helm grâce à un système de politiques basé sur l'intention.

## Le problème avec l'approche de Helm

La gestion des CRD par Helm présente plusieurs problèmes bien documentés :

1. **Les CRD ne sont jamais mis à jour** - Une fois installés, les CRD ne sont jamais mis à niveau ([#7735](https://github.com/helm/helm/issues/7735))
2. **Mauvaise stratégie de patch** - Strategic Merge Patch échoue pour les CRD ([#5853](https://github.com/helm/helm/issues/5853))
3. **Timing des dépendances** - Les CRD ne sont pas prêts avant l'application des CR ([#10585](https://github.com/helm/helm/issues/10585))
4. **Pas de templating** - Les CRD dans `crds/` ne peuvent pas utiliser la syntaxe de template
5. **Dry-run cassé** - `--dry-run` ne fonctionne pas avec les CRD
6. **Cascades de suppression** - Supprimer un CRD supprime TOUTES les ressources personnalisées

## La solution de Sherpack : Politiques basées sur l'intention

Au lieu de déterminer le comportement par l'emplacement du fichier (`crds/` vs `templates/`), Sherpack utilise des **politiques basées sur l'intention** qui déclarent explicitement comment chaque CRD doit être géré.

### Trois politiques

| Politique | Comportement | Cas d'usage |
|-----------|--------------|-------------|
| `managed` | Cycle de vie complet - installation, mise à jour, protection à la désinstallation | CRD possédés par votre pack |
| `shared` | Installation/mise à jour, jamais supprimé | CRD utilisés par plusieurs releases |
| `external` | Ne pas toucher | CRD pré-existants du cluster |

### Définir les politiques

Ajoutez une annotation à votre CRD :

```yaml
apiVersion: apiextensions.k8s.io/v1
kind: CustomResourceDefinition
metadata:
  name: certificates.cert-manager.io
  annotations:
    sherpack.io/crd-policy: shared
```

Ou utilisez l'annotation compatible Helm :

```yaml
annotations:
  helm.sh/resource-policy: keep  # Se traduit en "shared"
```

## Installation des CRD

### Ordonnancement automatique

Sherpack garantit que les CRD sont installés et prêts avant les ressources personnalisées :

```
1. CRD du répertoire crds/
2. CRD des templates/
3. Attendre que tous les CRD soient Established
4. Ressources régulières (Services, Deployments, etc.)
5. Ressources personnalisées (après que leur CRD soit prêt)
```

### Ignorer les CRD

Si les CRD sont déjà installés de manière externe :

```bash
sherpack install myrelease ./mypack --skip-crds
```

## Mises à jour des CRD

### Analyse de mise à jour sécurisée

Sherpack analyse les changements de CRD avec **24 types de changements différents** et les classe par sévérité :

| Sévérité | Action | Exemples |
|----------|--------|----------|
| **Safe** | Application automatique | Ajouter un champ optionnel, ajouter une version, ajouter une colonne d'impression |
| **Warning** | Afficher un avertissement | Changements de validation, changements de conversion |
| **Dangerous** | Bloquer (sauf si forcé) | Supprimer une version, changer la portée, supprimer un champ requis |

### Voir les changements avant application

```bash
sherpack upgrade myrelease ./mypack --show-crd-diff
```

Exemple de sortie :

```
CRD Changes for certificates.cert-manager.io:

  + spec.versions[0].schema.openAPIV3Schema.properties.newField:
      type: string
      description: "New optional field"

  ~ spec.versions[0].schema.openAPIV3Schema.properties.config.maxLength:
      - 256
      + 512

  ⚠ Validation change detected. Existing CRs may be affected.

Proceed with upgrade? [y/N]
```

### Forcer les mises à jour

Pour appliquer des changements dangereux (à utiliser avec précaution) :

```bash
sherpack upgrade myrelease ./mypack --force-crd-update
```

### Ignorer les mises à jour

Pour ne jamais mettre à jour les CRD :

```bash
sherpack upgrade myrelease ./mypack --skip-crd-update
```

## Désinstallation des CRD

### Comportement par défaut

Par défaut, les CRD sont **conservés** lors de la désinstallation d'une release. Cela évite la perte accidentelle de données.

### Supprimer les CRD

Pour supprimer les CRD (avec vérifications de sécurité) :

```bash
sherpack uninstall myrelease --delete-crds
```

Si les CRD ont des ressources personnalisées existantes, une confirmation est requise :

```
This will delete all CustomResources of these types:
  - certificates.cert-manager.io (15 resources in production)
  - issuers.cert-manager.io (3 resources in production)

Use --confirm-crd-deletion to proceed.
```

```bash
sherpack uninstall myrelease --delete-crds --confirm-crd-deletion
```

### Protection par politique

Les CRD avec une politique `shared` ou `external` ne sont **jamais supprimés**, même avec `--delete-crds` :

```
Blocked by policy:
  - certificates.cert-manager.io (policy: shared)

These CRDs will not be deleted. To override, change the policy annotation.
```

## CRD templatés

Contrairement à Helm, Sherpack prend en charge les CRD templatés :

```yaml
# templates/crd.yaml
apiVersion: apiextensions.k8s.io/v1
kind: CustomResourceDefinition
metadata:
  name: {{ values.crdName }}.{{ values.group }}
  labels:
    {{- values.labels | toyaml | indent(4) }}
```

### Avertissements de lint

Les CRD templatés génèrent des avertissements de lint pour information :

```bash
sherpack lint ./mypack
```

```
⚠ Warning: Templated CRD in templates/crd.yaml
  Consider placing in crds/ directory for:
  - Predictable installation order
  - Protection from accidental deletion
  - Clearer upgrade semantics
```

### CRD statiques dans crds/

Le répertoire `crds/` ne doit contenir **que du YAML statique**. Si une syntaxe Jinja est détectée :

```
✗ Error: Jinja syntax detected in crds/mycrd.yaml
  Files in crds/ are NOT templated by Sherpack.
  Move to templates/ if templating is needed.
```

## CRD de dépendances

Lors de la dépendance à un pack qui fournit des CRD :

```yaml
# Pack.yaml
dependencies:
  - name: cert-manager
    version: "1.x"
    repository: https://charts.jetstack.io
```

Sherpack construit un graphe de dépendances garantissant l'ordre correct :

```
1. CRD de cert-manager
2. Attendre que les CRD de cert-manager soient prêts
3. Templates de cert-manager
4. Vos CRD (le cas échéant)
5. Attendre que vos CRD soient prêts
6. Vos templates (peuvent maintenant utiliser Certificate, Issuer, etc.)
```

## Configuration Pack.yaml

Configurez le comportement des CRD dans votre Pack.yaml :

```yaml
apiVersion: sherpack/v1
kind: application
metadata:
  name: my-operator
  version: 1.0.0

crds:
  # Comportement d'installation
  install: true              # Installer les CRD (par défaut : true)

  # Comportement de mise à niveau
  upgrade:
    enabled: true            # Autoriser les mises à jour de CRD (par défaut : true)
    strategy: safe           # safe | force | skip

  # Comportement de désinstallation
  uninstall:
    keep: true               # Conserver les CRD à la désinstallation (par défaut : true)

  # Attendre l'enregistrement des CRD
  waitReady: true            # Attendre la condition Established
  waitTimeout: 60s           # Timeout pour la disponibilité
```

## Référence CLI

| Commande | Description |
|----------|-------------|
| `--skip-crds` | Ne pas installer les CRD |
| `--skip-crd-update` | Ne pas mettre à jour les CRD existants |
| `--force-crd-update` | Appliquer les changements dangereux de CRD |
| `--show-crd-diff` | Afficher les changements de CRD avant application |
| `--delete-crds` | Supprimer les CRD à la désinstallation |
| `--confirm-crd-deletion` | Confirmer la suppression de CRD avec perte de données |

## Comparaison avec Helm

| Fonctionnalité | Helm | Sherpack |
|----------------|------|----------|
| Modèle de politique | Basé sur l'emplacement | Annotations basées sur l'intention |
| Mises à jour de CRD | Jamais | Sécurisé par défaut, configurable |
| Stratégie de patch | Strategic Merge (cassé) | Server-Side Apply |
| Templating dans crds/ | Non | Non (avec erreur de lint) |
| Templating dans templates/ | Oui (mais supprimé à la désinstallation) | Oui (avec avertissement de lint) |
| Ordonnancement des dépendances | Aucun | Automatique |
| Attendre la disponibilité | Non | Oui (configurable) |
| Dry-run | Cassé | Support complet |
| Suppression | Toujours bloquée | Configurable avec confirmation |
| Sortie de diff | Aucune | Diff riche avec analyse d'impact |
| Détection de mise à jour sécurisée | Aucune | Analyse de 24 types de changements |

## Bonnes pratiques

1. **Utilisez la politique `managed`** pour les CRD que votre pack possède exclusivement
2. **Utilisez la politique `shared`** pour les CRD qui pourraient être utilisés par d'autres releases
3. **Utilisez la politique `external`** pour les CRD au niveau du cluster comme cert-manager
4. **Placez les CRD statiques dans `crds/`** pour un comportement prévisible
5. **Examinez `--show-crd-diff`** avant les mises à niveau en production
6. **N'utilisez jamais `--force-crd-update`** sans comprendre l'impact
7. **Testez les changements de CRD** avec `--dry-run` d'abord
