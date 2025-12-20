---
id: signing
title: Signature
sidebar_position: 2
---

# Signer des Archives

Signez cryptographiquement les archives pour la sécurité de la chaîne d'approvisionnement.

## Vue d'Ensemble

Sherpack utilise [Minisign](https://jedisct1.github.io/minisign/) pour les signatures :

- Signatures petites et sécurisées
- Gestion simple des clés
- Commentaires de confiance pour les métadonnées

## Générer des Clés

```bash
sherpack keygen [options]
```

### Interactif (avec mot de passe)

```bash
sherpack keygen -o ~/.sherpack/keys
# Entrez le mot de passe à l'invite
```

### Non-interactif (CI/CD)

```bash
sherpack keygen -o ~/.sherpack/keys --no-password
```

### Sortie

```
Generating keypair...

Created:
  Secret key: ~/.sherpack/keys/sherpack.key
  Public key: ~/.sherpack/keys/sherpack.pub

⚠️  Keep sherpack.key secret! Distribute sherpack.pub to users.
```

## Signer des Archives

```bash
sherpack sign <archive> -k <secret-key>
```

### Signature de Base

```bash
sherpack sign mypack-1.0.0.tar.gz -k ~/.sherpack/keys/sherpack.key
```

### Avec Commentaire de Confiance

```bash
sherpack sign mypack-1.0.0.tar.gz \
  -k ~/.sherpack/keys/sherpack.key \
  -c "Release v1.0.0 - Production ready"
```

### Sortie

```
Signing: mypack-1.0.0.tar.gz
  Key ID: RW...

Created: mypack-1.0.0.tar.gz.minisig
  Trusted comment: Release v1.0.0 - Production ready
```

## Fichier de Signature

Le fichier `.minisig` contient :

```
untrusted comment: sherpack signature
RW...base64-signature...
trusted comment: Release v1.0.0 - Production ready
...signature-of-trusted-comment...
```

## Gestion des Clés

### Sécurité de la Clé Secrète

- Stockez dans un emplacement sécurisé (coffre-fort, stockage chiffré)
- Utilisez la protection par mot de passe pour la signature manuelle
- Effectuez une rotation périodique des clés
- Ne jamais committer dans le contrôle de version

### Distribution de la Clé Publique

Distribuez `sherpack.pub` via :

- Métadonnées du dépôt de paquets
- Documentation
- Point de terminaison dédié aux clés
- Contrôle de version (clés publiques uniquement)

### Configuration CI/CD

```yaml
# Exemple GitHub Actions
- name: Sign release
  env:
    SIGNING_KEY: ${{ secrets.SHERPACK_SIGNING_KEY }}
  run: |
    echo "$SIGNING_KEY" > /tmp/sherpack.key
    sherpack sign mypack-*.tar.gz -k /tmp/sherpack.key
    rm /tmp/sherpack.key
```

## Rotation des Clés

Lors de la rotation des clés :

1. Générez une nouvelle paire de clés
2. Signez les nouvelles versions avec la nouvelle clé
3. Gardez l'ancienne clé publique disponible pour la vérification
4. Documentez la date de transition de clé

```bash
# Générer de nouvelles clés avec un nom différent
sherpack keygen -o ~/.sherpack/keys-2024 --no-password
```

## Signatures Multiples

Pour les environnements à haute sécurité, exigez plusieurs signatures :

```bash
# Signer avec la première clé
sherpack sign mypack-1.0.0.tar.gz -k key1.key

# Signer avec la deuxième clé (signataire différent)
sherpack sign mypack-1.0.0.tar.gz -k key2.key -o mypack-1.0.0.tar.gz.sig2
```

Les utilisateurs vérifient avec les deux :

```bash
sherpack verify mypack-1.0.0.tar.gz -k key1.pub
sherpack verify mypack-1.0.0.tar.gz -k key2.pub --signature mypack-1.0.0.tar.gz.sig2
```
