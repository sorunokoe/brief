# Contributing to Brief

Thank you for contributing to Brief! This is a community-first project.

## Ways to Contribute

- **Language design** — open an issue proposing new syntax or features
- **Bug reports** — use the bug report issue template
- **Code** — fix bugs, implement features, improve error messages
- **Skills** — contribute reusable `.briefskill` interfaces to the registry (coming soon)
- **Docs** — improve `docs/` and examples

## Development Setup

```bash
git clone https://github.com/yourusername/brief
cd brief
cargo build
cargo test    # all 9 tests must pass
```

## Submitting a PR

1. Fork the repo and create a branch: `git checkout -b feature/my-change`
2. Make your changes — tests are in each `src/*.rs` file using `#[cfg(test)]`
3. Ensure `cargo test` passes and `cargo build` has no new warnings
4. Run the examples: `brief check examples/01-hello.brief`
5. Open a PR — fill in the template

## Code Style

- Rust: use `cargo fmt` and `cargo clippy --all`
- `.brief` files: indentation is 4 spaces
- Error messages must follow the format in `errors.rs`: `brief[Exx]: ...` + `fix: ...` hint

## Project Structure

```
briefc/src/
├── main.rs      — CLI entry (brief run / check / gen / skillgen / repl)
├── lexer.rs     — Token definitions (Logos-based)
├── ast.rs       — AST node types
├── parser.rs    — Recursive-descent parser
├── checker.rs   — Semantic validation
├── runner.rs    — Tree-walking execution (v0.0.1 interpreter)
├── skillgen.rs  — .briefskill interface generation
├── gen.rs       — brief gen command
└── errors.rs    — Error types and pretty-printer
```

## Roadmap

See [plan in session state] or the `## Roadmap` section in README.md.
The current focus is **v0.1**: full type system + skill imports + LLM gen.

## Questions?

Open a [GitHub Discussion](https://github.com/yourusername/brief/discussions).
