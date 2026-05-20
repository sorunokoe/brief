# brief.toml Project Manifest

Every Brief project can have a `brief.toml` at its root. The manifest lets you:

- Name and version your project
- Override skill resolution paths
- Configure verifiers for dynamic annotations
- Control lock freshness policy
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

# Verifiers: annotation → verifier config
[verifiers."@url"]
skill = "builtin:url"          # ships with Brief

[verifiers."@github-repo"]
skill = "builtin:github-repo"  # ships with Brief — uses GITHUB_TOKEN if set

[verifiers."@local-path"]
skill = "builtin:local-path"   # ships with Brief — checks path exists

# Verification policy
[verify]
max_lock_age_hours = 24  # 0 = never expire (offline / air-gapped)
require_lock       = true

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

### `[verifiers."@annotation"]`

Routes a dynamic annotation to a verifier. Each entry is keyed by the annotation name (including the `@`).

| Field | Description |
|-------|-------------|
| `skill` | Built-in verifier name. `"builtin:url"`, `"builtin:local-path"`, `"builtin:github-repo"`, and `"builtin:shell-command"` ship with Brief. |
| `mcp_command` | Shell command to spawn a verifier MCP server. |
| `mcp_url` | URL of an already-running verifier MCP server. |

Exactly one of `skill`, `mcp_command`, or `mcp_url` should be present.

**Built-in verifiers (no npm required)**

| Builtin | What it checks |
|---------|----------------|
| `builtin:url` | URL is reachable via HTTP HEAD/GET. Blocks private IPs (SSRF guard). |
| `builtin:local-path` | File or directory exists on the local filesystem. |
| `builtin:github-repo` | GitHub repo is accessible via the GitHub API. Uses `GITHUB_TOKEN` env var if set. |
| `builtin:shell-command` | Command (or first token) is available in `PATH`. |

```toml
[verifiers."@url"]
skill = "builtin:url"

[verifiers."@github-repo"]
skill = "builtin:github-repo"

[verifiers."@local-path"]
skill = "builtin:local-path"

[verifiers."@shell-command"]
skill = "builtin:shell-command"
```

### `[verify]`

Controls lock freshness enforcement for `brief serve` and `brief check`.

| Field | Default | Description |
|-------|---------|-------------|
| `max_lock_age_hours` | `24` | How old a `.brief.lock` may be before `brief serve` refuses to start and `brief check` emits E303. Set to `0` to disable expiry. |
| `require_lock` | `true` | Whether `brief check` requires a lock file when dynamic annotations are present. |

```toml
[verify]
max_lock_age_hours = 48   # allow up to 48h old locks

# Disable expiry for offline / air-gapped environments:
# max_lock_age_hours = 0
```

### `[ci]`

Lists `.brief` files (or glob patterns) that `brief ci` should check.

```toml
[ci]
examples = [
  "examples/*.brief",       # glob: all .brief files in examples/
  "features/auth.brief",    # literal file
  "features/",              # directory: all .brief files inside
]
```

Running `brief ci` from the project root will:
1. Find the nearest `brief.toml` by walking up the directory tree
2. Expand all patterns in `examples`
3. Run `brief check` on each matched file
4. Exit `0` if all pass, `1` if any fail

This is the recommended way to integrate Brief into CI pipelines:

```yaml
# .github/workflows/ci.yml
- name: Brief CI checks
  run: brief ci
```

It replaces verbose shell loops and keeps the checked file list in version control (in `brief.toml`) rather than scattered across YAML files.

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
