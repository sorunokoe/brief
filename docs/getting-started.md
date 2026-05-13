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
| `brief gen "<desc>"` | Generate a brief from natural language |
| `brief skillgen <path>` | Generate `.briefskill` from skill's README.md |
| `brief repl` | Interactive REPL (v0.1) |
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

```yaml
# .github/workflows/ci.yml
- name: Validate briefs
  run: |
    for f in **/*.brief; do
      brief check "$f"
    done
```

Or with Make:
```makefile
brief-check:
    find . -name "*.brief" -exec brief check {} \;
```

---

## Next: v0.1

Coming in v0.1:
- Full type system (ADT, generics, protocols, algebraic effects)
- LLM-powered `brief gen` (set `BRIEF_LLM_API_KEY`)
- `brief test` with mock skill handlers
- VS Code extension with semantic highlighting

Watch [releases](https://github.com/yourusername/brief/releases) for updates.
