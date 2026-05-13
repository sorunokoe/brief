/// `brief repl` — Interactive Read-Eval-Print Loop for Brief.
///
/// Provides an interactive session where you can:
///   - Type Brief declarations (sealed types, structs, effects, tasks) line-by-line
///   - Run tasks immediately with tree-walking execution
///   - Use REPL commands: :help, :check, :clear, :quit
///
/// The REPL accumulates declarations across inputs (session state), so you can
/// build up a type environment incrementally.
///
/// No LLVM required — the REPL uses the existing tree-walking runner.

use std::io::{self, Write};

use colored::Colorize;

use crate::ast::Program;
use crate::checker::{self, CheckContext};
use crate::errors::print_diagnostics;
use crate::lexer::lex;
use crate::parser::parse;
use crate::typeck;

// ─────────────────────────────────────────────────────────────────────────────

const REPL_HELP: &str = r#"Brief REPL — interactive AI workflow authoring

Commands:
  :help      Show this help
  :check     Validate the current session state without running
  :clear     Reset session (discard all declarations)
  :quit :q   Exit the REPL

Usage:
  Type Brief code across multiple lines. Press Enter on an empty line after
  a complete declaration to evaluate it.

  Brace depth is tracked — multi-line blocks are accumulated until balanced.

Examples:
  brief> sealed type Status = Active | Inactive
  brief> struct User { name: @nonEmpty String }
  brief> task Hello : TaskBrief { goal = "Say hi" }
"#;

// ─────────────────────────────────────────────────────────────────────────────
// Session state
// ─────────────────────────────────────────────────────────────────────────────

struct ReplSession {
    /// Accumulated top-level declarations (persisted across inputs).
    declarations: String,
    /// Count of evaluations so far.
    count: usize,
}

impl ReplSession {
    fn new() -> Self {
        ReplSession { declarations: String::new(), count: 0 }
    }

    fn clear(&mut self) {
        self.declarations.clear();
        self.count = 0;
        println!("{} Session cleared.", "✓".green());
    }

    fn add_source(&mut self, src: &str) {
        if !self.declarations.is_empty() {
            self.declarations.push('\n');
        }
        self.declarations.push_str(src);
        self.count += 1;
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Entry point
// ─────────────────────────────────────────────────────────────────────────────

pub fn run_repl() {
    println!("{}", "Brief REPL v0.1.0 — type :help for commands, :quit to exit".dimmed());
    println!("{}", "If it compiles, the AI has everything it needs.".italic().dimmed());
    println!();

    let mut session = ReplSession::new();
    let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));

    loop {
        // Collect a complete input (handles multi-line blocks via brace depth).
        let input = match read_input("brief> ") {
            Some(s) => s,
            None    => break, // EOF
        };

        let trimmed = input.trim();
        if trimmed.is_empty() { continue; }

        // ── REPL commands ─────────────────────────────────────────────────
        match trimmed {
            ":quit" | ":q" => {
                println!("{}", "Goodbye.".dimmed());
                break;
            }
            ":help" | ":h" => {
                println!("{}", REPL_HELP);
                continue;
            }
            ":clear" => {
                session.clear();
                continue;
            }
            ":check" => {
                check_session(&session, &cwd);
                continue;
            }
            _ => {}
        }

        // ── Evaluate ─────────────────────────────────────────────────────
        eval_input(&mut session, trimmed, &cwd);
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Input reading with brace-depth tracking
// ─────────────────────────────────────────────────────────────────────────────

fn read_input(prompt: &str) -> Option<String> {
    let mut buffer = String::new();

    loop {
        let current_prompt = if buffer.is_empty() { prompt } else { "  ... " };
        print!("{}", current_prompt.cyan());
        io::stdout().flush().ok();

        let mut line = String::new();
        match io::stdin().read_line(&mut line) {
            Ok(0) => return if buffer.is_empty() { None } else { Some(buffer) },
            Ok(_) => {}
            Err(_) => return None,
        }

        buffer.push_str(&line);

        // Use the lexer for accurate brace-depth tracking — raw char counting
        // incorrectly increments inside string literals and comments.
        let depth = brace_depth_via_lexer(&buffer);

        // A complete input: depth balanced AND not empty AND last non-whitespace is `}`
        // or it's a single-line declaration (depth never went above 0).
        let balanced = depth <= 0;
        let has_block = buffer.contains('{');

        if balanced && (!has_block || buffer.trim_end().ends_with('}')) {
            return Some(buffer);
        }

        // Safety: if depth goes very negative, something is wrong.
        if depth < -2 {
            eprintln!("{} Unmatched braces — resetting input.", "⚠".yellow().bold());
            return Some(buffer);
        }
    }
}

/// Count unmatched open-braces in `src` by lexing it, ignoring any tokens the
/// lexer cannot parse. This is accurate over string literals and comments.
fn brace_depth_via_lexer(src: &str) -> i32 {
    let (tokens, _) = lex(src);
    let mut depth = 0i32;
    for tok in &tokens {
        match tok.token {
            crate::lexer::Token::LBrace => depth += 1,
            crate::lexer::Token::RBrace => depth -= 1,
            _ => {}
        }
    }
    depth
}

// ─────────────────────────────────────────────────────────────────────────────
// Evaluation
// ─────────────────────────────────────────────────────────────────────────────

fn eval_input(session: &mut ReplSession, src: &str, cwd: &std::path::Path) {
    // Build the full source: existing session declarations + new input.
    let full_source = if session.declarations.is_empty() {
        src.to_string()
    } else {
        format!("{}\n{}", session.declarations, src)
    };

    // Lex → parse.
    let (tokens, lex_errors) = lex(&full_source);
    if !lex_errors.is_empty() {
        for (start, end) in &lex_errors {
            eprintln!("{} unrecognised character at byte {}–{}", "error:".red().bold(), start, end);
        }
        return;
    }

    let (program, parse_errors) = parse(&tokens, &full_source);
    if !parse_errors.is_empty() {
        // Only show errors for the new input (not the whole session).
        let new_src_errors: Vec<_> = parse_errors.iter()
            .filter(|e| e.is_error())
            .collect();
        if !new_src_errors.is_empty() {
            print_diagnostics(&parse_errors, &full_source, "<repl>");
            return;
        }
    }

    // Semantic + type check.
    let file_dir = std::path::Path::new(".");
    let ctx = CheckContext { file_dir, cwd, manifest: None };
    let mut diags = checker::check(&program, &ctx);
    diags.extend(typeck::type_check_with_skills(
        &program,
        std::collections::HashMap::new(),
    ));

    let errors: Vec<_> = diags.iter().filter(|d| d.is_error()).collect();
    if !errors.is_empty() {
        print_diagnostics(&diags, &full_source, "<repl>");
        return;
    }

    // Print warnings but continue.
    let warnings: Vec<_> = diags.iter().filter(|d| d.is_warning()).collect();
    if !warnings.is_empty() {
        print_diagnostics(&warnings.into_iter().cloned().collect::<Vec<_>>(), &full_source, "<repl>");
    }

    // Successfully parsed and checked — add to session.
    session.add_source(src);

    // Print what was added.
    print_program_summary(&program, src);

    // Execute any tasks.
    execute_tasks(&program);
}

fn check_session(session: &ReplSession, cwd: &std::path::Path) {
    if session.declarations.is_empty() {
        println!("{} Session is empty.", "ℹ".blue().bold());
        return;
    }

    let (tokens, _) = lex(&session.declarations);
    let (program, parse_errors) = parse(&tokens, &session.declarations);

    let file_dir = std::path::Path::new(".");
    let ctx = CheckContext { file_dir, cwd, manifest: None };
    let mut diags = parse_errors;
    diags.extend(checker::check(&program, &ctx));
    diags.extend(typeck::type_check_with_skills(
        &program,
        std::collections::HashMap::new(),
    ));

    if diags.iter().any(|d| d.is_error()) {
        print_diagnostics(&diags, &session.declarations, "<repl-session>");
    } else {
        println!("{} Session is valid ({} declaration(s)).", "✅".green(), session.count);
    }
}

fn print_program_summary(program: &Program, src: &str) {
    // Determine what kind of thing was just added.
    let trimmed = src.trim();

    if !program.types.is_empty()
        && (trimmed.starts_with("sealed") || trimmed.starts_with("type"))
    {
        let names: Vec<_> = program.types.iter().map(|t| t.name.as_str()).collect();
        println!("{} type {}", "✓".green(), names.join(", ").cyan());
    } else if !program.structs.is_empty() && trimmed.starts_with("struct") {
        let names: Vec<_> = program.structs.iter().map(|s| s.name.as_str()).collect();
        println!("{} struct {}", "✓".green(), names.join(", ").cyan());
    } else if !program.effects.is_empty() && trimmed.starts_with("effect") {
        let names: Vec<_> = program.effects.iter().map(|e| e.name.as_str()).collect();
        println!("{} effect {}", "✓".green(), names.join(", ").cyan());
    } else if !program.protocols.is_empty() && trimmed.starts_with("protocol") {
        let names: Vec<_> = program.protocols.iter().map(|p| p.name.as_str()).collect();
        println!("{} protocol {}", "✓".green(), names.join(", ").cyan());
    } else if !program.tasks.is_empty() {
        // Tasks are executed separately — summary is printed by execute_tasks.
    } else {
        println!("{} ok", "✓".green());
    }
}

fn execute_tasks(program: &Program) {
    for task in &program.tasks {
        println!();
        for d in &task.decorators {
            println!("  {} @{}", "✦".blue(), d.name.cyan());
        }
        println!("{} {}", "◆ task".blue().bold(), task.name.bold());
        println!("  {:<8} {}", "goal:".dimmed(), task.goal.as_deref().unwrap_or("—").green());

        if !task.uses.is_empty() {
            println!("  {:<8} [{}]", "skills:".dimmed(), task.uses.join(", ").cyan());
        }

        for step in &task.steps {
            print!("  {} {}... ", "→".dimmed(), step.name);
            let effects = collect_effects(&step.body);
            if effects.is_empty() {
                println!("{}", "ok".green());
            } else {
                println!("{} {}", "performs".dimmed(), effects.join(", ").cyan());
            }
        }

        println!("{} Ready for AI.", "✅".green().bold());
    }
}

fn collect_effects(body: &[crate::ast::Stmt]) -> Vec<String> {
    body.iter().flat_map(|stmt| {
        let expr = match stmt {
            crate::ast::Stmt::Let  { value, .. } => value,
            crate::ast::Stmt::Expr { value, .. } => value,
        };
        collect_expr_effects(expr)
    }).collect()
}

fn collect_expr_effects(expr: &crate::ast::Expr) -> Vec<String> {
    match expr {
        crate::ast::Expr::Perform { skill, func, .. } => vec![format!("{skill}.{func}()")],
        crate::ast::Expr::Await { expr: inner, .. }   => collect_expr_effects(inner),
        crate::ast::Expr::Call  { args, .. }           => {
            args.iter().flat_map(collect_expr_effects).collect()
        }
        _ => Vec::new(),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn new_cwd() -> std::path::PathBuf {
        std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."))
    }

    #[test]
    fn test_eval_sealed_type() {
        let mut session = ReplSession::new();
        let cwd = new_cwd();
        eval_input(&mut session, "sealed type Status = Active | Inactive", &cwd);
        assert_eq!(session.count, 1);
        assert!(session.declarations.contains("Status"));
    }

    #[test]
    fn test_eval_struct() {
        let mut session = ReplSession::new();
        let cwd = new_cwd();
        eval_input(&mut session, "struct User { name: @nonEmpty String }", &cwd);
        assert_eq!(session.count, 1);
    }

    #[test]
    fn test_eval_invalid_does_not_add_to_session() {
        let mut session = ReplSession::new();
        let cwd = new_cwd();
        // Malformed — should not be added.
        eval_input(&mut session, "sealed type = Broken", &cwd);
        assert_eq!(session.count, 0);
    }

    #[test]
    fn test_eval_accumulates_across_inputs() {
        let mut session = ReplSession::new();
        let cwd = new_cwd();
        eval_input(&mut session, "sealed type Color = Red | Green | Blue", &cwd);
        eval_input(&mut session, "struct Theme { primary: Color }", &cwd);
        assert_eq!(session.count, 2);
        assert!(session.declarations.contains("Color"));
        assert!(session.declarations.contains("Theme"));
    }

    #[test]
    fn test_clear_resets_session() {
        let mut session = ReplSession::new();
        let cwd = new_cwd();
        eval_input(&mut session, "sealed type Foo = Bar", &cwd);
        assert_eq!(session.count, 1);
        session.clear();
        assert_eq!(session.count, 0);
        assert!(session.declarations.is_empty());
    }
}
