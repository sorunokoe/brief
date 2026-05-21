/// `brief verify` — annotation discovery, verifier dispatch, and lock writing.
///
/// Steps:
/// 1. Parse `.brief` file + load `.briefskill` interfaces
/// 2. Verify skill capabilities: connect to each skill MCP server, call `tools/list`,
///    confirm every function in `.briefskill` exists in the server (E401/E402)
/// 3. Collect `(annotation, value)` pairs from `perform Skill.fn(arg)` call sites
///    where the matching skill param has a dynamic annotation
/// 4. Look up each annotation in `[verifiers.*]`; report E309 if unconfigured
/// 5. Call each verifier via the MCP protocol client
/// 6. On all ok: write `.brief.lock` (with capability results + skill hashes)
/// 7. On any fail: print errors, exit 1 without writing lock

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use colored::Colorize;
use serde_json::json;

use crate::ast::{Expr, NeedKind, Program};
use crate::checker;
use crate::lexer::lex;
use crate::lock::{self, LockFile, LockMeta, VerificationResult as LockEntry, VerifyStatus, now_rfc3339, sha256_file_hash};
use crate::manifest::{self, BriefManifest};
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

    let ifaces      = load_skill_interfaces(&program, &ctx);
    let iface_paths = load_skill_paths(&program, &ctx);

    // Compute hashes of all loaded .briefskill files for the lock.
    let skill_hashes = compute_skill_hashes(&iface_paths);

    // ── 3. Capability verification (E401 / E402) ──────────────────────────
    // For each skill that has MCP config, confirm every .briefskill function
    // actually exists in the real server's tools/list.
    let (caps_ok, capabilities) = verify_skill_capabilities(&program, &ifaces, mf.as_ref());
    if !caps_ok {
        eprintln!();
        eprintln!("{} Capability verification failed — fix the above errors before re-running `brief verify`.",
            "✗".red().bold());
        return false;
    }

    // ── 3b. Needs verification (E411) ─────────────────────────────────────
    // Verify `needs { env "VAR", feature "FLAG" }` prerequisites.
    let (needs_ok, needs_results) = verify_needs(&program, mf.as_ref());
    if !needs_ok {
        eprintln!();
        eprintln!("{} Prerequisites not met — fix the above errors before re-running `brief verify`.",
            "✗".red().bold());
        return false;
    }

    // ── 4. Collect verification obligations ───────────────────────────────
    let obligations = collect_obligations(&program, &ifaces);

    if obligations.is_empty() && capabilities.is_empty() {
        println!("{} No dynamic annotations to verify.", "✅".green().bold());
        let mut all_verified = needs_results;
        write_lock_with_caps(path, all_verified, capabilities, skill_hashes);
        return true;
    }

    if obligations.is_empty() {
        // Capabilities verified; no annotation obligations — write lock and done.
        let all_verified = needs_results;
        write_lock_with_caps(path, all_verified, capabilities, skill_hashes);
        return true;
    }

    // ── 5. Check for unconfigured annotations (E309-equivalent) ──────────
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

    // ── 6. Call each verifier ─────────────────────────────────────────────
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

    // ── 7. Write .brief.lock ──────────────────────────────────────────────
    let mut all_verified = needs_results;
    all_verified.extend(verified);
    write_lock_with_caps(path, all_verified, capabilities, skill_hashes)
}

// ─────────────────────────────────────────────────────────────────────────────

/// Verify `needs { env "VAR", feature "FLAG" }` prerequisites across all tasks.
///
/// Returns `(all_ok, results_for_lock)`.
/// - `env "X"` → checked via `builtin_env` (no config needed)
/// - `feature "F"` / `config "K"` → routed to `[verifiers.feature]` / `[verifiers.config]`
///   from `brief.toml`; if no verifier configured, this is an error (not a warning)
fn verify_needs(
    program:  &Program,
    manifest: Option<&BriefManifest>,
) -> (bool, HashMap<String, LockEntry>) {
    let mut results = HashMap::new();
    let mut any_fail = false;

    for task in &program.tasks {
        if task.needs.is_empty() { continue; }

        println!("{} Verifying prerequisites for task '{}'...", "●".blue().bold(), task.name);

        for need in &task.needs {
            let kind_str = match need.kind {
                NeedKind::Env     => "env",
                NeedKind::Feature => "feature",
                NeedKind::Config  => "config",
            };
            let lock_key = format!("needs:{kind_str}:{}", need.key);

            // Deduplicate: skip if already checked.
            if results.contains_key(&lock_key) { continue; }

            print!("  {} {kind_str} {}... ", "→".dimmed(), need.key.cyan());

            let result = match need.kind {
                NeedKind::Env => verifier::builtin_env(&need.key),
                NeedKind::Feature | NeedKind::Config => {
                    match manifest.and_then(|m| m.verifiers.get(kind_str)) {
                        Some(cfg) => verifier::dispatch(cfg, kind_str, &need.key, json!({})),
                        None => verifier::VerificationResult::fail(&format!(
                            "no verifier configured for '{kind_str}' in brief.toml \
                             — add [verifiers.{kind_str}] to brief.toml"
                        )),
                    }
                }
            };

            match &result.status {
                VerifyStatus::Ok => {
                    println!("{}", "ok".green().bold());
                    results.insert(lock_key, LockEntry { status: VerifyStatus::Ok, message: result.message });
                }
                VerifyStatus::Fail => {
                    let msg = result.message.as_deref().unwrap_or("failed");
                    println!("{} {}", "fail".red().bold(), msg.dimmed());
                    results.insert(lock_key, LockEntry { status: VerifyStatus::Fail, message: result.message });
                    any_fail = true;
                }
            }
        }
    }

    (!any_fail, results)
}

/// Write the `.brief.lock` file with annotation results, capability results, and skill hashes.
fn write_lock_with_caps(
    brief_path:   &Path,
    verified:     HashMap<String, LockEntry>,
    capabilities: HashMap<String, LockEntry>,
    skill_hashes: HashMap<String, String>,
) -> bool {
    let lock_path = lock::lock_path(brief_path);
    let source_hash = std::fs::read(brief_path)
        .map(|b| sha256_file_hash(&b))
        .unwrap_or_else(|_| "unknown".into());

    let lock_file = LockFile {
        meta:         LockMeta { brief_hash: source_hash, verified_at: now_rfc3339(), skill_hashes },
        verified,
        capabilities,
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

/// Load the filesystem paths of `.briefskill` files for each imported skill.
fn load_skill_paths(
    program: &Program,
    ctx:     &checker::CheckContext<'_>,
) -> HashMap<String, PathBuf> {
    program.imports.iter()
        .filter_map(|import| {
            let path = checker::find_skill_interface(&import.name, ctx)?;
            Some((import.name.clone(), path))
        })
        .collect()
}

/// Compute `sha256:<hex>` hashes for each `.briefskill` file.
fn compute_skill_hashes(paths: &HashMap<String, PathBuf>) -> HashMap<String, String> {
    paths.iter()
        .filter_map(|(name, path)| {
            let bytes = std::fs::read(path).ok()?;
            Some((name.clone(), sha256_file_hash(&bytes)))
        })
        .collect()
}

// ─────────────────────────────────────────────────────────────────────────────
// Capability verification (E401 / E402)
// ─────────────────────────────────────────────────────────────────────────────

/// For each skill that has MCP config in `brief.toml [skills.*]`, connect to the
/// skill server, call `tools/list`, and verify every function in `.briefskill`
/// appears in the server's tool list.
///
/// - Skills with no MCP config (path-only) are skipped silently.
/// - Returns `(all_ok, capabilities_map)` where `capabilities_map` maps
///   `"SkillName.funcName"` → `VerificationResult`.
fn verify_skill_capabilities(
    program: &Program,
    ifaces:  &HashMap<String, SkillInterface>,
    mf:      Option<&BriefManifest>,
) -> (bool, HashMap<String, LockEntry>) {
    // Collect the skills actually used in perform calls.
    let used_skills: std::collections::HashSet<&str> = program.tasks.iter()
        .flat_map(|t| t.steps.iter())
        .flat_map(|s| s.body.iter())
        .filter_map(|stmt| {
            let expr = match stmt {
                crate::ast::Stmt::Expr { value, .. } | crate::ast::Stmt::Let { value, .. } => value,
            };
            if let Expr::Perform { skill, .. } = expr { Some(skill.as_str()) } else { None }
        })
        .collect();

    let skills_map = mf.map(|m| &m.skills);
    let mut capabilities: HashMap<String, LockEntry> = HashMap::new();
    let mut all_ok = true;
    let mut any_checked = false;

    for (skill_name, iface) in ifaces {
        let config = skills_map
            .and_then(|m| m.get(skill_name))
            .map(|e| e.as_config());

        let config = match config {
            Some(c) if c.has_mcp() => c,
            _ => {
                // No MCP config — skip capability check for this skill.
                // Warn if the skill is actually used in perform calls.
                if used_skills.contains(skill_name.as_str()) {
                    eprintln!(
                        "{}: skill '{}' is used in perform calls but has no mcp_command/mcp_url in brief.toml — \
                         add [skills.{}] to enable capability verification",
                        "warning".yellow().bold(), skill_name.cyan(), skill_name
                    );
                }
                continue;
            }
        };

        any_checked = true;
        print!("{} Checking capabilities: {}... ", "●".blue().bold(), skill_name.cyan());

        match verifier::list_skill_tools(&config, skill_name) {
            Err(e) => {
                println!("{}", "unreachable".red().bold());
                eprintln!(
                    "  {}: skill server for '{}' is unreachable: {e}",
                    "error[E402]".red().bold(), skill_name.cyan()
                );
                // Mark all functions for this skill as failed.
                for func in &iface.funcs {
                    let key = format!("{skill_name}.{}", func.name);
                    capabilities.insert(key, LockEntry {
                        status:  VerifyStatus::Fail,
                        message: Some(format!("server unreachable: {e}")),
                    });
                }
                all_ok = false;
            }
            Ok(server_tools) => {
                let mut skill_ok = true;
                for func in &iface.funcs {
                    let qualified = format!("{skill_name}.{}", func.name);
                    // Accept both qualified ("SkillName.funcName") and bare ("funcName").
                    let found = server_tools.contains(&qualified)
                        || server_tools.contains(&format!("{skill_name}.{}", func.name))
                        || server_tools.iter().any(|t| {
                            t == &func.name || t.ends_with(&format!(".{}", func.name))
                        });
                    if found {
                        capabilities.insert(qualified, LockEntry { status: VerifyStatus::Ok, message: None });
                    } else {
                        capabilities.insert(qualified.clone(), LockEntry {
                            status:  VerifyStatus::Fail,
                            message: Some("not found in skill server".to_string()),
                        });
                        eprintln!(
                            "\n  {}: function '{}' declared in {}.briefskill but not found in skill server",
                            "error[E401]".red().bold(), func.name.cyan(), skill_name
                        );
                        skill_ok = false;
                        all_ok   = false;
                    }
                }
                if skill_ok {
                    println!("{} ({} function{})",
                        "ok".green().bold(), iface.funcs.len(),
                        if iface.funcs.len() == 1 { "" } else { "s" });
                }
            }
        }
    }

    if any_checked && all_ok {
        println!();
    }

    (all_ok, capabilities)
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

    // Deduplicate by (annotation, value) — dedup_by only removes consecutive
    // duplicates and the vec is unsorted, so use a HashSet for correctness.
    let mut seen_pairs: std::collections::HashSet<(String, String)> = std::collections::HashSet::new();
    out.retain(|(ann, val, _)| seen_pairs.insert((ann.clone(), val.clone())));
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
        Expr::Match  { scrutinee, arms   } => {
            collect_expr_performs(scrutinee, out);
            for arm in arms { collect_expr_performs(&arm.body, out); }
        }
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
