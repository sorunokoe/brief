<div align="center">

<pre>
╔══════════════════════════════════════════════════╗
║                                                  ║
║                B   R   I   E   F                 ║
║                · · · · · · · · ·                 ║
║        typed · contracts · for AI agents         ║
║                                                  ║
╚══════════════════════════════════════════════════╝
</pre>

[![CI](https://github.com/sorunokoe/brief/actions/workflows/ci.yml/badge.svg)](https://github.com/sorunokoe/brief/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/sorunokoe/brief)](https://github.com/sorunokoe/brief/releases/latest)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.75%2B-orange.svg)](https://www.rust-lang.org)

*If it compiles and verifies, the AI has everything it needs.*

</div>

---

## The Problem

You give an AI agent access to MCP tools. Then things go wrong.

```
  prompt: "close stale issues in the repo"

  AI calls: github_delete_repository("my-prod-repo")   ← you never said it could do this
            github_merge_pull_request(999)              ← hallucinated a tool name
            stripe_refund(charge_id)                   ← skipped: STRIPE_KEY was unset
```

The issue isn't the AI. **The issue is there's no contract.**

You handed the AI a toolbox with no instructions — it used every tool it could find.

---

## The Solution

Brief is a typed DSL that defines exactly what an AI agent is allowed to do, and enforces it at three layers before the AI touches anything.

```brief
import skill "GitHub"

task CloseStaleIssues : TaskBrief uses [GitHub] {
    goal = "Close issues with the 'stale' label"

    needs {
        env "GITHUB_TOKEN"              // verified before the AI starts
    }

    forbids {
        func "GitHub.delete_repository" // hidden from tools/list at runtime
        func "GitHub.merge_pull_request"
    }

    step Find {
        let issues = perform GitHub.list_issues(
            @github-repo "owner/repo",
            @enum("open", "closed") "open"
        )?
    }

    step Close {
        perform GitHub.close_issue(issues.stale_ids)?
    }
}
```

```
  $ brief check task.brief    →  ✅  type-safe, scope validated       (< 1s)
  $ brief verify task.brief   →  ✅  GitHub reachable, token set      (sealed)
  $ brief serve task.brief    →  ✅  MCP server ready, AI connects
```

The AI sees only `list_issues` and `close_issue`. Nothing else exists.

---

## How It Enforces

Three layers. None are skippable.

```
  ┌─────────────────────────────────────────────────────────────┐
  │  brief check  ·  instant  ·  no network                     │
  │                                                             │
  │  · type-checks the .brief file                              │
  │  · validates literals, goal coverage, and step data-flow    │
  │  · E420/E421/E501/E503 catch scope and policy contradictions│
  │  · W410/W411 flag unused or unpropagated perform results    │
  ├─────────────────────────────────────────────────────────────┤
  │  brief verify  ·  once  ·  ~5–30s                           │
  │                                                             │
  │  · calls configured verifiers (URL, GitHub, Figma…)        │
  │  · checks needs {} — env vars, feature flags, config keys   │
  │  · writes .brief.lock  →  committed to git                  │
  │  · lock is invalidated automatically if source changes      │
  ├─────────────────────────────────────────────────────────────┤
  │  brief serve  ·  requires a valid .brief.lock               │
  │                                                             │
  │  · spawns MCP skill servers from brief.toml                 │
  │  · AI only sees tools declared in uses []                   │
  │  · forbidden tools are absent from tools/list               │
  │  · @once enforced at the protocol level                     │
  └─────────────────────────────────────────────────────────────┘
```

---

## Install

```bash
git clone https://github.com/sorunokoe/brief
cd brief && cargo build --release
cp target/release/brief /usr/local/bin/
```

Then scaffold a project:

```bash
brief init my-agent && cd my-agent
brief check task.brief
```

---

## Examples

### Guard a payment flow

```brief
task Checkout : TaskBrief uses [Cart, Payment, Order] {
    goal = "Charge the customer and create an order"

    needs {
        env "STRIPE_SECRET_KEY"
        feature "checkout_v2"
    }

    forbids {
        skill "Database"          // use Cart abstraction, not raw SQL
        func "Payment.refund"     // checkout never refunds
    }

    step Charge {
        @once let charge = perform Payment.charge(cart.total)?
    }

    step Confirm {
        @once let order = perform Order.create(charge.id)?
    }
}
```

```
  $ brief check task.brief
    E421  Payment.refund is forbidden  →  remove from this task

  $ brief verify task.brief
    E411  needs: STRIPE_SECRET_KEY is not set  →  export STRIPE_SECRET_KEY=...
```

---

### Lock down a code review agent

```brief
import skill "GitHub"
import skill "FileSystem"

task ReviewPR : TaskBrief uses [GitHub, FileSystem] {
    goal = "Read a PR diff and post a review comment"

    needs {
        env "GITHUB_TOKEN"
    }

    forbids {
        func "GitHub.merge_pull_request"   // review only, never merge
        func "GitHub.delete_branch"
        func "FileSystem.write_file"       // read-only
    }

    step Read {
        let diff = perform GitHub.get_pull_request(
            @github-repo "owner/repo", 42
        )?
    }

    step Comment {
        perform GitHub.create_review_comment(diff.id, "LGTM")?
    }
}
```

---

### Verify resources exist before the AI starts

```brief
import skill "FileSystem"
import skill "Shell"

task BuildAndDeploy : TaskBrief uses [FileSystem, Shell] {
    goal = "Build the project and deploy the artifact"

    needs {
        env "DEPLOY_TARGET"
        config "build.output_dir"
    }

    step Build {
        perform Shell.run_command(
            @shell-command "cargo",
            @enum("build", "test", "check") "build"
        )?
    }

    step Deploy {
        let artifact = perform FileSystem.read_file(
            @local-path "target/release/myapp"
        )?
    }

    test "allowed commands" {
        let _ = perform Shell.run_command("cargo", "build")?
        let _ = perform Shell.run_command("cargo", "test")?
        let _ = perform Shell.run_command("cargo", "check")?
    }
}
```

The `@shell-command` annotation is verified by `brief verify` — it confirms `cargo` is in `PATH` before the AI runs a single step. The `@enum` annotation is verified statically by `brief check` — the test block proves every allowed value is covered.

---

## Commands

| Command | Description |
|---------|-------------|
| `brief check <file>` | Type-check — instant, no network, CI-safe |
| `brief check <file> --report` | Emit machine-readable JSON to stdout for CI/tooling |
| `brief verify <file>` | Seal the contract → writes `.brief.lock` |
| `brief serve <file>` | Start MCP server (requires valid lock) |
| `brief serve <file> --draft` | Start MCP server — scope enforced, no lock required |
| `brief run <file>` | Execute against real MCP skill servers |
| `brief test <file>` | Run `test {}` blocks with mock skill system |
| `brief test <file> --live` | Same, but use real MCP calls |
| `brief gen "<desc>"` | Generate `.brief` from a description |
| `brief skillgen <path>` | Generate `.briefskill` from a README |
| `brief fmt <file>` | Auto-format to canonical style |
| `brief doc <file>` | Generate Markdown documentation |
| `brief watch <file>` | Live re-check on every save |
| `brief init <name>` | Scaffold a new Brief project |
| `brief ci` | Check all CI examples from `brief.toml` |
| `brief lsp` | Start LSP server for editor integration |
| `brief audit` | Inspect the runtime call log |
| `brief suggest <file>` | AI-powered fix suggestions |

---

## Language Features

| Feature | What it does |
|---------|-------------|
| `import skill "X"` + `uses [X]` | Typed skill import — only declared skills exist |
| `needs { env "KEY" }` | Prerequisite check — verified before AI starts |
| `forbids { func "Skill.fn" }` | Scope boundary — hidden from `tools/list` at runtime |
| `@range(1, 100)` · `@enum("a","b")` | Static constraints — checked by `brief check` |
| `@url` · `@github-repo` · `@local-path` | Dynamic annotations — verified by `brief verify` |
| `@once let x = perform ...` | Linear type — enforced at protocol level by `brief serve` |
| `test "name" { }` | Coverage block — `brief check` validates every literal |
| `allow {}` · `deny {}` | Argument-level policy enforcement |
| `///` doc comments in `.briefskill` | Function descriptions for E501/E503 goal coverage |
| `type FX = [Auth, Session]` | Effect group aliases |

---

## Error Codes

| Code | Phase | Meaning |
|------|-------|---------|
| `E107` | check | Missing `.briefskill` interface file |
| `E301` | check | `@range` boundary literal missing in `test {}` |
| `E302` | check | `@enum` value literal missing in `test {}` |
| `E303` | check | `.brief.lock` missing, stale, or source-changed |
| `E309` | check | Dynamic annotation has no configured verifier |
| `E401` | verify | Skill function not found in live MCP server |
| `E402` | verify | Skill MCP server unreachable |
| `E411` | verify | `needs {}` prerequisite not met |
| `E420` | check | `forbids { skill "X" }` — forbidden skill used |
| `E421` | check | `forbids { func "Skill.fn" }` — forbidden function called |
| `E423` | check | `allow{}` / `deny{}` pattern references unknown skill or function |
| `E424` | check | `allow {}` with zero patterns blocks all calls |
| `E501` | check | Goal mentions capability not covered by any `uses[]` function |
| `E503` | check | Goal intent contradicts `deny{}` policy |
| `W408` | check | `deny{}` pattern subsumes an `allow{}` pattern |
| `W409` | check | `allow{}` function with sensitive arg names left unconstrained |
| `W410` | check | `let x = perform ...` result is never used |
| `W411` | check | `perform X` returns `Result<T>` but no `?` propagation is used |

---

## How Skills Work

```
  .claude/skills/GitHub/
  ├── GitHub.briefskill   ←  typed interface (like a .d.ts)
  ├── README.md           ←  source for `brief skillgen`
  └──  (no code)          ←  implementation is an MCP server in brief.toml
```

```toml
# brief.toml
[skills.GitHub]
mcp_command = ["npx", "-y", "@modelcontextprotocol/server-github"]

[verifiers."@github-repo"]
skill = "builtin:github-repo"

[verifiers."@url"]
skill = "builtin:url"
```

Brief ships with four built-in verifiers: `builtin:url`, `builtin:local-path`, `builtin:github-repo`, `builtin:shell-command`. Everything else is a plugin — write your own as any MCP server.

---

## Contributing

See [CONTRIBUTING.md](.github/CONTRIBUTING.md) and [CODE_OF_CONDUCT.md](.github/CODE_OF_CONDUCT.md).

Skill and verifier authors are especially welcome — Brief is designed to grow through community-built extensions.

```bash
git clone https://github.com/sorunokoe/brief
cargo test    # all tests must pass
```

## License

MIT — see [LICENSE](LICENSE).
