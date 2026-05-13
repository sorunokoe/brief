# Error Code Reference

All `brief check` diagnostics follow this numbering scheme:

| Range | Category |
|-------|----------|
| E001 | Parse errors |
| E101–E106 | Structural/semantic errors |
| E201–E203 | Type errors |
| W101–W102 | Warnings |

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

## E201 — UnknownType

A type reference cannot be resolved.

## E202 — WrongArgCount

A skill function called with wrong number of arguments.

## E203 — AttributeConstraint

A field attribute value fails its constraint (`@url`, `@nonempty`, `@matches`).

## W101 — MissingSkillInterface

`import skill "Name"` has no `.briefskill` file found.

```
warning[W101]: no interface file found for 'GraphQL'
  hint: run: brief skillgen .claude/skills/GraphQL/
```

This is a warning — tasks still compile and run without it.

## W102 — StaleSkillInterface

Reserved for future use.
