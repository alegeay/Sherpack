---
id: verification
title: Verification
sidebar_position: 3
---

# Verifying Archives

Verify archive integrity and authenticity before deployment.

## Verify Command

```bash
sherpack verify <archive> [options]
```

## Integrity Check

Always performed - verifies SHA256 checksums:

```bash
sherpack verify mypack-1.0.0.tar.gz
```

Output:

```
Verifying: mypack-1.0.0.tar.gz

Integrity check:     [OK] All file checksums match
Signature check:     [SKIP] No signature file found

Archive integrity verified
```

## Signature Verification

Verify with public key:

```bash
sherpack verify mypack-1.0.0.tar.gz -k sherpack.pub
```

Output:

```
Verifying: mypack-1.0.0.tar.gz

Integrity check:     [OK] All file checksums match
Signature check:     [OK] Signature valid
  Key ID: RW...
  Trusted comment: Release v1.0.0 - Production ready

Archive verified successfully
```

## Require Signature

Fail if no signature present:

```bash
sherpack verify mypack-1.0.0.tar.gz --require-signature
```

Output when missing:

```
Verifying: mypack-1.0.0.tar.gz

Integrity check:     [OK] All file checksums match
Signature check:     [FAIL] Signature required but not found

Error: Verification failed
```

## Verification Failures

### Integrity Failure

```
Integrity check:     [FAIL] Checksum mismatch
  templates/deployment.yaml:
    Expected: sha256:a1b2c3...
    Actual:   sha256:x9y8z7...

Error: Archive may be corrupted or tampered
```

### Signature Failure

```
Signature check:     [FAIL] Invalid signature
  The signature does not match the public key

Error: Archive may be tampered or signed with different key
```

### Wrong Key

```
Signature check:     [FAIL] Key mismatch
  Archive signed with: RWabc...
  Provided key:        RWxyz...

Error: Use the correct public key for this archive
```

## Inspect Without Verification

View contents without full verification:

```bash
sherpack inspect mypack-1.0.0.tar.gz
```

Output:

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

### Show Checksums

```bash
sherpack inspect mypack-1.0.0.tar.gz --checksums
```

### Show Raw Manifest

```bash
sherpack inspect mypack-1.0.0.tar.gz --manifest
```

## CI/CD Integration

### GitHub Actions

```yaml
- name: Verify pack
  run: |
    # Download public key
    curl -o sherpack.pub https://example.com/keys/sherpack.pub

    # Verify with required signature
    sherpack verify mypack-*.tar.gz -k sherpack.pub --require-signature
```

### GitLab CI

```yaml
verify:
  script:
    - sherpack verify mypack-*.tar.gz -k $SHERPACK_PUBLIC_KEY --require-signature
```

## Best Practices

1. **Always verify before install**
   ```bash
   sherpack verify mypack-1.0.0.tar.gz -k trusted.pub
   sherpack install myapp mypack-1.0.0.tar.gz
   ```

2. **Require signatures in production**
   ```bash
   sherpack verify archive.tar.gz --require-signature -k production.pub
   ```

3. **Pin public keys** - Don't fetch keys from untrusted sources

4. **Automate in CI/CD** - Make verification a required step
