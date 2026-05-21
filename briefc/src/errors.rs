/// Error types and formatting for Brief v0.1.
///
/// Error codes follow the pattern used in the plan:
/// - E0xx: Parse errors
/// - E1xx: Structural/semantic errors
/// - E2xx: Type errors
/// - E3xx: Spec/constraint/lock errors
/// - W1xx: Warnings (stale skill interfaces)
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErrorCode {
    // ── Parse errors ──────────────────────────────────────────────────────
    ParseError,
    // ── Structural/semantic errors ────────────────────────────────────────
    MissingGoal,
    UndeclaredSkillInUses,
    PerformWithoutUses,
    // ── Linear / affine type errors ───────────────────────────────────────
    /// A `@once` (linear) binding was used more than once.
    LinearBindingReused,
    /// A `@once` (linear) binding was never consumed.
    LinearBindingDropped,
    // ── Type alias / effect group errors ─────────────────────────────────
    /// A `uses [...]` clause references an unknown effect group alias.
    UnknownEffectGroup,
    // ── Type errors ───────────────────────────────────────────────────────
    /// A type reference resolves to an unknown name
    UnknownType,
    /// Effect function called with wrong number of arguments
    WrongArgCount,
    /// Struct field attribute value fails constraint (e.g. @url on non-URL)
    AttributeConstraint,
    /// Generic type parameter shadows a real declared type (builtin or user-defined)
    ScopedGenericConflict,
    /// Match arms do not cover every variant of a sealed type (warning by default)
    NonExhaustiveMatch,
    // ── Spec / constraint coverage errors (Phase 1) ───────────────────────
    /// `@range(min, max)` boundary literal missing in test block (E301)
    RangeBoundaryMissing,
    /// `@enum(vals)` value literal missing in test block (E302)
    EnumValueMissing,
    /// `.brief.lock` missing, stale, or source-changed — run `brief verify` (E303)
    LockRequired,
    // ── Phase 2: verifier protocol errors ────────────────────────────────
    /// Dynamic annotation has no configured verifier in `brief.toml` (E309)
    UnconfiguredVerifier,
    /// Required env/feature/config prerequisite not met at verify time (E411)
    NeedNotMet,
    /// `forbids { skill "X" }` — task uses a skill that is explicitly forbidden (E420)
    ForbiddenSkill,
    /// `forbids { func "Skill.fn" }` — task calls a function that is explicitly forbidden (E421)
    ForbiddenFunc,
    // ── Warnings ──────────────────────────────────────────────────────────
    /// Missing `.briefskill` file — suppress with `--allow-missing-skills` (E107)
    MissingSkillInterface,
    #[allow(dead_code)]
    StaleSkillInterface,
    /// Old `extras = ["k": "v"]` syntax is deprecated — use `extras { ... }` (W103)
    DeprecatedStringExtras,
    /// `@BriefBuilder` task is missing a `provides { ... }` block (W104)
    BriefBuilderProvidesMissing,
    /// `opaque type` declaration is never referenced (W105)
    OpaqueTypeUnused,
    /// Typed `extras` field references an unknown type (E208)
    UnknownExtrasField,
    /// Task performs a skill whose effects are not declared in `effects [...]` (E209)
    EffectContractViolation,
    /// Workflow combinator references a step that is not declared in the task (E210)
    UndeclaredStepInCombinator,
    /// Field access on an opaque type (E211)
    OpaqueTypeFieldAccess,
    /// Skill ABI version header is missing or incompatible (E212)
    SkillAbiVersionMismatch,
    /// Skill ABI references a type the checker cannot resolve (E213)
    SkillAbiUnknownType,
    /// Skill declared in `uses []` but never `perform`-ed in any step (W106)
    UnusedSkillInUses,
}

impl fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let code = match self {
            ErrorCode::ParseError => "E001",
            ErrorCode::MissingGoal => "E101",
            ErrorCode::UndeclaredSkillInUses => "E102",
            ErrorCode::PerformWithoutUses => "E103",
            ErrorCode::LinearBindingReused => "E104",
            ErrorCode::LinearBindingDropped => "E105",
            ErrorCode::UnknownEffectGroup => "E106",
            ErrorCode::UnknownType => "E201",
            ErrorCode::WrongArgCount => "E202",
            ErrorCode::AttributeConstraint => "E203",
            ErrorCode::ScopedGenericConflict => "E206",
            ErrorCode::NonExhaustiveMatch => "E207",
            ErrorCode::UnknownExtrasField => "E208",
            ErrorCode::EffectContractViolation => "E209",
            ErrorCode::UndeclaredStepInCombinator => "E210",
            ErrorCode::OpaqueTypeFieldAccess => "E211",
            ErrorCode::SkillAbiVersionMismatch => "E212",
            ErrorCode::SkillAbiUnknownType => "E213",
            ErrorCode::RangeBoundaryMissing => "E301",
            ErrorCode::EnumValueMissing => "E302",
            ErrorCode::LockRequired => "E303",
            ErrorCode::UnconfiguredVerifier => "E309",
            ErrorCode::NeedNotMet => "E411",
            ErrorCode::ForbiddenSkill => "E420",
            ErrorCode::ForbiddenFunc => "E421",
            ErrorCode::MissingSkillInterface => "E107",
            ErrorCode::StaleSkillInterface => "W102",
            ErrorCode::DeprecatedStringExtras => "W103",
            ErrorCode::BriefBuilderProvidesMissing => "W104",
            ErrorCode::OpaqueTypeUnused => "W105",
            ErrorCode::UnusedSkillInUses => "W106",
        };
        write!(f, "{code}")
    }
}

impl ErrorCode {
    pub fn is_error(&self) -> bool {
        matches!(
            self,
            ErrorCode::ParseError
                | ErrorCode::MissingGoal
                | ErrorCode::UndeclaredSkillInUses
                | ErrorCode::PerformWithoutUses
                | ErrorCode::LinearBindingReused
                | ErrorCode::LinearBindingDropped
                | ErrorCode::UnknownEffectGroup
                | ErrorCode::UnknownType
                | ErrorCode::WrongArgCount
                | ErrorCode::AttributeConstraint
                | ErrorCode::ScopedGenericConflict
                | ErrorCode::UnknownExtrasField
                | ErrorCode::EffectContractViolation
                | ErrorCode::UndeclaredStepInCombinator
                | ErrorCode::OpaqueTypeFieldAccess
                | ErrorCode::SkillAbiVersionMismatch
                | ErrorCode::SkillAbiUnknownType
                | ErrorCode::RangeBoundaryMissing
                | ErrorCode::EnumValueMissing
                | ErrorCode::LockRequired
                | ErrorCode::UnconfiguredVerifier
                | ErrorCode::NeedNotMet
                | ErrorCode::ForbiddenSkill
                | ErrorCode::ForbiddenFunc
                | ErrorCode::MissingSkillInterface
        )
    }
}

// ─────────────────────────────────────────────────────────────────────────────

use crate::ast::Span;

#[derive(Debug, Clone)]
pub struct BriefError {
    pub code: ErrorCode,
    pub message: String,
    pub span: Span,
    pub hint: Option<String>,
}

impl BriefError {
    pub fn is_error(&self) -> bool {
        self.code.is_error()
    }
    pub fn is_warning(&self) -> bool {
        !self.is_error()
    }
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
    let mut col = 1usize;
    for (i, ch) in source.char_indices() {
        if i == offset {
            break;
        }
        if ch == '\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
    }
    (line, col)
}
