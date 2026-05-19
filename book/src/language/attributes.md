# Attributes & Decorators

## Decorators (before declarations)

| Decorator | Target | Effect |
|-----------|--------|--------|
| `@BriefBuilder` | `task` | Primary entry point |
| `@mcp <Server>` | `type` | MCP-server-backed skill |

## Field Attributes

| Attribute | Constraint | Error |
|-----------|-----------|-------|
| `@url` | Valid, publicly reachable URL | E203 |
| `@nonempty` | Not empty | E203 |
| `@matches("re")` | Regex match | E203 |

### `@url` — URL reachability

`@url` values are verified at `brief verify` time using the configured `[verifiers."@url"]` (or `builtin:url` if set). The built-in verifier performs an HTTP HEAD/GET and **blocks private and reserved IP ranges** (RFC-1918, link-local 169.254.x, loopback, IPv6 unique-local) to prevent SSRF. Redirects are disabled for the same reason. Configure in `brief.toml`:

```toml
[verifiers."@url"]
skill = "builtin:url"
```

## `@once` (Linear)

On `let`: binding must be consumed exactly once.
On effect return type: auto-linearizes all call sites.

`brief serve` also enforces `@once` at the MCP protocol level — see [Linear Types](linear-types.md).
