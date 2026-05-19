Last updated: v0.5 — Stage 1

# Brief Core Specification

## 1. Purpose & Scope

Brief Core v0.5 defines the minimum Brief subset required for **Stage 1 Mediated Self-Description**: Brief source describes compiler-pass structure, pass inputs/outputs, contracts, and orchestration, while Rust skill backends execute the concrete compiler algorithms.

Stage 1 is therefore **self-descriptive but not fully self-hosting**: the compiler can describe its own passes in Brief, but tokenization, parsing, checking, formatting, and lowering still run through Rust-mediated skills rather than Brief-native implementations.

Stage 2 full self-hosting begins when those Rust-mediated primitives are replaced by Brief-expressed implementations and the Brief pipeline executes the compiler without a Rust-only semantic core.

This split follows the TRIZ resolution used for bootstrap safety: **P10 Preliminary Action** prepares the compiler pipeline in a restricted, checkable subset before full replacement, and **P24 Mediator** inserts Rust skill backends as the temporary execution layer between Brief declarations and compiler behavior.

## 2. Brief Core Subset — IN Scope

Only the following features are part of Brief Core for Stage 1.

| Feature | Why it is needed in Stage 1 |
|---|---|
| Tasks (`task Name : TaskBrief uses [...]`) | Each compiler pass must be represented as a named, type-checked unit with explicit skill dependencies. |
| Steps (sequential, pre/post phase contracts) | Compiler passes need ordered internal phases plus documented invariants before and after each phase boundary. |
| Effects declaration (`effects [cpu, memory, filesystem]`) | Passes must declare their operational side effects so the checker can constrain orchestration and retries. |
| Typed extras record (`extras { field: Type }`) | Pass inputs such as source text, file paths, or prior artifacts must enter the task through statically declared fields. |
| Provides record (`provides { field: Type }`) | Downstream passes need explicit, typed outputs from upstream pass tasks. |
| Sealed types (`sealed type X = A | B | C`) | Pass results must use closed result domains so exhaustiveness can be checked statically. |
| Opaque types (`opaque type TokenStream`) | Rust-owned compiler artifacts must cross Brief boundaries without exposing internal representation. |
| Match expressions on sealed types | Pass orchestration must branch exhaustively over success, degraded, and failure outcomes with named payload bindings. |
| Workflow combinators: `parallel { }`, `retry(n) { }`, `fallback { }` | Stage 1 needs limited orchestration patterns without introducing full general-purpose control flow. |
| Skill imports (`import skill "LexerPrimitives"`) | Every mediated compiler primitive must be bound through an explicit imported ABI surface. |
| `@BriefBuilder` annotation | Compiler pass specs need a marker for composition into the larger compiler pipeline with declared outputs. |
| `@once` linear type annotation | Single-use handles or backend-owned resources must be prevented from being duplicated or dropped silently. |
| Test blocks (`test "name" { ... }`) | Each pass file needs colocated executable conformance checks without leaving the Brief source unit. |

## 3. Brief Core Subset — OUT of Scope for Stage 1

The following features are explicitly excluded from Stage 1 because they are not required for mediated self-description:

- Closures / lambda expressions
- HM type inference across task boundaries
- Recursive types
- Numeric arithmetic expressions
- Higher-kinded types
- `brief gen` AI generation features

Any Stage 1 compiler pass `.brief` file that depends on one of these features is outside Brief Core and is not valid bootstrap input.

## 4. Opaque Type Rules

An opaque type is declared in Brief source as:

```brief
opaque type Name
```

Stage 1 opaque types follow these rules:

- An opaque type is declared by name in a `.brief` file, but Brief never sees its concrete field layout.
- The concrete representation is supplied by an imported skill implementation; Brief type-checking only knows the symbolic type name.
- Opaque values participate in type-checking as atomic values: they may be bound, passed, returned, and wrapped, but their fields cannot be inspected, destructured, or pattern-matched directly.
- Opaque types may be used as task inputs, task outputs, step-local bindings, skill parameters, and skill return values.
- Opaque types may appear inside sealed variants, which is how pass outcomes carry Rust-backed artifacts across typed boundaries.

Example:

```brief
sealed type LexOutcome = Lexed(TokenStream) | LexFailed(DiagnosticSet)
```

In this example, Brief can distinguish `Lexed(...)` from `LexFailed(...)`, but it cannot inspect the internal structure of `TokenStream` or `DiagnosticSet`.

## 5. Sealed Result Types (Canonical Definitions)

The following opaque and sealed types are canonical for Stage 1 compiler pass files.

```brief
// Compiler artifact types (opaque — backed by Rust)
opaque type TokenStream
opaque type Ast
opaque type TypedAst
opaque type HirModule
opaque type DiagnosticSet
opaque type PassSpec
opaque type PassResult

// Result types per pass (concrete sealed types)
sealed type LexOutcome = Lexed(TokenStream) | LexFailed(DiagnosticSet)
sealed type ParseOutcome = Parsed(Ast) | ParseFailed(DiagnosticSet)
sealed type CheckOutcome = Checked(TypedAst) | CheckFailed(DiagnosticSet)
sealed type FormatOutcome = Formatted | FormatRecovered | FormatFailed(DiagnosticSet)
sealed type HirOutcome = Lowered(HirModule) | HirFailed(DiagnosticSet)
```

These definitions are the shared type vocabulary for the Stage 1 self-description pipeline and must not diverge across pass files.

## 6. Combinator Usage Rules

Workflow combinators are intentionally narrow in Brief Core.

| Combinator | Valid When | Invalid Example |
|---|---|---|
| `parallel { A, B }` | The referenced steps have **no data dependency** and neither step requires outputs produced by the other. | `parallel { ParseSource, ValidateImports }` where parse output feeds import validation. |
| `retry(n) { A }` | The referenced step performs recoverable external work and declares effects such as `filesystem`, `network`, or `external-process`. | `retry(2) { TypeCheck }` because type checking is deterministic CPU work, so retry adds no semantic value. |
| `fallback { A, B }` | Step `A` may fail or degrade, and step `B` is an explicitly distinct recovery path. | Treating formatter recovery as fully green is invalid: `FormatRecovered` is a degraded result, not equivalent to `Formatted`, even though formatter fallback is a valid use case. |

Additional Stage 1 rules:

- `parallel` may only reference declared steps and does not relax type or ordering constraints on produced values.
- `retry` is forbidden for pure in-memory deterministic steps.
- `fallback` does not erase outcome distinctions; downstream matches must still handle degraded variants explicitly.

## 7. Skill ABI Contract

Every Stage 1 `.briefskill` interface file is part of the compiler bootstrap ABI and must satisfy the following contract:

- The file must declare `abi_version: "1.0"`.
- Each declared function must specify the exact parameter types and exact return type used by the corresponding Brief task.
- Opaque types referenced in the interface must match the symbolic type names used in the pass `.brief` files.
- The Rust backend is required to implement the same function names, arity, parameter ordering, and return types as the `.briefskill` declaration.
- Any mismatch between the `.briefskill` declaration and the Rust backend is a **load-time error**, not a recoverable runtime error.

Minimal shape:

```brief
abi_version: "1.0"

interface LexerPrimitives {
    fn lex(spec: PassSpec) -> LexOutcome
}
```

Stage 1 intentionally fails fast at skill load time so compiler-pass validation is deterministic before execution begins.

## 8. Stage 1 → Stage 2 Migration Table

| Compiler Primitive | Stage 1 (Rust skill backend) | Stage 2 (Brief replacement needs) |
|---|---|---|
| Tokenizer | LexerPrimitives Rust wrapper | Brief string processing + regex effects |
| Parser | ParserPrimitives Rust wrapper | Brief recursive task composition |
| Type checker | CheckerPrimitives Rust wrapper | Brief constraint logic + sealed type traversal |
| Formatter | FormatterPrimitives Rust wrapper | Brief string building + AST pattern matching |
| HIR lowering | HirPrimitives Rust wrapper | Brief structural transformation tasks |

## 9. Verification Criteria for Stage 1 Completion

A Stage 1 implementation is complete only when all of the following are true:

- [ ] `cargo run --bin brief -- ci` → 56/56 (50 examples + 6 compiler passes)
- [ ] `cargo run --bin brief -- self-hosting check` → `6/6 compiler passes valid`
- [ ] `cargo run --bin brief -- self-hosting compare <file>` → identical diagnostics from both pipelines
- [ ] All pass `.brief` files use only Brief Core subset features
- [ ] No new Rust language features required
