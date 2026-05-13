# Types

## Primitives

| Type | Description |
|------|-------------|
| `String` | UTF-8 text |
| `Int` | 64-bit integer |
| `Float` | 64-bit float |
| `Bool` | Boolean |
| `Unit` | No value |

## Type Attributes

| Attribute | Applies to | Constraint |
|-----------|-----------|------------|
| `@url` | `String` | Valid URL |
| `@nonempty` | `String` | Not empty |
| `@matches("re")` | `String` | Matches regex |
| `@once` | any | Linear — consumed once |
| `@mcp` | type alias | MCP-server-backed |

```brief
struct Profile {
    @url      avatarUrl:   String
    @nonempty displayName: String
}
```

See sub-pages for [sealed types](sealed-types.md), [structs](structs.md), [type aliases](type-aliases.md), and [linear types](linear-types.md).
