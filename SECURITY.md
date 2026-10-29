# Security Policy

## Reporting a Vulnerability

If you discover a security vulnerability in Tylluan, **do not open a public issue**.

Instead, report it via **GitHub Private Vulnerability Reporting**: https://github.com/forja-orca/tylluan/security/advisories/new

Include:
- Description of the vulnerability
- Steps to reproduce
- Potential impact
- Suggested fix (if any)

We aim to acknowledge reports within 48 hours and provide a fix or mitigation plan within 7 days for critical issues.

## Supported Versions

| Version | Supported |
|---------|-----------|
| latest main | ✅ |
| older releases | Best effort |

## Security Model

For a detailed analysis of Tylluan's security posture, including its mapping against the [OWASP Top 10 for Agentic Applications (2026)](https://genai.owasp.org/resource/owasp-top-10-for-agentic-applications-for-2026/), see [docs/concepts/SECURITY.md](docs/concepts/SECURITY.md).

## Verifying a release

Every release publishes `checksums.txt` (SHA-256 of each archive) and a signed build-provenance attestation (GitHub / Sigstore, no keys to manage). To check a download:

```bash
sha256sum --check --ignore-missing checksums.txt
gh attestation verify tylluan-<version>-<target>.tar.gz -R Forja-orca/tylluan
```

The attestation proves the archive was built by this repository's release workflow and not altered afterwards. `tylluan update` already verifies the checksum before installing.

## Known Limitations

Tylluan is experimental research software. Key security gaps are documented in [DISCLAIMER.md](DISCLAIMER.md). The most significant:

1. No sandbox by default: bash/code guilds run with user privileges unless the opt-in Docker sandbox is enabled (`[security.sandbox]`, off by default)
2. The bearer token grants full access by default; an opt-in role ACL exists (`[security.acl]`, `default_role = "admin"` means no restriction until configured)
3. No automatic detection of rogue agents; a manual local kill switch exists (`POST /api/v1/admin/emergency-kill`)
