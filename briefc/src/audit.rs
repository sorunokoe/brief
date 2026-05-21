/// `brief audit` — summarize a JSONL trace file from `brief serve --record`.
///
/// Usage:
///   brief audit trace.jsonl [--fail-on-deny]
use colored::Colorize;
use serde_json::Value;
use std::path::Path;

pub fn run_audit(trace: &Path, fail_on_deny: bool) -> bool {
    // ── 1. Read and parse JSONL ──────────────────────────────────────────────
    let content = match std::fs::read_to_string(trace) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("{}: cannot read {}: {e}", "error".red().bold(), trace.display());
            return false;
        }
    };

    let mut session_task: Option<String> = None;
    let mut session_hash: Option<String> = None;
    let mut session_version: Option<String> = None;
    let mut calls: Vec<CallEntry> = Vec::new();

    for (lineno, line) in content.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() { continue; }
        let entry: Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("{}: malformed JSONL at line {}: {e}",
                    "warn".yellow().bold(), lineno + 1);
                continue;
            }
        };
        match entry["event"].as_str() {
            Some("session_start") => {
                session_task = entry["task"].as_str().map(str::to_string);
                session_hash = entry["policy_hash"].as_str().map(str::to_string);
                session_version = entry["brief_version"].as_str().map(str::to_string);
            }
            Some("call") => {
                let skill = entry["skill"].as_str().unwrap_or("?").to_string();
                let func = entry["fn"].as_str().unwrap_or("?").to_string();
                let allowed = entry["allowed"].as_bool().unwrap_or(true);
                let blocked_reason = entry["blocked_reason"].as_str().map(str::to_string);
                let ts = entry["ts"].as_str().unwrap_or("").to_string();
                let ms = entry["ms"].as_u64().unwrap_or(0);
                let result_size = entry["result_size"].as_u64();
                let result_error = entry["result_error"].as_bool().unwrap_or(false);
                calls.push(CallEntry {
                    skill, func, allowed, blocked_reason, ts, ms, result_size, result_error,
                });
            }
            _ => {}
        }
    }

    // ── 2. Print header ──────────────────────────────────────────────────────
    eprintln!("{}", "brief audit".bold());
    eprintln!("{} {}", "  trace:".dimmed(), trace.display());
    if let Some(task) = &session_task {
        eprintln!("{} {}", "  task:".dimmed(), task);
    }
    if let Some(v) = &session_version {
        eprintln!("{} brief v{}", "  recorded with:".dimmed(), v);
    }
    if let Some(h) = &session_hash {
        eprintln!("{} {}", "  policy hash:".dimmed(), h);
    }
    eprintln!();

    // ── 3. Print call table ──────────────────────────────────────────────────
    let mut allowed_count = 0usize;
    let mut blocked_count = 0usize;
    let mut error_count = 0usize;

    for call in &calls {
        if call.allowed {
            allowed_count += 1;
            let result_indicator = if call.result_error {
                error_count += 1;
                format!("  {} (skill error)", "⚠".yellow())
            } else if let Some(sz) = call.result_size {
                format!("  {}ms  {}B", call.ms, sz)
            } else {
                format!("  {}ms", call.ms)
            };
            println!(
                "{}  {}.{}{}",
                "PASS".green().bold(),
                call.skill,
                call.func,
                result_indicator
            );
        } else {
            blocked_count += 1;
            let reason = call.blocked_reason.as_deref().unwrap_or("no reason");
            println!(
                "{}  {}.{}  — {}",
                "DENY".red().bold(),
                call.skill,
                call.func,
                reason.dimmed()
            );
        }
    }

    // ── 4. Summary ───────────────────────────────────────────────────────────
    println!();
    print!("Summary: ");
    print!("{} allowed", allowed_count.to_string().green());
    if error_count > 0 {
        print!(", {} skill errors", error_count.to_string().yellow());
    }
    if blocked_count > 0 {
        print!(", {}", format!("{} blocked", blocked_count).red());
    }
    println!();

    if calls.is_empty() {
        println!("{}", "  (no calls recorded in trace)".dimmed());
    }

    // ── 5. Exit code ─────────────────────────────────────────────────────────
    if fail_on_deny && blocked_count > 0 {
        eprintln!();
        eprintln!("{} {} blocked call(s) found — exiting with code 1 (--fail-on-deny)",
            "✗".red().bold(), blocked_count);
        return false;
    }

    true
}

struct CallEntry {
    skill: String,
    func: String,
    allowed: bool,
    blocked_reason: Option<String>,
    ts: String,
    ms: u64,
    result_size: Option<u64>,
    result_error: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write_trace(dir: &std::path::Path, content: &str) -> std::path::PathBuf {
        let p = dir.join("trace.jsonl");
        std::fs::write(&p, content).unwrap();
        p
    }

    #[test]
    fn audit_all_allowed_succeeds() {
        let dir = tempfile::tempdir().unwrap();
        let trace = write_trace(dir.path(), r#"
{"event":"session_start","task":"test","brief_version":"0.1.0","policy_hash":"sha256:abc","ts":"2024-01-01T00:00:00Z"}
{"event":"call","ts":"2024-01-01T00:00:01Z","skill":"GitHub","fn":"get_pr","allowed":true,"ms":100,"result_size":512,"result_error":false}
{"event":"call","ts":"2024-01-01T00:00:02Z","skill":"GitHub","fn":"list_prs","allowed":true,"ms":50,"result_size":256,"result_error":false}
"#);
        assert!(run_audit(&trace, false), "all-allowed trace should succeed");
        assert!(run_audit(&trace, true), "all-allowed trace should succeed with --fail-on-deny");
    }

    #[test]
    fn audit_blocked_without_fail_flag_succeeds() {
        let dir = tempfile::tempdir().unwrap();
        let trace = write_trace(dir.path(), r#"
{"event":"call","ts":"2024-01-01T00:00:01Z","skill":"GitHub","fn":"merge_pr","allowed":false,"blocked_reason":"matched deny: GitHub.merge_pr","ms":0}
"#);
        assert!(run_audit(&trace, false), "blocked call without --fail-on-deny should exit 0");
    }

    #[test]
    fn audit_blocked_with_fail_flag_fails() {
        let dir = tempfile::tempdir().unwrap();
        let trace = write_trace(dir.path(), r#"
{"event":"call","ts":"2024-01-01T00:00:01Z","skill":"GitHub","fn":"merge_pr","allowed":false,"blocked_reason":"matched deny: GitHub.merge_pr","ms":0}
"#);
        assert!(!run_audit(&trace, true), "blocked call with --fail-on-deny should exit 1");
    }

    #[test]
    fn audit_empty_trace_succeeds() {
        let dir = tempfile::tempdir().unwrap();
        let trace = write_trace(dir.path(), "");
        assert!(run_audit(&trace, false));
        assert!(run_audit(&trace, true));
    }

    #[test]
    fn audit_missing_file_fails() {
        let p = std::path::PathBuf::from("/nonexistent/trace.jsonl");
        assert!(!run_audit(&p, false));
    }
}
