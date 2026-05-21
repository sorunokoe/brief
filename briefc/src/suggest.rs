/// `brief suggest` — read a trace file and suggest spec improvements.
///
/// Reads a `.brief.trace` JSONL file (from `brief serve --record`) + the current
/// `.brief` spec file to produce actionable suggestions:
///
/// - W501: AI called a skill not in `uses[]`
/// - W502: Skill in `uses[]` but never called across recorded traces
/// - W503: Env var value appeared in trace args — suggest adding to `needs {}`
/// - Blocked calls: REVIEW REQUIRED — suggest allow{} expansion
///
/// Usage:
///   brief suggest trace.jsonl [--file task.brief] [--apply]
///
/// --apply safety: ONLY adds conservative additions (needs{}, missing uses[]).
///   Never auto-applies allow{} expansions or deny{} removals.
use colored::Colorize;
use serde_json::Value;
use std::path::{Path, PathBuf};

use crate::lexer::lex;
use crate::parser::parse;

pub fn run_suggest(trace: &Path, file: Option<&Path>, apply: bool) -> bool {
    // ── 1. Find .brief file ──────────────────────────────────────────────────
    let brief_path = match file {
        Some(p) => p.to_path_buf(),
        None => match find_brief_file() {
            Some(p) => p,
            None => {
                eprintln!(
                    "{}: no .brief file found. Use --file to specify one.",
                    "error".red().bold()
                );
                return false;
            }
        },
    };

    // ── 2. Parse .brief file ─────────────────────────────────────────────────
    let brief_src = match std::fs::read_to_string(&brief_path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("{}: cannot read {}: {e}", "error".red().bold(), brief_path.display());
            return false;
        }
    };
    let (tokens, _) = lex(&brief_src);
    let (program, _) = parse(&tokens, &brief_src);

    // ── 3. Parse trace ───────────────────────────────────────────────────────
    let trace_content = match std::fs::read_to_string(trace) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("{}: cannot read {}: {e}", "error".red().bold(), trace.display());
            return false;
        }
    };

    let mut blocked_calls: Vec<BlockedCall> = Vec::new();
    let mut all_skill_calls: std::collections::HashMap<String, usize> = Default::default();

    for line in trace_content.lines() {
        let line = line.trim();
        if line.is_empty() { continue; }
        let entry: Value = match serde_json::from_str(line) { Ok(v) => v, Err(_) => continue };
        if entry["event"].as_str() != Some("call") { continue; }

        let skill = entry["skill"].as_str().unwrap_or("").to_string();
        let func = entry["fn"].as_str().unwrap_or("").to_string();
        let allowed = entry["allowed"].as_bool().unwrap_or(true);
        let reason = entry["blocked_reason"].as_str().map(str::to_string);

        *all_skill_calls.entry(skill.clone()).or_insert(0) += 1;

        if !allowed {
            blocked_calls.push(BlockedCall { skill, func, reason });
        }
    }

    // ── 4. Collect declared skills and env needs ─────────────────────────────
    let declared_skills: std::collections::HashSet<String> =
        program.tasks.iter().flat_map(|t| t.uses.iter().cloned()).collect();

    let declared_needs: std::collections::HashSet<String> = program.tasks.iter()
        .flat_map(|t| t.needs.iter())
        .filter(|n| n.kind == crate::ast::NeedKind::Env)
        .map(|n| n.key.clone())
        .collect();

    // ── 5. Collect env var values (for W503 detection) ───────────────────────
    let env_vals: Vec<(String, String)> = declared_needs.iter()
        .filter_map(|k| std::env::var(k).ok().map(|v| (k.clone(), v)))
        .filter(|(_, v)| !v.is_empty())
        .collect();

    // Also check common env vars that may be implicitly used.
    let common_env_keys = ["GITHUB_TOKEN", "OPENAI_API_KEY", "ANTHROPIC_API_KEY",
                           "DATABASE_URL", "API_KEY", "ACCESS_TOKEN"];
    let mut suggested_needs: std::collections::HashSet<String> = Default::default();
    for key in &common_env_keys {
        if declared_needs.contains(*key) { continue; }
        if let Ok(v) = std::env::var(key) {
            if v.is_empty() { continue; }
            // Check if this value appears in any trace arg.
            for line in trace_content.lines() {
                if line.contains(&v) {
                    suggested_needs.insert(key.to_string());
                    break;
                }
            }
        }
    }

    // ── 6. Build suggestions ─────────────────────────────────────────────────
    let mut suggestions: Vec<Suggestion> = Vec::new();

    // W501: skill called but not in uses[].
    for (skill, count) in &all_skill_calls {
        if !declared_skills.contains(skill) {
            suggestions.push(Suggestion {
                code: "W501",
                severity: SuggestionKind::Safe,
                message: format!("Skill '{skill}' was called {count}× but is not in uses[]"),
                patch: Some(format!("  uses [... {skill}]")),
            });
        }
    }

    // W502: skill in uses[] but never called.
    for skill in &declared_skills {
        if all_skill_calls.get(skill).copied().unwrap_or(0) == 0 {
            suggestions.push(Suggestion {
                code: "W502",
                severity: SuggestionKind::Info,
                message: format!("Skill '{skill}' is in uses[] but was never called in this trace"),
                patch: None,
            });
        }
    }

    // W503: suggest missing needs{} for env vars found in trace.
    for key in &suggested_needs {
        suggestions.push(Suggestion {
            code: "W503",
            severity: SuggestionKind::Safe,
            message: format!("Env var '{key}' value found in trace args — suggest adding to needs{{}}"),
            patch: Some(format!("  needs {{ env \"{key}\" }}")),
        });
    }

    // Blocked calls → REVIEW REQUIRED suggestions.
    for call in &blocked_calls {
        let reason = call.reason.as_deref().unwrap_or("policy blocked");
        suggestions.push(Suggestion {
            code: "BLOCK",
            severity: SuggestionKind::ReviewRequired,
            message: format!("AI needed {}.{} but it was blocked ({})", call.skill, call.func, reason),
            patch: Some(format!("  allow {{ {}.{} }}  // REVIEW REQUIRED", call.skill, call.func)),
        });
    }

    // ── 7. Print suggestions ─────────────────────────────────────────────────
    if suggestions.is_empty() {
        println!("{} No suggestions — spec looks good for this trace.", "✓".green().bold());
        return true;
    }

    println!("{}", "brief suggest".bold());
    println!();

    let safe: Vec<_> = suggestions.iter().filter(|s| s.severity == SuggestionKind::Safe).collect();
    let info: Vec<_> = suggestions.iter().filter(|s| s.severity == SuggestionKind::Info).collect();
    let review: Vec<_> = suggestions.iter().filter(|s| s.severity == SuggestionKind::ReviewRequired).collect();

    if !safe.is_empty() {
        println!("{}", "Conservative additions (safe to --apply):".bold());
        for s in &safe {
            println!("  {} [{}] {}", "→".green(), s.code.cyan(), s.message);
            if let Some(patch) = &s.patch {
                println!("    {}", patch.dimmed());
            }
        }
        println!();
    }

    if !info.is_empty() {
        println!("{}", "Informational:".bold());
        for s in &info {
            println!("  {} [{}] {}", "·".dimmed(), s.code.dimmed(), s.message);
        }
        println!();
    }

    if !review.is_empty() {
        println!("{}", "REVIEW REQUIRED (not auto-applied):".yellow().bold());
        for s in &review {
            println!("  {} [{}] {}", "!".yellow(), s.code.yellow(), s.message);
            if let Some(patch) = &s.patch {
                println!("    {}", patch.dimmed());
            }
        }
        println!();
    }

    // ── 8. --apply: only safe additions ─────────────────────────────────────
    if apply {
        let safe_patches: Vec<_> = safe.iter().filter_map(|s| s.patch.as_ref()).collect();
        if safe_patches.is_empty() {
            println!("{} Nothing to auto-apply (safe additions only).", "·".dimmed());
        } else {
            println!("{} Applying {} safe addition(s) to {} ...",
                "→".green(), safe_patches.len(), brief_path.display());
            match apply_safe_patches(&brief_path, &brief_src, &suggestions) {
                Ok(()) => println!("  {} Done. Review with `git diff`.", "✓".green()),
                Err(e) => {
                    eprintln!("  {} Failed to apply: {e}", "error".red().bold());
                    return false;
                }
            }
        }
    }

    true
}

/// Apply safe (W501, W503) patches to the .brief file.
/// Only adds `uses [X]` entries and `needs { env "X" }` entries.
fn apply_safe_patches(
    path: &Path,
    src: &str,
    suggestions: &[Suggestion],
) -> Result<(), String> {
    let mut patched = src.to_string();

    for s in suggestions {
        if s.severity != SuggestionKind::Safe { continue; }
        match s.code {
            "W501" => {
                // Add skill to uses[] in the first task.
                // Find `uses [` and insert before the `]`.
                if let Some(patch_skill) = s.patch.as_ref()
                    .and_then(|p| p.trim().strip_prefix("uses [... "))
                    .map(|p| p.trim_end_matches(']').trim())
                {
                    if let Some(pos) = patched.find("uses [") {
                        if let Some(close) = patched[pos..].find(']') {
                            let insert_at = pos + close;
                            let existing = &patched[pos..insert_at + 1];
                            // Only add if not already present.
                            if !existing.contains(patch_skill) {
                                let new_uses = if existing.trim_end_matches(']').trim().ends_with('[') {
                                    format!("{patch_skill}]")
                                } else {
                                    format!(", {patch_skill}]")
                                };
                                patched.replace_range(insert_at..insert_at + 1, &new_uses);
                            }
                        }
                    }
                }
            }
            "W503" => {
                // Add `needs { env "X" }` before the first `step` or `goal =`.
                if let Some(key) = s.patch.as_ref()
                    .and_then(|p| p.trim().strip_prefix("needs { env \""))
                    .map(|p| p.trim_end_matches("\" }").trim())
                {
                    let insertion = format!("    needs {{ env \"{key}\" }}\n");
                    // Insert after the `goal =` line.
                    if let Some(pos) = patched.find("goal =") {
                        if let Some(eol) = patched[pos..].find('\n') {
                            let insert_at = pos + eol + 1;
                            patched.insert_str(insert_at, &insertion);
                        }
                    }
                }
            }
            _ => {}
        }
    }

    std::fs::write(path, &patched).map_err(|e| e.to_string())
}

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

#[derive(Debug, PartialEq)]
enum SuggestionKind {
    Safe,           // W501, W503 — safe to auto-apply
    Info,           // W502 — informational only
    ReviewRequired, // Blocked calls — NEVER auto-applied
}

struct Suggestion {
    code: &'static str,
    severity: SuggestionKind,
    message: String,
    patch: Option<String>,
}

struct BlockedCall {
    skill: String,
    func: String,
    reason: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write_file(dir: &Path, name: &str, content: &str) -> PathBuf {
        let p = dir.join(name);
        std::fs::write(&p, content).unwrap();
        p
    }

    #[test]
    fn suggest_w501_missing_skill_in_uses() {
        let dir = tempfile::tempdir().unwrap();
        let trace = write_file(dir.path(), "trace.jsonl", r#"
{"event":"call","ts":"2024-01-01T00:00:01Z","skill":"FileSystem","fn":"write_file","allowed":true,"ms":10}
"#);
        let brief = write_file(dir.path(), "test.brief", r#"
import skill "GitHub"
task T : TaskBrief uses [GitHub] { goal = "test" }
"#);
        // run_suggest returns true even with suggestions
        let ok = run_suggest(&trace, Some(&brief), false);
        assert!(ok);
    }

    #[test]
    fn suggest_blocked_calls_show_review_required() {
        let dir = tempfile::tempdir().unwrap();
        let trace = write_file(dir.path(), "trace.jsonl", r#"
{"event":"call","ts":"2024-01-01T00:00:01Z","skill":"GitHub","fn":"merge_pr","allowed":false,"blocked_reason":"matched deny: GitHub.merge_pr","ms":0}
"#);
        let brief = write_file(dir.path(), "test.brief", r#"
import skill "GitHub"
task T : TaskBrief uses [GitHub] { goal = "test" }
"#);
        let ok = run_suggest(&trace, Some(&brief), false);
        assert!(ok);
    }

    #[test]
    fn suggest_missing_trace_fails() {
        let dir = tempfile::tempdir().unwrap();
        let brief = write_file(dir.path(), "test.brief", r#"task T : TaskBrief uses [] { goal = "x" }"#);
        let ok = run_suggest(Path::new("/nonexistent/trace.jsonl"), Some(&brief), false);
        assert!(!ok);
    }
}
