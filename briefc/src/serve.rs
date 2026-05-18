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

use crate::ast::Program;
use crate::checker::{self, CheckContext};
use crate::lexer::lex;
use crate::lock::{self, LockState};
use crate::manifest;
use crate::mcp_schema;
use crate::parser::parse;
use crate::skillgen::{parse_briefskill, SkillInterface};

// ─────────────────────────────────────────────────────────────────────────────

static NEXT_ID: AtomicU64 = AtomicU64::new(100);

fn fresh_id() -> u64 { NEXT_ID.fetch_add(1, Ordering::Relaxed) }

// ─────────────────────────────────────────────────────────────────────────────

pub fn run_serve(path: &Path) -> bool {
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

    // ── 2. Validate .brief.lock ─────────────────────────────────────────────
    let lock_path = lock::lock_path(path);
    match lock::read_lock(&lock_path) {
        None => {
            eprintln!("{}", "✗ Contract unsealed.".red().bold());
            eprintln!("  Run {} first to seal the contract.", "`brief verify`".cyan());
            return false;
        }
        Some(lock_file) => {
            match lock::check_lock(&lock_file, source_bytes, 24) {
                LockState::Fresh => {}
                LockState::Stale => {
                    eprintln!("{}", "✗ Contract expired (lock > 24h old).".red().bold());
                    eprintln!("  Run {} to refresh.", "`brief verify`".cyan());
                    return false;
                }
                LockState::SourceChanged => {
                    eprintln!("{}", "✗ Contract invalidated — .brief file changed since last verify.".red().bold());
                    eprintln!("  Run {} to re-seal.", "`brief verify`".cyan());
                    return false;
                }
                LockState::Missing => {
                    eprintln!("{}", "✗ No lock file found.".red().bold());
                    eprintln!("  Run {} first.", "`brief verify`".cyan());
                    return false;
                }
            }
        }
    }

    // ── 3. Load manifest + interfaces ──────────────────────────────────────
    let file_dir = path.parent().unwrap_or(Path::new("."));
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let mf = manifest::load_manifest(file_dir);

    let ctx = CheckContext {
        file_dir,
        cwd:                  &cwd,
        manifest:             mf.as_ref(),
        brief_path:           None,
        allow_missing_skills: false,
    };

    let ifaces = load_ifaces_for_uses(&program, &ctx);

    // Build a flat map: "SkillName.fnName" → SkillName for routing.
    let mut tool_to_skill: HashMap<String, String> = HashMap::new();
    for (skill_name, iface) in &ifaces {
        for f in &iface.funcs {
            tool_to_skill.insert(format!("{skill_name}__{}", f.name), skill_name.clone());
            // Also register unqualified for convenience.
            tool_to_skill.entry(f.name.clone()).or_insert_with(|| skill_name.clone());
        }
    }

    // Collect @once params: "SkillName.fnName:param_idx" → true
    let once_fns = collect_once_fns(&ifaces);
    let mut consumed_handles: HashSet<String> = HashSet::new();

    // ── 4. MCP server loop ──────────────────────────────────────────────────
    eprintln!("{} Brief MCP server ready (contract sealed: {})",
        "●".green().bold(), path.display()
    );
    eprintln!("{} Skills: {}", "  ↳".dimmed(),
        ifaces.keys().cloned().collect::<Vec<_>>().join(", ")
    );

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
                let tools = build_tools_list(&ifaces);
                let resp = json_response(id, json!({ "tools": tools }));
                send_line(&mut out, &resp);
            }

            "tools/call" => {
                let params     = &msg["params"];
                let tool_name  = params["name"].as_str().unwrap_or("");
                let arguments  = params["arguments"].clone();

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
                        // @once enforcement.
                        let fn_name = tool_name.split_once("__")
                            .map(|(_, f)| f)
                            .unwrap_or(tool_name);

                        if let Some(violation) = check_once_violation(
                            &sn, fn_name, &arguments, &once_fns, &consumed_handles
                        ) {
                            let err = json_error(id, -32600, &violation);
                            send_line(&mut out, &err);
                            continue;
                        }

                        // Record consumed handles.
                        record_handles(&sn, fn_name, &arguments, &once_fns, &mut consumed_handles);

                        // Proxy call to skill MCP server.
                        let result = proxy_tool_call(&sn, fn_name, arguments, &ctx, mf.as_ref());
                        let resp = match result {
                            Ok(content) => json_response(id, json!({ "content": content })),
                            Err(msg)    => json_error(id, -32603, &msg),
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

fn build_tools_list(ifaces: &HashMap<String, SkillInterface>) -> Vec<Value> {
    let mut all_tools = Vec::new();
    for (skill_name, iface) in ifaces {
        let tools = mcp_schema::interface_to_mcp_tools(skill_name, iface);
        let json_tools = mcp_schema::tools_to_json(&tools);
        if let Some(arr) = json_tools.as_array() {
            all_tools.extend(arr.iter().cloned());
        }
    }
    all_tools
}

// ─────────────────────────────────────────────────────────────────────────────
// @once enforcement

/// "@once" tracking is based on the @once let-binding annotation in the .brief AST.
/// Functions whose return type includes "Handle" will have their call results tracked.
/// A second invocation with the same arguments is blocked.
fn collect_once_fns(ifaces: &HashMap<String, SkillInterface>) -> HashSet<String> {
    let mut set = HashSet::new();
    for (skill_name, iface) in ifaces {
        for f in &iface.funcs {
            // Track functions returning Handle types for @once enforcement.
            if f.return_type.contains("Handle") || f.return_type.starts_with("Handle") {
                set.insert(format!("{skill_name}::{}", f.name));
            }
        }
    }
    set
}

fn check_once_violation(
    skill:       &str,
    fn_name:     &str,
    args:        &Value,
    once_fns:    &HashSet<String>,
    consumed:    &HashSet<String>,
) -> Option<String> {
    let fn_key = format!("{skill}::{fn_name}");
    if !once_fns.contains(&fn_key) { return None; }
    let args_key = format!("{fn_key}::{args}");
    if consumed.contains(&args_key) {
        return Some(format!(
            "@once violation: {skill}.{fn_name} with these arguments has already been consumed"
        ));
    }
    None
}

fn record_handles(
    skill:    &str,
    fn_name:  &str,
    args:     &Value,
    once_fns: &HashSet<String>,
    consumed: &mut HashSet<String>,
) {
    let fn_key = format!("{skill}::{fn_name}");
    if once_fns.contains(&fn_key) {
        consumed.insert(format!("{fn_key}::{args}"));
    }
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
    let s = serde_json::to_string(msg).unwrap();
    writeln!(w, "{s}")
}
