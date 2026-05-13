# Brief Examples

A collection of `.brief` files to explore the language features. Run any file with:

```bash
brief check  <file>.brief          # type-check (fast, CI-friendly)
brief run    <file>.brief          # check + execute steps
brief build  <file>.brief          # compile to native binary via LLVM IR
brief build  <file>.brief --emit-ir # inspect the generated LLVM IR
brief test   <file>.brief          # run test { } blocks (see 14-test-suite.brief)
brief fmt    <file>.brief          # auto-format to canonical style
```

---

## Examples (01–14 · Core language)

| File | What it shows | `brief check` |
|------|---------------|---------------|
| [01-hello.brief](01-hello.brief) | Minimal task — just a goal, no steps | ✅ |
| [02-profile-screen.brief](02-profile-screen.brief) | UI task with Figma + GraphQL skills | ✅ W101 |
| [03-domain-model.brief](03-domain-model.brief) | Domain layer task from GraphQL schema | ✅ W101 |
| [04-mapper.brief](04-mapper.brief) | Sealed types, structs, domain→presentation mapper | ✅ W101 |
| [05-effect-definition.brief](05-effect-definition.brief) | Declaring effects, protocols, generics | ✅ |
| [06-auth-login.brief](06-auth-login.brief) | Auth flow — login, token refresh, logout (3 tasks) | ✅ W101 |
| [07-notifications.brief](07-notifications.brief) | Push notification registration + handling | ✅ W101 |
| [08-onboarding.brief](08-onboarding.brief) | Multi-screen onboarding (Welcome → Permissions → Profile) | ✅ W101 |
| [09-settings.brief](09-settings.brief) | Settings CRUD — load prefs, update theme, delete account | ✅ W101 |
| [10-data-sync.brief](10-data-sync.brief) | Offline-first sync — incremental + force full re-sync | ✅ W101 |
| [11-ai-chat.brief](11-ai-chat.brief) | LLM-powered chat with streaming + context compression | ✅ W101 |
| [12-sealed-types.brief](12-sealed-types.brief) | Type library — full type system showcase (no task) | ✅ |
| [13-feature-flags.brief](13-feature-flags.brief) | A/B test flag evaluation + variant routing | ✅ W101 |
| [14-test-suite.brief](14-test-suite.brief) | `test { }` blocks — assertions, `assert_eq`, `assert_err` | `brief test` |

## Examples (15–22 · Real-world scenarios)

| File | Domain | What it shows | `brief check` |
|------|--------|---------------|---------------|
| [15-checkout-flow.brief](15-checkout-flow.brief) | E-commerce | Cart → Payment → Order (3 tasks, 4 skills) | ✅ W101 |
| [16-analytics.brief](16-analytics.brief) | Analytics | Event tracking, batch flush, session lifecycle | ✅ W101 |
| [17-localization.brief](17-localization.brief) | i18n | Locale detection, string loading, pluralisation | ✅ W101 |
| [18-upload-pipeline.brief](18-upload-pipeline.brief) | Storage | Validate → Compress → Store → CDN invalidate | ✅ W101 |
| [19-otp-verification.brief](19-otp-verification.brief) | Auth | SMS/email OTP issue + verify (2 tasks) | ✅ W101 |
| [20-search-filters.brief](20-search-filters.brief) | Search | Build query, cache, paginate, autocomplete | ✅ W101 |
| [21-error-recovery.brief](21-error-recovery.brief) | Resilience | Retry with back-off, circuit-breaker, fallback | ✅ W101 |
| [22-permissions.brief](22-permissions.brief) | RBAC | Check access, grant/revoke roles, audit log | ✅ W101 |

## Examples (23–26 · Phase 3 — Power Types)

| File | Feature | What it shows | `brief check` |
|------|---------|---------------|---------------|
| [23-linear-types.brief](23-linear-types.brief) | Linear types | `@once` on effect return types; auto-tracking that each handle is consumed exactly once | ✅ W101 |
| [24-type-aliases.brief](24-type-aliases.brief) | Refinement aliases | `type Email = @matches(...) String`; composable attribute constraints | ✅ W101 |
| [25-effect-groups.brief](25-effect-groups.brief) | Effect groups | `type SecurityEffects = [Auth, Session, Permissions]`; group expansion in `uses [...]` | ✅ W101 |
| [26-brief-doc.brief](26-brief-doc.brief) | Doc generation | Comprehensive showcase; run `brief doc` to see rendered Markdown output | ✅ W101 |

```bash
# Generate docs for the showcase file:
brief doc examples/26-brief-doc.brief

# Generate docs with output to a file:
brief doc examples/26-brief-doc.brief --output docs/26-brief-doc.md
```

---

## Error examples (intentional — learn from failures)

Located in [`errors/`](errors/). Run with `brief check` to see the diagnostic output.

| File | Intentional error | Expected diagnostic |
|------|-------------------|---------------------|
| [bad-01-missing-goal.brief](errors/bad-01-missing-goal.brief) | No `goal` field | `error[E101]: task '...' is missing a goal field` |
| [bad-02-undeclared-skill.brief](errors/bad-02-undeclared-skill.brief) | `perform` on skill not in `uses` | `error[E103]: effect 'GraphQL' is performed but not declared in uses [...]` |
| [bad-03-uses-but-no-import.brief](errors/bad-03-uses-but-no-import.brief) | Skill in `uses` but no `import` | `error[E102]: skill 'Analytics' is in uses but never imported` |
| [bad-04-empty-step.brief](errors/bad-04-empty-step.brief) | Step with no body | ✅ passes (graceful edge case) |

```bash
# See E101 in action:
brief check examples/errors/bad-01-missing-goal.brief

# See E103 in action:
brief check examples/errors/bad-02-undeclared-skill.brief

# See E102 in action:
brief check examples/errors/bad-03-uses-but-no-import.brief

# Confirm empty step doesn't crash:
brief check examples/errors/bad-04-empty-step.brief
```

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
        let user = perform GraphQL.query(userProfileQuery)?;
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

### Test blocks
```brief
test "goal must not be empty" {
    let t = task Foo : TaskBrief { goal = "" }
    assert_err(t)
}
```
Run with: `brief test 14-test-suite.brief`

### Phase 3 — Power types

#### Type aliases (refinements)
```brief
type Email    = @matches("[^@]+@[^@]+") String
type NonEmpty = @nonEmpty String
```

#### Effect group aliases
```brief
type AuthEffects = [Auth, Session, Permissions]

task Login : TaskBrief uses [AuthEffects] {   // expands to [Auth, Session, Permissions]
    goal = "authenticate user"
    // ...
}
```

#### Linear types (`@once`)
```brief
effect Payment {
    fn charge(amount: Int) -> @once PaymentHandle    // handle can be used exactly once
}

task ProcessPayment : TaskBrief uses [Payment] {
    goal = "charge and confirm"
    step Charge {
        @once let handle = perform Payment.charge(amount)?;
        let receipt = perform Payment.confirm(handle)?;   // consumes handle — OK
        // perform Payment.refund(handle)?               // would error: E104 reused
    }
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
brief build examples/11-ai-chat.brief --emit-ir
cat 11-ai-chat.ll
```

Each `task` becomes an LLVM function, each `step` a labelled section, and
`perform Skill.fn(args)` emits a `brief_rt_perform(skill, fn, argc)` call.

---

## WASM target

```bash
brief build examples/02-profile-screen.brief --target wasm32-unknown-unknown
# outputs: 02-profile-screen.wasm
```

Requires `wasm-ld` (ships with LLVM). The `brief_rt_*` functions become host imports.
