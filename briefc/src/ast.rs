//! Abstract Syntax Tree for Brief v0.1.
//!
//! The AST covers the full Phase-1 Brief grammar: tasks, skills, sealed types,
//! structs, protocols, effects, and their type expressions.
#![allow(dead_code)]

/// Byte-offset span in the source file, used for error messages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Span {
    pub start: usize,
    pub end:   usize,
}

impl Span {
    pub fn new(start: usize, end: usize) -> Self { Self { start, end } }
    pub fn merge(self, other: Span) -> Self { Self { start: self.start.min(other.start), end: self.end.max(other.end) } }
}

// ─────────────────────────────────────────────────────────────────────────────
// Top-level program
// ─────────────────────────────────────────────────────────────────────────────

/// A complete `.brief` source file.
#[derive(Debug, Clone)]
pub struct Program {
    pub imports:   Vec<SkillImport>,
    pub types:     Vec<SealedTypeDecl>,
    pub structs:   Vec<StructDecl>,
    pub protocols: Vec<ProtocolDecl>,
    pub effects:   Vec<EffectDecl>,
    pub tasks:     Vec<Task>,
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
    pub args: Vec<TypeRef>,   // generic arguments
    /// `T?` is sugar for `Option<T>`
    pub optional: bool,
    pub span: Span,
}

/// A field/type attribute: `@url`, `@nonEmpty`, `@matches("pattern")`
#[derive(Debug, Clone)]
pub struct Attribute {
    pub name: String,
    pub arg:  Option<String>,
    pub span: Span,
}

/// `sealed type Platform = iOS | Android | Web`
#[derive(Debug, Clone)]
pub struct SealedTypeDecl {
    pub name:     String,
    pub params:   Vec<String>,
    pub variants: Vec<TypeVariant>,
    pub span:     Span,
}

/// A single variant of a sealed type: `Done(String)` or just `iOS`
#[derive(Debug, Clone)]
pub struct TypeVariant {
    pub name:   String,
    pub fields: Vec<TypeRef>,
    pub span:   Span,
}

// ─────────────────────────────────────────────────────────────────────────────
// Structs
// ─────────────────────────────────────────────────────────────────────────────

/// `struct FigmaURL { url: @url String }`
#[derive(Debug, Clone)]
pub struct StructDecl {
    pub name:   String,
    pub params: Vec<String>,
    pub fields: Vec<StructField>,
    pub span:   Span,
}

#[derive(Debug, Clone)]
pub struct StructField {
    pub name:  String,
    pub attrs: Vec<Attribute>,
    pub ty:    TypeRef,
    pub span:  Span,
}

// ─────────────────────────────────────────────────────────────────────────────
// Protocols & Effects
// ─────────────────────────────────────────────────────────────────────────────

/// `protocol Renderable { fn render() -> Component }`
#[derive(Debug, Clone)]
pub struct ProtocolDecl {
    pub name:    String,
    pub params:  Vec<String>,
    pub methods: Vec<FnSignature>,
    pub span:    Span,
}

/// `effect GraphQL { fn query<T>(op: Operation) -> Result<T, QueryError> }`
#[derive(Debug, Clone)]
pub struct EffectDecl {
    pub name:   String,
    pub params: Vec<String>,
    pub fns:    Vec<FnSignature>,
    pub span:   Span,
}

#[derive(Debug, Clone)]
pub struct FnSignature {
    pub name:        String,
    pub type_params: Vec<String>,
    pub params:      Vec<Param>,
    pub ret:         TypeRef,
    #[allow(dead_code)]
    pub doc:         Option<String>,
    pub span:        Span,
}

#[derive(Debug, Clone)]
pub struct Param {
    pub name:  String,
    pub attrs: Vec<Attribute>,
    pub ty:    TypeRef,
    pub span:  Span,
}

// ─────────────────────────────────────────────────────────────────────────────
// Tasks
// ─────────────────────────────────────────────────────────────────────────────

/// A decorator like `@BriefBuilder` or `@deprecated("reason")`
#[derive(Debug, Clone)]
pub struct Decorator {
    pub name: String,
    pub arg:  Option<String>,
    pub span: Span,
}

/// A full task declaration.
///
/// ```brief
/// @BriefBuilder
/// task ProfileScreen : TaskBrief uses [DesignSystem, GraphQL] {
///     goal   = "..."
///     extras = ["platform": "iOS"]
///     step FetchData { ... }
/// }
/// ```
#[derive(Debug, Clone)]
pub struct Task {
    pub decorators: Vec<Decorator>,
    /// Convenience: true when any decorator is named `BriefBuilder`.
    pub has_builder: bool,
    pub name:        String,
    pub uses:        Vec<String>,
    pub goal:        Option<String>,
    pub extras:      Vec<(String, String)>,
    pub steps:       Vec<Step>,
    pub span:        Span,
}

// ─────────────────────────────────────────────────────────────────────────────
// Steps & Statements
// ─────────────────────────────────────────────────────────────────────────────

/// `step FetchData { ... }`
#[derive(Debug, Clone)]
pub struct Step {
    pub name:  String,
    pub body:  Vec<Stmt>,
    pub span:  Span,
}

#[derive(Debug, Clone)]
pub enum Stmt {
    /// `let x = expr;`
    Let { name: String, value: Expr, span: Span },
    /// `expr;`
    Expr { value: Expr, span: Span },
}

// ─────────────────────────────────────────────────────────────────────────────
// Expressions
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum Expr {
    /// `perform GraphQL.query(UserProfileQuery)?`
    Perform {
        skill: String,
        func:  String,
        args:  Vec<Expr>,
        /// Whether the `?` error-propagation operator is present.
        propagate: bool,
        span: Span,
    },
    /// `await expr`
    Await {
        expr: Box<Expr>,
        span: Span,
    },
    /// `foo.bar(args)` or `foo(args)`
    Call {
        receiver: Option<String>,
        func:     String,
        args:     Vec<Expr>,
        span:     Span,
    },
    /// Bare identifier (variable reference).
    Ident { name: String, span: Span },
    /// String literal value.
    Str { value: String, span: Span },
}

impl Expr {
    pub fn span(&self) -> Span {
        match self {
            Expr::Perform { span, .. } => *span,
            Expr::Await   { span, .. } => *span,
            Expr::Call    { span, .. } => *span,
            Expr::Ident   { span, .. } => *span,
            Expr::Str     { span, .. } => *span,
        }
    }
}
