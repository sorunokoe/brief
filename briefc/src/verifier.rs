/// Verifier protocol client for `brief verify`.
///
/// Routes each `(annotation, value)` pair to the configured verifier:
/// - `builtin:url`  — inline HTTP HEAD/GET check (no subprocess)
/// - `mcp_command`  — spawn process, MCP JSON-RPC over stdin/stdout
/// - `mcp_url`      — HTTP POST MCP JSON-RPC
///
/// Protocol: minimal MCP subset (initialize → tools/call).

use std::io::{BufRead, BufReader, Write};
use std::net::{IpAddr, ToSocketAddrs};
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

/// Returns true if `ip` belongs to a private/link-local/loopback/reserved range.
fn is_private_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            let o = v4.octets();
            // 10.0.0.0/8
            o[0] == 10 ||
            // 172.16.0.0/12
            (o[0] == 172 && (16..=31).contains(&o[1])) ||
            // 192.168.0.0/16
            (o[0] == 192 && o[1] == 168) ||
            // 127.0.0.0/8 loopback
            o[0] == 127 ||
            // 169.254.0.0/16 link-local (AWS metadata, APIPA)
            (o[0] == 169 && o[1] == 254) ||
            // 0.0.0.0/8
            o[0] == 0 ||
            // 100.64.0.0/10 carrier-grade NAT shared space
            (o[0] == 100 && (64..=127).contains(&o[1])) ||
            // 198.18.0.0/15 network benchmark
            (o[0] == 198 && (o[1] == 18 || o[1] == 19))
        }
        IpAddr::V6(v6) => {
            v6.is_loopback() ||
            v6.is_unspecified() ||
            // Link-local fe80::/10
            (v6.segments()[0] & 0xffc0) == 0xfe80 ||
            // Unique-local fc00::/7
            (v6.segments()[0] & 0xfe00) == 0xfc00 ||
            // IPv4-mapped ::ffff:x.x.x.x — check the mapped address
            v6.to_ipv4_mapped().map_or(false, |v4| is_private_ip(IpAddr::V4(v4)))
        }
    }
}

/// Extract the hostname from an http:// or https:// URL.
/// Returns `None` if the URL cannot be parsed or contains a userinfo component
/// (e.g. `http://evil@127.0.0.1/`), which can be used to bypass host extraction.
fn extract_host(url: &str) -> Option<(&str, u16)> {
    let rest = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))?;
    let is_https = url.starts_with("https://");

    // Reject userinfo (anything with `@` before the first `/`).
    let path_start = rest.find('/').unwrap_or(rest.len());
    if rest[..path_start].contains('@') {
        return None;
    }

    // Handle bracketed IPv6: `[::1]:8080`.
    if rest.starts_with('[') {
        let close = rest.find(']')?;
        let host = &rest[1..close];
        let after = &rest[close + 1..];
        let port = if let Some(port_str) = after.strip_prefix(':') {
            let port_end = port_str.find(['/', '?', '#']).unwrap_or(port_str.len());
            port_str[..port_end].parse().ok()?
        } else {
            if is_https { 443 } else { 80 }
        };
        return Some((host, port));
    }

    // Regular host[:port].
    let host_port = &rest[..path_start];
    if let Some((h, p)) = host_port.split_once(':') {
        Some((h, p.parse().ok()?))
    } else {
        Some((host_port, if is_https { 443 } else { 80 }))
    }
}

fn builtin_url(url: &str) -> VerificationResult {
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return VerificationResult::fail("only http/https URLs are supported");
    }

    // ── SSRF pre-flight ─────────────────────────────────────────────────────
    // Resolve the hostname to IP(s) and block private/link-local addresses.
    // Note: DNS rebinding can change the answer between this check and ureq's
    // own resolution. Redirects are disabled below to eliminate redirect-based
    // SSRF bypass vectors.
    let (host, port) = match extract_host(url) {
        Some(hp) => hp,
        None => return VerificationResult::fail("invalid or unsafe URL (userinfo component)"),
    };

    let addr_str = if host.contains(':') {
        // IPv6 literal — brackets already stripped by extract_host.
        format!("[{host}]:{port}")
    } else {
        format!("{host}:{port}")
    };

    let resolved: Vec<_> = match addr_str.to_socket_addrs() {
        Ok(it) => it.collect(),
        Err(e)  => return VerificationResult::fail(&format!("DNS resolution failed: {e}")),
    };

    if resolved.is_empty() {
        return VerificationResult::fail("hostname resolved to no addresses");
    }

    for addr in &resolved {
        if is_private_ip(addr.ip()) {
            return VerificationResult::fail(&format!(
                "SSRF blocked: {host} resolves to private/reserved address {}",
                addr.ip()
            ));
        }
    }

    // ── HTTP probe ──────────────────────────────────────────────────────────
    // Redirects are disabled: a public URL that redirects to 169.254.x.x
    // would otherwise bypass the pre-flight check above.
    let agent = ureq::builder()
        .timeout(Duration::from_secs(10))
        .redirects(0)
        .build();

    let head_result = agent.head(url).call();

    match head_result {
        Ok(r) if r.status() < 400 => VerificationResult::ok(),
        Err(ureq::Error::Status(405, _)) | Ok(_) => {
            // 405 (HEAD not allowed) or non-2xx HEAD — try GET.
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
