# Brief

> **If it compiles AND verifies, the AI has everything it needs.**

Brief is a typed DSL for AI agent task workflows. It is the boundary between humans and AI agents: a typed, sealed contract that an AI cannot bypass.

```
brief check   → fast static analysis (< 1s, no network, CI-safe)
brief verify  → seal the contract (runs verifiers, writes .brief.lock)
brief serve   → start MCP server (requires valid .brief.lock)
```

The AI gets a sealed, verified context — not a hopeful template.

## The Enforcement Chain

```
You write:  task.brief  +  .briefskill interfaces  +  brief.toml
              ↓ (instant, <1s)
brief check → E-codes → iterate until clean
              ↓ (once, ~5-30s — only when something changes)
brief verify → calls configured verifiers (URL, Figma, Stripe, k8s, custom…)
             → writes .brief.lock  ← committed to git; invalidated on source change
              ↓ (requires valid .brief.lock)
brief serve  → MCP server: ONLY tools in uses[] exist
             → @once enforced at protocol level
             → AI cannot hallucinate tool names
```

No network in `brief check`. No skippable steps. No compromises.

## Quick Start

```bash
# Build from source (requires Rust)
git clone https://github.com/yourusername/brief
cd brief && cargo build --release
cp target/release/brief /usr/local/bin/
```

```brief
// hello.brief
task Hello : TaskBrief {
    goal = "Say hello to the world"
}
```

```bash
brief check hello.brief     # ✅ instant type check
brief verify hello.brief    # ✅ seals the contract → hello.brief.lock
brief serve hello.brief     # ✅ starts MCP server (Claude connects here)
```

## A Real Workflow Brief

```brief
import skill "DesignSystem"
import skill "GraphQL"

@BriefBuilder
task ProfileScreen : TaskBrief uses [DesignSystem, GraphQL] {
    goal   = "Display user profile with design-system components"

    step FetchData {
        let user = perform GraphQL.query(UserProfileQuery)?
    }

    step Render {
        @once let card = perform DesignSystem.profileCard(user)?
        display(card)
    }
}

test "boundary coverage" {
    // @range coverage: brief check verifies these literals exist
    let r1 = perform GraphQL.query(UserProfileQuery)?;
}
```

```toml
# brief.toml — routing layer
[skills.DesignSystem]
mcp_command = ["npx", "-y", "@design-system/mcp-server"]

[skills.GraphQL]
mcp_url = "http://localhost:4000/mcp"

[verifiers."@url"]
skill = "builtin:url"   # ships with Brief

[verifiers."figmaURL"]
mcp_command = ["npx", "-y", "@figma/brief-verifier"]
```

## Commands

| Command | Status | Description |
|---------|--------|-------------|
| `brief check <file>` | ✅ works | Type-check — fast, no network, CI-safe |
| `brief verify <file>` | ✅ works | Seal the contract → writes `.brief.lock` |
| `brief serve <file>` | ✅ works | Start MCP server (requires valid lock) |
| `brief run <file>` | ✅ works | Execute `.brief` against real MCP skill servers |
| `brief test <file>` | ✅ works | Run `test { }` blocks with mock skill system |
| `brief test <file> --live` | ✅ works | Same, but make real MCP calls instead of mocks |
| `brief gen "<desc>"` | ✅ works | Generate `.brief` from description (LLM if `BRIEF_LLM_API_KEY` set) |
| `brief skillgen <path>` | ✅ works | Generate `.briefskill` from README (LLM if `BRIEF_LLM_API_KEY` set) |
| `brief fmt <file>` | ✅ works | Auto-format to canonical style |
| `brief doc <file>` | ✅ works | Generate Markdown documentation |
| `brief watch <file>` | ✅ works | Live re-check on every save |
| `brief init <name>` | ✅ works | Scaffold a new Brief project |
| `brief ci` | ✅ works | Check all `[ci] examples` from `brief.toml` |
| `brief lsp` | ✅ works | LSP server for editor integration |
| `brief build <file>` | 🔧 roadmap | Compile to native binary via LLVM |

## Composable Verification

Brief's compiler knows zero domain logic. Everything is a plugin:

```
Brief Core (small, stable)
├── Static: @range, @enum, @matches, @nonEmpty  (compiler, always)
├── builtin:url (ships with Brief — just HTTP GET)
└── Verifier protocol: dispatch(annotation, value, context) → VerificationResult

Verifier Bricks (same MCP format as skill bricks)
├── @brief/local-path-verifier   → npm install + brief verify
├── @brief/github-verifier       → npm install + brief verify
├── @brief/shell-verifier        → npm install + brief verify
└── mcp_command = ["python", "./my-verify.py"]  → user-written

Routing (brief.toml — your composition layer)
[verifiers."@url"]    skill = "builtin:url"
[verifiers."@figma"]  mcp_command = ["npx", "@figma/brief-verifier"]
[verifiers."@custom"] mcp_command = ["python", "./verify.py"]
```

## Error Codes

| Code | Where | Description |
|------|-------|-------------|
| E107 | check | Missing `.briefskill` interface file |
| E301 | check | `@range` boundary literal missing in test block |
| E302 | check | `@enum` value literal missing in test block |
| E303 | check | `.brief.lock` missing/stale/source-changed |
| E309 | check | Dynamic annotation has no configured verifier |

## Reference Skills

Five complete reference skills in [`examples/skills/`](examples/skills/):

| Skill | Key Annotations | What they verify |
|-------|----------------|-----------------|
| `FileSystem` | `@local-path` | Path exists and is accessible |
| `GitHub` | `@github-repo`, `@nonEmpty`, `@enum` | Repo accessible, state valid |
| `WebSearch` | `@url`, `@range` | URL reachable, result counts in range |
| `Memory` | `@nonEmpty`, `@range` | Keys valid, TTL in range |
| `Shell` | `@shell-command`, `@enum`, `@range` | Command exists in PATH |

Each skill includes: `.briefskill` interface, `README.md`, and a `.brief` example with test coverage.

## Language Features

- **Typed skill imports** — `import skill "GitHub"` + `uses [GitHub]`
- **Static constraint annotations** — `@range(1, 100)`, `@enum("a","b")`, `@matches("regex")`, `@nonEmpty`
- **Dynamic annotations** — `@local-path`, `@github-repo`, `@custom-xyz` → routed to verifiers
- **Linear types** — `@once let handle = perform...` enforced in `brief serve`
- **Effect group aliases** — `type AuthEffects = [Auth, Session]`
- **Test blocks** — spec coverage checking (E301/E302)
- **Result propagation** — `perform Skill.fn()?`
- **Lock gate** — E303 if `.brief.lock` missing when dynamic annotations present
- **`needs {}` block** — declare env vars, feature flags, and config keys that must exist before the AI starts; checked by `brief verify` and re-checked by `brief serve` at startup
- **`forbids {}` block** — declare skills and functions the AI must never use; enforced statically by `brief check` (E420/E421) and at runtime by `brief serve` (forbidden tools are hidden from `tools/list`)

## The Mise en Place Guarantee

Brief is a preflight system for AI agents. Before the AI touches a single line of code, all ingredients are confirmed to exist, be accessible, and be correct:

```brief
task CheckoutFlow : TaskBrief uses [Cart, Payment, Order] {
    goal = "Complete a purchase"

    needs {
        env "PAYMENT_API_KEY"    // must be set before AI starts
        feature "checkout_v2"    // feature flag must be enabled
    }

    forbids {
        skill "Database"         // use Cart abstraction, not raw DB
        func "Payment.refund"    // checkout only charges, never refunds
    }

    step Charge {
        @once let receipt = perform Payment.charge(cart.total)?
    }
}
```

- `brief check` — E420 if `Database` appears in `uses []` or any `perform`; E421 if `Payment.refund` is called
- `brief verify` — E411 if `PAYMENT_API_KEY` is unset or `checkout_v2` feature is disabled
- `brief serve` — `Payment.refund` and all `Database.*` tools are hidden from the AI's `tools/list`

## Error Codes

| Code | Where | Description |
|------|-------|-------------|
| E107 | check | Missing `.briefskill` interface file |
| E301 | check | `@range` boundary literal missing in test block |
| E302 | check | `@enum` value literal missing in test block |
| E303 | check | `.brief.lock` missing/stale/source-changed |
| E309 | check | Dynamic annotation has no configured verifier |
| E401 | verify | Skill function not found in live MCP server (`tools/list`) |
| E402 | verify | Skill MCP server unreachable |
| E411 | verify | `needs {}` prerequisite not met |
| E420 | check | `forbids { skill "X" }` — forbidden skill used |
| E421 | check | `forbids { func "Skill.fn" }` — forbidden function called |

## Roadmap

- ✅ Full type system, skill imports, error messages
- ✅ `brief test`, `brief fmt`, LSP, skill registry
- ✅ Linear types, type aliases, effect groups
- ✅ `brief watch`, `brief init`, `brief.toml` manifest
- ✅ `brief ci`, shell completions, VS Code grammar
- ✅ `brief verify` — composable verifier protocol (MCP)
- ✅ `brief serve` — MCP server with lock gate enforcement
- ✅ `brief run` — real execution against skill MCP servers
- ✅ `brief test --live` — empirical type validation against real MCP servers
- ✅ `brief gen` — LLM generation with compiler feedback loop
- ✅ `brief skillgen` — LLM annotation extraction
- ✅ **`needs {}` block** — prerequisite verification (env, feature, config)
- ✅ **`forbids {}` block** — scope boundaries enforced statically and at runtime
- 🔧 LLVM/WASM backend
- 🔧 npm verifier packages (`@brief/local-path-verifier`, etc.)

## Contributing

See [CONTRIBUTING.md](.github/CONTRIBUTING.md). All skill and verifier authors welcome — Brief is built for communities that build with AI.

## License

MIT
