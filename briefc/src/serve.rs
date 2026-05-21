/// `brief serve` — MCP server with verified contract enforcement.
///
/// Startup:
///   1. Read + parse the .brief file
///   2. Validate `.brief.lock` — refuse if missing, stale, or source-changed
///   3. Load manifest + .briefskill interfaces for all skills in `uses[]`
///   4. Start a JSON-RPC MCP server on stdin/stdout
///
/// Protocol handling:
///   - `initialize`              → return capabilities
///   - `notifications/initialized` → ack (no-op)
///   - `tools/list`              → return all functions from `uses[]` skills
///   - `tools/call`              → proxy to skill MCP server; enforce `@once`
///
/// Guarantees:
///   - No logging or banner to stdout (corrupts JSON-RPC protocol)
///   - Logs go to stderr only
///   - @once params: second call with same handle value → rejected

use std::collections::{HashMap, HashSet};
use std::io::{self, BufRead, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};

use colored::Colorize;
use serde_json::{json, Value};

use crate::ast::{CallPattern, ForbidKind, NeedKind, Program};
use crate::checker::{self, CheckContext};
use crate::enforcer;
use crate::lexer::lex;
use crate::lock::{self, LockState};
use crate::manifest;
use crate::mcp_schema;
use crate::parser::parse;
use crate::skillgen::{parse_briefskill, SkillInterface};
use crate::verifier;

// ─────────────────────────────────────────────────────────────────────────────

static NEXT_ID: AtomicU64 = AtomicU64::new(100);

fn fresh_id() -> u64 { NEXT_ID.fetch_add(1, Ordering::Relaxed) }

// ─────────────────────────────────────────────────────────────────────────────

/// Trace recording privacy level.
#[derive(Clone, Copy, PartialEq)]
pub enum RecordMode {
    /// Record arg names + value types only (no actual values). Default.
    Schema,
    /// Record full arg values (opt-in; may contain sensitive data).
    Full,
}

impl RecordMode {
    pub fn from_str(s: &str) -> Self {
        if s.eq_ignore_ascii_case("full") { RecordMode::Full } else { RecordMode::Schema }
    }
}

pub fn run_serve(path: &Path, draft: bool, record: Option<&Path>, record_mode: RecordMode) -> bool {
    // ── 1. Read + parse source ──────────────────────────────────────────────
    let source = match std::fs::read_to_string(path) {
        Ok(s)  => s,
        Err(e) => {
            eprintln!("{}: cannot read {}: {e}", "error".red().bold(), path.display());
            return false;
        }
    };

    let source_bytes = source.as_bytes();

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

    // ── 2. Load manifest (needed for lock-age config) ───────────────────────
    let file_dir = path.parent().unwrap_or(Path::new("."));
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let mf = manifest::load_manifest(file_dir);
    let max_lock_age = mf.as_ref().map_or(24, |m| m.verify.max_lock_age_hours);

    // ── 3. Validate .brief.lock (skipped in draft mode) ────────────────────
    let lock_path = lock::lock_path(path);
    if draft {
        // Draft mode: skip lock gate. Warn on stderr (safe — not part of MCP JSON-RPC).
        eprintln!("{} {} brief serve --draft",
            "⚠".yellow().bold(),
            "Draft mode — dynamic annotations not verified. Run `brief verify` to seal the contract."
                .yellow()
        );
        eprintln!("{} scope enforcement (uses[], forbids{{}}, needs{{}}) is still active.",
            "  ↳".dimmed()
        );
    } else {
        match lock::read_lock(&lock_path) {
        None => {
            eprintln!("{}", "✗ Contract unsealed.".red().bold());
            eprintln!("  Run {} first to seal the contract.", "`brief verify`".cyan());
            return false;
        }
        Some(lock_file) => {
            match lock::check_lock(&lock_file, source_bytes, max_lock_age) {
                LockState::Fresh => {}
                LockState::Stale => {
                    let age_display = if max_lock_age == 0 {
                        "never expires — treating as fresh".to_string()
                    } else {
                        format!("lock > {}h old", max_lock_age)
                    };
                    // max_lock_age == 0 means "never expire"; check_lock returns Fresh for that.
                    // This arm is only reached for genuinely stale locks.
                    eprintln!("{}", format!("✗ Contract expired ({age_display}).").red().bold());
                    eprintln!("  Run {} to refresh.", "`brief verify`".cyan());
                    return false;
                }
                LockState::SourceChanged => {
                    eprintln!("{}", "✗ Contract invalidated — .brief file changed since last verify.".red().bold());
                    eprintln!("  Run {} to re-seal.", "`brief verify`".cyan());
                    return false;
                }
            }
            }
        }
    } // end lock check

    // ── 4. Load interfaces ──────────────────────────────────────────────────

    let ctx = CheckContext {
        file_dir,
        cwd:                  &cwd,
        manifest:             mf.as_ref(),
        brief_path:           None,
        allow_missing_skills: false,
    };

    let ifaces = load_ifaces_for_uses(&program, &ctx);

    // ── 4b. Collect forbidden skills/funcs from all tasks ──────────────────
    let (forbidden_skills, forbidden_funcs) = collect_forbidden(&program);

    // ── 4c. Collect allow/deny patterns from all tasks ─────────────────────
    let (task_allow, task_deny) = collect_allow_deny(&program);

    // ── 4c. Re-check env needs at startup ──────────────────────────────────
    // Env vars may differ between `brief verify` time and `brief serve` time.
    if !check_env_needs_at_startup(&program) {
        return false;
    }

    // Build a flat map: tool_name → SkillName for routing.
    // Registers both MCP dot-format ("SkillName.fn") and legacy double-underscore ("SkillName__fn").
    let mut tool_to_skill: HashMap<String, String> = HashMap::new();
    for (skill_name, iface) in &ifaces {
        for f in &iface.funcs {
            // Primary: dot format — matches what mcp_schema produces for tools/list names.
            tool_to_skill.insert(format!("{skill_name}.{}", f.name), skill_name.clone());
            // Legacy: double-underscore format for backward compatibility.
            tool_to_skill.insert(format!("{skill_name}__{}", f.name), skill_name.clone());
            // Unqualified fallback (last-writer wins on collision).
            tool_to_skill.entry(f.name.clone()).or_insert_with(|| skill_name.clone());
        }
    }

    // Collect @once functions from two sources:
    // 1. .briefskill return type annotations (`-> @once ReturnType`)
    // 2. @once let bindings in the parsed .brief program
    let once_fns = collect_once_fns(&ifaces, &program);
    let mut consumed_handles: HashSet<String> = HashSet::new();

    // ── 5. Open trace writer (if --record) ─────────────────────────────────
    let mut trace_writer: Option<std::io::BufWriter<std::fs::File>> = None;
    if let Some(trace_path) = record {
        match std::fs::OpenOptions::new().create(true).append(true).open(trace_path) {
            Ok(f) => {
                let mut w = std::io::BufWriter::new(f);
                // Session header line.
                let task_name = program.tasks.first().map(|t| t.name.as_str()).unwrap_or("unknown");
                let policy_hash = compute_policy_hash(&program);
                let header = json!({
                    "event": "session_start",
                    "task": task_name,
                    "brief_version": env!("CARGO_PKG_VERSION"),
                    "policy_hash": policy_hash,
                    "ts": chrono_now(),
                });
                let _ = writeln!(w, "{}", header);
                let _ = w.flush();
                if record_mode == RecordMode::Full {
                    eprintln!("{} {} Tracing to {} (full args — may contain sensitive data)",
                        "⚠".yellow().bold(), "record-args=full:".yellow(), trace_path.display());
                } else {
                    eprintln!("{} Tracing to {} (arg schema only)",
                        "●".dimmed(), trace_path.display());
                }
                trace_writer = Some(w);
            }
            Err(e) => {
                eprintln!("{}: cannot open trace file {}: {e}",
                    "error".red().bold(), trace_path.display());
                return false;
            }
        }
    }

    // ── 5. MCP server loop ──────────────────────────────────────────────────
    if draft {
        eprintln!("{} Brief MCP server ready {} ({})",
            "●".yellow().bold(),
            "[DRAFT — unverified]".yellow(),
            path.display()
        );
    } else {
        eprintln!("{} Brief MCP server ready (contract sealed: {})",
            "●".green().bold(), path.display()
        );
    }
    eprintln!("{} Skills: {}", "  ↳".dimmed(),
        ifaces.keys().cloned().collect::<Vec<_>>().join(", ")
    );
    if !forbidden_skills.is_empty() || !forbidden_funcs.is_empty() {
        eprintln!("{} Forbidden: {} skill(s), {} func(s)",
            "  ↳".dimmed(), forbidden_skills.len(), forbidden_funcs.len()
        );
    }

    let stdin  = io::stdin();
    let stdout = io::stdout();
    let mut out = BufWriter::new(stdout.lock());

    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l)  => l,
            Err(_) => break,
        };
        let line = line.trim();
        if line.is_empty() { continue; }

        let msg: Value = match serde_json::from_str(line) {
            Ok(v)  => v,
            Err(e) => {
                eprintln!("[brief serve] invalid JSON: {e}: {line}");
                continue;
            }
        };

        let id     = msg.get("id").cloned();
        let method = msg["method"].as_str().unwrap_or("");

        match method {
            "initialize" => {
                let resp = json_response(id, json!({
                    "protocolVersion": "2024-11-05",
                    "capabilities": { "tools": {} },
                    "serverInfo": {
                        "name":    "brief",
                        "version": env!("CARGO_PKG_VERSION")
                    }
                }));
                send_line(&mut out, &resp);
            }

            "notifications/initialized" => {
                // Notification — no response.
            }

            "tools/list" => {
                let tools = build_tools_list(&ifaces, &forbidden_skills, &forbidden_funcs);
                let resp = json_response(id, json!({ "tools": tools }));
                send_line(&mut out, &resp);
            }

            "tools/call" => {
                let params     = &msg["params"];
                let tool_name  = params["name"].as_str().unwrap_or("");
                let arguments  = params["arguments"].clone();

                // Enforce forbids before routing.
                if is_tool_forbidden(tool_name, &forbidden_skills, &forbidden_funcs) {
                    let err = json_error(id, -32603,
                        &format!("Tool '{tool_name}' is explicitly forbidden by the task contract"));
                    send_line(&mut out, &err);
                    continue;
                }

                // Resolve skill from tool name.
                let skill_name = tool_to_skill.get(tool_name)
                    .or_else(|| {
                        // Try "SkillName__fnName" → SkillName
                        tool_name.split_once("__").and_then(|(s, _)| tool_to_skill.get(s))
                    });

                match skill_name {
                    None => {
                        let err = json_error(id, -32601,
                            &format!("Unknown tool '{tool_name}' — not in uses[]"));
                        send_line(&mut out, &err);
                    }
                    Some(sn) => {
                        let sn = sn.clone();
                        // Extract bare function name from either "Skill.fn" or "Skill__fn".
                        let fn_name = tool_name.split_once("__")
                            .or_else(|| tool_name.split_once('.'))
                            .map(|(_, f)| f)
                            .unwrap_or(tool_name);

                        let call_start = std::time::Instant::now();

                        // ── allow{}/deny{} enforcement (E422) ──────────────
                        let decision = enforcer::check_call(
                            &sn, fn_name, &arguments, &task_allow, &task_deny,
                        );
                        if let crate::enforcer::CallDecision::Blocked { ref reason } = decision {
                            let err = json_error(id, -32022, &format!(
                                "Brief policy blocked call: {sn}.{fn_name} — {reason}",
                            ));
                            // Emit structured data for agent consumption.
                            let mut err_obj = err.clone();
                            if let Some(e) = err_obj["error"].as_object_mut() {
                                e.insert("data".to_string(), json!({
                                    "brief_code": "E422",
                                    "skill": &sn,
                                    "function": fn_name,
                                    "reason": reason,
                                }));
                            }
                            if let Some(tw) = trace_writer.as_mut() {
                                write_call_trace(tw, &sn, fn_name, &arguments,
                                    false, Some(reason), 0, None, false,
                                    record_mode, &[]);
                            }
                            send_line(&mut out, &err_obj);
                            eprintln!("{} E422 blocked: {sn}.{fn_name} — {reason}",
                                "✗".red().bold());
                            continue;
                        }

                        // Proxy call to skill MCP server.
                        let result = proxy_tool_call(&sn, fn_name, arguments.clone(), &ctx, mf.as_ref());
                        let elapsed = call_start.elapsed().as_millis() as u64;
                        let resp = match result {
                            Ok(content) => {
                                if let Some(tw) = trace_writer.as_mut() {
                                    let result_size = serde_json::to_string(&content)
                                        .map(|s| s.len()).unwrap_or(0);
                                    write_call_trace(tw, &sn, fn_name, &arguments,
                                        true, None, elapsed, Some(result_size), false,
                                        record_mode, &[]);
                                }
                                // @once enforcement: check the response content hash.
                                // We hash the RESPONSE (not the call args) so that two
                                // legitimate calls with the same args (producing different
                                // handles/tokens) are both allowed, while a duplicate
                                // handle being produced a second time is caught.
                                if once_fns.contains(&format!("{sn}::{fn_name}")) {
                                    let hash = lock::sha256_file_hash(
                                        serde_json::to_string(&content)
                                            .unwrap_or_default()
                                            .as_bytes(),
                                    );
                                    if consumed_handles.contains(&hash) {
                                        let err = json_error(id, -32600,
                                            &format!("@once violation: {sn}.{fn_name} returned a handle that has already been consumed"));
                                        send_line(&mut out, &err);
                                        continue;
                                    }
                                    consumed_handles.insert(hash);
                                }
                                json_response(id, json!({ "content": content }))
                            }
                            Err(msg) => {
                                if let Some(tw) = trace_writer.as_mut() {
                                    write_call_trace(tw, &sn, fn_name, &arguments,
                                        true, None, elapsed, Some(0), true,
                                        record_mode, &[]);
                                }
                                json_error(id, -32603, &msg)
                            },
                        };
                        send_line(&mut out, &resp);
                    }
                }
            }

            "ping" => {
                let resp = json_response(id, json!({}));
                send_line(&mut out, &resp);
            }

            _ => {
                if id.is_some() {
                    // Only respond to requests, not notifications.
                    let err = json_error(id, -32601, &format!("Method not found: {method}"));
                    send_line(&mut out, &err);
                }
            }
        }

        let _ = out.flush();
    }

    true
}

// ─────────────────────────────────────────────────────────────────────────────
// Tool list generation

fn build_tools_list(
    ifaces: &HashMap<String, SkillInterface>,
    forbidden_skills: &HashSet<String>,
    forbidden_funcs:  &HashSet<String>,
) -> Vec<Value> {
    let mut all_tools = Vec::new();
    for (skill_name, iface) in ifaces {
        // Skip entire skill if it's forbidden.
        if forbidden_skills.contains(skill_name) { continue; }
        let tools = mcp_schema::interface_to_mcp_tools(skill_name, iface);
        let json_tools = mcp_schema::tools_to_json(&tools);
        if let Some(arr) = json_tools.as_array() {
            for tool in arr {
                // Skip individual forbidden functions.
                let tool_name = tool["name"].as_str().unwrap_or("");
                if !is_tool_forbidden(tool_name, forbidden_skills, forbidden_funcs) {
                    all_tools.push(tool.clone());
                }
            }
        }
    }
    all_tools
}

// ─────────────────────────────────────────────────────────────────────────────
// @once enforcement

/// Collect the set of "SkillName::fnName" keys that have `@once` semantics.
///
/// Two sources are consulted:
/// 1. `.briefskill` return type annotations — `fn charge() -> @once PaymentHandle`
/// 2. `@once let handle = perform Skill.fn(...)` bindings in the parsed `.brief` program
fn collect_once_fns(
    ifaces:  &HashMap<String, SkillInterface>,
    program: &Program,
) -> HashSet<String> {
    let mut set = HashSet::new();

    // Source 1: .briefskill return type — starts with "@once " prefix.
    // Also keep the legacy "Handle" heuristic for briefskill files generated
    // before explicit @once return-type support was added.
    for (skill_name, iface) in ifaces {
        for f in &iface.funcs {
            let rt = f.return_type.trim();
            if rt.starts_with("@once ") || rt.contains("Handle") {
                set.insert(format!("{skill_name}::{}", f.name));
            }
        }
    }

    // Source 2: `@once let handle = perform Skill.fn(...)` in task bodies.
    use crate::ast::{Stmt, Expr};
    for task in &program.tasks {
        for step in &task.steps {
            for stmt in &step.body {
                if let Stmt::Let { attrs, value: Expr::Perform { skill, func, .. }, .. } = stmt {
                    if attrs.iter().any(|a| a == "once") {
                        set.insert(format!("{skill}::{func}"));
                    }
                }
            }
        }
    }

    set
}

// ─────────────────────────────────────────────────────────────────────────────
// Skill proxy

fn proxy_tool_call(
    skill_name: &str,
    fn_name:    &str,
    arguments:  Value,
    _ctx:       &CheckContext<'_>,
    mf:         Option<&manifest::BriefManifest>,
) -> Result<Value, String> {
    call_skill_tool(skill_name, fn_name, arguments, mf)
}

/// Public API: call a skill tool by name, routing via brief.toml config.
/// Returns the MCP result content on success, or an error string.
pub fn call_skill_tool(
    skill_name: &str,
    fn_name:    &str,
    arguments:  Value,
    mf:         Option<&manifest::BriefManifest>,
) -> Result<Value, String> {
    let config = mf
        .and_then(|m| m.skills.get(skill_name))
        .map(|e| e.as_config());

    let cfg = match config {
        Some(c) => c,
        None => return Err(format!("no skill config found for '{skill_name}' in brief.toml")),
    };

    if let Some(cmd) = &cfg.mcp_command {
        call_skill_mcp_process(cmd, fn_name, arguments)
    } else if let Some(url) = &cfg.mcp_url {
        call_skill_mcp_http(url, fn_name, arguments)
    } else {
        Err(format!("skill '{skill_name}' has no mcp_command or mcp_url in brief.toml"))
    }
}

fn call_skill_mcp_process(
    cmd:       &[String],
    tool_name: &str,
    arguments: Value,
) -> Result<Value, String> {
    if cmd.is_empty() {
        return Err("mcp_command is empty".into());
    }

    let mut child = Command::new(&cmd[0])
        .args(&cmd[1..])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|e| format!("spawn '{}' failed: {e}", cmd[0]))?;

    let result = {
        let stdin  = child.stdin.take().unwrap();
        let stdout = child.stdout.take().unwrap();
        let reader = std::io::BufReader::new(stdout);
        run_skill_mcp_session(stdin, reader, tool_name, arguments)
    };

    // The session has fully completed (stdin dropped, response read).
    // Give the process a moment to exit cleanly before force-killing.
    let exited = child.try_wait().map_or(false, |s| s.is_some());
    if !exited {
        let _ = child.kill();
        let _ = child.wait();
    }
    result
}

fn run_skill_mcp_session<W: Write, R: BufRead>(
    mut stdin:  W,
    mut reader: R,
    tool_name:  &str,
    arguments:  Value,
) -> Result<Value, String> {
    let init = json!({
        "jsonrpc": "2.0", "id": fresh_id(), "method": "initialize",
        "params": {
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": { "name": "brief", "version": env!("CARGO_PKG_VERSION") }
        }
    });
    write_jsonrpc(&mut stdin, &init).map_err(|e| e.to_string())?;

    let mut buf = String::new();
    reader.read_line(&mut buf).map_err(|e| e.to_string())?;

    let notif = json!({ "jsonrpc": "2.0", "method": "notifications/initialized" });
    write_jsonrpc(&mut stdin, &notif).map_err(|e| e.to_string())?;

    let call = json!({
        "jsonrpc": "2.0", "id": fresh_id(), "method": "tools/call",
        "params": { "name": tool_name, "arguments": arguments }
    });
    write_jsonrpc(&mut stdin, &call).map_err(|e| e.to_string())?;
    drop(stdin);

    buf.clear();
    reader.read_line(&mut buf).map_err(|e| e.to_string())?;

    let v: Value = serde_json::from_str(buf.trim())
        .map_err(|e| format!("invalid JSON from skill: {e}"))?;

    if let Some(err) = v.get("error") {
        let msg = err["message"].as_str().unwrap_or("skill error");
        return Err(msg.to_string());
    }

    Ok(v["result"]["content"].clone())
}

fn call_skill_mcp_http(base_url: &str, tool_name: &str, arguments: Value) -> Result<Value, String> {
    let req = json!({
        "jsonrpc": "2.0", "id": fresh_id(), "method": "tools/call",
        "params": { "name": tool_name, "arguments": arguments }
    });

    let resp = ureq::builder()
        .timeout(std::time::Duration::from_secs(60))
        .build()
        .post(base_url)
        .set("Content-Type", "application/json")
        .send_json(&req)
        .map_err(|e| e.to_string())?;

    use std::io::Read;
    let mut body = String::new();
    let _ = resp.into_reader().take(1024 * 1024).read_to_string(&mut body);

    let v: Value = serde_json::from_str(&body)
        .map_err(|e| format!("invalid JSON from skill: {e}"))?;

    if let Some(err) = v.get("error") {
        let msg = err["message"].as_str().unwrap_or("skill error");
        return Err(msg.to_string());
    }

    Ok(v["result"]["content"].clone())
}

// ─────────────────────────────────────────────────────────────────────────────
// Helpers

fn load_ifaces_for_uses(
    program: &Program,
    ctx:     &CheckContext<'_>,
) -> HashMap<String, SkillInterface> {
    let mut map = HashMap::new();
    for import in &program.imports {
        if let Some(path) = checker::find_skill_interface(&import.name, ctx) {
            if let Ok(content) = std::fs::read_to_string(&path) {
                if let Some(iface) = parse_briefskill(&content) {
                    map.insert(import.name.clone(), iface);
                }
            }
        }
    }
    map
}

fn json_response(id: Option<Value>, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

fn json_error(id: Option<Value>, code: i64, message: &str) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": { "code": code, "message": message }
    })
}

fn send_line(out: &mut impl Write, msg: &Value) {
    let s = serde_json::to_string(msg).unwrap_or_else(|_| "{}".into());
    // Write each JSON-RPC message as a single newline-terminated line.
    let _ = writeln!(out, "{s}");
}

fn write_jsonrpc(w: &mut impl Write, msg: &Value) -> std::io::Result<()> {
    let s = serde_json::to_string(msg).unwrap_or_else(|_| "{}".to_string());
    writeln!(w, "{s}")
}

// ─────────────────────────────────────────────────────────────────────────────
// Forbidden tool helpers

/// Collect allow/deny call patterns from all tasks (union).
///
/// Semantics at runtime:
/// - Any task's deny blocks the call (union deny is more restrictive — safe).
/// - Whitelist mode is active if any task in the file has a non-empty allow list.
///   In whitelist mode, the call must match at least one allow pattern.
fn collect_allow_deny(program: &Program) -> (Vec<CallPattern>, Vec<CallPattern>) {
    let mut allow: Vec<CallPattern> = Vec::new();
    let mut deny: Vec<CallPattern> = Vec::new();
    for task in &program.tasks {
        allow.extend_from_slice(&task.allow);
        deny.extend_from_slice(&task.deny);
    }
    (allow, deny)
}

/// Collect forbidden skill names and func tool names from all tasks.
fn collect_forbidden(program: &Program) -> (HashSet<String>, HashSet<String>) {
    let mut skills: HashSet<String> = HashSet::new();
    let mut funcs:  HashSet<String> = HashSet::new();
    for task in &program.tasks {
        for item in &task.forbids {
            match item.kind {
                ForbidKind::Skill => { skills.insert(item.name.clone()); }
                ForbidKind::Func  => {
                    // "Payment.refund" → "Payment__refund" (MCP tool name format)
                    let mcp_name = item.name.replacen('.', "__", 1);
                    funcs.insert(mcp_name);
                    // Also store the dot form for tools/call that use dot format.
                    funcs.insert(item.name.clone());
                }
            }
        }
    }
    (skills, funcs)
}

/// Returns `true` if `tool_name` is forbidden by the contract.
///
/// Tool names may arrive as `"Payment__refund"` or `"Payment.refund"`.
fn is_tool_forbidden(
    tool_name:        &str,
    forbidden_skills: &HashSet<String>,
    forbidden_funcs:  &HashSet<String>,
) -> bool {
    // Check if the whole skill is forbidden (tool_name starts with "Skill__")
    if let Some((skill, _)) = tool_name.split_once("__") {
        if forbidden_skills.contains(skill) { return true; }
    }
    // Check by dot-qualified name (e.g. from tools/call "name" field).
    if let Some((skill, _)) = tool_name.split_once('.') {
        if forbidden_skills.contains(skill) { return true; }
    }
    // Check specific func prohibition.
    forbidden_funcs.contains(tool_name)
}

/// Re-check `needs { env "VAR" }` at `brief serve` startup.
///
/// Env vars may have changed since `brief verify` was run.
/// Returns `true` if all env prerequisites are met, `false` (+ error message) otherwise.
fn check_env_needs_at_startup(program: &Program) -> bool {
    let mut all_ok = true;
    for task in &program.tasks {
        for need in &task.needs {
            if need.kind != NeedKind::Env { continue; }
            let result = verifier::builtin_env(&need.key);
            if !result.is_ok() {
                eprintln!("{}: needs {{ env \"{}\" }} — {}",
                    "error".red().bold(), need.key,
                    result.message.as_deref().unwrap_or("not set")
                );
                all_ok = false;
            }
        }
    }
    if !all_ok {
        eprintln!();
        eprintln!("{} Prerequisites not met. Set the required env vars and run `brief serve` again.",
            "✗".red().bold());
    }
    all_ok
}

// ─────────────────────────────────────────────────────────────────────────────
// Trace recording helpers

/// Current time as ISO 8601 string (UTC, second precision).
fn chrono_now() -> String {
    // Use std::time since we avoid chrono dep; format as RFC 3339-compatible.
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    // Approximate ISO date from Unix timestamp (no leap-second handling needed).
    let (y, mo, d, h, mi, s) = unix_to_ymdhms(secs);
    format!("{y:04}-{mo:02}-{d:02}T{h:02}:{mi:02}:{s:02}Z")
}

fn unix_to_ymdhms(ts: u64) -> (u64, u64, u64, u64, u64, u64) {
    let s = ts % 60;
    let mi = (ts / 60) % 60;
    let h = (ts / 3600) % 24;
    let days = ts / 86400;
    // Days since 1970-01-01.
    let mut y = 1970u64;
    let mut rem = days;
    loop {
        let ydays = if is_leap(y) { 366 } else { 365 };
        if rem < ydays { break; }
        rem -= ydays;
        y += 1;
    }
    let mdays = [31, if is_leap(y) { 29 } else { 28 }, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let mut mo = 0usize;
    while mo < 12 && rem >= mdays[mo] { rem -= mdays[mo]; mo += 1; }
    (y, mo as u64 + 1, rem + 1, h, mi, s)
}

fn is_leap(y: u64) -> bool { y % 4 == 0 && (y % 100 != 0 || y % 400 == 0) }

/// Compute a short policy hash (first 12 hex chars of SHA-256) from the task allow/deny/uses/forbids.
fn compute_policy_hash(program: &crate::ast::Program) -> String {
    let mut data = String::new();
    for task in &program.tasks {
        data.push_str(&task.name);
        for u in &task.uses { data.push_str(u); }
        for f in &task.forbids { data.push_str(&f.name); }
        for p in &task.allow {
            data.push_str(&format!("allow:{}.{:?}", p.skill, p.func));
        }
        for p in &task.deny {
            data.push_str(&format!("deny:{}.{:?}", p.skill, p.func));
        }
    }
    let hash_bytes = lock::sha256_file_hash(data.as_bytes());
    format!("sha256:{}", &hash_bytes[..12])
}

/// Sensitive argument key names — values are redacted in schema mode.
const SENSITIVE_TRACE_KEYS: &[&str] = &[
    "token", "secret", "password", "authorization", "cookie", "key",
    "api_key", "auth", "credential", "private",
];

/// Schematize args: replace values with their JSON type name.
/// Sensitive keys are always redacted.
fn schematize_args(args: &Value, sensitive_env_values: &[String]) -> Value {
    match args {
        Value::Object(map) => {
            let mut out = serde_json::Map::new();
            for (k, v) in map {
                let lower = k.to_lowercase();
                if SENSITIVE_TRACE_KEYS.iter().any(|s| lower.contains(s)) {
                    out.insert(k.clone(), Value::String("[REDACTED]".into()));
                } else if let Value::String(s) = v {
                    // Check if value matches a known secret env var value.
                    if sensitive_env_values.iter().any(|env_val| env_val == s) {
                        out.insert(k.clone(), Value::String("[REDACTED:secret]".into()));
                    } else {
                        out.insert(k.clone(), Value::String("string".into()));
                    }
                } else {
                    out.insert(k.clone(), Value::String(json_type_name(v).into()));
                }
            }
            Value::Object(out)
        }
        _ => Value::String("object".into()),
    }
}

/// Redact sensitive keys from full args for --record-args=full mode.
fn redact_full_args(args: &Value, sensitive_env_values: &[String]) -> Value {
    match args {
        Value::Object(map) => {
            let mut out = serde_json::Map::new();
            for (k, v) in map {
                let lower = k.to_lowercase();
                if SENSITIVE_TRACE_KEYS.iter().any(|s| lower.contains(s)) {
                    out.insert(k.clone(), Value::String("[REDACTED]".into()));
                } else if let Value::String(s) = v {
                    if sensitive_env_values.iter().any(|env_val| env_val == s) {
                        out.insert(k.clone(), Value::String("[REDACTED:secret]".into()));
                    } else if s.len() > 512 {
                        let hash = &lock::sha256_file_hash(s.as_bytes())[..8];
                        out.insert(k.clone(), Value::String(format!("[TRUNCATED:{}:{}]", s.len(), hash)));
                    } else {
                        out.insert(k.clone(), v.clone());
                    }
                } else {
                    out.insert(k.clone(), v.clone());
                }
            }
            Value::Object(out)
        }
        _ => args.clone(),
    }
}

fn json_type_name(v: &Value) -> &'static str {
    match v {
        Value::String(_) => "string",
        Value::Number(_) => "number",
        Value::Bool(_) => "bool",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
        Value::Null => "null",
    }
}

/// Write one JSONL call trace entry.
fn write_call_trace(
    writer: &mut std::io::BufWriter<std::fs::File>,
    skill: &str,
    func: &str,
    args: &Value,
    allowed: bool,
    blocked_reason: Option<&str>,
    elapsed_ms: u64,
    result_size: Option<usize>,
    result_error: bool,
    record_mode: RecordMode,
    sensitive_env_values: &[String],
) {
    let args_recorded = if record_mode == RecordMode::Full {
        redact_full_args(args, sensitive_env_values)
    } else {
        schematize_args(args, sensitive_env_values)
    };

    let mut entry = json!({
        "event": "call",
        "ts": chrono_now(),
        "skill": skill,
        "fn": func,
        "args": args_recorded,
        "allowed": allowed,
        "ms": elapsed_ms,
    });

    if let Some(reason) = blocked_reason {
        entry["blocked_reason"] = Value::String(reason.to_string());
    }
    if let Some(sz) = result_size {
        entry["result_size"] = json!(sz);
        entry["result_error"] = json!(result_error);
    }

    let _ = writeln!(writer, "{}", entry);
    let _ = writer.flush();
}
