# Brief Language Specification — v0.1

> *"If it compiles, the AI has everything it needs."*

This document is the normative reference for the Brief language, version 0.1.

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
fn         async      await      return
```

### 3.2 Decorators and Attributes

Decorators begin with `@` and appear before declarations:

```brief
@BriefBuilder
task ProfileScreen : TaskBrief { ... }
```

Attributes appear on struct fields:

```brief
struct FigmaURL {
    url: @url String
}
```

Built-in attributes: `@url`, `@nonEmpty`, `@matches("pattern")`

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

| Code | Meaning |
|------|---------|
| `E001` | Parse error |
| `E101` | Task missing required `goal` field |
| `E102` | Skill in `uses [...]` is not imported |
| `E103` | `perform X.fn()` uses a skill not in the task's `uses [...]` clause |
| `E104` | Struct field attribute constraint fails (e.g. `@url` on non-URL string) |
| `E105` | Type mismatch (v0.1 — structural) |

### Warnings (non-fatal — task may still be handed to AI)

| Code | Meaning |
|------|---------|
| `W101` | Imported skill has no `.briefskill` interface file |
| `W102` | Skill interface file is stale (checksum mismatch) |

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
top_decl       ::= import_decl | type_decl | struct_decl
                 | protocol_decl | effect_decl | task_decl

import_decl    ::= 'import' 'skill' STRING

type_decl      ::= 'sealed' 'type' Ident type_params? '=' type_variant ('|' type_variant)*
type_variant   ::= Ident ( '(' type_ref (',' type_ref)* ')' )?

struct_decl    ::= 'struct' Ident type_params? '{' struct_field* '}'
struct_field   ::= Ident ':' attribute* type_ref

protocol_decl  ::= 'protocol' Ident type_params? '{' fn_sig* '}'
effect_decl    ::= 'effect'   Ident type_params? '{' fn_sig* '}'

fn_sig         ::= 'fn' Ident type_params? '(' param_list? ')' '->' type_ref
param_list     ::= param (',' param)*
param          ::= Ident ':' type_ref

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

stmt           ::= let_stmt | expr_stmt
let_stmt       ::= 'let' Ident '=' expr ';'
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

## 11. Versioning

This document describes Brief v0.1. The language is under active development.
Syntax and semantics are subject to change between minor versions until v1.0.

For the current grammar accepted by the compiler, run:
```bash
brief --grammar  # (available in v0.2)
```
