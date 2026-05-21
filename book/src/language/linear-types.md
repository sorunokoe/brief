# Linear Types (`@once`)

Linear types enforce that a resource is consumed exactly once — not zero (dropped), not two (reused).

## Usage

```brief
@once let handle = perform Payment.charge(amount)?;
perform Order.confirm(handle)?;  // consumed ✅
```

## Auto-Linear from Effect Return Types

```brief
effect Auth {
    fn login(creds: Credentials) -> @once Token
}

let token = perform Auth.login(creds)?;  // automatically linear
perform Session.start(token)?;
```

## Errors

- **E104** — `@once` binding used more than once
- **E105** — `@once` binding never consumed

## Runtime Enforcement in `brief serve`

The static checker (E104/E105) catches `@once` violations at compile time. When running as an MCP server, `brief serve` adds a second layer of enforcement at the protocol level:

- Functions declared as `-> @once T` in a `.briefskill` file, or called via `@once let` in the `.brief` program, are tracked at runtime.
- After a successful call, the response content is fingerprinted (SHA-256). If the **same handle** (same response content) is produced again in the same session — meaning the skill returned a duplicate — the second call is rejected with an `@once violation` error.
- This allows legitimate repeat calls that produce *different* handles (e.g., two separate `Payment.charge` calls with different transaction IDs) while catching duplicate handles.

## Why

Linear types prevent AI-generated code from forgetting to confirm a payment, acknowledge a message, or release a lock. Missing these is a compile error, not a runtime bug. The `brief serve` layer adds a runtime safety net for cases where the static checker cannot see across dynamic MCP boundaries.
