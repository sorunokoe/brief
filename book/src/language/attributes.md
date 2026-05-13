# Attributes & Decorators

## Decorators (before declarations)

| Decorator | Target | Effect |
|-----------|--------|--------|
| `@BriefBuilder` | `task` | Primary entry point |
| `@mcp <Server>` | `type` | MCP-server-backed skill |

## Field Attributes

| Attribute | Constraint | Error |
|-----------|-----------|-------|
| `@url` | Valid URL | E203 |
| `@nonempty` | Not empty | E203 |
| `@matches("re")` | Regex match | E203 |

## `@once` (Linear)

On `let`: binding must be consumed exactly once.
On effect return type: auto-linearizes all call sites.
