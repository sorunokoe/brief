# Tasks

A `task` is the primary unit of work in Brief.

## Syntax

```brief
[@decorator]
task <Name> : <Base> [uses [<Skill1>, <Skill2>, ...]] {
    goal = "<description>"

    [step <StepName> { ... }]
}
```

## Required: `goal`

```brief
task BuildFeature : TaskBrief {
    goal = "Add dark mode toggle to the settings screen"
}
```

Missing `goal` → compiler error **E101**.

## `uses` Clause

```brief
import skill "UserDB"
import skill "Cache"

task GetUser : TaskBrief uses [UserDB, Cache] {
    goal = "Fetch a user by ID, checking cache first"
}
```

- **E102** — skill in `uses` not imported
- **E103** — `perform` calls skill not in `uses`

## Steps

```brief
task Checkout : TaskBrief uses [Cart, Payment, Order] {
    goal = "Complete a purchase"

    step Validate { let cart = perform Cart.getActive(userId)?; }
    step Charge   { @once let receipt = perform Payment.charge(cart.total)?; }
    step Confirm  { perform Order.create(cart, receipt)?; }
}
```

## Decorators

| Decorator | Meaning |
|-----------|---------|
| `@BriefBuilder` | Marks the task as the primary entry point |
| `@mcp <Server>` | Task is backed by an MCP server |
