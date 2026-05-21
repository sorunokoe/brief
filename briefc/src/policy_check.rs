/// `brief policy-check` — simulate a tool call against a task's allow/deny policy
/// without starting an MCP server.
///
/// Usage:
///   brief policy-check --task review-pr --tool FileSystem.write_file \
///                      --args '{"path":"./src/main.rs"}'
use colored::Colorize;
use std::path::{Path, PathBuf};

use crate::enforcer::{self, CallDecision};
use crate::lexer::lex;
use crate::parser::parse;

pub fn run_policy_check(
    file: Option<&Path>,
    task_name: &str,
    tool: &str,
    args_str: &str,
) -> bool {
    // ── 1. Resolve .brief file path ─────────────────────────────────────────
    let brief_path = match file {
        Some(p) => p.to_path_buf(),
        None => match find_brief_file() {
            Some(p) => p,
            None => {
                eprintln!(
                    "{}: no .brief file found in current directory. \
                     Use --file to specify one.",
                    "error".red().bold()
                );
                return false;
            }
        },
    };

    // ── 2. Parse the .brief file ─────────────────────────────────────────────
    let source = match std::fs::read_to_string(&brief_path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!(
                "{}: cannot read {}: {e}",
                "error".red().bold(),
                brief_path.display()
            );
            return false;
        }
    };
    let (tokens, _) = lex(&source);
    let (program, parse_errs) = parse(&tokens, &source);
    if parse_errs.iter().any(|d| d.is_error()) {
        for d in &parse_errs {
            eprintln!("{}: {}", "error".red().bold(), d.message);
        }
        return false;
    }

    // ── 3. Find the named task ────────────────────────────────────────────────
    let task = match program.tasks.iter().find(|t| t.name == task_name) {
        Some(t) => t,
        None => {
            eprintln!(
                "{}: task '{}' not found in {}",
                "error".red().bold(),
                task_name,
                brief_path.display()
            );
            let names: Vec<_> = program.tasks.iter().map(|t| t.name.as_str()).collect();
            if !names.is_empty() {
                eprintln!("  Available tasks: {}", names.join(", "));
            }
            return false;
        }
    };

    // ── 4. Parse tool string: Skill.function ─────────────────────────────────
    let (skill, func) = match tool.split_once('.') {
        Some((s, f)) => (s, f),
        None => {
            eprintln!(
                "{}: --tool must be in Skill.function format (e.g. FileSystem.write_file)",
                "error".red().bold()
            );
            return false;
        }
    };

    // ── 5. Parse args JSON ────────────────────────────────────────────────────
    let args_json: serde_json::Value = match serde_json::from_str(args_str) {
        Ok(v) => v,
        Err(e) => {
            eprintln!(
                "{}: --args is not valid JSON: {e}",
                "error".red().bold()
            );
            return false;
        }
    };

    // ── 6. Run the enforcer ───────────────────────────────────────────────────
    let decision = enforcer::check_call(skill, func, &args_json, &task.allow, &task.deny);

    // ── 7. Print result ───────────────────────────────────────────────────────
    match &decision {
        CallDecision::Permitted => {
            println!(
                "{}  {}.{}  {}",
                "PERMITTED".green().bold(),
                skill,
                func,
                format_args_display(&args_json)
            );
        }
        CallDecision::Blocked { reason } => {
            println!(
                "{}  {}.{}  {}",
                "BLOCKED".red().bold(),
                skill,
                func,
                format_args_display(&args_json)
            );
            println!("  {}", format!("reason: {reason}").dimmed());
        }
    }

    // Exit 0 for both decisions — non-zero only for usage errors.
    true
}

/// Find the first `.brief` file in the current directory.
fn find_brief_file() -> Option<PathBuf> {
    let cwd = std::env::current_dir().ok()?;
    for entry in std::fs::read_dir(&cwd).ok()? {
        let entry = entry.ok()?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("brief") {
            return Some(path);
        }
    }
    None
}

fn format_args_display(args: &serde_json::Value) -> String {
    match args {
        serde_json::Value::Object(map) if !map.is_empty() => {
            let pairs: Vec<String> = map
                .iter()
                .map(|(k, v)| format!("{k}={v}"))
                .collect();
            format!("({})", pairs.join(", "))
        }
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write_brief(dir: &std::path::Path, content: &str) -> PathBuf {
        let p = dir.join("test.brief");
        std::fs::write(&p, content).unwrap();
        p
    }

    #[test]
    fn policy_check_permitted() {
        let dir = tempfile::tempdir().unwrap();
        let brief_src = r#"
import skill "GitHub"

task Review : TaskBrief uses [GitHub] {
    goal = "review pr"
    allow {
        GitHub.get_pull_request
    }
}
"#;
        let path = write_brief(dir.path(), brief_src);
        let ok = run_policy_check(
            Some(&path),
            "Review",
            "GitHub.get_pull_request",
            r#"{"number": 1}"#,
        );
        assert!(ok, "policy check should succeed");
    }

    #[test]
    fn policy_check_blocked() {
        let dir = tempfile::tempdir().unwrap();
        let brief_src = r#"
import skill "GitHub"

task Review : TaskBrief uses [GitHub] {
    goal = "review pr"
    allow {
        GitHub.get_pull_request
    }
    deny {
        GitHub.*
    }
}
"#;
        let path = write_brief(dir.path(), brief_src);
        let ok = run_policy_check(
            Some(&path),
            "Review",
            "GitHub.get_pull_request",
            "{}",
        );
        // Returns true (exit 0) even for blocked — only false on usage error
        assert!(ok, "policy check should return ok even for blocked decision");
    }

    #[test]
    fn policy_check_invalid_tool_format() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_brief(dir.path(), r#"task T : TaskBrief uses [] { goal = "x" }"#);
        let ok = run_policy_check(Some(&path), "T", "NoDotsHere", "{}");
        assert!(!ok, "should fail for invalid tool format");
    }

    #[test]
    fn policy_check_invalid_args_json() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_brief(dir.path(), r#"task T : TaskBrief uses [] { goal = "x" }"#);
        let ok = run_policy_check(Some(&path), "T", "Skill.func", "not-json");
        assert!(!ok, "should fail for invalid JSON args");
    }
}
