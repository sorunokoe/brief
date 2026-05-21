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

use std::collections::HashMap;
use std::fmt;
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

#[derive(Debug, Clone, PartialEq, Eq)]
enum Value {
    Unit,
    String(String),
    Int(i64),
    Variant(String, Option<Box<Value>>),
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Unit => write!(f, "()"),
            Value::String(value) => write!(f, "\"{value}\""),
            Value::Int(value) => write!(f, "{value}"),
            Value::Variant(name, Some(inner)) => write!(f, "{name}({inner})"),
            Value::Variant(name, None) => write!(f, "{name}"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ReplError {
    Parse(String),
    UnsupportedExpr(&'static str),
    NoMatchingArm(String),
}

impl fmt::Display for ReplError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ReplError::Parse(message) => write!(f, "{message}"),
            ReplError::UnsupportedExpr(kind) => write!(f, "unsupported expression in REPL evaluator: {kind}"),
            ReplError::NoMatchingArm(value) => write!(f, "no match arm for {value}"),
        }
    }
}

type ReplEnv = HashMap<String, Value>;

#[derive(Default)]
struct ReplEvaluator;

impl ReplEvaluator {
    fn eval_expr(&self, expr: &crate::ast::Expr, env: &ReplEnv) -> Result<Value, ReplError> {
        match expr {
            crate::ast::Expr::Str { value, .. } => Ok(Value::String(value.clone())),
            crate::ast::Expr::Int { value, .. } => Ok(Value::Int(*value)),
            crate::ast::Expr::Ident { name, .. } => Ok(env.get(name).cloned().unwrap_or_else(|| Value::Variant(name.clone(), None))),
            crate::ast::Expr::Call { receiver, func, args, .. } => self.eval_call(receiver.as_deref(), func, args, env),
            crate::ast::Expr::Match { scrutinee, arms } => {
                let scrutinee_val = self.eval_expr(scrutinee, env)?;

                for arm in arms {
                    if Self::pattern_matches(&arm.pattern, &scrutinee_val) {
                        let mut arm_env = env.clone();
                        if let crate::ast::Pattern::Variant1(_, binding) = &arm.pattern {
                            arm_env.insert(binding.clone(), Self::binding_value(&scrutinee_val));
                        }
                        return self.eval_expr(&arm.body, &arm_env);
                    }
                }

                Err(ReplError::NoMatchingArm(scrutinee_val.to_string()))
            }
            crate::ast::Expr::Await { .. } => Err(ReplError::UnsupportedExpr("await")),
            crate::ast::Expr::Perform { .. } => Err(ReplError::UnsupportedExpr("perform")),
        }
    }

    fn eval_call(&self, receiver: Option<&str>, func: &str, args: &[crate::ast::Expr], env: &ReplEnv) -> Result<Value, ReplError> {
        if receiver.is_some() {
            return Err(ReplError::UnsupportedExpr("field access"));
        }

        match args {
            [] => Ok(Value::Variant(func.to_string(), None)),
            [arg] => Ok(Value::Variant(func.to_string(), Some(Box::new(self.eval_expr(arg, env)?)))),
            _ => Err(ReplError::UnsupportedExpr("multi-argument call")),
        }
    }

    fn pattern_matches(pattern: &crate::ast::Pattern, value: &Value) -> bool {
        match (pattern, value) {
            (crate::ast::Pattern::Wildcard, _) => true,
            (crate::ast::Pattern::Variant(name), Value::Variant(variant, _)) => name == variant,
            (crate::ast::Pattern::Variant(name), Value::String(value)) => name == value,
            (crate::ast::Pattern::Variant1(name, _), Value::Variant(variant, _)) => name == variant,
            (crate::ast::Pattern::Variant1(name, _), Value::String(value)) => name == value,
            _ => false,
        }
    }

    fn binding_value(value: &Value) -> Value {
        match value {
            Value::Variant(_, Some(inner)) => inner.as_ref().clone(),
            Value::Variant(_, None) => Value::Unit,
            other => other.clone(),
        }
    }
}

fn parse_repl_expr(src: &str) -> Result<crate::ast::Expr, ReplError> {
    let expr_src = src.trim().trim_end_matches(';');
    let wrapped = format!(
        "task __repl_expr__ : TaskBrief {{\n    goal = \"eval\"\n    step Eval {{\n        {expr_src};\n    }}\n}}"
    );

    let (tokens, lex_errors) = lex(&wrapped);
    if !lex_errors.is_empty() {
        return Err(ReplError::Parse("failed to lex REPL expression".to_string()));
    }

    let (program, parse_errors) = parse(&tokens, &wrapped);
    if parse_errors.iter().any(|error| error.is_error()) {
        return Err(ReplError::Parse("failed to parse REPL expression".to_string()));
    }

    program
        .tasks
        .first()
        .and_then(|task| task.steps.first())
        .and_then(|step| step.body.first())
        .and_then(|stmt| match stmt {
            crate::ast::Stmt::Expr { value, .. } => Some(value.clone()),
            crate::ast::Stmt::Let { .. } => None,
        })
        .ok_or_else(|| ReplError::Parse("expected a REPL expression".to_string()))
}

fn try_eval_expression_input(src: &str) -> Option<Result<Value, ReplError>> {
    let expr = parse_repl_expr(src).ok()?;
    Some(ReplEvaluator.eval_expr(&expr, &ReplEnv::new()))
}

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
            if let Some(result) = try_eval_expression_input(src) {
                match result {
                    Ok(value) => println!("{} {}", "=".green(), value),
                    Err(err) => eprintln!("{} {}", "error:".red().bold(), err),
                }
                return;
            }

            print_diagnostics(&parse_errors, &full_source, "<repl>");
            return;
        }
    }

    // Semantic + type check.
    let file_dir = std::path::Path::new(".");
    let ctx = CheckContext { file_dir, cwd, manifest: None, brief_path: None, allow_missing_skills: false };
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
    let ctx = CheckContext { file_dir, cwd, manifest: None, brief_path: None, allow_missing_skills: false };
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
        if !task.effects.is_empty() {
            println!("  {:<8} [{}]", "effects:".dimmed(), task.effects.join(", ").cyan());
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
        crate::ast::Expr::Match { scrutinee, arms }   => {
            let mut out = collect_expr_effects(scrutinee);
            for arm in arms {
                out.extend(collect_expr_effects(&arm.body));
            }
            out
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

    fn eval_expr_src(src: &str) -> Result<Value, ReplError> {
        let expr = parse_repl_expr(src)?;
        ReplEvaluator.eval_expr(&expr, &ReplEnv::new())
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

    #[test]
    fn test_eval_match_variant_expression() {
        let value = eval_expr_src(r#"match iOS { iOS => "mobile" Android => "desktop" _ => "other" }"#)
            .expect("match expression should evaluate");
        assert_eq!(value, Value::String("mobile".to_string()));
    }

    #[test]
    fn test_eval_match_wildcard_expression() {
        let value = eval_expr_src(r#"match Web { iOS => "mobile" _ => "other" }"#)
            .expect("wildcard arm should evaluate");
        assert_eq!(value, Value::String("other".to_string()));
    }

    #[test]
    fn test_eval_match_variant_binding_expression() {
        let value = eval_expr_src(r#"match Ok("Ada") { Ok(user) => user _ => "unknown" }"#)
            .expect("variant binding should evaluate");
        assert_eq!(value, Value::String("Ada".to_string()));
    }

    #[test]
    fn test_eval_match_no_matching_arm_returns_error() {
        let err = eval_expr_src(r#"match Web { iOS => "mobile" Android => "desktop" }"#)
            .expect_err("missing arm should be an error");
        assert!(matches!(err, ReplError::NoMatchingArm(value) if value == "Web"));
    }
}
