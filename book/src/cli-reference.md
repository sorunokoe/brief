# CLI Reference

Quick reference for all `brief` subcommands. Run `brief --help` or `brief <subcommand> --help` for full flag documentation.

## Checking & Running

| Command | Description |
|---------|-------------|
| `brief check <file>` | Type-check only — fast, CI-friendly, exits non-zero on errors |
| `brief run <file>` | Validate then execute |
| `brief build <file>` | Compile to native binary (requires LLVM 18) |
| `brief build <file> --emit-ir` | Emit LLVM IR to a `.ll` file |
| `brief build <file> --target wasm32-unknown-unknown` | Compile to WebAssembly |

## Testing & Formatting

| Command | Description |
|---------|-------------|
| `brief test <file>` | Run `test { }` blocks against the mock skill system |
| `brief fmt <file>` | Auto-format to canonical style (in-place) |
| `brief fmt <file> --check` | Fail if file is not already canonical (CI gate) |

## Project Management

| Command | Description |
|---------|-------------|
| `brief init <name>` | Scaffold a new project in `<name>/` |
| `brief watch <path>` | Live re-check on every file save |
| `brief add skill <Name>` | Install a skill from the registry |

## Documentation & Generation

| Command | Description |
|---------|-------------|
| `brief doc <file>` | Render Markdown documentation from a `.brief` file |
| `brief gen "<description>"` | AI-generate a `.brief` file from natural language |
| `brief skillgen <path>` | Generate a `.briefskill` interface from skill markdown |

## Advanced

| Command | Description |
|---------|-------------|
| `brief repl` | Interactive REPL |
| `brief lsp` | LSP server (stdio) — for editor integration |

---

## Exit Codes

| Code | Meaning |
|------|---------|
| `0` | No errors |
| `1` | One or more errors |
| `2` | Internal error (bug — please report) |

---

## CI Usage

```yaml
- name: Check all briefs
  run: brief check path/to/file.brief

- name: Enforce canonical formatting
  run: brief fmt --check path/to/file.brief
```

See [`.github/workflows/ci.yml`](https://github.com/yourusername/brief/blob/main/.github/workflows/ci.yml) for a full example.
