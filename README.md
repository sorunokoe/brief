# Brief

> **If it compiles, the AI has everything it needs.**

Brief is a programming language for AI-assisted development workflows. It is the language of task briefs — the boundary between humans/AI agents and the work that needs doing.

Every Brief file defines a typed contract: what a task needs (skills, data, design assets), what it produces, and what steps it follows. If `brief check` passes, an AI agent can execute the task with confidence that all ingredients are present.

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
brief check hello.brief
# brief v1.0.0
# ● Brief: Hello
#   goal:    Say hello to the world
#   skills:  none required
# ✅ All ingredients present. Ready for AI.
```

## A Real Workflow Brief

```brief
import skill "DesignSystem"
import skill "GraphQL"

@BriefBuilder
task ProfileScreen : TaskBrief uses [DesignSystem, GraphQL] {
    goal   = "Display user profile with design-system components"
    extras = ["platform": "iOS", "figmaURL": "https://figma.com/file/abc?node-id=1:2"]

    step FetchData {
        let user = perform GraphQL.query(UserProfileQuery)?
    }

    step Render {
        let card = perform DesignSystem.profileCard(user)?
        display(card)
    }
}
```

## Commands

| Command | Description |
|---------|-------------|
| `brief check <file>` | Type-check only — fast, CI-friendly |
| `brief run <file>` | Compile and execute |
| `brief build <file>` | Compile to native binary via LLVM |
| `brief build <file> --target wasm32-unknown-unknown` | Compile to WASM |
| `brief test <file>` | Run `test { }` blocks with mock skill system |
| `brief fmt <file>` | Auto-format to canonical style |
| `brief doc <file>` | Generate Markdown documentation |
| `brief watch <file>` | Live re-check on every save (dev loop) |
| `brief init <name>` | Scaffold a new Brief project directory |
| `brief ci` | Check all `[ci] examples` from `brief.toml` |
| `brief completions <shell>` | Print shell completion script (bash/zsh/fish/powershell) |
| `brief repl` | Interactive REPL |
| `brief lsp` | LSP server (stdio) — for editor integration |
| `brief gen "<description>"` | AI-generates a `.brief` file from natural language |
| `brief skillgen <path>` | Generate `.briefskill` interface from skill markdown |
| `brief add skill <Name>` | Install a skill from the registry |

## The Skill System

Skills live in `.claude/skills/<name>/`. Brief auto-discovers them:

```bash
brief skillgen .claude/skills/DesignSystem/
# Reads README.md → emits DesignSystem.briefskill
# ✅ Interface generated: .claude/skills/DesignSystem/DesignSystem.briefskill
```

The `.briefskill` file is the typed contract — committed to your repo. `brief check` validates against it at compile time.

## Why Brief?

| Problem | Brief's answer |
|---------|----------------|
| "Did I give the AI everything it needs?" | `brief check` tells you at compile time |
| "Which skills does this task use?" | The type `TaskBrief uses [X, Y]` says it all |
| "I keep repeating the same uses clause" | `type AuthEffects = [Auth, Session, Permissions]` |
| "I don't want to write this brief by hand" | `brief gen "describe your task"` |
| "How do I know the skill interface is correct?" | `brief skillgen` generates it from your skill's docs |
| "I want to see all tasks and their effects documented" | `brief doc my-feature.brief` |

## Language Features (v0.4)

- **Algebraic data types** — `sealed type`, `struct`
- **Generics** — `Result<T, E>`, `Option<T>`
- **Structural protocols** — `protocol Renderable { fn render() -> Component }`
- **Algebraic effects** — `uses [Skill1, Skill2]` tracked in the type signature
- **Effect group aliases** — `type AuthEffects = [Auth, Session, Permissions]` — name sets of skills
- **Refinement type aliases** — `type Email = @matches("[^@]+@[^@]+") String`
- **MCP type aliases** — `type FileMCP = @mcp FileSystem` — mark MCP-backed skills
- **Linear types** — `@once` enforces handles are consumed exactly once (E104/E105)
- **Result propagation** — `perform Skill.fn()?`
- **Test blocks** — `test { }` with `mock`, `run`, `assert` — parsed by both `brief test` and `brief check`
- **Doc generation** — `brief doc` renders Markdown from any `.brief` file

## Examples

32 examples in [`examples/`](examples/):

| Range | What they cover |
|-------|----------------|
| 01–14 | Core language: hello, UI task, domain model, mapper, effects, auth, notifications, onboarding, settings, sync, AI chat, sealed types, feature flags, test suite |
| 15–22 | Real-world: checkout, analytics, i18n, upload pipeline, OTP, search, resilience, RBAC |
| 23–26 | Phase 3 power types: linear types, type aliases, effect groups, doc showcase |
| 27–32 | Advanced patterns: composition, AI/RAG pipeline, platform branching, event sourcing, concurrency, MCP integration |

## Roadmap

- **v0.1** ✅ — Full type system, skill imports, error messages, examples
- **v0.2** ✅ — `brief test`, `brief fmt`, LSP, WASM, skill registry
- **v0.3** ✅ — Linear types (`@once`), type aliases, effect groups, `brief doc`
- **v0.4** ✅ — Test block parsing, `@mcp` attribute, 32 examples, CI/release workflows
- **v1.0** ✅ — `brief watch`, `brief init`, `brief.toml` manifest, mdBook docs site
- **v1.0+** ✅ — `brief ci`, shell completions, GitHub Pages, VS Code grammar update

## Contributing

See [CONTRIBUTING.md](.github/CONTRIBUTING.md). All skill authors are welcome — Brief is built for communities that build with AI.

## License

MIT
