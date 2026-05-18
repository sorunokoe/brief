/// `brief verify` — annotation discovery, verifier dispatch, and lock writing.
///
/// Steps:
/// 1. Parse `.brief` file + load `.briefskill` interfaces
/// 2. Collect `(annotation, value)` pairs from `perform Skill.fn(arg)` call sites
///    where the matching skill param has a dynamic annotation
/// 3. Look up each annotation in `[verifiers.*]`; report E309 if unconfigured
/// 4. Call each verifier via the MCP protocol client
/// 5. On all ok: write `.brief.lock`
/// 6. On any fail: print errors, exit 1 without writing lock

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use colored::Colorize;
use serde_json::json;

use crate::ast::{Expr, Program};
use crate::checker;
use crate::lexer::lex;
use crate::lock::{self, LockFile, LockMeta, VerificationResult as LockEntry, VerifyStatus, now_rfc3339, sha256_file_hash};
use crate::manifest::{self};
use crate::parser::parse;
use crate::skillgen::{self, SkillInterface};
use crate::verifier::{self};

// ─────────────────────────────────────────────────────────────────────────────

pub fn run_verify(path: &Path) -> bool {
    // ── 1. Read + parse source ────────────────────────────────────────────
    let source = match std::fs::read_to_string(path) {
        Ok(s)  => s,
        Err(e) => {
            eprintln!("{}: cannot read {}: {e}", "error".red().bold(), path.display());
            return false;
        }
    };

    let (tokens, lex_errs) = lex(&source);
    if !lex_errs.is_empty() {
        eprintln!("{}: lex errors in {}", "error".red().bold(), path.display());
        return false;
    }

    let (program, parse_errs) = parse(&tokens, &source);
    if parse_errs.iter().any(|d| d.is_error()) {
        for d in &parse_errs { eprintln!("{}", d.message); }
        return false;
    }

    // ── 2. Load manifest + skill interfaces ────────────────────────────────
    let file_dir = path.parent().unwrap_or(Path::new("."));
    let cwd      = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let mf       = manifest::load_manifest(file_dir);

    let ctx = checker::CheckContext {
        file_dir,
        cwd:                  &cwd,
        manifest:             mf.as_ref(),
        brief_path:           None, // skip E303 — we ARE building the lock
        allow_missing_skills: false,
    };

    let ifaces = load_skill_interfaces(&program, &ctx);

    // ── 3. Collect verification obligations ───────────────────────────────
    let obligations = collect_obligations(&program, &ifaces);

    if obligations.is_empty() {
        println!("{} No dynamic annotations to verify.", "✅".green().bold());
        write_empty_lock(path);
        return true;
    }

    // ── 4. Check for unconfigured annotations (E309-equivalent) ──────────
    let verifiers = mf.as_ref().map(|m| &m.verifiers).cloned().unwrap_or_default();
    let mut missing_verifiers: Vec<String> = Vec::new();

    for (annotation, _value, _context) in &obligations {
        let ann_key = strip_at(annotation);
        if !verifiers.contains_key(ann_key) && !verifiers.contains_key(annotation.as_str()) {
            if !missing_verifiers.contains(annotation) {
                missing_verifiers.push(annotation.clone());
            }
        }
    }

    if !missing_verifiers.is_empty() {
        for ann in &missing_verifiers {
            eprintln!(
                "{}: no verifier configured for {} — add [verifiers.\"{}\"] to brief.toml",
                "error[E309]".red().bold(), ann.cyan(), ann
            );
        }
        eprintln!();
        eprintln!("{} brief verify cannot proceed — configure missing verifiers first.", "✗".red().bold());
        return false;
    }

    // ── 5. Call each verifier ─────────────────────────────────────────────
    println!("{} Verifying {} annotation{}...",
        "●".blue().bold(), obligations.len(),
        if obligations.len() == 1 { "" } else { "s" }
    );
    println!();

    let mut verified: HashMap<String, LockEntry> = HashMap::new();
    let mut all_ok = true;

    for (annotation, value, context) in &obligations {
        let lock_key = format!("{annotation}:{value}");
        if verified.contains_key(&lock_key) {
            continue; // already verified (deduplicated)
        }

        let ann_key = strip_at(annotation);
        let config = verifiers.get(ann_key)
            .or_else(|| verifiers.get(annotation.as_str()))
            .unwrap(); // safe — we checked above

        print!("  {} {} {}... ",
            "→".dimmed(), annotation.cyan(), value.italic()
        );

        let result = verifier::dispatch(config, annotation, &value, context.clone());

        match &result.status {
            VerifyStatus::Ok => {
                println!("{}", "ok".green().bold());
                verified.insert(lock_key, LockEntry {
                    status:  VerifyStatus::Ok,
                    message: result.message,
                });
            }
            VerifyStatus::Fail => {
                let msg = result.message.as_deref().unwrap_or("verification failed");
                println!("{} {}", "fail".red().bold(), msg.dimmed());
                verified.insert(lock_key, LockEntry {
                    status:  VerifyStatus::Fail,
                    message: result.message,
                });
                all_ok = false;
            }
        }
    }

    println!();

    if !all_ok {
        eprintln!("{} Verification failed — fix the above errors before running `brief serve`.",
            "✗".red().bold());
        return false;
    }

    // ── 6. Write .brief.lock ──────────────────────────────────────────────
    let lock_path = lock::lock_path(path);
    let source_hash = std::fs::read(path)
        .map(|b| sha256_file_hash(&b))
        .unwrap_or_else(|_| "unknown".into());

    let lock_file = LockFile {
        meta:     LockMeta { brief_hash: source_hash, verified_at: now_rfc3339() },
        verified: verified,
    };

    match lock::write_lock(&lock_path, &lock_file) {
        Ok(_) => {
            println!("{} Lock written: {}", "✅".green().bold(), lock_path.display());
            println!("{}", "Contract sealed. You may now run `brief serve`.".dimmed());
            true
        }
        Err(e) => {
            eprintln!("{}: cannot write {}: {e}", "error".red().bold(), lock_path.display());
            false
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────

/// Load all `.briefskill` interfaces referenced by the program.
fn load_skill_interfaces(
    program: &Program,
    ctx:     &checker::CheckContext<'_>,
) -> HashMap<String, SkillInterface> {
    let mut map = HashMap::new();
    for import in &program.imports {
        if let Some(path) = checker::find_skill_interface(&import.name, ctx) {
            if let Ok(content) = std::fs::read_to_string(&path) {
                if let Some(iface) = skillgen::parse_briefskill(&content) {
                    map.insert(import.name.clone(), iface);
                }
            }
        }
    }
    map
}

// ─────────────────────────────────────────────────────────────────────────────

/// An item to be verified: (annotation_name, value_string, context_json)
type Obligation = (String, String, serde_json::Value);

/// Traverse all `perform Skill.fn(args)` calls; for each param with a dynamic
/// annotation and a literal argument value, produce a verification obligation.
fn collect_obligations(
    program: &Program,
    ifaces:  &HashMap<String, SkillInterface>,
) -> Vec<Obligation> {
    let mut out: Vec<Obligation> = Vec::new();
    let mut performs: Vec<(&str, &str, &[Expr], crate::ast::Span)> = Vec::new();

    // Collect from task steps.
    for task in &program.tasks {
        for step in &task.steps {
            for stmt in &step.body {
                collect_expr_performs(stmt_expr(stmt), &mut performs);
            }
        }
    }
    // Collect from test blocks.
    for test in &program.tests {
        for stmt in &test.body {
            collect_expr_performs(stmt_expr(stmt), &mut performs);
        }
    }

    for (skill_name, fn_name, args, _span) in &performs {
        let iface = match ifaces.get(*skill_name) { Some(i) => i, None => continue };
        let skill_fn = match iface.funcs.iter().find(|f| f.name == *fn_name) { Some(f) => f, None => continue };

        for (param_idx, param) in skill_fn.params.iter().enumerate() {
            if param.dynamic_attrs.is_empty() { continue; }
            let arg_val = args.get(param_idx).and_then(expr_literal_str);
            if let Some(val) = arg_val {
                let context = json!({
                    "skill":    skill_name,
                    "function": fn_name,
                    "param":    param.name,
                });
                for attr in &param.dynamic_attrs {
                    out.push((attr.clone(), val.clone(), context.clone()));
                }
            }
        }
    }

    // Deduplicate by (annotation, value).
    out.dedup_by(|a, b| a.0 == b.0 && a.1 == b.1);
    out
}

fn collect_expr_performs<'a>(
    expr: &'a Expr,
    out:  &mut Vec<(&'a str, &'a str, &'a [Expr], crate::ast::Span)>,
) {
    match expr {
        Expr::Perform { skill, func, args, span, .. } => {
            out.push((skill, func, args, *span));
            for arg in args { collect_expr_performs(arg, out); }
        }
        Expr::Await  { expr: inner, .. } => collect_expr_performs(inner, out),
        Expr::Call   { args, ..         } => { for a in args { collect_expr_performs(a, out); } }
        Expr::Ident { .. } | Expr::Str { .. } | Expr::Int { .. } => {}
    }
}

fn stmt_expr(stmt: &crate::ast::Stmt) -> &Expr {
    match stmt {
        crate::ast::Stmt::Expr { value, .. } => value,
        crate::ast::Stmt::Let  { value, .. } => value,
    }
}

/// Extract a string representation from a literal expression.
fn expr_literal_str(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Str { value, .. } => Some(value.clone()),
        Expr::Int { value, .. } => Some(value.to_string()),
        _                       => None,
    }
}

fn strip_at(annotation: &str) -> &str {
    annotation.strip_prefix('@').unwrap_or(annotation)
}

// ─────────────────────────────────────────────────────────────────────────────

fn write_empty_lock(brief_path: &Path) {
    let lock_path = lock::lock_path(brief_path);
    let source_hash = std::fs::read(brief_path)
        .map(|b| sha256_file_hash(&b))
        .unwrap_or_else(|_| "unknown".into());
    let lf = LockFile {
        meta:     LockMeta { brief_hash: source_hash, verified_at: now_rfc3339() },
        verified: HashMap::new(),
    };
    if lock::write_lock(&lock_path, &lf).is_err() {
        eprintln!("warn: could not write lock file {}", lock_path.display());
    }
}
