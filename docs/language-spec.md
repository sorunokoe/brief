# Brief Language Specification — v1.0

> *"If it compiles, the AI has everything it needs."*

This document is the normative reference for the Brief language, version 1.0.

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
fn         async      await      match      return     test
extras     provides   effects    pre        post
parallel   retry      fallback
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
{  }  [  ]  (  )  :  ,  .  =  ?  ;  ->  =>  <  >  |  @
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

Linear types enforce that a value is consumed **exactly once**. They prevent resource leaks and double-use of handles. The `@once` checker tracks bindings both **within a step** and **across steps** in a task.

**Declaration on effect functions:**
```brief
effect Payment {
    fn charge(amount: Int) -> @once PaymentHandle
    fn confirm(handle: PaymentHandle) -> PaymentConfirmation
    fn refund(handle: PaymentHandle) -> RefundResult
}
```

A function annotated with `-> @once T` returns a linear value that must be passed to exactly one `perform` call during the task's lifetime.

**Declaration on `let` bindings:**
```brief
step Process {
    @once let handle = perform Payment.charge(amount)?;   // handle is linear
    let confirm = perform Payment.confirm(handle)?;       // ✅ consumed in same step
}
```

**Cross-step linear tracking:**

If a `@once` binding is declared in step N but **not consumed there**, the compiler promotes it to task-level tracking. It must then be consumed in exactly one later step:

```brief
step Acquire {
    @once let token = perform Auth.acquireToken(scope)?;
    // token not used here → promoted to cross-step tracking
}

step Use {
    let result = perform Auth.callWithToken(token, endpoint)?;  // ✅ consumed once across steps
}
```

**Error cases:**
- `E104 LinearBindingReused` — a `@once` binding is consumed more than once (within a step, or counting across steps).
- `E105 LinearBindingDropped` — a `@once` binding is declared but never consumed in any step of the task.

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

// Cross-step double-use:
step Acquire {
    @once let token = perform Auth.acquireToken(scope)?;
}
step UseA { let _ = perform Auth.callWithToken(token, endpointA)?; }
step UseB { let _ = perform Auth.callWithToken(token, endpointB)?; }  // error[E104]: consumed 2 times across steps
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
    (
        'extras' '=' '[' kv_pairs ']'      // deprecated — emits W103
      | 'extras' '{' typed_fields '}'
    )?
    ('provides' '{' typed_fields '}')?
    ('effects' '[' ident_list ']')?
    step_group_decl*
    step_decl*
```

### 5.2 Required Fields

- `goal` — **required** in v0.1. A human-readable description of what this task accomplishes. This is the primary input the AI agent uses to understand the task scope.

### 5.3 Extras

Brief supports two `extras` forms:

- Legacy string-map syntax:

  ```brief
  extras = ["key": "value", "version": "1.0"]  // deprecated — emits W103
  ```

- Typed record syntax:

  ```brief
  extras {
      platform: Platform
      environment: Environment
  }
  ```

Typed extras declare metadata or runtime inputs the compiler can check. Each field type must resolve to a declared type (typically a sealed type) or to one of the built-in scalars `String`, `Bool`, `Int`, or `Float`. If an extras field references an unknown type, Brief emits **E208**.

### 5.4 The `@BriefBuilder` Decorator

`@BriefBuilder` marks a task as using the composable builder pattern. In v0.1, this is a marker that indicates the task may be composed with other tasks. Full composability semantics are defined in v0.2.

Builder tasks should declare the typed outputs they produce:

```brief
@BriefBuilder
task MyBuilder : TaskBrief {
    provides {
        artifact: Artifact
        buildId: String
    }
    ...
}
```

If an `@BriefBuilder` task omits `provides { ... }`, Brief emits **W104**.

### 5.5 Example

```brief
import skill "DesignSystem"

sealed type Platform = iOS | Android | Web
sealed type Environment = Production | Staging | Development

@BriefBuilder
task DeploymentBrief : TaskBrief uses [DesignSystem] {
    goal = "Deploy the app for a given platform and environment"

    extras {
        platform: Platform
        environment: Environment
    }

    provides {
        deploymentUrl: String
        buildId: String
    }

    step Build {
        let artifact = perform DesignSystem.buildArtifact(platform, environment)?
    }

    step Deploy {
        let deploymentUrl = "https://staging.example.com"
        let buildId = "build-42"
    }
}
```

---

## 6. Match Expressions

Brief's `match` expression dispatches on sealed type variants or result values.

### 6.1 Syntax

```brief
let result = match scrutinee {
    VariantA     => expr_a
    VariantB(x)  => expr_using_x
    _            => fallback_expr
}
```

### 6.2 Rules

- The scrutinee may be any expression; typically a variable bound in a prior step.
- Arms are tried top-to-bottom; the first matching arm executes.
- `_` (wildcard) matches any value and must appear last.
- `VariantName(binding)` destructures a single payload and binds it in the arm body.
- **Exhaustiveness (E207):** When the scrutinee is a declared `sealed type`, all variants must be covered or a `_` wildcard must be present. Violation emits `warning[E207]`.

### 6.3 Examples

```brief
sealed type Platform = iOS | Android | Web

let label = match platform {
    iOS     => "Mobile (Apple)"
    Android => "Mobile (Google)"
    _       => "Web"
}
```

```brief
let name = match fetchResult {
    Ok(user) => user.name
    Err(msg) => "anonymous"
}
```

### 6.4 Error Codes

| Code | Meaning |
|------|---------|
| E207 | Non-exhaustive match — missing sealed type variants |

---

## 7. Phase Contracts

Steps may declare optional `pre` and `post` condition blocks.

```brief
step Charge {
    pre { amount > 0, account.isActive }
    post { receipt.isValid }

    let receipt = perform PaymentService.charge(amount)?
}
```

Conditions are stored as documentation-level assertions. Future versions will evaluate them at runtime.

---

## 8. Effect Contracts

Tasks declare the effects they produce with an `effects [...]` block.

```brief
task FetchWithCache : TaskBrief uses [NetworkService, CacheService] {
    effects [network, cache-read]
    ...
}
```

If a task performs a skill that produces an undeclared effect, `E209 EffectContractViolation` is emitted.

| Code | Meaning |
|------|---------|
| E209 | Skill effect not declared in task `effects [...]` |

---

## 9. Workflow Combinators

Steps can be grouped with combinators for parallel execution, retry logic, and fallback chains.

### parallel
```brief
parallel {
    FetchUsers
    FetchProducts
}
```
The named steps run concurrently.

### retry
```brief
retry(3) {
    SyncToBackend
}
```
The named step retries up to N times on failure.

### fallback
```brief
fallback {
    SyncPrimary
    SyncFallback
}
```
Steps are tried in order; the first to succeed wins.

**Step reference validation (E210):** If a combinator names a step not declared in the task, `E210` is emitted.

---

## 10. Steps

Steps describe the sequenced workflow of a task. They are ordered — step bodies execute in declaration order.

```brief
step StepName {
    statement*
}
```

### 10.1 Statements

- `let x = expr;` — bind a value to a name (immutable binding)
- `expr;` — evaluate an expression for its side effect

### 10.2 Expressions

| Form | Description |
|------|-------------|
| `match expr { ... }` | Pattern match on sealed variants or result values — see §6 |
| `perform Skill.fn(args)?` | Perform an effect; `?` propagates Err |
| `await expr` | Await an async expression (v0.1: async effects are managed transparently) |
| `x.method(args)` | Method call |
| `f(args)` | Function call |
| `x` | Variable reference |
| `"..."` | String literal |

---

## 11. Skill System

### 11.1 Import and Resolution

```brief
import skill "DesignSystem"
```

Resolution order for `DesignSystem`:
1. `<file_dir>/.claude/skills/DesignSystem/DesignSystem.briefskill`
2. `<cwd>/.claude/skills/DesignSystem/DesignSystem.briefskill`
3. `~/.brief/skills/DesignSystem.briefskill`
4. Brief skill registry (v0.2)

### 11.2 The `.briefskill` Interface File

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

### 11.3 Staleness Detection

The `.briefskill` header contains a SHA-256 checksum of the source `README.md`. When the checksum does not match the current `README.md`, `brief check` emits a `W102` warning:

```
warning[W102]: skill interface 'DesignSystem' is stale
  → .claude/skills/DesignSystem/DesignSystem.briefskill was generated from an older README.md
  fix: brief skillgen .claude/skills/DesignSystem/
```

---

## 12. Error Codes

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
| `E206` | `ScopedGenericConflict` | Generic type parameter shadows a builtin or declared type name |
| `E208` | `UnknownExtrasField` | Typed `extras { field: Type }` references an unknown type |
| `E209` | `EffectContractViolation` | Skill effect is not declared in the task `effects [...]` block |
| `E210` | `UndeclaredStepInCombinator` | `parallel`, `retry`, or `fallback` references a step not declared in the task |

### Warnings (non-fatal — task may still be handed to AI)

| Code | Name | Meaning |
|------|------|---------|
| `E207` | `NonExhaustiveMatch` | `match` on a sealed type omits variants and has no wildcard arm |
| `W101` | `MissingSkillInterface` | Imported skill has no `.briefskill` interface file; type checking is partial |
| `W102` | `StaleSkillInterface` | Skill interface file checksum does not match current `README.md` |
| `W103` | `DeprecatedStringExtras` | Legacy `extras = ["key": "value"]` syntax is deprecated in favor of typed `extras { ... }` |
| `W104` | `BriefBuilderProvidesMissing` | `@BriefBuilder` task omits the recommended `provides { ... }` block |

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

## 13. Standard Library

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

## 14. Grammar (EBNF)

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
                   (
                     'extras' '=' '[' kv_pairs ']'
                   | 'extras' '{' typed_fields '}'
                   )?
                   ('provides' '{' typed_fields '}')?
                   ('effects' '[' ident_list ']')?
                   step_group_decl*
                   step_decl*
step_group_decl ::= 'parallel' '{' ident_list '}'
                  | 'retry' '(' IntLiteral ')' '{' Ident '}'
                  | 'fallback' '{' ident_list '}'
step_decl      ::= 'step' Ident '{' phase_contract* stmt* '}'
phase_contract ::= 'pre' '{' contract_list '}' | 'post' '{' contract_list '}'
contract_list  ::= contract (',' contract)*
contract       ::= /* documentation-level assertion text */

test_decl      ::= 'test' STRING '{' test_body '}'
                 (* test_body uses mock/run/assert syntax; parsed by brief test, *)
                 (* skipped by brief check — see §4.10 for test body grammar *)

stmt           ::= let_stmt | expr_stmt
let_stmt       ::= attribute* 'let' Ident '=' expr ';'
expr_stmt      ::= expr ';'

expr           ::= match_expr
                 | 'perform' Ident '.' Ident type_args? '(' arg_list ')' '?'?
                 | 'await' expr
                 | Ident '.' Ident '(' arg_list ')'
                 | Ident '(' arg_list ')'
                 | Ident
                 | STRING

match_expr     ::= 'match' expr '{' match_arm+ '}'
match_arm      ::= pattern '=>' expr
pattern        ::= '_' | Ident | Ident '(' Ident ')'

arg_list       ::= ( expr (',' expr)* )?
decorator      ::= '@' Ident ( '(' arg_list ')' )?
ident_list     ::= Ident (',' Ident)*
kv_pairs       ::= STRING ':' STRING (',' STRING ':' STRING)*
typed_fields   ::= typed_field*
typed_field    ::= Ident ':' type_ref
```

---

## 15. CLI Reference

| Command | Description |
|---------|-------------|
| `brief check <file>.brief` | Type-check only — fast, CI-friendly. Exit code 0 = valid. |
| `brief run <file>.brief` | Validate then execute the task. |
| `brief build <file>.brief` | Compile to native binary via LLVM. |
| `brief build <file>.brief --emit-ir` | Emit LLVM IR for inspection. |
| `brief build <file>.brief --target wasm32-unknown-unknown` | Compile to WASM. |
| `brief test <file>.brief` | Run `test { }` blocks with mock skill system. |
| `brief fmt <file>.brief` | Auto-format to canonical style (idempotent). |
| `brief fmt <file>.brief --check` | Fail with exit code 1 if file is not formatted (CI mode). |
| `brief doc <file>.brief` | Generate Markdown documentation from declarations. |
| `brief doc <file>.brief --output <path>` | Write generated docs to file. |
| `brief repl` | Interactive REPL (tree-walking, fast iteration). |
| `brief lsp` | Start LSP server on stdio (for editor integration). |
| `brief gen "<description>"` | AI-generate a `.brief` file from natural language. |
| `brief gen "<description>" --force` | Overwrite an existing output file. |
| `brief skillgen <skill-path>` | Generate `.briefskill` interface from skill README. |
| `brief add skill <Name>` | Install a skill from the registry. |
| `brief add skill ./path/` | Install a skill from a local directory. |
| `brief add skill --list` | List available skills in the registry. |
| `brief watch <path>` | Watch a file or directory; re-check on every save. |
| `brief init <name>` | Scaffold a new Brief project in a new directory. |
| `brief ci` | Run all checks listed in `brief.toml` `[ci]` examples. |
| `brief completions <shell>` | Generate shell completions (bash, zsh, fish, powershell). |

---

## 16. Version History

This document describes Brief v1.0. The language is stable.

### v0.1
- Core language: tasks, steps, effects, sealed types, structs, protocols, skill imports

### v0.2
- Ecosystem: `brief test`, `brief fmt`, LSP go-to-def/find-refs, WASM, skill registry

### v0.3
- Power types: `@once` linear types, type aliases, effect groups, `brief doc`

### v0.4
- Test block support in main parser (`brief check` handles `test { }` files)
- `@mcp` alias attribute
- Examples 27–40 (composition, AI pipeline, platform branching, event sourcing, concurrency, MCP, background jobs, distributed transactions)
- `brief watch`, `brief init`, `brief ci`

### v0.5
- Scoped generic type params per function signature (E206)
- Comment trivia preservation — fmt --write refuses files with comments
- LSP O(1) offset→position via binary search + 50ms debounce
- Typed extras record syntax: `extras { field: Type }` (W103, E208)
- @BriefBuilder `provides { }` enforcement (W104)
- `match` expressions with exhaustiveness checking (E207)
- Typed HIR module (`hir.rs`) with `lower()` function
- Phase contracts: `pre { } / post { }` on steps
- Effect contracts: `effects [...]` on tasks (E209)
- Workflow combinators: `parallel`, `retry(n)`, `fallback` (E210)

### v1.0
- Stability: cross-step `@once` linear type tracking (E104/E105 across steps)
- `brief gen --force`
- `brief fmt --check`
- Security-hardened skill registry (name validation, size caps, HTTP timeouts)
- Diagnostic deduplication
- `ExitCode` API (no `process::exit` in library modules)
- StringPool O(1) dedup
- OnceLock-based builtin type sets
- 117 compiler tests
- 46 verified examples
