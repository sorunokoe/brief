# Contributing to Brief

Thank you for contributing to Brief! This is a community-first project.

## Ways to Contribute

- **Language design** — open an issue proposing new syntax or features
- **Bug reports** — use the bug report issue template
- **Code** — fix bugs, implement features, improve error messages
- **Skills** — contribute reusable `.briefskill` interfaces to the registry
- **Examples** — add or improve examples in `examples/` (numbered 01–32+)
- **Docs** — improve `docs/` and examples

## Development Setup

```bash
git clone https://github.com/sorunokoe/brief
cd brief
cargo build
cargo test                          # 93 tests must all pass
```

## PR Checklist

1. Fork the repo and create a branch: `git checkout -b feature/my-change`
2. Make your changes — tests live in `#[cfg(test)]` modules in each `src/*.rs`
3. `cargo test` — all 93 tests must pass
4. `brief check examples/01-hello.brief` — and any examples you touched
5. `brief fmt --write examples/<your-file>.brief` — enforce canonical style
6. `brief fmt --check examples/<your-file>.brief` — confirm idempotent
7. Open a PR — fill in the template

**The CI gate runs all of these automatically** (`.github/workflows/ci.yml`).

## Code Style

- Rust: use `cargo fmt` and `cargo clippy --all`
- `.brief` files: always run `brief fmt --write` before committing
- Error messages must include a `fix:` hint — see `errors.rs` for the pattern
- New examples must be numbered sequentially and added to `examples/README.md`

## Project Structure

```
briefc/src/
├── main.rs      — CLI entry (brief run / check / build / test / fmt / doc / watch / init)
├── lexer.rs     — Token definitions (Logos-based)
├── ast.rs       — AST node types
├── parser.rs    — Recursive-descent parser (93 unit tests)
├── checker.rs   — Semantic validation (E101–E106, W101–W102)
├── typeck.rs    — Type checking and inference
├── fmt.rs       — Canonical source formatter (brief fmt)
├── tester.rs    — Test runner (brief test) — own mock/run/assert parser
├── doc.rs       — Doc generator (brief doc)
├── lsp.rs       — LSP server (brief lsp)
├── codegen.rs   — LLVM IR emission (brief build)
├── runner.rs    — Tree-walking execution (brief run)
├── repl.rs      — Interactive REPL (brief repl)
├── skillgen.rs  — .briefskill interface generation (brief skillgen)
├── registry.rs  — Skill package manager (brief add skill)
└── gen.rs       — AI generation (brief gen)
```

## Adding a New Error Code

1. Add the variant to `ErrorCode` in `errors.rs`
2. Emit it in `checker.rs` or `typeck.rs` with a `fix:` hint
3. Add an intentional-error example to `examples/errors/`
4. Add a unit test in `checker.rs` that triggers the error

## Roadmap

See the `## Roadmap` section in `README.md`.
Current version: **v0.4** — 32 examples, test blocks, MCP support, CI/release workflows.
Next: **v1.0** — `brief watch`, `brief init`, mdBook docs site.

## Questions?

Open a [GitHub Discussion](https://github.com/sorunokoe/brief/discussions).
