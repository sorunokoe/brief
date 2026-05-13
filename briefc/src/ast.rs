//! Abstract Syntax Tree for Brief v0.0.1.
//!
//! The AST is intentionally simple — it covers the subset of Brief needed to
//! express TaskBrief-equivalent task definitions with skill imports and steps.
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

/// A complete `.brief` source file.
#[derive(Debug, Clone)]
pub struct Program {
    pub imports: Vec<SkillImport>,
    pub tasks:   Vec<Task>,
}

// ─────────────────────────────────────────────────────────────────────────────

/// `import skill "DesignSystem"`
#[derive(Debug, Clone)]
pub struct SkillImport {
    /// The skill name as given in the string literal (e.g. `"DesignSystem"` → `DesignSystem`).
    pub name: String,
    pub span: Span,
}

// ─────────────────────────────────────────────────────────────────────────────

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
    /// Whether `@BriefBuilder` was present before `task`.
    pub has_builder: bool,
    pub name:        String,
    /// Skills listed in `uses [X, Y]` — may be empty.
    pub uses:        Vec<String>,
    /// Value of the `goal = "..."` field — required.
    pub goal:        Option<String>,
    /// Key-value pairs from `extras = ["k": "v", ...]`.
    pub extras:      Vec<(String, String)>,
    pub steps:       Vec<Step>,
    pub span:        Span,
}

// ─────────────────────────────────────────────────────────────────────────────

/// `step FetchData { ... }`
#[derive(Debug, Clone)]
pub struct Step {
    pub name:  String,
    pub body:  Vec<Stmt>,
    pub span:  Span,
}

// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum Stmt {
    /// `let x = expr;`
    Let { name: String, value: Expr, span: Span },
    /// `expr;`
    Expr { value: Expr, span: Span },
}

// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum Expr {
    /// `perform GraphQL.query(UserProfileQuery)?`
    Perform {
        skill: String,
        func:  String,
        args:  Vec<Expr>,
        /// Whether the `?` propagation operator is present.
        propagate: bool,
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
            Expr::Perform  { span, .. } => *span,
            Expr::Call     { span, .. } => *span,
            Expr::Ident    { span, .. } => *span,
            Expr::Str      { span, .. } => *span,
        }
    }
}
