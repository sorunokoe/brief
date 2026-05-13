# Steps & Effects

## Steps

Steps break a task into named, sequential phases:

```brief
task ProcessOrder : TaskBrief uses [Inventory, Payment, Shipping] {
    goal = "Process a customer order end-to-end"

    step CheckStock { let item    = perform Inventory.check(orderId)?; }
    step Charge     { @once let r = perform Payment.charge(item.price)?; }
    step Ship       { perform Shipping.dispatch(r, item)?; }
}
```

## `perform`

```brief
let result = perform SkillName.functionName(arg1, arg2)?;
perform Notify.sendEmail(user.email)?;  // without assignment
```

## `let` Bindings

```brief
let user = perform UserDB.findById(userId)?;
```

Scoped to the current step.

## Linear Bindings (`@once`)

```brief
@once let handle = perform Payment.charge(amount)?;
perform Order.confirm(handle)?;  // must consume exactly once
```

- **E104** — reused more than once
- **E105** — never consumed

## Effects

```brief
effect Auth {
    fn login(creds: Credentials) -> Session
    fn refresh(token: @once Token) -> @once Token
}
```

Effect functions returning `@once` auto-linearize their call sites.
