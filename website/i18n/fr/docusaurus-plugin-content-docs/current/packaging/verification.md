---
id: verification
title: Vérification
sidebar_position: 3
---

# Vérifier des Archives

Vérifiez l'intégrité et l'authenticité de l'archive avant le déploiement.

## Commande Verify

```bash
sherpack verify <archive> [options]
```

## Vérification d'Intégrité

Toujours effectuée - vérifie les sommes de contrôle SHA256 :

```bash
sherpack verify mypack-1.0.0.tar.gz
```

Sortie :

```
Verifying: mypack-1.0.0.tar.gz

Integrity check:     [OK] All file checksums match
Signature check:     [SKIP] No signature file found

Archive integrity verified
```

## Vérification de Signature

Vérifiez avec la clé publique :

```bash
sherpack verify mypack-1.0.0.tar.gz -k sherpack.pub
```

Sortie :

```
Verifying: mypack-1.0.0.tar.gz

Integrity check:     [OK] All file checksums match
Signature check:     [OK] Signature valid
  Key ID: RW...
  Trusted comment: Release v1.0.0 - Production ready

Archive verified successfully
```

## Exiger une Signature

Échoue si aucune signature n'est présente :

```bash
sherpack verify mypack-1.0.0.tar.gz --require-signature
```

Sortie en cas d'absence :

```
Verifying: mypack-1.0.0.tar.gz

Integrity check:     [OK] All file checksums match
Signature check:     [FAIL] Signature required but not found

Error: Verification failed
```

## Échecs de Vérification

### Échec d'Intégrité

```
Integrity check:     [FAIL] Checksum mismatch
  templates/deployment.yaml:
    Expected: sha256:a1b2c3...
    Actual:   sha256:x9y8z7...

Error: Archive may be corrupted or tampered
```

### Échec de Signature

```
Signature check:     [FAIL] Invalid signature
  The signature does not match the public key

Error: Archive may be tampered or signed with different key
```

### Mauvaise Clé

```
Signature check:     [FAIL] Key mismatch
  Archive signed with: RWabc...
  Provided key:        RWxyz...

Error: Use the correct public key for this archive
```

## Inspecter sans Vérification

Voir le contenu sans vérification complète :

```bash
sherpack inspect mypack-1.0.0.tar.gz
```

Sortie :

```
Archive: mypack-1.0.0.tar.gz (4.2 KB)

Pack: mypack
Version: 1.0.0
Description: My application

Files:
  MANIFEST                    234 B
  Pack.yaml                   156 B
  values.yaml                 892 B
  templates/deployment.yaml   1.8 KB
  templates/service.yaml      456 B

Digest: sha256:a1b2c3d4...
```

### Afficher les Sommes de Contrôle

```bash
sherpack inspect mypack-1.0.0.tar.gz --checksums
```

### Afficher le Manifest Brut

```bash
sherpack inspect mypack-1.0.0.tar.gz --manifest
```

## Intégration CI/CD

### GitHub Actions

```yaml
- name: Verify pack
  run: |
    # Télécharger la clé publique
    curl -o sherpack.pub https://example.com/keys/sherpack.pub

    # Vérifier avec signature obligatoire
    sherpack verify mypack-*.tar.gz -k sherpack.pub --require-signature
```

### GitLab CI

```yaml
verify:
  script:
    - sherpack verify mypack-*.tar.gz -k $SHERPACK_PUBLIC_KEY --require-signature
```

## Bonnes Pratiques

1. **Toujours vérifier avant l'installation**
   ```bash
   sherpack verify mypack-1.0.0.tar.gz -k trusted.pub
   sherpack install myapp mypack-1.0.0.tar.gz
   ```

2. **Exiger des signatures en production**
   ```bash
   sherpack verify archive.tar.gz --require-signature -k production.pub
   ```

3. **Épingler les clés publiques** - Ne récupérez pas les clés depuis des sources non fiables

4. **Automatiser dans le CI/CD** - Faites de la vérification une étape obligatoire
