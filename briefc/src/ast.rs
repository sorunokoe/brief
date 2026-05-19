//! Abstract Syntax Tree for Brief v0.1.
//!
//! The AST covers the full Phase-1 Brief grammar: tasks, skills, sealed types,
//! structs, protocols, effects, and their type expressions.
#![allow(dead_code)]

/// Byte-offset span in the source file, used for error messages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    pub fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }
    pub fn merge(self, other: Span) -> Self {
        Self {
            start: self.start.min(other.start),
            end: self.end.max(other.end),
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Top-level program
// ─────────────────────────────────────────────────────────────────────────────

/// A complete `.brief` source file.
#[derive(Debug, Clone)]
pub struct Program {
    pub imports: Vec<SkillImport>,
    pub opaque_types: Vec<OpaqueTypeDecl>,
    pub types: Vec<SealedTypeDecl>,
    pub type_aliases: Vec<TypeAliasDecl>,
    pub effect_groups: Vec<EffectGroupDecl>,
    pub structs: Vec<StructDecl>,
    pub protocols: Vec<ProtocolDecl>,
    pub effects: Vec<EffectDecl>,
    pub tasks: Vec<Task>,
    pub tests: Vec<TestDecl>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Import
// ─────────────────────────────────────────────────────────────────────────────

/// `import skill "DesignSystem"`
#[derive(Debug, Clone)]
pub struct SkillImport {
    pub name: String,
    pub span: Span,
}

// ─────────────────────────────────────────────────────────────────────────────
// Types
// ─────────────────────────────────────────────────────────────────────────────

/// A reference to a type: `String`, `Result<T, E>`, `Option<User>`, `Theme?`
#[derive(Debug, Clone)]
pub struct TypeRef {
    pub name: String,
    pub args: Vec<TypeRef>, // generic arguments
    /// `T?` is sugar for `Option<T>`
    pub optional: bool,
    pub span: Span,
}

/// A field/type attribute: `@url`, `@nonEmpty`, `@matches("pattern")`
#[derive(Debug, Clone)]
pub struct Attribute {
    pub name: String,
    pub arg: Option<String>,
    pub span: Span,
}

/// `type Email = @matches("[^@]+@[^@]+") String`
/// User-defined refinement type alias — expands at check-time.
#[derive(Debug, Clone)]
pub struct TypeAliasDecl {
    pub name: String,
    pub attrs: Vec<Attribute>,
    pub base: TypeRef,
    pub span: Span,
}

/// `type AuthEffects = [Auth, Session]`
/// Named group of effects that can be used in `uses [AuthEffects]`.
#[derive(Debug, Clone)]
pub struct EffectGroupDecl {
    pub name: String,
    pub members: Vec<String>,
    pub span: Span,
}

/// `opaque type TokenStream`
#[derive(Debug, Clone)]
pub struct OpaqueTypeDecl {
    pub name: String,
    pub span: Span,
}

/// `sealed type Platform = iOS | Android | Web`
#[derive(Debug, Clone)]
pub struct SealedTypeDecl {
    pub name: String,
    pub params: Vec<String>,
    pub variants: Vec<TypeVariant>,
    pub span: Span,
}

/// A single variant of a sealed type: `Done(String)` or just `iOS`
#[derive(Debug, Clone)]
pub struct TypeVariant {
    pub name: String,
    pub fields: Vec<TypeRef>,
    pub span: Span,
}

// ─────────────────────────────────────────────────────────────────────────────
// Structs
// ─────────────────────────────────────────────────────────────────────────────

/// `struct FigmaURL { url: @url String }`
#[derive(Debug, Clone)]
pub struct StructDecl {
    pub name: String,
    pub params: Vec<String>,
    pub fields: Vec<StructField>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct StructField {
    pub name: String,
    pub attrs: Vec<Attribute>,
    pub ty: TypeRef,
    pub span: Span,
}

// ─────────────────────────────────────────────────────────────────────────────
// Protocols & Effects
// ─────────────────────────────────────────────────────────────────────────────

/// `protocol Renderable { fn render() -> Component }`
#[derive(Debug, Clone)]
pub struct ProtocolDecl {
    pub name: String,
    pub params: Vec<String>,
    pub methods: Vec<FnSignature>,
    pub span: Span,
}

/// `effect GraphQL { fn query<T>(op: Operation) -> Result<T, QueryError> }`
#[derive(Debug, Clone)]
pub struct EffectDecl {
    pub name: String,
    pub params: Vec<String>,
    pub fns: Vec<FnSignature>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct FnSignature {
    pub name: String,
    pub type_params: Vec<String>,
    pub params: Vec<Param>,
    /// Return-type attributes, e.g. `@once` in `-> @once PaymentHandle`.
    pub ret_attrs: Vec<Attribute>,
    pub ret: TypeRef,
    #[allow(dead_code)]
    pub doc: Option<String>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct Param {
    pub name: String,
    pub attrs: Vec<Attribute>,
    pub ty: TypeRef,
    pub span: Span,
}

// ─────────────────────────────────────────────────────────────────────────────
// Tasks
// ─────────────────────────────────────────────────────────────────────────────

/// A decorator like `@BriefBuilder` or `@deprecated("reason")`
#[derive(Debug, Clone)]
pub struct Decorator {
    pub name: String,
    pub arg: Option<String>,
    pub span: Span,
}

// ─────────────────────────────────────────────────────────────────────────────
// needs {} block
// ─────────────────────────────────────────────────────────────────────────────

/// Kind of prerequisite in a `needs {}` block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NeedKind {
    /// `env "VAR_NAME"` — requires an environment variable to be set and non-empty.
    Env,
    /// `feature "FLAG"` — requires a feature flag to be enabled.
    Feature,
    /// `config "KEY"` — requires a config value to be present.
    Config,
}

/// A single item in a `needs {}` block.
#[derive(Debug, Clone)]
pub struct NeedItem {
    pub kind: NeedKind,
    pub key: String,
    pub span: Span,
}

// ─────────────────────────────────────────────────────────────────────────────
// forbids {} block
// ─────────────────────────────────────────────────────────────────────────────

/// Kind of prohibition in a `forbids {}` block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ForbidKind {
    /// `skill "Name"` — the AI must not use this skill at all.
    Skill,
    /// `func "Skill.fn"` — the AI must not call this specific function.
    Func,
}

/// A single item in a `forbids {}` block.
#[derive(Debug, Clone)]
pub struct ForbidItem {
    /// `Skill` or `Func`.
    pub kind: ForbidKind,
    /// `"Database"` for skill, `"Payment.refund"` for func.
    pub name: String,
    pub span: Span,
}

/// A full task declaration.
///
/// ```brief
/// @BriefBuilder
/// task ProfileScreen : TaskBrief uses [DesignSystem, GraphQL] {
///     goal   = "..."
///     extras {
///         platform: Platform
///     }
///     step FetchData { ... }
/// }
/// ```
#[derive(Debug, Clone)]
pub enum ExtrasNode {
    /// Old syntax: `extras = ["key": "value", ...]` — deprecated.
    StringMap(Vec<(String, String)>),
    /// New syntax: `extras { field: TypeRef, ... }`
    TypedRecord(Vec<ExtrasField>),
}

#[derive(Debug, Clone)]
pub struct ExtrasField {
    pub name: String,
    pub type_ref: TypeRef,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct Task {
    pub decorators: Vec<Decorator>,
    /// Convenience: true when any decorator is named `BriefBuilder`.
    pub has_builder: bool,
    pub name: String,
    pub uses: Vec<String>,
    pub effects: Vec<String>,
    pub goal: Option<String>,
    pub extras: Option<ExtrasNode>,
    pub extras_span: Option<Span>,
    pub provides: Option<Vec<ExtrasField>>,
    /// Prerequisites that must be met before the AI starts (`needs {}`).
    pub needs: Vec<NeedItem>,
    /// Capabilities the AI must never use (`forbids {}`).
    pub forbids: Vec<ForbidItem>,
    pub step_groups: Vec<StepGroup>,
    pub steps: Vec<Step>,
    pub span: Span,
}

// ─────────────────────────────────────────────────────────────────────────────
// Steps & Statements
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum StepGroup {
    Sequential(Step),
    Parallel(Vec<String>),
    Retry { count: u32, step: String },
    Fallback(Vec<String>),
}

/// `step FetchData { ... }`
#[derive(Debug, Clone)]
pub struct Step {
    pub name: String,
    pub pre_conditions: Vec<String>,
    pub post_conditions: Vec<String>,
    pub body: Vec<Stmt>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum Stmt {
    /// `let x = expr;` or `@once let x = expr;`
    Let {
        /// Binding-level attributes, e.g. `["once"]` from `@once let`.
        attrs: Vec<String>,
        name: String,
        value: Expr,
        span: Span,
    },
    /// `expr;`
    Expr { value: Expr, span: Span },
}

// ─────────────────────────────────────────────────────────────────────────────
// Expressions
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct MatchArm {
    pub pattern: Pattern,
    pub body: Box<Expr>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum Pattern {
    Variant(String),
    Variant1(String, String),
    Wildcard,
}

#[derive(Debug, Clone)]
pub enum Expr {
    /// `perform GraphQL.query(UserProfileQuery)?`
    Perform {
        skill: String,
        func: String,
        args: Vec<Expr>,
        /// Whether the `?` error-propagation operator is present.
        propagate: bool,
        span: Span,
    },
    /// `await expr`
    Await { expr: Box<Expr>, span: Span },
    /// `foo.bar(args)` or `foo(args)`
    Call {
        receiver: Option<String>,
        func: String,
        args: Vec<Expr>,
        span: Span,
    },
    /// `match value { Pattern => expr }`
    Match {
        scrutinee: Box<Expr>,
        arms: Vec<MatchArm>,
    },
    /// Bare identifier (variable reference).
    Ident { name: String, span: Span },
    /// String literal value.
    Str { value: String, span: Span },
    /// Integer literal value.
    Int { value: i64, span: Span },
}

impl Expr {
    pub fn span(&self) -> Span {
        match self {
            Expr::Perform { span, .. } => *span,
            Expr::Await { span, .. } => *span,
            Expr::Call { span, .. } => *span,
            Expr::Match { scrutinee, arms } => arms
                .last()
                .map(|arm| scrutinee.span().merge(arm.span))
                .unwrap_or_else(|| scrutinee.span()),
            Expr::Ident { span, .. } => *span,
            Expr::Str { span, .. } => *span,
            Expr::Int { span, .. } => *span,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Test declaration
// ─────────────────────────────────────────────────────────────────────────────

/// `test "name" { stmts }`
///
/// Test blocks are collected by the parser but executed only by `brief test`.
/// `brief check` validates the task declarations in the same file but does not
/// run test body statements through effect/skill checking.
#[derive(Debug, Clone)]
pub struct TestDecl {
    pub name: String,
    pub body: Vec<Stmt>,
    pub span: Span,
}
