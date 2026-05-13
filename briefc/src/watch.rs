/// brief watch <file> — live re-check on file save
///
/// Watches a .brief file (or all *.brief files in a directory) for changes.
/// On each save, clears the terminal, runs `brief check`, and shows a diff
/// of how the error count changed vs the previous run.

use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use colored::Colorize;
use notify::{Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};

use crate::checker;
use crate::errors::{BriefError, ErrorCode, print_diagnostics};
use crate::lexer::lex;
use crate::parser::parse;

/// Entry point for `brief watch`.
/// `path` can be a single `.brief` file or a directory (watches all `*.brief` files).
pub fn watch(path: &Path) -> bool {
    let display_path = path.display();

    if !path.exists() {
        eprintln!("{} path not found: {display_path}", "error:".red().bold());
        return false;
    }

    println!("{}", format!("brief watch — {display_path}").dimmed());
    println!("{}", "Watching for changes. Press Ctrl-C to stop.".dimmed());
    println!();

    // Run an initial check immediately
    let mut last_results = run_check(path);
    let last_count = last_results.iter().flat_map(|(e, _, _)| e.iter()).filter(|e| e.is_error()).count();
    print_check_result(path, &last_results, None);

    // Set up file-system watcher
    let (tx, rx) = mpsc::channel::<notify::Result<Event>>();
    let mut watcher = match RecommendedWatcher::new(tx, Config::default()) {
        Ok(w)  => w,
        Err(e) => {
            eprintln!("{} could not start watcher: {e}", "error:".red().bold());
            return false;
        }
    };

    let watch_target = if path.is_dir() { path } else { path.parent().unwrap_or(path) };
    if let Err(e) = watcher.watch(watch_target, RecursiveMode::NonRecursive) {
        eprintln!("{} could not watch {}: {e}", "error:".red().bold(), watch_target.display());
        return false;
    }

    // Debounce: ignore duplicate events within 100ms
    let debounce = Duration::from_millis(100);
    let mut last_event = Instant::now() - debounce * 2;
    let mut prev_hard = last_count;

    loop {
        match rx.recv() {
            Ok(Ok(event)) => {
                // Only react to create/modify/rename events on .brief files
                let is_brief_change = matches!(
                    event.kind,
                    EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_)
                ) && event.paths.iter().any(|p| is_brief_target(p, path));

                if is_brief_change {
                    let now = Instant::now();
                    if now.duration_since(last_event) < debounce {
                        continue; // debounce
                    }
                    last_event = now;

                    // Small pause to let the editor finish writing
                    std::thread::sleep(Duration::from_millis(50));

                    let new_results = run_check(path);
                    clear_terminal();
                    print_check_result(path, &new_results, Some(prev_hard));
                    prev_hard = new_results.iter().flat_map(|(e, _, _)| e.iter()).filter(|e| e.is_error()).count();
                    last_results = new_results;
                }
            }
            Ok(Err(e)) => eprintln!("{} watcher error: {e}", "warn:".yellow()),
            Err(_)     => break, // channel closed
        }
    }

    let _ = last_results; // keep alive until loop ends
    true
}

/// Returns true if `event_path` is the watched file or a `.brief` file in the watched dir.
fn is_brief_target(event_path: &Path, watched: &Path) -> bool {
    if watched.is_file() {
        event_path == watched
    } else {
        event_path.extension().map_or(false, |e| e == "brief")
    }
}

/// Parse and check a file (or all `.brief` files in a directory).
/// Returns `Vec<(errors, source, filename)>` per checked file.
fn run_check(path: &Path) -> Vec<(Vec<BriefError>, String, String)> {
    let files: Vec<PathBuf> = if path.is_file() {
        vec![path.to_path_buf()]
    } else {
        collect_brief_files(path)
    };

    let ctx_dir = path.parent().unwrap_or(Path::new("."));
    let cwd = std::env::current_dir().unwrap_or_else(|_| ctx_dir.to_path_buf());
    let ctx = checker::CheckContext { file_dir: ctx_dir, cwd: &cwd };

    let mut results = Vec::new();

    for file in &files {
        let Ok(source) = std::fs::read_to_string(file) else { continue };
        let file_str = file.to_string_lossy().into_owned();
        let (tokens, lex_errors) = lex(&source);

        let mut errors: Vec<BriefError> = lex_errors.into_iter()
            .map(|(start, end)| BriefError {
                code:    ErrorCode::ParseError,
                message: format!("unrecognised character at byte {start}–{end}"),
                span:    crate::ast::Span { start, end },
                hint:    None,
            })
            .collect();

        let (program, parse_errors) = parse(&tokens, &source);
        errors.extend(parse_errors);
        errors.extend(checker::check(&program, &ctx));
        results.push((errors, source, file_str));
    }

    results
}

fn collect_brief_files(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else { return vec![] };
    let mut files: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().map_or(false, |e| e == "brief"))
        .collect();
    files.sort();
    files
}

fn clear_terminal() {
    print!("\x1b[2J\x1b[H"); // ANSI clear screen + move cursor to top
}

fn print_check_result(path: &Path, results: &[(Vec<BriefError>, String, String)], prev_hard: Option<usize>) {
    use std::time::SystemTime;

    let ts = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| {
            let secs = d.as_secs();
            let h = (secs / 3600) % 24;
            let m = (secs / 60) % 60;
            let s = secs % 60;
            format!("{h:02}:{m:02}:{s:02}")
        })
        .unwrap_or_default();

    println!("{}", format!("brief watch — {} — {ts}", path.display()).dimmed());
    println!();

    let total_errors: usize = results.iter()
        .flat_map(|(e, _, _)| e.iter())
        .filter(|e| e.is_error())
        .count();

    if total_errors == 0 {
        println!("{}", "✅  No errors.".green().bold());
        // Show warnings if any
        for (errors, source, file) in results {
            let warnings: Vec<_> = errors.iter().filter(|e| e.is_warning()).collect();
            if !warnings.is_empty() {
                print_diagnostics(warnings.iter().copied().cloned().collect::<Vec<_>>().as_slice(), source, file);
            }
        }
    } else {
        for (errors, source, file) in results {
            if errors.iter().any(|e| e.is_error()) {
                print_diagnostics(errors, source, file);
            }
        }
    }

    // Diff vs previous run
    if let Some(prev) = prev_hard {
        println!();
        match total_errors.cmp(&prev) {
            std::cmp::Ordering::Less    =>
                println!("{}", format!("▼ {} error(s) fixed", prev - total_errors).green()),
            std::cmp::Ordering::Greater =>
                println!("{}", format!("▲ {} new error(s)", total_errors - prev).red()),
            std::cmp::Ordering::Equal if total_errors == 0 =>
                println!("{}", "No change — still clean.".dimmed()),
            std::cmp::Ordering::Equal =>
                println!("{}", format!("No change — {total_errors} error(s) remain.").dimmed()),
        }
    }

    println!();
}
