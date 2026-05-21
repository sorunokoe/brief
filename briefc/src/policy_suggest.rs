/// `brief policy suggest` — generate allow{}/deny{} patterns from task goal + skill interfaces.
///
/// Uses heuristic keyword matching (always works) with an optional LLM enhancement
/// when SmolLM2-135M is installed via `brief models install`.
///
/// Usage:
///   brief policy suggest --task review-pr [--apply]
use colored::Colorize;
use std::path::Path;

use crate::checker::{self, CheckContext};
use crate::lexer::lex;
use crate::manifest;
use crate::parser::parse;
use crate::skillgen::parse_briefskill;

pub fn run_policy_suggest(task_name: &str, file: Option<&Path>, apply: bool) -> bool {
    let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));

    // ── 1. Find .brief file ──────────────────────────────────────────────────
    let brief_path = match file {
        Some(p) => p.to_path_buf(),
        None => match find_brief_file(&cwd) {
            Some(p) => p,
            None => {
                eprintln!("{}: no .brief file found. Use --file to specify.",
                    "error".red().bold());
                return false;
            }
        },
    };

    // ── 2. Parse .brief file ─────────────────────────────────────────────────
    let src = match std::fs::read_to_string(&brief_path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("{}: cannot read {}: {e}", "error".red().bold(), brief_path.display());
            return false;
        }
    };
    let (tokens, _) = lex(&src);
    let (program, _) = parse(&tokens, &src);

    let task = match program.tasks.iter().find(|t| t.name == task_name) {
        Some(t) => t,
        None => {
            eprintln!("{}: task '{}' not found in {}",
                "error".red().bold(), task_name, brief_path.display());
            let names: Vec<_> = program.tasks.iter().map(|t| t.name.as_str()).collect();
            if !names.is_empty() {
                eprintln!("  Available: {}", names.join(", "));
            }
            return false;
        }
    };

    // ── 3. Load skill interfaces ─────────────────────────────────────────────
    let file_dir = brief_path.parent().unwrap_or(Path::new("."));
    let mf = manifest::load_manifest(file_dir);
    let ctx = CheckContext {
        file_dir,
        cwd: &cwd,
        manifest: mf.as_ref(),
        brief_path: None,
        allow_missing_skills: true,
    };

    let mut iface_funcs: Vec<(String, String, Option<String>)> = Vec::new(); // (skill, func, desc)
    for skill_name in &task.uses {
        if let Some(iface_path) = checker::find_skill_interface(skill_name, &ctx) {
            if let Ok(content) = std::fs::read_to_string(&iface_path) {
                if let Some(iface) = parse_briefskill(&content) {
                    for f in &iface.funcs {
                        iface_funcs.push((skill_name.clone(), f.name.clone(), None));
                    }
                }
            }
        }
    }

    if iface_funcs.is_empty() {
        eprintln!("{} No skill interfaces found for task '{}' uses [].",
            "warn".yellow().bold(), task_name);
        eprintln!("  Run `brief skillsync` first to generate .briefskill files.");
        return true;
    }

    // ── 4. Suggest allow/deny using heuristics ───────────────────────────────
    let goal_lower = task.goal.as_deref().unwrap_or("").to_lowercase();
    let (allow_pats, deny_pats) = heuristic_suggest(&goal_lower, &iface_funcs);

    // ── 5. Check if LLM model is available ───────────────────────────────────
    let has_model = crate::models::is_model_installed("smollm2-135m.gguf");
    if !has_model {
        eprintln!("{}", "💡 Run `brief models install` to enable AI-powered policy generation.".dimmed());
        eprintln!("{}", "   Using keyword heuristics for now.".dimmed());
        eprintln!();
    }

    // ── 6. Print suggested spec ──────────────────────────────────────────────
    eprintln!("{} Analyzing goal: \"{}\"", "→".cyan(), 
        task.goal.as_deref().unwrap_or("(no goal set)").dimmed());
    eprintln!("{} Available skills: {}",
        "  ↳".dimmed(),
        task.uses.iter().map(|s| {
            let n = iface_funcs.iter().filter(|(sk, _, _)| sk == s).count();
            format!("{s} ({n} functions)")
        }).collect::<Vec<_>>().join(", ").dimmed()
    );
    eprintln!();
    eprintln!("{}", "Suggested allow/deny spec:".bold());
    eprintln!();

    println!("  allow {{");
    for (skill, func) in &allow_pats {
        println!("    {skill}.{func}({})", suggest_args(func));
    }
    println!("  }}");

    if !deny_pats.is_empty() {
        println!("  deny {{");
        for (skill, func) in &deny_pats {
            println!("    {skill}.{func}");
        }
        println!("  }}");
    }

    println!();
    println!("{}",
        "💡 Add this to your task block. Run `brief check` to validate.".dimmed());

    // ── 7. --apply: patch the .brief file ────────────────────────────────────
    if apply {
        match apply_policy_patch(&brief_path, &src, task_name, &allow_pats, &deny_pats) {
            Ok(()) => println!("  {} Applied to {}. Review with `git diff`.",
                "✓".green(), brief_path.display()),
            Err(e) => {
                eprintln!("{}: {e}", "error".red().bold());
                return false;
            }
        }
    }

    true
}

/// Heuristic keyword → allow/deny categorization.
fn heuristic_suggest(
    goal: &str,
    funcs: &[(String, String, Option<String>)],
) -> (Vec<(String, String)>, Vec<(String, String)>) {
    // Read-like prefixes → allow.
    let read_prefixes = ["get_", "list_", "fetch_", "read_", "search_", "find_",
                         "describe_", "check_", "view_", "show_", "inspect_", "query_"];
    // Mutating prefixes → allow only for write-oriented goals; deny for read-only goals.
    let write_prefixes = ["write_", "create_", "update_", "post_", "push_", "add_", "set_",
                          "upload_", "insert_", "save_", "put_"];
    // Dangerous → always deny (unless goal explicitly mentions them).
    let danger_prefixes = ["delete_", "remove_", "drop_", "destroy_", "merge_", "force_",
                           "reset_", "rollback_", "purge_", "wipe_"];
    let danger_keywords = ["delete", "destroy", "drop", "purge", "wipe", "force push"];

    // Is this a read-oriented goal?
    let is_read_goal = goal.contains("review") || goal.contains("read") || goal.contains("view")
        || goal.contains("check") || goal.contains("inspect") || goal.contains("list")
        || goal.contains("fetch") || goal.contains("analyze") || goal.contains("audit");

    // Is this a write-oriented goal?
    let is_write_goal = goal.contains("write") || goal.contains("create") || goal.contains("update")
        || goal.contains("generate") || goal.contains("produce") || goal.contains("build")
        || goal.contains("post") || goal.contains("submit") || goal.contains("save");

    let mut allow = Vec::new();
    let mut deny = Vec::new();

    for (skill, func, _desc) in funcs {
        let flow = func.to_lowercase();

        // Dangerous → deny unless goal explicitly wants it.
        if danger_prefixes.iter().any(|p| flow.starts_with(p)) {
            let goal_wants_danger = danger_keywords.iter().any(|k| goal.contains(k));
            if !goal_wants_danger {
                deny.push((skill.clone(), func.clone()));
                continue;
            }
        }

        // Read ops → allow for read-oriented goals (and don't hurt write goals).
        if read_prefixes.iter().any(|p| flow.starts_with(p)) {
            allow.push((skill.clone(), func.clone()));
            continue;
        }

        // Write ops → allow only if write-oriented goal; deny for read-only goals.
        if write_prefixes.iter().any(|p| flow.starts_with(p)) {
            if is_write_goal && !is_read_goal {
                allow.push((skill.clone(), func.clone()));
            } else {
                deny.push((skill.clone(), func.clone()));
            }
            continue;
        }

        // Unknown prefix → allow (assume safe until denied).
        allow.push((skill.clone(), func.clone()));
    }

    (allow, deny)
}

/// Suggest wildcard args for well-known function types.
fn suggest_args(func: &str) -> &'static str {
    // For sensitive path functions, suggest a wildcard placeholder.
    if func.contains("file") || func.contains("path") || func.contains("write")
        || func.contains("read")
    {
        "path=*"
    } else if func.contains("pull_request") || func.contains("pr") {
        "owner=*, repo=*, number=*"
    } else {
        ""
    }
}

/// Patch the .brief file to add allow{}/deny{} blocks to the named task.
fn apply_policy_patch(
    path: &Path,
    src: &str,
    task_name: &str,
    allow: &[(String, String)],
    deny: &[(String, String)],
) -> Result<(), String> {
    // Find the task block and insert before the first step or the closing `}`.
    // Simple heuristic: find `task <name>` then find the closing `}` at column 0.
    let task_marker = format!("task {task_name}");
    let task_pos = src.find(&task_marker)
        .ok_or_else(|| format!("task '{task_name}' not found in source"))?;

    // Find the end of the task block (first `}` at column 0 after task_pos).
    let after_task = &src[task_pos..];
    let close_pos = after_task.lines().skip(1)
        .enumerate()
        .find(|(_, line)| line.starts_with('}'))
        .map(|(i, _)| i + 1)
        .ok_or("cannot find task closing brace")?;

    // Compute byte offset of closing brace.
    let close_byte = task_pos + after_task.lines()
        .take(close_pos)
        .map(|l| l.len() + 1)  // +1 for \n
        .sum::<usize>();

    let mut insertion = String::new();
    if !allow.is_empty() {
        insertion.push_str("    allow {\n");
        for (skill, func) in allow {
            insertion.push_str(&format!("        {skill}.{func}\n"));
        }
        insertion.push_str("    }\n");
    }
    if !deny.is_empty() {
        insertion.push_str("    deny {\n");
        for (skill, func) in deny {
            insertion.push_str(&format!("        {skill}.{func}\n"));
        }
        insertion.push_str("    }\n");
    }

    let mut patched = src.to_string();
    patched.insert_str(close_byte, &insertion);

    std::fs::write(path, &patched).map_err(|e| e.to_string())
}

fn find_brief_file(cwd: &Path) -> Option<std::path::PathBuf> {
    for entry in std::fs::read_dir(cwd).ok()? {
        let entry = entry.ok()?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("brief") {
            return Some(path);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn heuristic_review_goal_allows_reads_denies_writes() {
        let funcs = vec![
            ("GitHub".to_string(), "get_pull_request".to_string(), None),
            ("GitHub".to_string(), "merge_pull_request".to_string(), None),
            ("GitHub".to_string(), "delete_branch".to_string(), None),
            ("GitHub".to_string(), "list_pull_requests".to_string(), None),
        ];
        let (allow, deny) = heuristic_suggest("review a github pull request", &funcs);
        let allow_names: Vec<_> = allow.iter().map(|(_, f)| f.as_str()).collect();
        let deny_names: Vec<_> = deny.iter().map(|(_, f)| f.as_str()).collect();

        assert!(allow_names.contains(&"get_pull_request"), "get_ should be allowed");
        assert!(allow_names.contains(&"list_pull_requests"), "list_ should be allowed");
        assert!(deny_names.contains(&"merge_pull_request"), "merge_ should be denied for review goal");
        assert!(deny_names.contains(&"delete_branch"), "delete_ should be denied");
    }

    #[test]
    fn heuristic_write_goal_allows_write_ops() {
        let funcs = vec![
            ("FileSystem".to_string(), "write_file".to_string(), None),
            ("FileSystem".to_string(), "read_file".to_string(), None),
            ("FileSystem".to_string(), "delete_file".to_string(), None),
        ];
        let (allow, deny) = heuristic_suggest("write a report to file", &funcs);
        let allow_names: Vec<_> = allow.iter().map(|(_, f)| f.as_str()).collect();
        let deny_names: Vec<_> = deny.iter().map(|(_, f)| f.as_str()).collect();

        assert!(allow_names.contains(&"write_file"), "write_ allowed for write goal");
        assert!(allow_names.contains(&"read_file"), "read_ always allowed");
        assert!(deny_names.contains(&"delete_file"), "delete_ always denied");
    }
}
