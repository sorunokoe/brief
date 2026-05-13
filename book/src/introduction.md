# Brief

> *"If it compiles, the AI has everything it needs."*

**Brief** is a statically-checked DSL for AI-assisted development workflows.

You write a `.brief` file that describes:

- **What** the work is (`goal`)
- **What capabilities** the AI agent needs (`import skill`, `uses`)
- **How** the work breaks into steps (`step`, `perform`)
- **The types and effects** the workflow operates over

The compiler checks your brief for structural completeness before it ever reaches an AI agent. No missing goals, no undeclared skills, no phantom dependencies.

## Why Brief?

When you hand an AI agent an ambiguous task description, it either hallucinates the missing pieces or asks you to fill them in. Brief makes ambiguity a compile-time error.

```brief
@BriefBuilder
task UpdateProfile : TaskBrief
    uses [UserDB, Storage, Notify]
{
    goal = "Update a user's profile photo and notify followers"

    step Validate {
        let photo = perform Storage.upload(input.photo)?;
    }

    step Save {
        perform UserDB.update(userId, photo)?;
    }

    step Notify {
        perform Notify.fanOut(userId.followers)?;
    }
}
```

Run `brief check UpdateProfile.brief` — the compiler verifies that `UserDB`, `Storage`, and `Notify` are all declared, that every `perform` references a declared skill, and that all linear bindings are consumed exactly once.

## Quick Start

```bash
# Scaffold a new project
brief init my-project
cd my-project

# Type-check
brief check hello.brief

# Live re-check on save
brief watch hello.brief
```

See the [Getting Started](getting-started.md) guide for a full walkthrough.

## What's in this book?

| Section | Contents |
|---------|----------|
| [Getting Started](getting-started.md) | Install, first brief, basic workflow |
| [CLI Reference](cli-reference.md) | All `brief` subcommands |
| [Language Reference](language/overview.md) | Complete language specification |
| [Examples](examples/index.md) | 32 annotated example `.brief` files |
| [Contributing](contributing.md) | How to add skills, fix bugs, write docs |
