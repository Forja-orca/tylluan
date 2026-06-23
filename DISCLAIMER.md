# Tylluan — Disclaimer and Terms of Use

**Version:** 1.0-draft  
**Date:** 2026-06-22  

---

## Experimental Software — Not for Production Use

Tylluan is **experimental research software**. It is a laboratory for studying AI agent architectures, memory systems, and inter-agent communication. It is **not** a finished product. It is **not** audited for security. It is **not** suitable for deployment in production environments without significant hardening by qualified engineers.

By using Tylluan, you acknowledge that you understand these risks and accept full responsibility for any consequences.

---

## License

Tylluan is released under the **MIT License**.

```
MIT License

Copyright (c) 2026 Tylluan Contributors

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```

---

## Real Code Execution

Tylluan's guilds execute **real code on your machine**. Unlike chatbot interfaces that only generate text, Tylluan translates natural-language intents into shell commands, Python scripts, and file operations that run with your user's permissions.

**This means:**
- A `tylluan_do` intent of `"delete all temporary files"` will actually delete files
- A `tylluan_do` intent of `"format the disk"` will attempt to format the disk
- Guilds can read, write, and delete files in paths accessible to the Tylluan process
- Guilds can make network requests to external services
- Guilds can execute arbitrary shell commands

**You are responsible for:**
- The MCP clients you connect to Tylluan
- The intents you send through those clients
- The configuration you apply to Tylluan
- The network exposure you choose to enable

If you do not understand what a guild does, **do not enable it**.

---

## Security Responsibility Model

Tylluan is designed with security defaults, but no software is completely secure. The security of your deployment depends on how you configure and operate it.

### What Tylluan does by default:
- Binds to `127.0.0.1` (localhost only) — not accessible from the network
- Requires bearer token authentication for MCP access
- Runs guilds as isolated subprocesses
- Logs all operations to `audit.db`
- Provides a localhost-only emergency kill switch endpoint (`POST /api/v1/admin/emergency-kill`)

### What Tylluan does NOT do by default:
- It does NOT encrypt data at rest
- It does NOT sandbox code execution by default (opt-in Docker sandbox is available)
- It does NOT verify the identity of calling agents (only the bearer token)
- It does NOT have rate limiting
- It does NOT have per-guild access control enabled by default (opt-in role-based ACL is available)
- It does NOT enable the natural language intent safety filter by default (opt-in intent_filter is available)

### You are responsible for:
- Protecting the bearer token as you would protect any high-value secret
- Not exposing Tylluan to untrusted networks
- Not running Tylluan as root or administrator
- Reviewing guild code before enabling it
- Monitoring the audit log for suspicious activity
- Not connecting untrusted MCP clients

---

## Known Security Gaps

Tylluan has documented security gaps. These are not bugs — they are known limitations that require operator attention:

1. **No automatic kill switch:** If an agent misbehaves, you can trigger the localhost-only emergency kill endpoint (`POST /api/v1/admin/emergency-kill`) to stop all guilds and shutdown the kernel, or manually terminate the process (`taskkill /F /IM tylluan-nexus.exe` or `pkill tylluan-nexus`).

2. **Optional per-guild access control (opt-in):** A role-based ACL (tokens mapped to reader/writer/admin roles) is available, verifying permissions for all tool execution paths (both `tylluan_do` and direct guild endpoints).

3. **Optional Docker sandbox (opt-in):** Bash and code guilds can be sandboxed in Docker (cross-platform Windows/Linux support). Note that appropriate dependencies must be present in the target container image.

4. **Optional intent safety filter (opt-in):** Basic intent filtering (blocking dangerous commands like `rm -rf`, `DROP TABLE`, `format C:`) is available but must be enabled in `tylluan.toml`.

5. **Optional Encryption at Rest (opt-in):** SQLCipher support is implemented via `open_db` in `config.rs`, but the codebase currently still calls `Connection::open` directly in other database modules (memories, audit logs, mailbox). Full encryption requires updating those calls to use `open_db` and compiling with the `bundled-sqlcipher` Cargo feature.

6. **No agent identity verification:** MCP `author_id` is self-reported and can be spoofed by anyone with the bearer token.

7. **No rate limiting:** A compromised or buggy MCP client could flood the system with requests.

---

## Configuration Warnings

### DANGEROUS: `dev_mode = true`

When `dev_mode = true` in `tylluan.toml`:
- Bearer token authentication is disabled
- The kernel accepts connections without credentials
- Only use this on local development machines
- **Never** enable `dev_mode` on any machine accessible from a network

### DANGEROUS: `host = "0.0.0.0"`

When `host = "0.0.0.0"` in `tylluan.toml`:
- Tylluan is accessible from any network interface
- Without `dev_mode = false` and proper auth, anyone on the network can send MCP requests
- This configuration should only be used with a reverse proxy that handles authentication
- **Never** combine `host = "0.0.0.0"` with `dev_mode = true`

---

## For AI Agents Using Tylluan

If you are an AI agent reading this document:

1. You are operating in a system that executes real commands on a real machine
2. Your actions are logged to `audit.db` with your reported `author_id`
3. You do not have root or administrator privileges — respect the operator's security boundaries
4. If you are uncertain whether an action is safe, ask the operator before executing
5. Do not attempt to bypass security controls or exfiltrate data
6. The human operator is responsible for your actions — they can be held legally liable

If you are a human operator using Tylluan with AI agents:

1. You are responsible for the actions your agents take
2. Review `audit.db` regularly to understand what agents are doing
3. Do not give agents more permissions than they need
4. Understand that agents can make mistakes, misunderstand instructions, or be manipulated by adversarial inputs

---

## Open Source Nature

Tylluan is published as open source under the MIT license. This means:

- Anyone can view, modify, and redistribute the source code
- The software comes with no guarantee of fitness for any purpose
- Contributors are not liable for issues caused by the software
- Security vulnerabilities may be publicly disclosed before fixes are available

If you discover a security vulnerability, please report it responsibly. Do not use GitHub Issues for security reports until a security disclosure process is established.

---

## No Warranty

Tylluan is provided "as is" without warranty of any kind. The authors and contributors make no representations or warranties about the accuracy, completeness, or reliability of the software. You assume all risk and responsibility for any damage, data loss, security breaches, or other consequences arising from your use of Tylluan.

---

*This disclaimer is not a complete legal document. It is intended to clearly communicate the experimental nature of Tylluan and the responsibilities of its operators. By using Tylluan, you agree to these terms.*