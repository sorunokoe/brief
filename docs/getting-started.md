# Getting Started with Brief

Brief is a typed DSL that gives AI agents a verified, sealed context. Instead of hoping the AI uses the right tools, you declare exactly which skills it can use — and prove they work — before the AI ever connects.

**The workflow in one line:** write → check → verify → serve → AI connects.

---

## Install

```bash
git clone https://github.com/sorunokoe/brief
cd brief
cargo build --release
cp target/release/brief /usr/local/bin/
brief --version
```

---

## 5-Minute Walkthrough

### Step 1 — Write a task

Create `review-pr.brief`:

```brief
import skill "GitHub"
import skill "FileSystem"

task ReviewPR : TaskBrief uses [GitHub, FileSystem] {
    goal = "Fetch a pull request and write a review summary to a file"

    step FetchPR {
        let pr = perform GitHub.get_file_contents("owner", "repo", "CHANGELOG.md", "main")?;
    }

    step WriteReport {
        let _ = perform FileSystem.write_file("/workspace/review.md", "summary")?;
    }
}

test "boundary coverage" {
    let _ = perform GitHub.list_issues("owner", "repo", "open")?;
    let _ = perform GitHub.list_issues("owner", "repo", "closed")?;
    let _ = perform GitHub.list_issues("owner", "repo", "all")?;
}
```

### Typed Extras

When a task needs structured metadata or runtime inputs, prefer typed extras over the deprecated string-map form:

```brief
sealed type Platform = iOS | Android | Web

task ReviewPR : TaskBrief {
    goal = "Fetch a pull request and write a review summary to a file"

    extras {
        platform: Platform
        repository: String
    }
}
```

`extras { ... }` gives the compiler a schema it can validate. The old `extras = ["key": "value"]` form still parses, but it emits W103.

### Opaque Types

Use `opaque type` when a skill returns a value that Brief shouldn't inspect — typically compiler artifacts, SDK handles, or database cursors:

```brief
import skill "DatabasePrimitives"

opaque type Connection
opaque type QueryResult

sealed type DbOutcome = Connected(Connection) | ConnectionFailed(String)

@BriefBuilder
task QueryUsers : TaskBrief uses [DatabasePrimitives] {
    goal = "Open a database connection and run a user query"

    effects [network]

    provides {
        rowCount: Int
    }

    step Connect {
        let outcome = perform DatabasePrimitives.connect("postgres://localhost/app")?;
        let conn = match outcome {
            Connected(c)          => c
            ConnectionFailed(msg) => msg
        };
    }

    step RunQuery {
        pre { conn.isReady }

        let result = perform DatabasePrimitives.query(conn, "SELECT id FROM users")?;
        let rowCount = perform DatabasePrimitives.rowCount(result)?;
    }
}
```

Brief type-checks the entire task — `conn: Connection` and `result: QueryResult` are fully tracked — without needing to know the internal structure of either type.

### Match Expressions

Use `match` to branch on sealed type variants:

```brief
sealed type Environment = Production | Staging | Development

step Configure {
    let config = match environment {
        Production  => perform Config.loadProd()?
        Staging     => perform Config.loadStaging()?
        Development => perform Config.loadDev()?
    }
}
```

Brief checks that your match is exhaustive — if you forget a variant, you get `warning[E207]` with the missing variant names listed.

## Phase Contracts

Document step invariants with `pre` and `post` blocks:

```brief
step ProcessPayment {
    pre { amount > 0 }
    post { receipt.isValid }

    let receipt = perform PaymentService.charge(amount)?
}
```

## Effect Contracts

Declare what side-effects your task produces:

```brief
task FetchData : TaskBrief uses [NetworkService] {
    effects [network]
    ...
}
```

Missing effect declarations emit `error[E209]`.

## Workflow Combinators

Compose steps with `parallel`, `retry`, and `fallback`:

```brief
parallel { FetchUsers, FetchProducts }
retry(3) { SyncToBackend }
fallback { SyncPrimary, SyncFallback }
```

### Step 2 — Generate skill interfaces

Brief needs typed `.briefskill` interfaces to type-check your `perform` calls. Put skill docs in `.claude/skills/<Name>/README.md` with an `## Interface` section, then:

```bash
brief skillgen .claude/skills/GitHub/
brief skillgen .claude/skills/FileSystem/
```

Or skip this for now — `brief check` will warn (W101) but still pass.

### Step 3 — Type-check

```bash
brief check review-pr.brief
# ✅ All ingredients present. Ready for AI.
```

Fast, no network, CI-safe. Run this on every save with `brief watch review-pr.brief`.

### Step 4 — Configure brief.toml

Create `brief.toml` in your project root:

```toml
[skills.GitHub]
mcp_command = ["npx", "-y", "@modelcontextprotocol/server-github"]

[skills.FileSystem]
mcp_command = ["npx", "-y", "@modelcontextprotocol/server-filesystem", "/workspace"]

[verifiers."@url"]
skill = "builtin:url"          # ships with Brief

[verifiers."@local-path"]
skill = "builtin:local-path"   # ships with Brief — checks path exists
```

### Step 5 — Verify (seal the contract)

```bash
brief verify review-pr.brief
# ✅ Verified 3 annotations → review-pr.brief.lock
```

This runs your verifiers (URL checks, repo access, etc.) and writes `review-pr.brief.lock`. Commit this file — it proves the brief was valid at a point in time. The lock is invalidated automatically if you change the `.brief` source.

### Step 6 — Serve to AI

```bash
brief serve review-pr.brief
```

This starts an MCP server on stdio. Point Claude Code or GitHub Copilot at it:

**`.claude/mcp.json`:**
```json
{
  "mcpServers": {
    "review-pr": {
      "command": "brief",
      "args": ["serve", "review-pr.brief"]
    }
  }
}
```

The AI now only sees `GitHub.get_file_contents`, `GitHub.list_issues`, `GitHub.create_pull_request`, `FileSystem.write_file`, etc. — exactly the tools declared in `uses []`. Nothing else.

---

## Key Concepts

### The enforcement chain

```
brief check   — static types, ~0ms, no network
      ↓
brief verify  — runtime proofs, writes .brief.lock
      ↓
brief serve   — MCP server, requires valid lock, no lock = no server
```

Each step is a gate. You can't reach `serve` without a valid lock.

### Skill interfaces (`.briefskill`)

A `.briefskill` file declares the typed API of a skill:

```
interface GitHub {
    fn get_file_contents(owner: @nonEmpty String, repo: @nonEmpty String, path: String, ref: @nonEmpty String) -> FileContent
    fn list_issues(owner: @nonEmpty String, repo: @nonEmpty String, state: @enum("open","closed","all") String) -> IssueList
}
```

Generate from a skill's `README.md`:
```bash
brief skillgen .claude/skills/GitHub/
```

Or generate with an LLM (set `BRIEF_LLM_API_KEY`):
```bash
BRIEF_LLM_API_KEY=sk-... brief skillgen .claude/skills/GitHub/
```

### Annotations

| Annotation | Checked by | Meaning |
|------------|-----------|---------|
| `@nonEmpty` | `brief check` | String/collection must not be empty |
| `@range(1, 100)` | `brief check` | Integer must be in [1, 100] |
| `@enum("a","b")` | `brief check` | Must be one of the listed values |
| `@matches("regex")` | `brief check` | String must match pattern |
| `@url` | `brief verify` | URL must be reachable (HTTP HEAD/GET). The built-in `builtin:url` verifier blocks private/internal IPs (RFC-1918, 169.254.x, loopback). |
| `@local-path` | `brief verify` | File/directory must exist |
| `@github-repo` | `brief verify` | GitHub repo must be accessible |
| `@once` | `brief serve` | Handle identity tracked at runtime — the same handle value cannot be produced twice in a session (linear type) |

Static annotations (`@nonEmpty`, `@range`, `@enum`, `@matches`) are checked instantly. Dynamic annotations (`@url`, `@local-path`, etc.) require a configured verifier in `brief.toml` and are checked by `brief verify`.

---

## Commands

| Command | What it does |
|---------|-------------|
| `brief check <file>` | Type-check, fast, no network. Use in CI and on every save. |
| `brief verify <file>` | Run verifiers, write `.brief.lock`. Run when brief changes. |
| `brief serve <file>` | Start MCP server. Requires valid lock. This is what AI connects to. |
| `brief run <file>` | Execute the brief against real skill MCP servers directly. |
| `brief test <file>` | Run `test { }` blocks with mock skills. |
| `brief test <file> --live` | Run tests against real MCP servers (no mocks). |
| `brief gen "<desc>"` | Generate a `.brief` file from a natural language description. |
| `brief skillgen <path>` | Generate `.briefskill` from a skill directory's README. |
| `brief watch <file>` | Re-check on every save (dev loop). |
| `brief init <name>` | Scaffold a new Brief project. |
| `brief fmt <file>` | Auto-format to canonical style. |
| `brief doc <file>` | Generate Markdown documentation. |
| `brief ci` | Check all `[ci] examples` from `brief.toml`. |

---

## Generating a Brief with AI

If you have an LLM API key, Brief can write the `.brief` for you:

```bash
export BRIEF_LLM_API_KEY=sk-...
brief gen "fetch a PR, review changed files, post a comment with findings"
# → ReviewPR.brief  (3 attempts with compiler feedback)
```

The generator runs `brief check` on each attempt and feeds error codes back to the LLM until it compiles cleanly.

---

## Testing

Write `test { }` blocks directly in your `.brief` files:

```brief
test "lists open and closed issues" {
    mock GitHub {
        fn list_issues(owner, repo, state) -> Ok([Issue { id: "1", title: "Bug" }])
    }

    run ReviewPR.FetchPR
    assert performed GitHub.get_file_contents
    assert result is Ok
}
```

Run with mocks (fast, no network):
```bash
brief test review-pr.brief
```

Run against real skill servers (integration test):
```bash
brief test review-pr.brief --live
```

---

## CI Integration

```yaml
# .github/workflows/brief.yml
- name: Check briefs
  run: brief ci

- name: Format check
  run: brief fmt --check tasks/*.brief
```

### Compiler Self-Hosting CI

Brief's own compiler pipeline is described as type-checked Brief tasks. Validate them:

```bash
# Validate all 6 compiler pass Brief files
brief self-hosting check

# Compare Rust and Brief-mediated pipelines
brief self-hosting compare examples/01-book-flight.brief
```

`brief.toml`:
```toml
[ci]
examples = [
    "tasks/review-pr.brief",
    "tasks/deploy.brief",
]
```

---

## Reference Skills

Five complete reference skills live in [`examples/skills/`](../examples/skills/):

| Skill | Purpose | Key annotations |
|-------|---------|----------------|
| `FileSystem` | Read/write local files | `@local-path` |
| `GitHub` | GitHub API — files, PRs, issues | `@github-repo`, `@nonEmpty`, `@enum` |
| `WebSearch` | Web search and page fetching | `@url`, `@range` |
| `Memory` | Key-value store with TTL | `@nonEmpty`, `@range` |
| `Shell` | Run shell commands | `@shell-command`, `@enum`, `@range` |

Each includes a `.briefskill` interface, `README.md`, and a working `.brief` example.

Copy `examples/skills/brief.toml` as a starting point for your project.

---

## Error Codes

| Code | When | Fix |
|------|------|-----|
| `E101` | Missing `goal` field | Add `goal = "..."` to the task |
| `E102` | Skill in `uses` but not imported | Add `import skill "..."` |
| `E103` | `perform` calls skill not in `uses` | Add skill to `uses [...]` |
| `E107` | No `.briefskill` file found | Run `brief skillgen .claude/skills/<Name>/` |
| `E207` | Match on a sealed type is not exhaustive | Add the missing variants or a trailing `_` arm |
| `E211` | Field access on opaque type | Remove the field access — opaque types are abstract |
| `W105` | `opaque type` declared but never used | Reference it in a sealed type or task extras |
| `E301` | `@range` boundary not in test block | Add `perform` calls with min/max values in a `test` block |
| `E302` | `@enum` value not in test block | Add `perform` calls for each enum value in a `test` block |
| `E303` | `.brief.lock` missing or stale | Run `brief verify` |
| `E309` | Annotation has no verifier | Add `[verifiers."@annotation"]` to `brief.toml` |
