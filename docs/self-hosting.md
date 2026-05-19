# Brief Self-Hosting — Stage 1: Mediated Self-Description

> **Stage 1** means: Brief describes compiler pass *structure* as type-checked tasks.
> Rust provides *execution* via skill backends. Stage 2 will replace Rust backends
> with Brief tasks when the language gains sufficient expressiveness.

## Why This Approach (TRIZ Rationale)

The self-hosting contradiction:
- **Want:** Brief compiler described in Brief (ideality, single source of truth)
- **Problem:** Brief v0.5 lacks closures, HM inference, and string processing
  primitives needed to actually replace the Rust implementation

Resolution via **P10 Preliminary Action + P24 Mediator**:
> Instead of waiting for Brief to be expressive enough to replace Rust,
> use the *skill system* as a mediator. Brief describes *what* each pass does;
> Rust skills provide *how*.

## Architecture

```
┌─────────────────────────────────────────────────────┐
│                compiler/*.brief                      │
│   01-lex  02-parse  03-check  04-fmt  05-hir         │
│   06-pipeline (composes all passes)                  │
│                                                      │
│   Type-checked by Brief checker                      │
│   Opaque types at skill boundaries                   │
└──────────────────────┬──────────────────────────────┘
                       │ import skill "..."
┌──────────────────────▼──────────────────────────────┐
│              compiler/skills/                        │
│   LexerPrimitives    ParserPrimitives                │
│   CheckerPrimitives  FormatterPrimitives             │
│   HirPrimitives      CompilerDriver                  │
│                                                      │
│   .briefskill interfaces (ABI v1.0)                  │
│   Verified at load time by skill_loader.rs           │
└──────────────────────┬──────────────────────────────┘
                       │ Rust dispatch
┌──────────────────────▼──────────────────────────────┐
│           briefc/src/skill_backends.rs               │
│   Wraps: lexer.rs  parser.rs  checker.rs             │
│          fmt.rs    hir.rs     runner.rs              │
└─────────────────────────────────────────────────────┘
```

## Key Design Decisions

### Opaque Types
Compiler artifacts (`TokenStream`, `Ast`, `TypedAst`, `HirModule`) are declared as
`opaque type` in Brief files. They pass all type checks and cannot have fields accessed.
This enables type-safe pipelines without exposing Rust struct internals to Brief.

### Sealed Result Types
Each pass returns a concrete sealed type:
- `LexOutcome = Lexed(TokenStream) | LexFailed(DiagnosticSet)`
- `ParseOutcome = Parsed(Ast) | ParseFailed(DiagnosticSet)`
- `CheckOutcome = Checked(TypedAst) | CheckFailed(DiagnosticSet)`
- `FormatOutcome = Formatted | FormatRecovered | FormatFailed(DiagnosticSet)`
- `HirOutcome = Lowered(HirModule) | HirFailed(DiagnosticSet)`

`FormatRecovered` is a **degraded** state (comment trivia lost) — CI must not treat it as green.

### Combinator Rules
| Combinator | Rule |
|------------|------|
| `parallel` | Only for data-independent steps |
| `retry(n)` | Only for `effects [filesystem, network, external-process]` steps |
| `fallback` | For degraded recovery (e.g., formatter with trivia loss) |

## Usage

```bash
# Validate all 6 compiler pass Brief files
brief self-hosting check

# Execute the Brief-mediated pipeline on a source file
brief self-hosting run examples/01-book-flight.brief

# Compare Rust pipeline vs Brief-mediated pipeline
brief self-hosting compare examples/01-book-flight.brief
```

## Stage 1 → Stage 2 Migration

| Compiler Primitive | Stage 1 | Stage 2 Needs |
|-------------------|---------|----------------|
| Tokenizer | Rust `lexer.rs` wrapper | Brief string processing + character iteration |
| Parser | Rust `parser.rs` wrapper | Brief recursive task composition |
| Type checker | Rust `checker.rs` wrapper | Brief constraint logic + sealed type traversal |
| Formatter | Rust `fmt.rs` wrapper | Brief string building + AST match arms |
| HIR lowering | Rust `hir.rs` wrapper | Brief structural transformation tasks |

## Verification

```bash
cargo run --bin brief -- ci                                    # 59/59
cargo run --bin brief -- self-hosting check                    # 6/6 compiler passes valid
cargo run --bin brief -- self-hosting compare examples/01-*.brief  # MATCH
cargo test -q                                                  # all tests pass
```
