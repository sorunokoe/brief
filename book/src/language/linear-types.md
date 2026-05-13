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

## Why

Linear types prevent AI-generated code from forgetting to confirm a payment, acknowledge a message, or release a lock. Missing these is a compile error, not a runtime bug.
