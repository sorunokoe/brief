# Security Policy

## Supported Versions

| Version | Supported |
|---------|-----------|
| 1.x     | ✅        |
| < 1.0   | ❌        |

## Reporting a Vulnerability

**Please do not report security vulnerabilities through public GitHub issues.**

Report vulnerabilities privately via [GitHub Security Advisories](https://github.com/sorunokoe/brief/security/advisories/new).

Include:
- A description of the vulnerability
- Steps to reproduce
- Potential impact
- Suggested fix (if any)

You will receive a response within **48 hours** and a fix timeline within **7 days** for confirmed issues.

## Security Model

Brief is a static analysis and MCP proxy tool. Key security boundaries:

- `brief check` — pure static analysis, reads files only, no network, no execution
- `brief verify` — makes outbound HTTP requests to configured verifier endpoints; URLs come from `brief.toml` which is under user control
- `brief serve` — spawns MCP server subprocesses declared in `brief.toml`; treat your `brief.toml` as trusted configuration

**Do not load `brief.toml` files from untrusted sources.** The `mcp_command` field executes arbitrary subprocesses.
