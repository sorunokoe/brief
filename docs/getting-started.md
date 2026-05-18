# Getting Started with Brief

> *If it compiles, the AI has everything it needs.*

This guide takes you from zero to your first validated `.brief` file in 5 minutes.

---

## Install

**Build from source (requires Rust):**

```bash
git clone https://github.com/yourusername/brief
cd brief
cargo build --release
cp target/release/brief /usr/local/bin/
```

**Verify:**
```bash
brief --version
# brief 0.0.1
```

---

## Your First Brief

Create `hello.brief`:

```brief
task Hello : TaskBrief {
    goal = "Say hello to the world"
}
```

Validate it:
```bash
brief check hello.brief
# brief v0.0.1
#
# ● Brief: Hello
#   goal:    Say hello to the world
#   skills:  none required
# ✅ All ingredients present. Ready for AI.
```

Run it:
```bash
brief run hello.brief
# ● Running brief: Hello
# ✅ Complete.
```

---

## Adding Skills

Skills represent external capabilities — Figma, GraphQL, a design system, an API client.  
They live in `.claude/skills/<SkillName>/` as a `README.md`.

### 1. Generate a skill interface

```bash
brief skillgen .claude/skills/GraphQL/
# ✅ .claude/skills/GraphQL/GraphQL.briefskill
```

This reads the skill's `README.md` and emits a typed `.briefskill` interface file.

### 2. Import the skill

```brief
import skill "GraphQL"

@BriefBuilder
task FetchUser : TaskBrief uses [GraphQL] {
    goal = "Fetch user profile from the GraphQL API"

    step Fetch {
        let user = perform GraphQL.query(UserProfileQuery)?;
    }
}
```

### 3. Validate

```bash
brief check fetch-user.brief
# ✅ All ingredients present. Ready for AI.
```

---

## Generating Briefs with AI

```bash
brief gen "A task that displays a user profile screen using our design system and GraphQL"
```

This produces a `.brief` template with the description as the `goal`. In v0.1, it will call a configured LLM to generate a complete brief.

---

## Brief File Structure

```brief
// Optional: import skills
import skill "DesignSystem"
import skill "GraphQL"

// Optional decorator (marks that task uses @BriefBuilder composition)
@BriefBuilder

// Task declaration
task TaskName : TaskBrief uses [DesignSystem, GraphQL] {
    // Required: what this task accomplishes
    goal = "Human-readable description"

    // Optional: key-value metadata
    extras = ["platform": "iOS", "figmaURL": "https://..."]

    // Optional: workflow steps
    step StepName {
        // Invoke a skill effect with `perform`
        let result = perform SkillName.functionName(arg)?;
        // The `?` propagates errors automatically
    }
}
```

---

## Commands Reference

| Command | Description |
|---------|-------------|
| `brief check <file>` | Type-check only (fast, ideal for CI) |
| `brief run <file>` | Validate + execute (shows step-by-step output) |
| `brief build <file>` | Compile to native binary via LLVM |
| `brief build <file> --emit-ir` | Emit LLVM IR for inspection |
| `brief build <file> --target wasm32-unknown-unknown` | Compile to WASM |
| `brief test <file>` | Run `test { }` blocks with mock skill system |
| `brief fmt <file>` | Auto-format to canonical style |
| `brief fmt <file> --check` | Fail if file is not formatted (CI mode) |
| `brief doc <file>` | Generate Markdown documentation |
| `brief gen "<desc>"` | Generate a brief from natural language |
| `brief gen "<desc>" --force` | Overwrite existing output file |
| `brief skillgen <path>` | Generate `.briefskill` from skill's README.md |
| `brief add skill <Name>` | Install a skill from the registry |
| `brief watch <path>` | Re-check on every file save (dev loop) |
| `brief init <name>` | Scaffold a new Brief project |
| `brief ci` | Run all checks in `brief.toml` `[ci]` section |
| `brief repl` | Interactive REPL |
| `brief lsp` | LSP server (stdio) — for editor integration |
| `brief --help` | Full command reference |

---

## Error Messages

Brief's errors are designed to teach, not just report:

```
error[E101]: task 'Hello' is missing a `goal` field
  → hello.brief:1:1
  fix: add: goal = "describe what this task accomplishes"

warning[W101]: skill 'GraphQL' has no interface file
  → fetch-user.brief:1:1
  fix: .claude/skills/GraphQL/GraphQL.briefskill not found — run: brief skillgen .claude/skills/GraphQL/

error[E102]: skill 'GraphQL' is in `uses` but never imported
  → fetch-user.brief:3:1
  fix: add: import skill "GraphQL" at the top of the file
```

---

## Skill README Format for `brief skillgen`

Add an `## Interface` section to your skill's `README.md`:

```markdown
## Interface

- `fn query(op: Operation) -> Result<T, QueryError>`
- `fn mutation(op: Mutation) -> Result<T, MutationError>`
- `fn schema(operationName: String) -> Schema`
```

`brief skillgen` parses this section and emits the typed `.briefskill` file.

---

## CI Integration

The recommended way to validate all briefs in a project is `brief ci`:

```bash
brief ci
# brief v1.0.0
# ✅ 40/40 examples passed
```

`brief ci` reads `brief.toml`'s `[ci]` section and runs `brief check` on each listed file.
It exits with code 0 only when all checks pass — ideal for CI gates.

**GitHub Actions:**

```yaml
# .github/workflows/ci.yml
- name: Validate briefs
  run: brief ci
```

**`brief.toml` example:**

```toml
[ci]
examples = [
  "examples/01-hello.brief",
  "examples/06-auth-login.brief",
  "tasks/*.brief",
]
```

You can also validate a single file:

```bash
brief check my-feature.brief   # fast, single-file type-check
```

**Format enforcement (no unformatted briefs in CI):**

```yaml
- name: Check Brief formatting
  run: brief fmt --check examples/*.brief
```

---

## Phase 3 Power Types (v0.3)

### Type Aliases

Give a name to a refined type so you can reuse it everywhere:

```brief
type Email    = @matches("[^@]+@[^@]+") String
type NonEmpty = @nonEmpty String

struct User {
    id:    NonEmpty     // cleaner than @nonEmpty String inline
    email: Email
}
```

### Effect Groups

Name a set of skills that always travel together:

```brief
type AuthEffects = [Auth, Session, Permissions]

task Login : TaskBrief uses [AuthEffects] {
    goal = "Authenticate user"
    // uses expands to [Auth, Session, Permissions]
    step Verify {
        let identity = perform Auth.verify(credentials)?;
    }
}
```

### Linear Types (`@once`)

Ensure resources are consumed exactly once — no leaks, no double-use. The compiler tracks `@once` bindings **within and across steps**:

```brief
effect Payment {
    fn charge(amount: Int) -> @once PaymentHandle
    fn confirm(handle: PaymentHandle) -> PaymentConfirmation
}

task ProcessPayment : TaskBrief uses [Payment] {
    goal = "Charge and confirm payment"

    step Charge {
        @once let handle = perform Payment.charge(100)?;
        let receipt = perform Payment.confirm(handle)?;   // ✅ consumed in same step
    }
}
```

**Cross-step tracking** — if the binding is not consumed in the step it was declared, it is promoted to task-level and must be consumed in exactly one later step:

```brief
task TwoStepPayment : TaskBrief uses [Payment] {
    goal = "Acquire handle in step 1, confirm in step 2"

    step Acquire {
        @once let handle = perform Payment.charge(100)?;
        // handle NOT used here → promoted to cross-step tracking
    }

    step Confirm {
        let receipt = perform Payment.confirm(handle)?;   // ✅ consumed across steps
    }
}
```

The compiler enforces: `E104` if the handle is reused (in the same or across steps), `E105` if it is never consumed.

### Generate Docs

```bash
brief doc examples/26-brief-doc.brief
# Renders: # Brief Module: 26-brief-doc
# ## Skills, ## Types, ## Tasks, etc.

brief doc my-feature.brief --output docs/my-feature.md
```

---

## Advanced Patterns (v0.4–v1.0 — examples 27–42)

### Test Blocks (`brief test`)

Write tests directly in your `.brief` files. `brief check` parses them without errors;
`brief test` executes them with mock skills:

```brief
test "FetchProfile loads user via GraphQL" {
    mock GraphQL {
        fn query(op) -> Ok(User { id: "u1", name: "Ada Lovelace" })
    }

    run FetchProfile
    assert performed GraphQL.query
    assert result is Ok
}
```

```bash
brief test examples/14-test-suite.brief
# ✅ 5 tests passed, 0 failed
```

### MCP Integration (`@mcp` attribute)

Mark skills that are backed by MCP servers using the `@mcp` type alias:

```brief
type GitHubMCP   = @mcp GitHub
type FileSystemMCP = @mcp FileSystem
```

This signals to the AI agent (and future tooling) that these effects are served
by a Model Context Protocol server rather than a traditional HTTP API.

See `examples/32-mcp-integration.brief` for a full AI code-review pipeline using
GitHub + FileSystem + Browser + Database MCP skills.

### AI / RAG Pipeline

Chain LLM calls as typed effects — the compiler ensures every AI capability is declared:

```brief
import skill "LLM"
import skill "Embeddings"
import skill "VectorStore"

task RagSearch : TaskBrief uses [Embeddings, VectorStore, LLM] {
    goal = "Embed → retrieve → generate grounded answer"

    step Embed   { let embedding = perform Embeddings.embed(query)?; }
    step Retrieve { let results  = perform VectorStore.search(embedding, topK)?; }
    step Generate { let answer   = perform LLM.complete(ragPrompt, temperature)?; }
}
```

See `examples/28-ai-pipeline.brief` for the full RAG pipeline with tests.

### Event Sourcing

Commands emit typed events; projections replay them. All via `EventStore` effect:

```brief
sealed type OrderEvent = OrderCreated(String) | OrderPaid(String) | OrderShipped(String)

task PlaceOrder : TaskBrief uses [EventStore, GraphQL] {
    goal = "Place a new order — emit OrderCreated event"

    step Validate { let cart = perform GraphQL.query(CartQuery)?; }
    step Emit     { let _    = perform EventStore.append(orderId, OrderCreated(orderId))?; }
}
```

See `examples/30-event-sourcing.brief` for commands, projections, and saga orchestration.

---

## What's New in v1.0

- **Cross-step `@once` linear tracking** — resource handles can be acquired in one step and consumed in another; `E104`/`E105` cover the whole task lifetime.
- **`brief ci`** — single command to validate all briefs in a project (`brief.toml` driven).
- **`brief gen --force`** — regenerate a brief without manual deletion.
- **`brief fmt --check`** — CI format enforcement.
- **`brief watch`** and **`brief init`** — dev-loop and scaffolding commands.
- **Security-hardened registry** — skill name validation, 512 KB size cap, HTTP timeouts.
- **117 compiler tests**, 40 verified examples.

Watch [releases](https://github.com/yourusername/brief/releases) for updates.
