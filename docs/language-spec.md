# Brief Language Specification — v0.4

> *"If it compiles, the AI has everything it needs."*

This document is the normative reference for the Brief language, version 0.3.

---

## 1. Overview

Brief is a domain-specific language for AI-assisted development workflow tasks. It is NOT a general-purpose programming language. Every language feature exists to serve one goal: **making it structurally impossible to hand an AI agent an incomplete task description.**

A `.brief` file describes:
- What work needs to be done (`goal`)
- What external capabilities the work requires (`import skill`, `uses`)
- The steps of the workflow (`step`, `perform`)
- The types and effects the workflow operates over

---

## 2. File Format

Brief source files use the `.brief` extension. Encoding is UTF-8. Line comments begin with `//`. There are no block comments in v0.1.

---

## 3. Lexical Structure

### 3.1 Keywords

The following identifiers are reserved:

```
task       step       import     skill      uses       perform
let        sealed     type       struct     protocol   effect
fn         async      await      return     test
```

### 3.2 Decorators and Attributes

Decorators begin with `@` and appear before declarations or `let` bindings:

```brief
@BriefBuilder
task ProfileScreen : TaskBrief { ... }
```

```brief
@once let handle = perform Payment.charge(amount)?;   // linear binding
```

Attributes appear on struct fields and effect function return types:

```brief
struct FigmaURL {
    url: @url String
}

effect Payment {
    fn charge(amount: Int) -> @once PaymentHandle   // linear return type
}
```

Built-in attributes: `@url`, `@nonEmpty`, `@matches("pattern")`, `@once`

### 3.3 Literals

- **String literals:** `"..."` — UTF-8, supports `\"` and `\\` escape sequences
- **Identifiers:** `[a-zA-Z_][a-zA-Z0-9_]*`

### 3.4 Operators and Punctuation

```
{  }  [  ]  (  )  :  ,  .  =  ?  ;  ->  <  >  |  @
```

---

## 4. Type System

### 4.1 Primitive Types

| Type | Description |
|------|-------------|
| `String` | UTF-8 string |
| `Bool` | Boolean |
| `Int` | 64-bit signed integer |
| `Float` | 64-bit IEEE 754 float |
| `Unit` | The "no value" type (return type of side-effecting steps) |

### 4.2 Generic Types

```brief
type Result<T, E> = Ok(T) | Err(E)
type Option<T>    = Some(T) | None
```

`T?` is syntactic sugar for `Option<T>`.

### 4.3 Sealed (Algebraic) Types

```brief
sealed type Platform = iOS | Android | Web | All
sealed type TaskStatus = Pending | Running | Done(String) | Failed(String)
```

Sealed types are closed — the compiler knows all variants. They form the foundation of the effect row system.

### 4.4 Structs

```brief
struct FigmaURL {
    url: @url String
}

struct UserProfile {
    id:    @nonEmpty String
    name:  @nonEmpty String
    email: @matches("[^@]+@[^@]+") String
}
```

**Simplified refinements** are checked at construction, surfaced as compile errors.

### 4.5 Protocols (Structural Typing)

```brief
protocol Renderable {
    fn render() -> Component
}

protocol Fetchable<T> {
    fn fetch(id: @nonEmpty String) -> Result<T, FetchError>
}
```

Brief uses structural protocols: a type conforms to a protocol if it has the required members, without explicit declaration.

### 4.6 Effects and the `perform` keyword

Brief tracks side effects in the type system. An *effect* declares the operations a skill provides:

```brief
effect GraphQL {
    fn query<T>(op: Operation) -> Result<T, QueryError>
    fn mutation<T>(op: Mutation) -> Result<T, MutationError>
    fn schema(name: @nonEmpty String) -> Schema
}

effect DesignSystem {
    fn profileCard(user: UserProfile, theme: Theme?) -> Result<Component, DesignError>
    fn button(label: @nonEmpty String, style: ButtonStyle) -> Component
    fn colorToken(name: @nonEmpty String) -> Result<Color, TokenError>
}
```

Effects are performed with the `perform` keyword:

```brief
let user = perform GraphQL.query(UserProfileQuery)?
```

The `?` propagates `Err` — equivalent to `match result { Ok(v) => v, Err(e) => return Err(e) }`.

A task's `uses [X, Y]` clause declares which effects the task needs. The compiler enforces this:
- A `perform X.fn()` requires `X` to be in `uses [...]`
- A skill in `uses [...]` must be `import`-ed at the top of the file
- If a required `.briefskill` interface is missing, a warning is emitted with the exact fix command

---

## 4.7 Type Aliases (Refinement Aliases)

A type alias binds a name to a refined version of an existing type:

```brief
type Email      = @matches("[^@]+@[^@]+") String
type NonEmpty   = @nonEmpty String
type Url        = @url String
```

Type aliases are resolved at compile time. Using `Email` as a field or parameter type is equivalent to declaring `@matches("[^@]+@[^@]+") String` inline.

**Syntax:**
```
type_alias_decl ::= 'type' Ident '=' attribute+ type_ref
```

### 4.8 Effect Group Aliases

An effect group alias names a set of skills that always appear together in a `uses [...]` clause:

```brief
type AuthEffects      = [Auth, Session]
type SecurityEffects  = [Auth, Session, Permissions]
type FullUserEffects  = [Auth, Session, Permissions, UserService, AuditLog]
```

Effect groups are expanded by the compiler when used in a task's `uses` clause:

```brief
task Login : TaskBrief uses [SecurityEffects] {   // expands to [Auth, Session, Permissions]
    goal = "authenticate user"
    ...
}
```

**Rules:**
- Group members must each be resolvable as either a skill name or another group alias.
- Circular group references are rejected at compile time.
- Groups can appear alongside individual skills: `uses [SecurityEffects, AuditLog]`.

**Syntax:**
```
effect_group_decl ::= 'type' Ident '=' '[' Ident (',' Ident)* ']'
```

### 4.9 Linear Types (`@once`)

Linear types enforce that a value is consumed **exactly once** in a step body. They prevent resource leaks and double-use of handles.

**Declaration on effect functions:**
```brief
effect Payment {
    fn charge(amount: Int) -> @once PaymentHandle
    fn confirm(handle: PaymentHandle) -> PaymentConfirmation
    fn refund(handle: PaymentHandle) -> RefundResult
}
```

A function annotated with `-> @once T` means: the returned value of type `T` must be used in exactly one subsequent `perform` call within the same step.

**Declaration on `let` bindings:**
```brief
step Process {
    @once let handle = perform Payment.charge(amount)?;   // handle is linear
    let confirm = perform Payment.confirm(handle)?;       // ✅ consumed once
}
```

**Error cases:**
- `E104 LinearBindingReused` — a `@once` binding is passed to `perform` more than once in the same step.
- `E105 LinearBindingDropped` — a `@once` binding is declared but never passed to any `perform` call in the step.

```brief
step BadDouble {
    @once let h = perform Payment.charge(100)?;
    let _ = perform Payment.confirm(h)?;
    let _ = perform Payment.refund(h)?;   // error[E104]: @once binding 'h' consumed 2 times
}

step BadDrop {
    @once let h = perform Payment.charge(100)?;
    // error[E105]: @once binding 'h' is never consumed — resource leak
}
```

---

### 4.10 Test Blocks (`test`)

Test blocks declare in-source tests that can be run with `brief test`. They live at the top level of a `.brief` file, after all task declarations.

**Syntax:**
```
'test' STRING '{' test_body '}'

test_body ::=
    mock_decl*
    run_stmt
    assert_stmt*

mock_decl   ::= 'mock' Ident '{' fn_mock+ '}'
fn_mock     ::= 'fn' Ident '(' ident_list ')' '->' expr
run_stmt    ::= 'run' Ident ('.' Ident)?
assert_stmt ::= 'assert' ('not')? assertion_expr
assertion_expr ::=
      'performed' Ident '.' Ident
    | 'result' 'is' ('Ok' | 'Err')
    | 'eq' expr expr
```

**Example:**
```brief
test "FetchProfile loads user via GraphQL" {
    mock GraphQL {
        fn query(op) -> Ok(User { id: "u1", name: "Ada Lovelace" })
    }

    run FetchProfile
    assert performed GraphQL.query
    assert result is Ok
}

test "Login does not call DesignSystem" {
    mock Auth {
        fn login(email, password) -> Ok("tok_abc")
    }

    run Login.Authenticate
    assert performed Auth.login
    assert not performed DesignSystem.profileCard
}
```

**Notes:**
- `brief test <file>` executes all `test { }` blocks in the file using the mock skill system.
- `brief check <file>` parses and validates the task declarations; test bodies are skipped during type-checking (mocks replace real skill types).
- `run Task.Step` runs a single step in isolation. `run Task` runs all steps in sequence.
- Test blocks do **not** contribute to the task's `uses` declaration — they are invisible to the type checker.

---

## 5. Task Declaration

A *task* is the top-level unit of work in Brief.

### 5.1 Syntax

```brief
decorator*
'task' Ident ':' 'TaskBrief' ('uses' '[' ident_list ']')? '{' task_body '}'

task_body ::=
    ('goal' '=' STRING)?
    ('extras' '=' '[' kv_pairs ']')?
    step_decl*
```

### 5.2 Required Fields

- `goal` — **required** in v0.1. A human-readable description of what this task accomplishes. This is the primary input the AI agent uses to understand the task scope.

### 5.3 Optional Fields

- `extras` — a string key-value map for arbitrary metadata. Common keys:
  - `"platform"` — `"iOS"`, `"Android"`, `"KMP"`, `"Web"`
  - `"figmaURL"` — Figma node URL for UI tasks
  - `"graphqlSchema"` — path or URL to GraphQL schema for domain tasks

### 5.4 The `@BriefBuilder` Decorator

`@BriefBuilder` marks a task as using the composable builder pattern. In v0.1, this is a marker that indicates the task may be composed with other tasks. Full composability semantics are defined in v0.2.

### 5.5 Example

```brief
import skill "DesignSystem"
import skill "GraphQL"

@BriefBuilder
task ProfileScreen : TaskBrief uses [DesignSystem, GraphQL] {
    goal   = "Display the user profile screen with avatar, name, and recent activity"
    extras = ["platform": "iOS", "figmaURL": "https://figma.com/file/abc?node-id=1:2"]

    step FetchProfile {
        let user = perform GraphQL.query(UserProfileQuery)?
    }

    step RenderProfile {
        let card  = perform DesignSystem.profileCard(user, theme: .default)?
        let badge = perform DesignSystem.button("Edit Profile", style: .secondary)?
        display(card, badge)
    }
}
```

---

## 6. Steps

Steps describe the sequenced workflow of a task. They are ordered — step bodies execute in declaration order.

```brief
step StepName {
    statement*
}
```

### 6.1 Statements

- `let x = expr;` — bind a value to a name (immutable binding)
- `expr;` — evaluate an expression for its side effect

### 6.2 Expressions

| Form | Description |
|------|-------------|
| `perform Skill.fn(args)?` | Perform an effect; `?` propagates Err |
| `await expr` | Await an async expression (v0.1: async effects are managed transparently) |
| `x.method(args)` | Method call |
| `f(args)` | Function call |
| `x` | Variable reference |
| `"..."` | String literal |

---

## 7. Skill System

### 7.1 Import and Resolution

```brief
import skill "DesignSystem"
```

Resolution order for `DesignSystem`:
1. `<file_dir>/.claude/skills/DesignSystem/DesignSystem.briefskill`
2. `<cwd>/.claude/skills/DesignSystem/DesignSystem.briefskill`
3. `~/.brief/skills/DesignSystem.briefskill`
4. Brief skill registry (v0.2)

### 7.2 The `.briefskill` Interface File

`.briefskill` files are auto-generated by `brief skillgen` — they are **never written by hand**.

```brief
// Auto-generated by `brief skillgen v0.1`
// Source: .claude/skills/DesignSystem/README.md (sha256: abc123...)
// Regenerate: brief skillgen .claude/skills/DesignSystem/
// Do not edit manually.

interface DesignSystem {
    fn profileCard(user: UserProfile, theme: Theme?) -> Result<Component, DesignError>
    fn button(label: @nonEmpty String, style: ButtonStyle) -> Component
    fn colorToken(name: @nonEmpty String) -> Result<Color, TokenError>
}
```

### 7.3 Staleness Detection

The `.briefskill` header contains a SHA-256 checksum of the source `README.md`. When the checksum does not match the current `README.md`, `brief check` emits a `W102` warning:

```
warning[W102]: skill interface 'DesignSystem' is stale
  → .claude/skills/DesignSystem/DesignSystem.briefskill was generated from an older README.md
  fix: brief skillgen .claude/skills/DesignSystem/
```

---

## 8. Error Codes

### Errors (fatal — task is invalid)

| Code | Name | Meaning |
|------|------|---------|
| `E001` | `ParseError` | Syntax error — unexpected token or malformed declaration |
| `E101` | `MissingGoal` | Task is missing the required `goal` field |
| `E102` | `UndeclaredSkillInUses` | Skill name in `uses [...]` clause has no matching `import skill` |
| `E103` | `PerformWithoutUses` | `perform X.fn()` — `X` is not declared in the task's `uses [...]` clause |
| `E104` | `LinearBindingReused` | A `@once` binding is consumed more than once in the same step |
| `E105` | `LinearBindingDropped` | A `@once` binding is declared but never consumed in its step |
| `E106` | `UnknownEffectGroup` | `uses [...]` references an effect group alias that was never declared |
| `E201` | `UnknownType` | A type name cannot be resolved to any declaration in scope |
| `E202` | `WrongArgCount` | `perform` call passes wrong number of arguments to a typed effect function |
| `E203` | `AttributeConstraint` | Struct field attribute constraint fails (e.g. `@url` on non-URL string) |

### Warnings (non-fatal — task may still be handed to AI)

| Code | Name | Meaning |
|------|------|---------|
| `W101` | `MissingSkillInterface` | Imported skill has no `.briefskill` interface file; type checking is partial |
| `W102` | `StaleSkillInterface` | Skill interface file checksum does not match current `README.md` |

### Diagnostic format

Every diagnostic includes:
1. A code (`error[E103]` or `warning[W101]`)
2. A human-readable description
3. A source span (`→ file.brief:line:col`)
4. A `fix:` suggestion with the exact command or code change to resolve it

```
error[E103]: effect 'GraphQL' is performed but not declared in `uses [...]`
  → examples/02-profile-screen.brief:14:19
  fix: add 'GraphQL' to the task's `uses` clause

warning[W101]: skill 'DesignSystem' has no interface file
  → examples/02-profile-screen.brief:1:1
  fix: .claude/skills/DesignSystem/DesignSystem.briefskill not found — run: brief skillgen .claude/skills/DesignSystem/
```

---

## 9. Standard Library

Brief's standard library is defined in `briefs/core/` and `briefs/effects/`.

### Core Types (v0.1)

```brief
// briefs/core/Result.brief
sealed type Result<T, E> = Ok(T) | Err(E)

// briefs/core/Option.brief
sealed type Option<T> = Some(T) | None

// briefs/core/String.brief
struct NonEmptyString {
    value: @nonEmpty String
}
```

### Core Effects (v0.1)

```brief
// briefs/effects/IO.brief
effect IO {
    fn print(message: String) -> Unit
    fn readLine() -> Result<String, IOError>
}

// briefs/effects/Async.brief
effect Async {
    fn spawn<T>(task: TaskBrief) -> Handle<T>
    fn await<T>(handle: Handle<T>) -> Result<T, AsyncError>
}
```

---

## 10. Grammar (EBNF)

```ebnf
program        ::= top_decl*
top_decl       ::= import_decl | sealed_type_decl | type_alias_decl
                 | effect_group_decl | struct_decl
                 | protocol_decl | effect_decl | task_decl | test_decl

import_decl    ::= 'import' 'skill' STRING

sealed_type_decl  ::= 'sealed' 'type' Ident type_params? '=' type_variant ('|' type_variant)*
type_variant      ::= Ident ( '(' type_ref (',' type_ref)* ')' )?

type_alias_decl   ::= 'type' Ident '=' attribute+ type_ref

effect_group_decl ::= 'type' Ident '=' '[' Ident (',' Ident)* ']'

struct_decl    ::= 'struct' Ident type_params? '{' struct_field* '}'
struct_field   ::= Ident ':' attribute* type_ref

protocol_decl  ::= 'protocol' Ident type_params? '{' fn_sig* '}'
effect_decl    ::= 'effect'   Ident type_params? '{' fn_sig* '}'

fn_sig         ::= 'fn' Ident type_params? '(' param_list? ')' '->' ret_type
ret_type       ::= attribute* type_ref
param_list     ::= param (',' param)*
param          ::= Ident ':' attribute* type_ref

type_params    ::= '<' Ident (',' Ident)* '>'
type_ref       ::= Ident type_args? '?'?
type_args      ::= '<' type_ref (',' type_ref)* '>'

attribute      ::= '@' Ident ( '(' STRING ')' )?

task_decl      ::= decorator* 'task' Ident ':' 'TaskBrief'
                   ( 'uses' '[' ident_list ']' )?
                   '{' task_body '}'
task_body      ::= ('goal' '=' STRING)?
                   ('extras' '=' '[' kv_pairs ']')?
                   step_decl*
step_decl      ::= 'step' Ident '{' stmt* '}'

test_decl      ::= 'test' STRING '{' test_body '}'
                 (* test_body uses mock/run/assert syntax; parsed by brief test, *)
                 (* skipped by brief check — see §4.10 for test body grammar *)

stmt           ::= let_stmt | expr_stmt
let_stmt       ::= attribute* 'let' Ident '=' expr ';'
expr_stmt      ::= expr ';'

expr           ::= 'perform' Ident '.' Ident type_args? '(' arg_list ')' '?'?
                 | 'await' expr
                 | Ident '.' Ident '(' arg_list ')'
                 | Ident '(' arg_list ')'
                 | Ident
                 | STRING

arg_list       ::= ( expr (',' expr)* )?
decorator      ::= '@' Ident ( '(' arg_list ')' )?
ident_list     ::= Ident (',' Ident)*
kv_pairs       ::= STRING ':' STRING (',' STRING ':' STRING)*
```

---

## 11. CLI Reference

| Command | Description |
|---------|-------------|
| `brief check <file>.brief` | Type-check only — fast, CI-friendly. Exit code 0 = valid. |
| `brief run <file>.brief` | Validate then execute the task. |
| `brief build <file>.brief` | Compile to native binary via LLVM. |
| `brief build <file>.brief --emit-ir` | Emit LLVM IR for inspection. |
| `brief build <file>.brief --target wasm32-unknown-unknown` | Compile to WASM. |
| `brief test <file>.brief` | Run `test { }` blocks with mock skill system. |
| `brief fmt <file>.brief` | Auto-format to canonical style (idempotent). |
| `brief doc <file>.brief` | Generate Markdown documentation from declarations. |
| `brief doc <file>.brief --output <path>` | Write generated docs to file. |
| `brief repl` | Interactive REPL (tree-walking, fast iteration). |
| `brief lsp` | Start LSP server on stdio (for editor integration). |
| `brief gen "<description>"` | AI-generate a `.brief` file from natural language. |
| `brief skillgen <skill-path>` | Generate `.briefskill` interface from skill README. |
| `brief add skill <Name>` | Install a skill from the registry. |
| `brief add skill ./path/` | Install a skill from a local directory. |
| `brief add skill --list` | List available skills in the registry. |

---

## 12. Versioning

This document describes Brief v0.4. The language is under active development.
Syntax and semantics may change between minor versions until v1.0.

**Version history:**
- `v0.1` — Core language: tasks, steps, effects, sealed types, structs, protocols, skill imports
- `v0.2` — Ecosystem: `brief test`, `brief fmt`, LSP go-to-def/find-refs, WASM, skill registry
- `v0.3` — Power types: `@once` linear types, type aliases, effect groups, `brief doc`
- `v0.4` — Test block support in main parser (`brief check` handles `test { }` files); `@mcp` alias attribute; 32 examples (27–32: composition, AI pipeline, platform branching, event sourcing, concurrency, MCP)
