/// Verifier protocol client for `brief verify`.
///
/// Routes each `(annotation, value)` pair to the configured verifier:
/// - `builtin:url`  — inline HTTP HEAD/GET check (no subprocess)
/// - `mcp_command`  — spawn process, MCP JSON-RPC over stdin/stdout
/// - `mcp_url`      — HTTP POST MCP JSON-RPC
///
/// Protocol: minimal MCP subset (initialize → tools/call).

use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};
use std::time::Duration;

use colored::Colorize;
use serde_json::{json, Value};

use crate::lock::VerifyStatus;
use crate::manifest::VerifierConfig;

// ─────────────────────────────────────────────────────────────────────────────

/// Result returned from a verifier call.
#[derive(Debug, Clone)]
pub struct VerificationResult {
    pub status:  VerifyStatus,
    pub message: Option<String>,
}

impl VerificationResult {
    pub fn ok()            -> Self { Self { status: VerifyStatus::Ok,   message: None } }
    pub fn fail(msg: &str) -> Self { Self { status: VerifyStatus::Fail, message: Some(msg.to_string()) } }
    pub fn is_ok(&self)    -> bool { matches!(self.status, VerifyStatus::Ok) }
}

// ─────────────────────────────────────────────────────────────────────────────

/// Route a verification call to the correct backend.
///
/// - `config`     — verifier config from `brief.toml [verifiers.*]`
/// - `annotation` — annotation name, e.g. `"@url"` or `"figmaURL"`
/// - `value`      — the value to verify, e.g. `"https://api.example.com/health"`
/// - `context`    — extra context passed to the verifier (perform call info)
pub fn dispatch(
    config:     &VerifierConfig,
    annotation: &str,
    value:      &str,
    context:    Value,
) -> VerificationResult {
    if let Some(builtin) = &config.skill {
        return match builtin.as_str() {
            "builtin:url" => builtin_url(value),
            other => {
                eprintln!(
                    "  {} unknown builtin verifier '{}' for @{}",
                    "⚠".yellow().bold(), other, annotation
                );
                VerificationResult::fail(&format!("unknown builtin: {other}"))
            }
        };
    }
    if let Some(cmd) = &config.mcp_command {
        return call_mcp_process(cmd, annotation, value, context);
    }
    if let Some(url) = &config.mcp_url {
        return call_mcp_http(url, annotation, value, context);
    }
    VerificationResult::fail("verifier config has no skill, mcp_command, or mcp_url")
}

// ─────────────────────────────────────────────────────────────────────────────
// Built-in: @url

fn builtin_url(url: &str) -> VerificationResult {
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return VerificationResult::fail("only http/https URLs are supported");
    }

    // Try HEAD first; fall back to GET if HEAD is not allowed.
    let agent = ureq::builder()
        .timeout(Duration::from_secs(10))
        .redirects(3)
        .build();

    let head_result = agent.head(url).call();

    match head_result {
        Ok(r) if r.status() < 400 => VerificationResult::ok(),
        Err(ureq::Error::Status(405, _)) | Ok(_) => {
            // Either 405 (HEAD not allowed) or a non-2xx HEAD — try GET.
            match agent.get(url).call() {
                Ok(r) if r.status() < 400 => VerificationResult::ok(),
                Ok(r)  => VerificationResult::fail(&format!("HTTP {}", r.status())),
                Err(e) => VerificationResult::fail(&e.to_string()),
            }
        }
        Err(e) => VerificationResult::fail(&e.to_string()),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// MCP subprocess

fn call_mcp_process(
    cmd:        &[String],
    annotation: &str,
    value:      &str,
    context:    Value,
) -> VerificationResult {
    if cmd.is_empty() {
        return VerificationResult::fail("mcp_command is empty");
    }

    let mut child = match Command::new(&cmd[0])
        .args(&cmd[1..])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
    {
        Ok(c)  => c,
        Err(e) => return VerificationResult::fail(&format!("spawn '{}' failed: {e}", cmd[0])),
    };

    let result = {
        let stdin  = child.stdin.take().unwrap();
        let stdout = child.stdout.take().unwrap();
        let reader = BufReader::new(stdout);
        run_mcp_verify_session(stdin, reader, annotation, value, context)
    };

    // Give the process a moment to exit cleanly before force-killing.
    let exited = child.try_wait().map_or(false, |s| s.is_some());
    if !exited {
        let _ = child.kill();
        let _ = child.wait();
    }
    result
}

/// Drive an MCP session: initialize → tools/call → return result.
fn run_mcp_verify_session<W: Write, R: BufRead>(
    mut stdin:  W,
    mut reader: R,
    annotation: &str,
    value:      &str,
    context:    Value,
) -> VerificationResult {
    // 1. Send initialize request.
    let init_req = json!({
        "jsonrpc": "2.0",
        "id":      1,
        "method":  "initialize",
        "params": {
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": { "name": "brief", "version": env!("CARGO_PKG_VERSION") }
        }
    });
    if let Err(e) = send_jsonrpc(&mut stdin, &init_req) {
        return VerificationResult::fail(&format!("write error: {e}"));
    }

    // 2. Consume initialize response.
    let mut buf = String::new();
    if reader.read_line(&mut buf).is_err() {
        return VerificationResult::fail("failed to read initialize response");
    }

    // 3. Send initialized notification.
    let notif = json!({ "jsonrpc": "2.0", "method": "notifications/initialized" });
    if let Err(e) = send_jsonrpc(&mut stdin, &notif) {
        return VerificationResult::fail(&format!("write error: {e}"));
    }

    // 4. Send tools/call for "verify".
    let call_req = json!({
        "jsonrpc": "2.0",
        "id":      2,
        "method":  "tools/call",
        "params": {
            "name": "verify",
            "arguments": {
                "annotation": annotation,
                "value":      value,
                "context":    context
            }
        }
    });
    if let Err(e) = send_jsonrpc(&mut stdin, &call_req) {
        return VerificationResult::fail(&format!("write error: {e}"));
    }
    drop(stdin); // Signal EOF.

    // 5. Read tools/call response.
    buf.clear();
    if reader.read_line(&mut buf).is_err() {
        return VerificationResult::fail("failed to read tools/call response");
    }

    parse_mcp_verify_response(&buf)
}

fn send_jsonrpc(w: &mut impl Write, msg: &Value) -> std::io::Result<()> {
    let s = serde_json::to_string(msg).unwrap();
    writeln!(w, "{s}")
}

fn parse_mcp_verify_response(line: &str) -> VerificationResult {
    let v: Value = match serde_json::from_str(line.trim()) {
        Ok(v)  => v,
        Err(e) => return VerificationResult::fail(&format!("invalid JSON: {e}")),
    };

    if let Some(err) = v.get("error") {
        let msg = err["message"].as_str().unwrap_or("MCP error");
        return VerificationResult::fail(msg);
    }

    // Extract text content from the tools/call result array.
    let text = v["result"]["content"]
        .as_array()
        .and_then(|arr| arr.iter().find(|c| c["type"] == "text"))
        .and_then(|c| c["text"].as_str())
        .unwrap_or("");

    // The verifier should return JSON: `{"status":"ok"}` or `{"status":"fail","message":"..."}`.
    if let Ok(result) = serde_json::from_str::<Value>(text) {
        let status = result["status"].as_str().unwrap_or("fail");
        let message = result["message"].as_str().map(|s| s.to_string());
        return VerificationResult {
            status:  if status == "ok" { VerifyStatus::Ok } else { VerifyStatus::Fail },
            message,
        };
    }

    // Fallback: treat as plain-text status.
    if text.contains("ok") || text.contains("success") {
        VerificationResult::ok()
    } else {
        VerificationResult::fail(if text.is_empty() { "empty response" } else { text })
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// MCP HTTP

fn call_mcp_http(base_url: &str, annotation: &str, value: &str, context: Value) -> VerificationResult {
    let call_req = json!({
        "jsonrpc": "2.0",
        "id":      1,
        "method":  "tools/call",
        "params": {
            "name": "verify",
            "arguments": { "annotation": annotation, "value": value, "context": context }
        }
    });

    let result = ureq::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .post(base_url)
        .set("Content-Type", "application/json")
        .send_json(&call_req);

    match result {
        Ok(resp) => {
            use std::io::Read;
            let mut body = String::new();
            let _ = resp.into_reader().take(64 * 1024).read_to_string(&mut body);
            parse_mcp_verify_response(&body)
        }
        Err(e) => VerificationResult::fail(&e.to_string()),
    }
}
