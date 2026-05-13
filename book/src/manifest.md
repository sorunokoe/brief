# brief.toml Project Manifest

Every Brief project can have a `brief.toml` at its root. The manifest lets you:

- Name and version your project
- Override skill resolution paths
- List example files for CI

`brief check` walks up from the `.brief` file to find the nearest `brief.toml` automatically.

## Format

```toml
[project]
name    = "my-app"
version = "0.1.0"
authors = ["Your Name <you@example.com>"]

# Skills: name → directory (relative to this file)
[skills]
GraphQL  = ".claude/skills/GraphQL"
Auth     = ".claude/skills/Auth"
Payments = ".claude/skills/Payments"

# CI: which files to type-check in CI
[ci]
examples = [
    "briefs/auth-flow.brief",
    "briefs/checkout.brief",
]
```

## Sections

### `[project]`

| Field | Required | Default | Description |
|-------|----------|---------|-------------|
| `name` | ✅ | — | Project name |
| `version` | ❌ | `"0.1.0"` | Semver version |
| `authors` | ❌ | `[]` | Author list |

### `[skills]`

Maps skill names to their directory paths (relative to `brief.toml`).

When a `.brief` file does `import skill "GraphQL"`, `brief check` looks for:
1. The path from `[skills]` in `brief.toml` (if present)
2. `.claude/skills/GraphQL/GraphQL.briefskill` (default)

```toml
[skills]
GraphQL  = "infra/skills/GraphQL"
Auth     = "infra/skills/Auth"
```

### `[ci]`

Lists `.brief` files that CI should check.

```toml
[ci]
examples = ["hello.brief", "auth.brief"]
```

## Scaffold with `brief init`

Running `brief init my-project` generates a starter `brief.toml`:

```toml
[project]
name    = "my-project"
version = "0.1.0"
authors = []

[skills]
# GraphQL = ".claude/skills/GraphQL"

[ci]
examples = ["hello.brief"]
```
