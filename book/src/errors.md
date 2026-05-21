# Error Code Reference

All `brief check` diagnostics follow this numbering scheme:

| Range | Category |
|-------|----------|
| E001 | Parse errors |
| E101–E107 | Structural/semantic errors |
| E201–E210 | Type errors |
| E301–E309 | Spec/constraint/lock errors |
| E411 | Prerequisite errors (`needs {}`) |
| E420–E421 | Scope boundary errors (`forbids {}`) |
| W101–W104 | Warnings |

## E001 — ParseError

A token the parser could not recognize.

## E101 — MissingGoal

Every `task` must have a `goal = "..."` field.

```brief
task Broken : TaskBrief {}  // E101: missing 'goal'
```

## E102 — UndeclaredSkillInUses

A skill in `uses [...]` was not `import skill`-ed.

## E103 — PerformWithoutUses

A `perform` references a skill not in `uses`.

## E104 — LinearBindingReused

An `@once` binding was used more than once.

## E105 — LinearBindingDropped

An `@once` binding was never consumed.

## E106 — UnknownEffectGroup

`uses [GroupName]` references an undefined effect group.

## E107 — MissingSkillInterface

`import skill "Name"` has no `.briefskill` file found.

```
warning[E107]: no interface file found for 'GraphQL'
  hint: run: brief skillgen .claude/skills/GraphQL/
```

Suppress with `brief check --allow-missing-skills`.

## E201 — UnknownType

A type reference cannot be resolved.

## E202 — WrongArgCount

A skill function called with wrong number of arguments.

## E203 — AttributeConstraint

A field attribute value fails its constraint (`@url`, `@nonempty`, `@matches`).

## E208 — UnknownExtrasField

A typed `extras` field references an unknown type.

## E209 — EffectContractViolation

A task performs a skill whose effects are not declared in `effects [...]`.

## E210 — UndeclaredStepInCombinator

A workflow combinator (`parallel`, `retry`, `fallback`) references a step that doesn't exist.

## E301 — RangeBoundaryMissing

`@range(min, max)` on a skill param but the test block doesn't exercise the boundary.

## E302 — EnumValueMissing

`@enum(vals)` on a skill param but the test block doesn't cover all values.

## E303 — LockRequired

`.brief.lock` is missing, stale, or its source hash doesn't match.

```
error[E303]: contract is unsealed — run 'brief verify' first
```

## E309 — UnconfiguredVerifier

A dynamic annotation (e.g. `@figmaURL`) has no `[verifiers."@figmaURL"]` entry in `brief.toml`.

## E411 — NeedNotMet

A `needs {}` prerequisite was not satisfied at `brief verify` time.

```brief
needs {
    env "GRAPHQL_ENDPOINT"   // E411 if this env var is unset or empty
}
```

**Fix:** Set the required env var, feature flag, or config value, then re-run `brief verify`.

## E420 — ForbiddenSkill

A task uses a skill that is explicitly listed in `forbids { skill "..." }`.

```brief
forbids { skill "Database" }
// ...
uses [Database]   // E420: forbidden skill in uses[]
// or
perform Database.query(...)  // E420: forbidden skill called via perform
```

**Fix:** Remove the `perform` call or the `uses` entry, or remove the `forbids` rule if intentional.

## E421 — ForbiddenFunc

A task calls a function that is explicitly listed in `forbids { func "..." }`.

```brief
forbids { func "Payment.refund" }
// ...
perform Payment.refund(userId)  // E421
```

**Fix:** Remove the `perform Payment.refund(...)` call, or remove the `forbids` rule if intentional.

## W101–W102 — Stale / deprecated warnings

Reserved; see E107 for missing skill interface.

## W103 — DeprecatedStringExtras

Old `extras = ["key": "value"]` syntax. Use `extras { field: Type }` instead.

## W104 — BriefBuilderProvidesMissing

`@BriefBuilder` task is missing a `provides { ... }` block.
