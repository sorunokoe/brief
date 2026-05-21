# Changelog

All notable changes to Brief are documented here.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
Brief uses [Semantic Versioning](https://semver.org/).

## [Unreleased]

## [1.0.0] — 2026-05-21

### Added
- `brief check` — fast static analysis (<1s, no network, CI-safe)
- `brief verify` — seal the contract, write `.brief.lock`
- `brief serve` — MCP server with lock gate; `--draft` mode for scope-only enforcement
- `brief run` — real execution against skill MCP servers
- `brief test` / `brief test --live` — mock and live test runner
- `brief gen` — LLM-assisted `.brief` generation with compiler feedback loop
- `brief skillgen` — `.briefskill` interface generation from README
- `brief fmt` — canonical source formatter
- `brief doc` — Markdown doc generation
- `brief watch` — live re-check on every save
- `brief init` — scaffold a new Brief project
- `brief ci` — check all CI examples from `brief.toml`
- `brief lsp` — LSP server for editor integration
- `brief suggest` — AI-powered fix suggestions
- `brief audit` — runtime call log auditing
- `brief policy-check` / `brief policy suggest` — policy enforcement
- `brief models` — list available LLM models
- `brief skillsync` — sync `.briefskill` interfaces from live MCP servers
- `needs {}` block — prerequisite verification (env, feature, config)
- `forbids {}` block — scope boundaries enforced statically and at runtime
- `allow {}` / `deny {}` — argument-level policy enforcement
- Builtin verifiers: `builtin:url`, `builtin:local-path`, `builtin:github-repo`, `builtin:shell-command`
- Linear types (`@once`), type aliases, effect groups
- 40+ annotated example files
- VS Code grammar and LSP support
- mdBook documentation site
- Cross-platform release binaries (Linux, macOS, Windows)

[Unreleased]: https://github.com/sorunokoe/brief/compare/v1.0.0...HEAD
[1.0.0]: https://github.com/sorunokoe/brief/releases/tag/v1.0.0
