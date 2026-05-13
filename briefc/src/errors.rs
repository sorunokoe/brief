/// Error types and formatting for Brief v0.0.1.
///
/// Error codes follow the pattern used in the plan:
/// - E0xx: Parse errors
/// - E1xx: Structural/semantic errors
/// - W1xx: Warnings (missing interfaces, stale skills)

use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErrorCode {
    /// Parse-level errors
    ParseError,
    /// Task is missing a required `goal` field
    MissingGoal,
    /// A skill listed in `uses [X]` was never imported
    UndeclaredSkillInUses,
    /// A `perform X.fn()` call references a skill not listed in `uses [...]`
    PerformWithoutUses,
    /// A skill import has no matching `.briefskill` interface file (warning)
    MissingSkillInterface,
    /// A `.briefskill` interface is potentially stale (warning)
    #[allow(dead_code)]
    StaleSkillInterface,
}

impl fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let code = match self {
            ErrorCode::ParseError              => "E001",
            ErrorCode::MissingGoal             => "E101",
            ErrorCode::UndeclaredSkillInUses   => "E102",
            ErrorCode::PerformWithoutUses       => "E103",
            ErrorCode::MissingSkillInterface    => "W101",
            ErrorCode::StaleSkillInterface      => "W102",
        };
        write!(f, "{code}")
    }
}

impl ErrorCode {
    pub fn is_error(&self) -> bool {
        matches!(self, ErrorCode::ParseError | ErrorCode::MissingGoal
            | ErrorCode::UndeclaredSkillInUses | ErrorCode::PerformWithoutUses)
    }
}

// ─────────────────────────────────────────────────────────────────────────────

use crate::ast::Span;

#[derive(Debug, Clone)]
pub struct BriefError {
    pub code:    ErrorCode,
    pub message: String,
    pub span:    Span,
    pub hint:    Option<String>,
}

impl BriefError {
    pub fn is_error(&self)   -> bool { self.code.is_error() }
    pub fn is_warning(&self) -> bool { !self.is_error() }
}

// ─────────────────────────────────────────────────────────────────────────────
// Pretty-printing
// ─────────────────────────────────────────────────────────────────────────────

use colored::Colorize;

pub fn print_diagnostics(diags: &[BriefError], source: &str, file: &str) {
    for d in diags {
        print_diagnostic(d, source, file);
    }
}

fn print_diagnostic(d: &BriefError, source: &str, file: &str) {
    // Header line: "error[E101]: message"  or  "warning[W101]: message"
    let (label, code_str, msg_str) = if d.is_error() {
        (
            "error".red().bold(),
            d.code.to_string().red(),
            d.message.red().to_string(),
        )
    } else {
        (
            "warning".yellow().bold(),
            d.code.to_string().yellow(),
            d.message.yellow().to_string(),
        )
    };

    eprintln!("{label}[{code_str}]: {msg_str}");

    // Location
    let (line, col) = offset_to_line_col(source, d.span.start);
    eprintln!("  → {}:{}:{}", file.dimmed(), line, col);

    // Hint
    if let Some(hint) = &d.hint {
        eprintln!("  {} {}", "fix:".cyan().bold(), hint.cyan());
    }

    eprintln!();
}

fn offset_to_line_col(source: &str, offset: usize) -> (usize, usize) {
    let mut line = 1usize;
    let mut col  = 1usize;
    for (i, ch) in source.char_indices() {
        if i == offset { break; }
        if ch == '\n' { line += 1; col = 1; } else { col += 1; }
    }
    (line, col)
}
