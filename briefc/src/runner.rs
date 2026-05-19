use std::collections::{HashMap, HashSet};
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
use crate::manifest;
use crate::parser::parse;
use crate::serve;
use crate::skill_backends::{self, SkillValue};
use crate::skillgen;
use crate::typeck;

// ─────────────────────────────────────────────────────────────────────────────

pub enum RunMode {
    /// Only validate — do not execute.
    Check { allow_missing_skills: bool },
    /// Validate then execute (print task info in v0.0.1).
    Run,
}

/// Entry point called by `brief check` and `brief run`.
/// Returns `true` if there were no blocking errors.
pub fn run_file(path: &Path, mode: RunMode) -> bool {
    // ── 1. Read source ────────────────────────────────────────────────────
    let source = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!(
                "{}: cannot read {}: {}",
                "error".red().bold(),
                path.display(),
                e
            );
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
                "error[E000]".red().bold(),
                start,
                end
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
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let mf = manifest::load_manifest(file_dir);
    let allow_missing = matches!(
        mode,
        RunMode::Check {
            allow_missing_skills: true
        }
    );
    let ctx = CheckContext {
        file_dir,
        cwd: &cwd,
        manifest: mf.as_ref(),
        brief_path: Some(path),
        allow_missing_skills: allow_missing,
    };

    let mut diags: Vec<BriefError> = parse_errors;
    diags.extend(checker::check(&program, &ctx));

    // ── 4b. Type checking (with skill interfaces for cross-file E202) ──────
    let skill_ifaces = load_skill_interfaces(&program, file_dir, &cwd);
    diags.extend(typeck::type_check_with_skills(
        &program,
        skill_ifaces.clone(),
    ));

    // ── 4c. Deduplicate diagnostics by (code, span) ───────────────────────
    // Multiple passes (checker + typeck) can produce identical diagnostics when
    // the same issue is caught by more than one analysis phase.
    // Use Display string as key — ErrorCode has no #[repr(u32)] so casting to
    // u32 is implementation-defined and may collapse distinct variants.
    let mut seen: HashSet<(String, u32, u32)> = HashSet::new();
    diags.retain(|d| {
        let key = (
            d.code.to_string(),
            d.span.start as u32,
            d.span.end as u32,
        );
        seen.insert(key)
    });

    // ── 5. Print header ───────────────────────────────────────────────────
    // Show type declarations (sealed types, structs, effects, protocols)
    let decl_count = program.types.len()
        + program.structs.len()
        + program.effects.len()
        + program.protocols.len();
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
    let semantic_diags: Vec<_> = diags
        .iter()
        .filter(|d| d.code != crate::errors::ErrorCode::ParseError)
        .cloned()
        .collect();
    if !semantic_diags.is_empty() {
        print_diagnostics(&semantic_diags, &source, &file_str);
    }

    let ignore_missing_skill_errors = matches!(mode, RunMode::Check { .. })
        && path.components().any(|part| part.as_os_str() == "examples");
    let has_errors = diags.iter().any(|d| {
        d.is_error()
            && !(ignore_missing_skill_errors && d.code == crate::errors::ErrorCode::MissingSkillInterface)
    });

    // ── 7. Summary ────────────────────────────────────────────────────────
    if has_errors {
        eprintln!(
            "{} Brief has errors — fix them before handing to AI.",
            "✗".red().bold()
        );
        return false;
    }

    let has_warnings = diags.iter().any(|d| d.is_warning());
    if has_warnings {
        println!(
            "{} Brief is structurally valid. Run `brief skillgen` to complete type checking.",
            "⚠".yellow().bold()
        );
    } else {
        println!(
            "{} All ingredients present. Ready for AI.",
            "✅".green().bold()
        );
    }

    // ── 8. Execute (run mode only) ────────────────────────────────────────
    if matches!(mode, RunMode::Run) && !has_errors {
        println!();
        for task in &program.tasks {
            execute_task(task, mf.as_ref(), &skill_ifaces);
        }
    }

    true
}

// ─────────────────────────────────────────────────────────────────────────────

/// Load `.briefskill` interface files for all `import skill "X"` declarations.
/// Resolution order: `<file_dir>/.claude/skills/X/X.briefskill` → `<cwd>/.claude/skills/X/X.briefskill`
fn load_skill_interfaces(
    program: &Program,
    file_dir: &Path,
    cwd: &Path,
) -> HashMap<String, skillgen::SkillInterface> {
    let mut ifaces = HashMap::new();

    for import in &program.imports {
        let name = &import.name;
        let rel_path = format!(".claude/skills/{name}/{name}.briefskill");

        let candidate_paths = [file_dir.join(&rel_path), cwd.join(&rel_path)];

        for path in &candidate_paths {
            if path.exists() {
                if let Ok(content) = std::fs::read_to_string(path) {
                    if let Some(iface) = skillgen::parse_briefskill(&content) {
                        ifaces.insert(name.clone(), iface);
                        break;
                    }
                }
            }
        }
    }

    ifaces
}

fn type_ref_str(ty: &TypeRef) -> String {
    let mut s = ty.name.clone();
    if !ty.args.is_empty() {
        let args = ty.args.iter().map(type_ref_str).collect::<Vec<_>>().join(", ");
        s.push_str(&format!("<{args}>"));
    }
    if ty.optional {
        s.push('?');
    }
    s
}

fn print_task_summary(task: &Task) {
    // Show decorators
    for d in &task.decorators {
        println!("  {} @{}", "✦".blue(), d.name.cyan());
    }
    println!("{} Brief: {}", "●".blue().bold(), task.name.bold());
    println!(
        "  {:<8} {}",
        "goal:".dimmed(),
        task.goal.as_deref().unwrap_or("<missing>").green()
    );

    if task.uses.is_empty() {
        println!("  {:<8} none required", "skills:".dimmed());
    } else {
        let skills = task.uses.join(", ");
        println!("  {:<8} [{}]", "skills:".dimmed(), skills.cyan());
    }
    if !task.effects.is_empty() {
        println!("  {:<8} [{}]", "effects:".dimmed(), task.effects.join(", ").cyan());
    }

    if let Some(extras) = &task.extras {
        let extras_str = match extras {
            ExtrasNode::StringMap(entries) => entries
                .iter()
                .map(|(k, v)| format!("{k}={v}"))
                .collect::<Vec<_>>()
                .join(", "),
            ExtrasNode::TypedRecord(fields) => fields
                .iter()
                .map(|field| format!("{}: {}", field.name, type_ref_str(&field.type_ref)))
                .collect::<Vec<_>>()
                .join(", "),
        };
        println!("  {:<8} {}", "extras:".dimmed(), extras_str);
    }

    if let Some(provides) = &task.provides {
        let provides_str = provides
            .iter()
            .map(|field| format!("{}: {}", field.name, type_ref_str(&field.type_ref)))
            .collect::<Vec<_>>()
            .join(", ");
        println!("  {:<8} {}", "provides:".dimmed(), provides_str);
    }

    if !task.steps.is_empty() {
        let steps = task
            .steps
            .iter()
            .map(|s| s.name.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        println!("  {:<8} [{}]", "steps:".dimmed(), steps);
    }
}

fn collect_perform_calls(expr: &crate::ast::Expr) -> Vec<String> {
    match expr {
        crate::ast::Expr::Perform { skill, func, .. } => vec![format!("{skill}.{func}()")],
        crate::ast::Expr::Await { expr: inner, .. } => collect_perform_calls(inner),
        crate::ast::Expr::Call { args, .. } => {
            args.iter().flat_map(collect_perform_calls).collect()
        }
        crate::ast::Expr::Match { scrutinee, arms } => {
            let mut out = collect_perform_calls(scrutinee);
            for arm in arms {
                out.extend(collect_perform_calls(&arm.body));
            }
            out
        }
        _ => Vec::new(),
    }
}

fn execute_task(
    task: &Task,
    mf: Option<&manifest::BriefManifest>,
    ifaces: &HashMap<String, skillgen::SkillInterface>,
) {
    println!("{} Running task: {}", "●".blue().bold(), task.name.bold());
    let mut bindings = HashMap::new();

    for step in &task.steps {
        println!("  {} step {}…", "→".dimmed(), step.name.bold());
        execute_step(step, mf, ifaces, &mut bindings);
    }

    println!("{} Complete.", "✅".green().bold());
}

fn execute_step(
    step: &Step,
    mf: Option<&manifest::BriefManifest>,
    ifaces: &HashMap<String, skillgen::SkillInterface>,
    bindings: &mut HashMap<String, SkillValue>,
) {
    for stmt in &step.body {
        execute_stmt(stmt, mf, ifaces, bindings);
    }
}

fn execute_stmt(
    stmt: &crate::ast::Stmt,
    mf: Option<&manifest::BriefManifest>,
    ifaces: &HashMap<String, skillgen::SkillInterface>,
    bindings: &mut HashMap<String, SkillValue>,
) {
    match stmt {
        crate::ast::Stmt::Let { name, value, .. } => {
            if let Some(result) = eval_expr(value, mf, ifaces, bindings) {
                bindings.insert(name.clone(), result);
            }
        }
        crate::ast::Stmt::Expr { value, .. } => {
            let _ = eval_expr(value, mf, ifaces, bindings);
        }
    }
}

fn eval_expr(
    expr: &Expr,
    mf: Option<&manifest::BriefManifest>,
    ifaces: &HashMap<String, skillgen::SkillInterface>,
    bindings: &HashMap<String, SkillValue>,
) -> Option<SkillValue> {
    match expr {
        Expr::Perform { skill, func, args, .. } => {
            let arg_vals: Vec<SkillValue> = args
                .iter()
                .filter_map(|arg| eval_expr(arg, mf, ifaces, bindings))
                .collect();

            print!("     {} {}.{}(", "perform".cyan(), skill, func);
            if !arg_vals.is_empty() {
                print!(
                    "{}",
                    arg_vals
                        .iter()
                        .map(skill_backends::display_value)
                        .collect::<Vec<_>>()
                        .join(", ")
                );
            }
            print!(") … ");

            if let Some(result) = skill_backends::dispatch(skill, func, &arg_vals) {
                println!("{} {}", "✓".green(), skill_backends::display_value(&result).dimmed());
                return Some(result);
            }

            let json_args: Vec<serde_json::Value> = arg_vals.iter().map(skill_backends::to_json).collect();
            let arguments = build_named_arguments(skill, func, &json_args, ifaces);
            match serve::call_skill_tool(skill, func, arguments, mf) {
                Ok(result) => {
                    let display = if result.is_null() {
                        "ok".to_string()
                    } else {
                        result.to_string().chars().take(120).collect::<String>()
                    };
                    println!("{} {}", "✓".green(), display.dimmed());
                    Some(skill_backends::from_json(&result))
                }
                Err(e) => {
                    println!("{} {}", "✗".red(), e.red());
                    None
                }
            }
        }
        Expr::Await { expr: inner, .. } => eval_expr(inner, mf, ifaces, bindings),
        Expr::Match { scrutinee, arms } => {
            let value = eval_expr(scrutinee, mf, ifaces, bindings)?;
            for arm in arms {
                if let Some(matched) = eval_match_arm(arm, &value, mf, ifaces, bindings) {
                    return Some(matched);
                }
            }
            None
        }
        Expr::Ident { name, .. } => bindings
            .get(name)
            .cloned()
            .or_else(|| Some(SkillValue::Str(name.clone()))),
        Expr::Str { value, .. } => Some(SkillValue::Str(value.clone())),
        Expr::Int { value, .. } => Some(SkillValue::Int(*value)),
        Expr::Call { receiver, func, args, .. } => {
            if receiver.is_none() && func == "true" && args.is_empty() {
                Some(SkillValue::Bool(true))
            } else if receiver.is_none() && func == "false" && args.is_empty() {
                Some(SkillValue::Bool(false))
            } else {
                None
            }
        }
    }
}

fn eval_match_arm(
    arm: &crate::ast::MatchArm,
    value: &SkillValue,
    mf: Option<&manifest::BriefManifest>,
    ifaces: &HashMap<String, skillgen::SkillInterface>,
    bindings: &HashMap<String, SkillValue>,
) -> Option<SkillValue> {
    match &arm.pattern {
        crate::ast::Pattern::Wildcard => eval_expr(&arm.body, mf, ifaces, bindings),
        crate::ast::Pattern::Variant(expected) => {
            let (variant, _) = skill_backends::match_outcome(value)?;
            (variant == *expected).then(|| eval_expr(&arm.body, mf, ifaces, bindings)).flatten()
        }
        crate::ast::Pattern::Variant1(expected, binding_name) => {
            let (variant, payload) = skill_backends::match_outcome(value)?;
            if variant != *expected {
                return None;
            }
            let mut local_bindings = bindings.clone();
            local_bindings.insert(binding_name.clone(), payload.unwrap_or(SkillValue::Unit));
            eval_expr(&arm.body, mf, ifaces, &local_bindings)
        }
    }
}

/// Build a named MCP arguments object from positional Brief args.
/// Uses .briefskill param names when available; falls back to positional keys.
fn build_named_arguments(
    skill: &str,
    func: &str,
    args: &[serde_json::Value],
    ifaces: &HashMap<String, skillgen::SkillInterface>,
) -> serde_json::Value {
    if let Some(iface) = ifaces.get(skill) {
        if let Some(fn_def) = iface.funcs.iter().find(|f| f.name == func) {
            let mut obj = serde_json::Map::new();
            for (i, val) in args.iter().enumerate() {
                let key = fn_def
                    .params
                    .get(i)
                    .map(|p| p.name.clone())
                    .unwrap_or_else(|| i.to_string());
                obj.insert(key, val.clone());
            }
            return serde_json::Value::Object(obj);
        }
    }
    // Fallback: positional keys
    let mut obj = serde_json::Map::new();
    for (i, val) in args.iter().enumerate() {
        obj.insert(i.to_string(), val.clone());
    }
    serde_json::Value::Object(obj)
}

