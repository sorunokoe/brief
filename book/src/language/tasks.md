# Tasks

A `task` is the primary unit of work in Brief.

## Syntax

```brief
[@decorator]
task <Name> : <Base> [uses [<Skill1>, <Skill2>, ...]] {
    goal = "<description>"

    [needs { ... }]
    [forbids { ... }]

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

## `needs {}` — Prerequisites

Declare what must exist **before** the AI starts. `brief verify` confirms each requirement.

```brief
task SyncProfile : TaskBrief uses [GraphQL, Storage] {
    goal = "Sync user profile data"

    needs {
        env "GRAPHQL_ENDPOINT"    // must be set and non-empty
        env "STORAGE_API_TOKEN"
        feature "profile_v2"      // routes to [verifiers.feature] in brief.toml
        config "sync_interval"    // routes to [verifiers.config] in brief.toml
    }

    step Fetch { let data = perform GraphQL.query(ProfileQuery)?; }
    step Store { perform Storage.save(data)?; }
}
```

**`env "VAR"`** — checked with the built-in env verifier (no extra config needed). Fails if the variable is unset or empty.

**`feature "FLAG"` / `config "KEY"`** — routed to the `[verifiers.feature]` / `[verifiers.config]` entry in `brief.toml`:

```toml
# brief.toml
[verifiers.feature]
mcp_command = ["npx", "@myapp/feature-verifier"]

[verifiers.config]
mcp_url = "http://config-server/mcp"
```

Missing verifier configuration → `brief verify` fails with a clear error.

`brief serve` re-checks `env` prerequisites at startup (env vars may differ from verify time).

**Error codes:**
- **E411** — prerequisite not met at verify time

## `forbids {}` — Scope Boundaries

Declare what the AI must **never** use. Enforced by `brief check` (static, no network) and `brief serve` (runtime — forbidden tools are hidden from the AI entirely).

```brief
task CheckoutFlow : TaskBrief uses [Cart, Payment] {
    goal = "Handle a purchase"

    forbids {
        skill "Database"          // use Cart abstraction, not raw DB
        skill "Auth"              // existing session only — no re-auth
        func "Payment.refund"     // checkout only charges, never refunds
    }

    step Charge {
        let cart = perform Cart.getActive(userId)?;
        perform Payment.charge(cart.total)?;
    }
}
```

**`skill "Name"`** — the AI must not use this skill at all.  
- E420 if the skill appears in `uses []` or in any `perform` call.  
- At `brief serve` time: all tools from this skill are hidden from `tools/list`.

**`func "Skill.fn"`** — the AI must not call this specific function.  
- E421 if `perform Skill.fn(...)` appears anywhere in steps.  
- At `brief serve` time: this tool is hidden from `tools/list` and rejected if called directly.

**Error codes:**
- **E420** — forbidden skill used in `uses []` or `perform` call
- **E421** — forbidden function called via `perform`

## Decorators

| Decorator | Meaning |
|-----------|---------|
| `@BriefBuilder` | Marks the task as the primary entry point |
| `@mcp <Server>` | Task is backed by an MCP server |
