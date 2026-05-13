# Brief Examples

A collection of `.brief` files to explore the language features. Run any file with:

```bash
brief check  <file>.brief   # type-check (fast, CI-friendly)
brief run    <file>.brief   # check + execute steps
brief build  <file>.brief   # compile to native binary via LLVM IR
brief build  <file>.brief --emit-ir  # inspect the generated LLVM IR
```

---

## Examples

| File | What it shows |
|------|---------------|
| [01-hello.brief](01-hello.brief) | Minimal task — just a goal, no steps |
| [02-profile-screen.brief](02-profile-screen.brief) | UI task with Figma + GraphQL skills |
| [03-domain-model.brief](03-domain-model.brief) | Domain layer task from GraphQL schema |
| [04-mapper.brief](04-mapper.brief) | Sealed types, structs, domain→presentation mapper |
| [05-effect-definition.brief](05-effect-definition.brief) | Declaring effects, protocols, generics |
| [06-auth-login.brief](06-auth-login.brief) | Auth flow — login, token refresh, logout (3 tasks) |
| [07-notifications.brief](07-notifications.brief) | Push notification registration + handling |
| [08-onboarding.brief](08-onboarding.brief) | Multi-screen onboarding (Welcome → Permissions → Profile) |
| [09-settings.brief](09-settings.brief) | Settings CRUD — load prefs, update theme, delete account |
| [10-data-sync.brief](10-data-sync.brief) | Offline-first sync — incremental + force full re-sync |
| [11-ai-chat.brief](11-ai-chat.brief) | LLM-powered chat with streaming + context compression |
| [12-sealed-types.brief](12-sealed-types.brief) | Type library — full type system showcase (no task) |
| [13-feature-flags.brief](13-feature-flags.brief) | A/B test flag evaluation + variant routing |

---

## Quick tour

### Minimal task
```brief
task Hello : TaskBrief {
    goal = "Say hello to the world"
}
```

### Task with skills and steps
```brief
import skill "GraphQL"
import skill "DesignSystem"

@BriefBuilder
task ProfileScreen : TaskBrief uses [GraphQL, DesignSystem] {
    goal   = "Display user profile"
    extras = ["platform": "iOS", "figmaURL": "https://figma.com/file/abc"]

    step FetchData {
        let user = perform GraphQL.query(UserProfileQuery)?;
    }

    step Render {
        let card = perform DesignSystem.profileCard(user)?;
    }
}
```

### Sealed types and structs
```brief
sealed type Status = Active | Inactive | Suspended(String)

struct User {
    id:    @nonEmpty String
    email: @matches("[^@]+@[^@]+") String
}
```

### Custom effect (skill interface)
```brief
effect Storage {
    fn save(key: @nonEmpty String, value: @nonEmpty String) -> Result
    fn load(key: @nonEmpty String) -> Option
}
```

---

## Skill warnings (W101) — expected

When you run `brief check` on examples that import skills, you'll see:

```
warning[W101]: skill 'GraphQL' has no interface file
  fix: run: brief skillgen .claude/skills/GraphQL/
```

This is **normal** — it means the `.briefskill` type interface hasn't been generated yet.
To silence it, create the skill directory and run `brief skillgen`:

```bash
mkdir -p .claude/skills/GraphQL
# add a README.md with an ## Interface section
brief skillgen .claude/skills/GraphQL/
```

The examples are all structurally valid — W101 is a soft warning, not an error.

---

## Compile to IR and inspect

```bash
brief build 11-ai-chat.brief --emit-ir
cat 11-ai-chat.ll
```

Each `task` becomes an LLVM function, each `step` a labelled section, and
`perform Skill.fn(args)` emits a `brief_rt_perform(skill, fn, argc)` call.
