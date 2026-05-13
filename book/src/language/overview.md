# Language Overview

Brief is a statically-typed DSL for AI-assisted development workflows. It is **not** a general-purpose programming language.

Every language feature exists to answer one question: *"Does the AI agent have everything it needs to execute this task?"*

## Design Philosophy

| Principle | How Brief enforces it |
|-----------|----------------------|
| No missing goals | `goal` field is required on every `task` |
| No undeclared skills | Skills must be `import skill`-ed before use |
| No phantom dependencies | Every `perform` must reference a declared skill |
| No ambiguous types | Types are explicit and checked at compile time |
| No orphaned resources | Linear (`@once`) bindings must be consumed exactly once |

## Hello, Brief

```brief
task Hello : TaskBrief {
    goal = "Say hello to the world"
}
```

This is the smallest valid Brief program.

## A Richer Example

```brief
import skill "UserDB"
import skill "Notify"

@BriefBuilder
task SendWelcomeEmail : TaskBrief
    uses [UserDB, Notify]
{
    goal = "Send a welcome email to a newly registered user"

    step Fetch {
        let user = perform UserDB.findById(userId)?;
    }

    step Send {
        perform Notify.email(user.email, WelcomeTemplate)?;
    }
}
```

## Program Structure

A `.brief` file can contain:

1. **`import skill`** — declare external capabilities
2. **`type`** — type aliases and effect groups
3. **`sealed`** — algebraic sum types
4. **`struct`** — data structures
5. **`protocol`** — abstract interfaces
6. **`effect`** — side-effectful capability interfaces
7. **`task`** — the main workflow declaration
8. **`test`** — test blocks for the mock skill system
