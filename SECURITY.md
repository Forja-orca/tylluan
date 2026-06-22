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

For a detailed analysis of Tylluan's security posture, including its mapping against the [OWASP Top 10 for Agentic Applications (2026)](https://genai.owasp.org/resource/owasp-top-10-for-agentic-applications-for-2026/), see [docs/architecture/SECURITY.md](docs/architecture/SECURITY.md).

## Known Limitations

Tylluan is experimental research software. Key security gaps are documented in [DISCLAIMER.md](DISCLAIMER.md). The most significant:

1. No code execution sandbox (bash/code guilds run with user privileges)
2. No per-agent access control (bearer token grants full access)
3. No automatic kill switch for rogue agents
