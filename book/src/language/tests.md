# Test Blocks

```brief
test "login succeeds" {
    mock Auth.login    { return { token: "abc" } }
    mock UserDB.findBySession { return { id: "u1", name: "Alice" } }
    run LoginTask
    assert result.ok == true
}
```

## Running

```bash
brief test examples/14-test-suite.brief
```

## Test Isolation

Each block is isolated — mocks don't leak between tests. No real IO occurs.

## CI

```yaml
- name: Run Brief tests
  run: brief test briefs/login.brief
```
