/// Tree-walking runner for Brief v0.1.
///
/// Used by both `brief check` (validation only) and `brief run` (validate + execute).
/// In Phase 1 this will be complemented by an LLVM backend for `brief build`.

use std::path::{Path, PathBuf};

use colored::Colorize;

use crate::ast::*;
use crate::checker::{self, CheckContext};
use crate::errors::{print_diagnostics, BriefError};
use crate::lexer::lex;
use crate::parser::parse;

// ─────────────────────────────────────────────────────────────────────────────

pub enum RunMode {
    /// Only validate — do not execute.
    Check,
    /// Validate then execute (print task info in v0.0.1).
    Run,
}

/// Entry point called by `brief check` and `brief run`.
/// Returns `true` if there were no blocking errors.
pub fn run_file(path: &Path, mode: RunMode) -> bool {
    // ── 1. Read source ────────────────────────────────────────────────────
    let source = match std::fs::read_to_string(path) {
        Ok(s)  => s,
        Err(e) => {
            eprintln!("{}: cannot read {}: {}", "error".red().bold(), path.display(), e);
            return false;
        }
    };

    let file_str = path.to_string_lossy().to_string();

    // ── 2. Lex ────────────────────────────────────────────────────────────
    let (tokens, lex_errors) = lex(&source);
    if !lex_errors.is_empty() {
        for (start, end) in &lex_errors {
            eprintln!(
                "{}: unrecognised character at byte offset {}–{}",
                "error[E000]".red().bold(), start, end
            );
        }
        eprintln!();
        return false;
    }

    // ── 3. Parse ──────────────────────────────────────────────────────────
    let (program, parse_errors) = parse(&tokens, &source);
    if !parse_errors.is_empty() {
        print_diagnostics(&parse_errors, &source, &file_str);
        if parse_errors.iter().any(|d| d.is_error()) {
            return false;
        }
    }

    // ── 4. Semantic check ─────────────────────────────────────────────────
    let file_dir = path.parent().unwrap_or(Path::new("."));
    let cwd      = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let ctx      = CheckContext { file_dir, cwd: &cwd };

    let mut diags: Vec<BriefError> = parse_errors;
    diags.extend(checker::check(&program, &ctx));

    // ── 5. Print header ───────────────────────────────────────────────────
    // Show type declarations (sealed types, structs, effects, protocols)
    let decl_count = program.types.len() + program.structs.len()
                   + program.effects.len() + program.protocols.len();
    if decl_count > 0 {
        println!();
        if !program.types.is_empty() {
            let names: Vec<_> = program.types.iter().map(|t| t.name.as_str()).collect();
            println!("{} sealed types: {}", "●".blue(), names.join(", ").cyan());
        }
        if !program.structs.is_empty() {
            let names: Vec<_> = program.structs.iter().map(|s| s.name.as_str()).collect();
            println!("{} structs: {}", "●".blue(), names.join(", ").cyan());
        }
        if !program.effects.is_empty() {
            let names: Vec<_> = program.effects.iter().map(|e| e.name.as_str()).collect();
            println!("{} effects: {}", "●".blue(), names.join(", ").cyan());
        }
        if !program.protocols.is_empty() {
            let names: Vec<_> = program.protocols.iter().map(|p| p.name.as_str()).collect();
            println!("{} protocols: {}", "●".blue(), names.join(", ").cyan());
        }
    }

    for task in &program.tasks {
        print_task_summary(task);
    }

    // ── 6. Print diagnostics ──────────────────────────────────────────────
    let semantic_diags: Vec<_> = diags.iter().filter(|d| d.code != crate::errors::ErrorCode::ParseError).cloned().collect();
    if !semantic_diags.is_empty() {
        print_diagnostics(&semantic_diags, &source, &file_str);
    }

    let has_errors = diags.iter().any(|d| d.is_error());

    // ── 7. Summary ────────────────────────────────────────────────────────
    if has_errors {
        eprintln!("{} Brief has errors — fix them before handing to AI.", "✗".red().bold());
        return false;
    }

    let has_warnings = diags.iter().any(|d| d.is_warning());
    if has_warnings {
        println!("{} Brief is structurally valid. Run `brief skillgen` to complete type checking.",
            "⚠".yellow().bold());
    } else {
        println!("{} All ingredients present. Ready for AI.", "✅".green().bold());
    }

    // ── 8. Execute (run mode only) ────────────────────────────────────────
    if matches!(mode, RunMode::Run) && !has_errors {
        println!();
        for task in &program.tasks {
            execute_task(task);
        }
    }

    true
}

// ─────────────────────────────────────────────────────────────────────────────

fn print_task_summary(task: &Task) {
    println!();
    // Show decorators
    for d in &task.decorators {
        println!("  {} @{}", "✦".blue(), d.name.cyan());
    }
    println!("{} Brief: {}", "●".blue().bold(), task.name.bold());
    println!("  {:<8} {}", "goal:".dimmed(), task.goal.as_deref().unwrap_or("<missing>").green());

    if task.uses.is_empty() {
        println!("  {:<8} none required", "skills:".dimmed());
    } else {
        let skills = task.uses.join(", ");
        println!("  {:<8} [{}]", "skills:".dimmed(), skills.cyan());
    }

    if !task.extras.is_empty() {
        let extras_str = task.extras.iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect::<Vec<_>>()
            .join(", ");
        println!("  {:<8} {}", "extras:".dimmed(), extras_str);
    }

    if !task.steps.is_empty() {
        let steps = task.steps.iter().map(|s| s.name.as_str()).collect::<Vec<_>>().join(", ");
        println!("  {:<8} [{}]", "steps:".dimmed(), steps);
    }
}

fn collect_perform_calls(expr: &crate::ast::Expr) -> Vec<String> {
    match expr {
        crate::ast::Expr::Perform { skill, func, .. } => vec![format!("{skill}.{func}()")],
        crate::ast::Expr::Await { expr: inner, .. }   => collect_perform_calls(inner),
        crate::ast::Expr::Call  { args, .. }           => {
            args.iter().flat_map(collect_perform_calls).collect()
        }
        _ => Vec::new(),
    }
}

fn execute_task(task: &Task) {
    println!("{} Running brief: {}", "●".blue().bold(), task.name.bold());

    for step in &task.steps {
        print!("  {} step {}... ", "→".dimmed(), step.name.bold());
        let effects: Vec<String> = step.body.iter()
            .flat_map(|stmt| {
                let expr = match stmt {
                    crate::ast::Stmt::Let { value, .. }  => value,
                    crate::ast::Stmt::Expr { value, .. } => value,
                };
                collect_perform_calls(expr)
            })
            .collect();

        if effects.is_empty() {
            println!("{}", "done".green());
        } else {
            println!("{} {}", "invokes".dimmed(), effects.join(", ").cyan());
        }
    }

    println!("{} Complete.", "✅".green().bold());
}
