/// `brief ci` — run all checks declared in `[ci] examples` of `brief.toml`.
///
/// Loads the nearest `brief.toml`, collects all paths from `[ci] examples`,
/// and runs `brief check` logic on each. Exits non-zero if any file fails.
/// This is a pure-Rust alternative to writing shell loops in CI YAML.
use colored::Colorize;
use std::path::PathBuf;
use crate::manifest;
use crate::runner::{self, RunMode};

pub fn run_ci(start: &PathBuf) -> bool {
    // ── Find manifest ─────────────────────────────────────────────────────────
    let manifest = match manifest::load_manifest(start) {
        Some(m) => m,
        None => {
            eprintln!("{} No brief.toml found (searched from {})",
                "error:".red().bold(),
                start.display());
            return false;
        }
    };

    if manifest.ci.examples.is_empty() {
        println!("{} No [ci] examples defined in brief.toml — nothing to check.",
            "brief ci:".dimmed());
        return true;
    }

    println!("{}", format!(
        "brief ci — checking {} pattern(s) from brief.toml [ci] examples",
        manifest.ci.examples.len()
    ).bold());

    let mut pass = 0usize;
    let mut fail = 0usize;

    for pattern in &manifest.ci.examples {
        let paths = expand_pattern(&manifest.root_dir, pattern);

        if paths.is_empty() {
            eprintln!("  {} no files matched: {pattern}", "warn:".yellow().bold());
            continue;
        }

        for path in paths {
            // Run in check-only mode; runner prints its own diagnostics.
            let ok = runner::run_file(&path, RunMode::Check { allow_missing_skills: false });
            if ok {
                println!("  {} {}", "✓".green(), path.display());
                pass += 1;
            } else {
                println!("  {} {}", "✗".red(), path.display());
                fail += 1;
            }
        }
    }

    println!();
    if fail == 0 {
        println!("{} {} file(s) passed.", "✅".green().bold(), pass);
        true
    } else {
        eprintln!("{} {}/{} file(s) failed.", "❌".red().bold(), fail, pass + fail);
        false
    }
}

fn expand_pattern(root: &PathBuf, pattern: &str) -> Vec<PathBuf> {
    let full = root.join(pattern);

    if full.is_file() {
        return vec![full];
    }

    // Handle glob like `examples/*.brief`
    if let Some(parent) = full.parent() {
        if let Some(file_name) = full.file_name() {
            let name_str = file_name.to_string_lossy();
            if name_str.starts_with('*') {
                let ext = name_str.trim_start_matches('*').trim_start_matches('.');
                if let Ok(rd) = std::fs::read_dir(parent) {
                    let mut results: Vec<PathBuf> = rd
                        .filter_map(|e| e.ok())
                        .map(|e| e.path())
                        .filter(|p| {
                            p.is_file()
                                && p.extension().and_then(|e| e.to_str()) == Some(ext)
                        })
                        .collect();
                    results.sort();
                    return results;
                }
            }
        }
    }

    // Pattern is a directory — collect all *.brief inside it
    if full.is_dir() {
        if let Ok(rd) = std::fs::read_dir(&full) {
            let mut results: Vec<PathBuf> = rd
                .filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| {
                    p.is_file()
                        && p.extension().and_then(|e| e.to_str()) == Some("brief")
                })
                .collect();
            results.sort();
            return results;
        }
    }

    vec![]
}
