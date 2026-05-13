# Brief

> **If it compiles, the AI has everything it needs.**

Brief is a programming language for AI-assisted development workflows. It is the language of task briefs — the boundary between humans/AI agents and the work that needs doing.

Every Brief file defines a typed contract: what a task needs (skills, data, design assets), what it produces, and what steps it follows. If `brief check` passes, an AI agent can execute the task with confidence that all ingredients are present.

## Quick Start

```bash
# Install (macOS/Linux)
curl -sSf https://install.brieftool.io | sh   # coming soon — build from source below

# Build from source
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
# ✅ Brief: Hello
#    goal:   Say hello to the world
#    skills: none required
# ✅ All ingredients present. Ready for AI.

brief run hello.brief
# ● Running brief: Hello
# ● Goal: Say hello to the world
# ✅ Complete.
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

```bash
brief check 02-profile-screen.brief
# ✅ Brief: ProfileScreen
#    goal:   Display user profile with design-system components
#    skills: [DesignSystem, GraphQL]
#    steps:  [FetchData, Render]
# ✅ All ingredients present. Ready for AI.
```

## Commands

| Command | Description |
|---------|-------------|
| `brief check <file>` | Type-check only — fast, CI-friendly |
| `brief run <file>` | Compile and execute |
| `brief repl` | Interactive REPL |
| `brief skillgen <path>` | Generate `.briefskill` interface from skill markdown |
| `brief gen "<description>"` | AI-generates a `.brief` file from natural language |

## The Skill System

Skills live in `.claude/skills/<name>/`. Brief auto-discovers them:

```bash
brief skillgen .claude/skills/DesignSystem/
# Reads README.md → emits DesignSystem.briefskill
# ✅ Interface generated: .claude/skills/DesignSystem/DesignSystem.briefskill
```

The `.briefskill` file is the typed contract for the skill — committed to your repo alongside the markdown. `brief check` validates against it at compile time.

## Why Brief?

| Problem | Brief's answer |
|---------|----------------|
| "Did I give the AI everything it needs?" | `brief check` tells you at compile time |
| "Which skills does this task use?" | The type `TaskBrief uses [X, Y]` says it all |
| "I don't want to write this brief by hand" | `brief gen "describe your task"` |
| "How do I know the skill interface is correct?" | `brief skillgen` generates it from your skill's docs |

## Language Features (v0.1)

- **Algebraic data types** — `sealed type`, `struct`
- **Generics** — `Result<T, E>`, `Option<T>`
- **Structural protocols** — `protocol Renderable { fn render() -> Component }`
- **Algebraic effects** — `uses [Skill1, Skill2]` tracked in the type signature
- **Simplified refinements** — `@url String`, `@nonEmpty String`
- **Result propagation** — `perform Skill.fn()?`
- **Structured concurrency** — `async`, `await`, scoped `spawn`

## Roadmap

- **v0.0.1** — `brief run hello.brief` works (tree-walking interpreter) ← *current*
- **v0.1** — Full type system, skill imports, `brief gen`, community launch
- **v0.2** — VS Code extension, test runner with mock skills, skill registry
- **v0.3** — Linear types, full refinement types, effect polymorphism
- **v1.0** — Language specification 1.0, self-hosting exploration

## Contributing

See [CONTRIBUTING.md](.github/CONTRIBUTING.md). All skill authors are welcome — Brief is built for communities that build with AI.

## License

MIT
