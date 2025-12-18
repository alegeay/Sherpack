---
id: signing
title: Signing
sidebar_position: 2
---

# Signing Archives

Cryptographically sign archives for supply chain security.

## Overview

Sherpack uses [Minisign](https://jedisct1.github.io/minisign/) for signatures:

- Small, secure signatures
- Simple key management
- Trusted comments for metadata

## Generate Keys

```bash
sherpack keygen [options]
```

### Interactive (with password)

```bash
sherpack keygen -o ~/.sherpack/keys
# Enter password when prompted
```

### Non-interactive (CI/CD)

```bash
sherpack keygen -o ~/.sherpack/keys --no-password
```

### Output

```
Generating keypair...

Created:
  Secret key: ~/.sherpack/keys/sherpack.key
  Public key: ~/.sherpack/keys/sherpack.pub

⚠️  Keep sherpack.key secret! Distribute sherpack.pub to users.
```

## Sign Archives

```bash
sherpack sign <archive> -k <secret-key>
```

### Basic Signing

```bash
sherpack sign mypack-1.0.0.tar.gz -k ~/.sherpack/keys/sherpack.key
```

### With Trusted Comment

```bash
sherpack sign mypack-1.0.0.tar.gz \
  -k ~/.sherpack/keys/sherpack.key \
  -c "Release v1.0.0 - Production ready"
```

### Output

```
Signing: mypack-1.0.0.tar.gz
  Key ID: RW...

Created: mypack-1.0.0.tar.gz.minisig
  Trusted comment: Release v1.0.0 - Production ready
```

## Signature File

The `.minisig` file contains:

```
untrusted comment: sherpack signature
RW...base64-signature...
trusted comment: Release v1.0.0 - Production ready
...signature-of-trusted-comment...
```

## Key Management

### Secret Key Security

- Store in secure location (vault, encrypted storage)
- Use password protection for manual signing
- Rotate keys periodically
- Never commit to version control

### Public Key Distribution

Distribute `sherpack.pub` via:

- Package repository metadata
- Documentation
- Dedicated keys endpoint
- Version control (public keys only)

### CI/CD Setup

```yaml
# GitHub Actions example
- name: Sign release
  env:
    SIGNING_KEY: ${{ secrets.SHERPACK_SIGNING_KEY }}
  run: |
    echo "$SIGNING_KEY" > /tmp/sherpack.key
    sherpack sign mypack-*.tar.gz -k /tmp/sherpack.key
    rm /tmp/sherpack.key
```

## Key Rotation

When rotating keys:

1. Generate new keypair
2. Sign new releases with new key
3. Keep old public key available for verification
4. Document key transition date

```bash
# Generate new keys with different name
sherpack keygen -o ~/.sherpack/keys-2024 --no-password
```

## Multiple Signatures

For high-security environments, require multiple signatures:

```bash
# Sign with first key
sherpack sign mypack-1.0.0.tar.gz -k key1.key

# Sign with second key (different signer)
sherpack sign mypack-1.0.0.tar.gz -k key2.key -o mypack-1.0.0.tar.gz.sig2
```

Users verify with both:

```bash
sherpack verify mypack-1.0.0.tar.gz -k key1.pub
sherpack verify mypack-1.0.0.tar.gz -k key2.pub --signature mypack-1.0.0.tar.gz.sig2
```
