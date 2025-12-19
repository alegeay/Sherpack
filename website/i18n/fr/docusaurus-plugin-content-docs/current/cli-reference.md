---
id: cli-reference
title: Référence CLI
sidebar_position: 100
---

# Référence CLI

Référence complète de toutes les commandes Sherpack.

## Options globales

Toutes les commandes supportent :

| Option | Description |
|--------|-------------|
| `--debug` | Activer la sortie de débogage |
| `-h, --help` | Afficher l'aide |
| `-V, --version` | Afficher la version |

---

## Commandes de templating

### template

Rendre les templates vers stdout ou des fichiers.

```bash
sherpack template <NOM> <PACK> [OPTIONS]
```

| Option | Description |
|--------|-------------|
| `-n, --namespace <NS>` | Namespace cible [défaut: default] |
| `-f, --values <FICHIER>` | Fichier de valeurs (répétable) |
| `--set <CLÉ=VALEUR>` | Override de valeurs (répétable) |
| `-o, --output <DIR>` | Répertoire de sortie |
| `-s, --show-only <NOM>` | Rendre uniquement le template spécifié |
| `--show-values` | Afficher les valeurs calculées |
| `--skip-schema` | Ignorer la validation du schéma |

### lint

Valider la structure du pack.

```bash
sherpack lint <PACK> [OPTIONS]
```

| Option | Description |
|--------|-------------|
| `--strict` | Échouer sur les variables non définies |
| `--skip-schema` | Ignorer la validation du schéma |

### validate

Valider les valeurs contre le schéma.

```bash
sherpack validate <PACK> [OPTIONS]
```

| Option | Description |
|--------|-------------|
| `-f, --values <FICHIER>` | Fichier de valeurs additionnel |
| `--set <CLÉ=VALEUR>` | Override de valeurs |
| `--json` | Sortie JSON |
| `-v, --verbose` | Sortie verbeuse |

### show

Afficher les informations du pack.

```bash
sherpack show <PACK> [OPTIONS]
```

| Option | Description |
|--------|-------------|
| `--all` | Afficher toutes les informations |
| `--values` | Afficher les valeurs par défaut |

### create

Créer un nouveau pack.

```bash
sherpack create <NOM> [OPTIONS]
```

| Option | Description |
|--------|-------------|
| `-o, --output <DIR>` | Répertoire de sortie |

---

## Commandes de packaging

### package

Créer une archive depuis un pack.

```bash
sherpack package <PACK> [OPTIONS]
```

| Option | Description |
|--------|-------------|
| `-o, --output <FICHIER>` | Fichier de sortie |

### inspect

Afficher le contenu d'une archive.

```bash
sherpack inspect <ARCHIVE> [OPTIONS]
```

| Option | Description |
|--------|-------------|
| `--manifest` | Afficher le manifeste brut |
| `--checksums` | Afficher les checksums des fichiers |

### keygen

Générer une paire de clés de signature.

```bash
sherpack keygen [OPTIONS]
```

| Option | Description |
|--------|-------------|
| `-o, --output <DIR>` | Répertoire de sortie |
| `--no-password` | Ne pas chiffrer la clé privée |
| `--force` | Écraser les clés existantes |

### sign

Signer une archive.

```bash
sherpack sign <ARCHIVE> -k <CLÉ> [OPTIONS]
```

| Option | Description |
|--------|-------------|
| `-k, --key <FICHIER>` | Fichier de clé privée |
| `-c, --comment <TEXTE>` | Commentaire de confiance |

### verify

Vérifier une archive.

```bash
sherpack verify <ARCHIVE> [OPTIONS]
```

| Option | Description |
|--------|-------------|
| `-k, --key <FICHIER>` | Fichier de clé publique |
| `--require-signature` | Échouer si pas de signature |

---

## Commandes Kubernetes

### install

Installer un pack sur le cluster.

```bash
sherpack install <NOM> <PACK> [OPTIONS]
```

| Option | Description |
|--------|-------------|
| `-n, --namespace <NS>` | Namespace |
| `-f, --values <FICHIER>` | Fichier de valeurs |
| `--set <CLÉ=VALEUR>` | Override de valeurs |
| `--wait` | Attendre que les ressources soient prêtes |
| `--timeout <DURÉE>` | Timeout d'attente [défaut: 5m] |
| `--atomic` | Rollback en cas d'échec |
| `--dry-run` | Ne pas appliquer |
| `--create-namespace` | Créer le namespace |

### upgrade

Mettre à jour un release existant.

```bash
sherpack upgrade <NOM> <PACK> [OPTIONS]
```

| Option | Description |
|--------|-------------|
| `-n, --namespace <NS>` | Namespace |
| `-f, --values <FICHIER>` | Fichier de valeurs |
| `--set <CLÉ=VALEUR>` | Override de valeurs |
| `--wait` | Attendre que les ressources soient prêtes |
| `--timeout <DURÉE>` | Timeout d'attente |
| `--atomic` | Rollback en cas d'échec |
| `--dry-run` | Ne pas appliquer |
| `--diff` | Afficher le diff |
| `--reuse-values` | Réutiliser les valeurs précédentes |
| `--reset-values` | Réinitialiser aux valeurs par défaut |
| `--install` | Installer si n'existe pas |

### uninstall

Supprimer un release.

```bash
sherpack uninstall <NOM> [OPTIONS]
```

| Option | Description |
|--------|-------------|
| `-n, --namespace <NS>` | Namespace |
| `--keep-history` | Conserver les enregistrements du release |
| `--wait` | Attendre la suppression |
| `--dry-run` | Ne pas supprimer |

### rollback

Revenir à une révision précédente.

```bash
sherpack rollback <NOM> <RÉVISION> [OPTIONS]
```

| Option | Description |
|--------|-------------|
| `-n, --namespace <NS>` | Namespace |
| `--wait` | Attendre le rollback |
| `--dry-run` | Ne pas appliquer |

### list

Lister les releases.

```bash
sherpack list [OPTIONS]
```

| Option | Description |
|--------|-------------|
| `-n, --namespace <NS>` | Filtrer par namespace |
| `-A, --all-namespaces` | Tous les namespaces |
| `-a, --all` | Inclure les supersédés |
| `-o, --output <FMT>` | Format de sortie |

### history

Afficher l'historique d'un release.

```bash
sherpack history <NOM> [OPTIONS]
```

| Option | Description |
|--------|-------------|
| `-n, --namespace <NS>` | Namespace |
| `--max <N>` | Nombre maximum de révisions |

### status

Afficher le statut d'un release.

```bash
sherpack status <NOM> [OPTIONS]
```

| Option | Description |
|--------|-------------|
| `-n, --namespace <NS>` | Namespace |
| `--show-resources` | Afficher le statut des ressources |

### recover

Récupérer un release bloqué.

```bash
sherpack recover <NOM> [OPTIONS]
```

| Option | Description |
|--------|-------------|
| `-n, --namespace <NS>` | Namespace |

---

## Commandes de dépôt

### repo add

Ajouter un dépôt.

```bash
sherpack repo add <NOM> <URL> [OPTIONS]
```

| Option | Description |
|--------|-------------|
| `--username <USER>` | Nom d'utilisateur |
| `--password <PASS>` | Mot de passe |
| `--token <TOKEN>` | Token |

### repo list

Lister les dépôts.

```bash
sherpack repo list [OPTIONS]
```

| Option | Description |
|--------|-------------|
| `--auth` | Afficher le statut d'authentification |

### repo update

Mettre à jour l'index d'un dépôt.

```bash
sherpack repo update [NOM]
```

### repo remove

Supprimer un dépôt.

```bash
sherpack repo remove <NOM>
```

### search

Rechercher des packs.

```bash
sherpack search <REQUÊTE> [OPTIONS]
```

| Option | Description |
|--------|-------------|
| `-r, --repo <NOM>` | Rechercher dans un dépôt spécifique |
| `--versions` | Afficher toutes les versions |
| `--json` | Sortie JSON |

### pull

Télécharger un pack.

```bash
sherpack pull <PACK> [OPTIONS]
```

| Option | Description |
|--------|-------------|
| `--ver <VERSION>` | Version spécifique |
| `-o, --output <CHEMIN>` | Chemin de sortie |
| `--untar` | Extraire vers un répertoire |

### push

Pousser vers un registre OCI.

```bash
sherpack push <ARCHIVE> <DESTINATION>
```

---

## Commandes de dépendances

### dependency list

Lister les dépendances.

```bash
sherpack dependency list <PACK>
```

### dependency update

Résoudre et verrouiller les dépendances.

```bash
sherpack dependency update <PACK> [OPTIONS]
```

| Option | Description |
|--------|-------------|
| `--policy <POLICY>` | Politique de verrouillage |

### dependency build

Télécharger les dépendances verrouillées.

```bash
sherpack dependency build <PACK> [OPTIONS]
```

| Option | Description |
|--------|-------------|
| `--verify` | Vérifier les checksums |

### dependency tree

Afficher l'arbre des dépendances.

```bash
sherpack dependency tree <PACK>
```

---

## Codes de sortie

| Code | Signification |
|------|---------------|
| 0 | Succès |
| 1 | Erreur générale |
| 2 | Erreur de validation |
| 3 | Erreur de template |
| 4 | Erreur d'entrée/sortie |
| 5 | Erreur Kubernetes |
