use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use colored::Colorize;

use crate::ast::{Expr, Step, StepGroup, Stmt, Task};
use crate::lexer::lex;
use crate::parser;
use crate::runner::{self, RunMode};

const COMPILER_BRIEFS: &[&str] = &[
    "compiler/01-lex-pass.brief",
    "compiler/02-parse-pass.brief",
    "compiler/03-check-pass.brief",
    "compiler/04-fmt-pass.brief",
    "compiler/05-hir-pass.brief",
    "compiler/06-pipeline.brief",
];

struct CommandReport {
    exit_code: i32,
    errors: usize,
    warnings: usize,
}

pub fn cmd_check() -> bool {
    let mut valid = 0usize;

    for rel_path in COMPILER_BRIEFS {
        let path = repo_root().join(rel_path);
        let ok = runner::run_file(&path, RunMode::Check { allow_missing_skills: false });
        if ok {
            println!("  {} {}", "✓".green(), rel_path);
            valid += 1;
        } else {
            println!("  {} {}", "✗".red(), rel_path);
        }
    }

    println!();
    println!("{valid}/{} compiler passes valid", COMPILER_BRIEFS.len());
    valid == COMPILER_BRIEFS.len()
}

pub fn cmd_run(source_file: &Path) -> bool {
    let Some((source_file, source_note)) = resolve_existing_source_path(source_file) else {
        let source_file = resolve_source_path(source_file);
        eprintln!("{} source file not found: {}", "error:".red().bold(), source_file.display());
        return false;
    };

    if let Some(note) = source_note {
        println!("{}", note.dimmed());
    }

    let pipeline_path = repo_root().join("compiler/06-pipeline.brief");
    println!("{} validating {}", "self-hosting:".bold(), display_repo_relative(&pipeline_path));
    let ok = runner::run_file(&pipeline_path, RunMode::Check { allow_missing_skills: false });
    if !ok {
        return false;
    }

    println!("{} validating {}", "self-hosting:".bold(), display_repo_relative(&source_file));
    if !validate_source_file(&source_file) {
        return false;
    }

    let Some(task) = load_pipeline_task(&pipeline_path) else {
        eprintln!("{} could not load CompilerPipeline task", "error:".red().bold());
        return false;
    };

    println!();
    println!("{}", "Self-hosting pipeline plan".bold());
    println!("  {:<8} {}", "source:".dimmed(), source_file.display().to_string().cyan());
    println!("  {:<8} {}", "passes:".dimmed(), "lex → parse → check → hir → fmt (fallback: fmt-recovery)".cyan());

    for (index, line) in describe_step_groups(&task, &source_file).into_iter().enumerate() {
        println!("  {:>2}. {}", index + 1, line);
    }

    println!();
    println!(
        "{}",
        "Stage 1 note: this validates compiler/06-pipeline.brief and prints the mediated execution plan; Stage 2 will execute the plan against the source file.".dimmed()
    );
    true
}

pub fn cmd_compare(source_file: &Path) -> bool {
    let Some((source_file, source_note)) = resolve_existing_source_path(source_file) else {
        let source_file = resolve_source_path(source_file);
        eprintln!("{} source file not found: {}", "error:".red().bold(), source_file.display());
        return false;
    };

    if let Some(note) = source_note {
        println!("{}", note.dimmed());
    }

    let rust = match run_cli(&["check", &source_file.display().to_string()]) {
        Some(report) => report,
        None => return false,
    };
    let mediated = match run_cli(&["self-hosting", "run", &source_file.display().to_string()]) {
        Some(report) => report,
        None => return false,
    };

    let matches = rust.exit_code == mediated.exit_code && rust.errors == mediated.errors;
    let verdict = if matches { "MATCH".green().bold() } else { "DIFFER".yellow().bold() };

    println!("{}", "Self-hosting comparison".bold());
    println!("  {:<16} exit={} errors={} warnings={}", "Rust native:".dimmed(), rust.exit_code, rust.errors, rust.warnings);
    println!("  {:<16} exit={} errors={} warnings={}", "Brief-mediated:".dimmed(), mediated.exit_code, mediated.errors, mediated.warnings);
    println!("  {}", verdict);
    println!(
        "{}",
        "Stage 1 note: compare currently validates native `brief check` against the self-hosting pipeline definition and execution plan. Stage 2 will compare fully mediated diagnostics/output.".dimmed()
    );

    matches
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."))
}

fn resolve_source_path(path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(path)
    }
}

fn resolve_existing_source_path(path: &Path) -> Option<(PathBuf, Option<String>)> {
    let resolved = resolve_source_path(path);
    if resolved.is_file() {
        return Some((resolved, None));
    }

    let fallback = find_example_prefix_match(&resolved)?;
    let note = format!(
        "using {} for {}",
        fallback.display(),
        path.display()
    );
    Some((fallback, Some(note)))
}

fn find_example_prefix_match(path: &Path) -> Option<PathBuf> {
    let file_name = path.file_name()?.to_str()?;
    let prefix = file_name.split('-').next()?;
    if prefix.len() != 2 || !prefix.chars().all(|ch| ch.is_ascii_digit()) {
        return None;
    }

    let parent = path.parent()?;
    let prefix = format!("{prefix}-");
    let mut matches: Vec<PathBuf> = fs::read_dir(parent)
        .ok()?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|candidate| {
            candidate.is_file()
                && candidate.extension().and_then(|ext| ext.to_str()) == Some("brief")
                && candidate
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.starts_with(&prefix))
        })
        .collect();
    matches.sort();
    (matches.len() == 1).then(|| matches.remove(0))
}

fn display_repo_relative(path: &Path) -> String {
    path.strip_prefix(repo_root())
        .unwrap_or(path)
        .display()
        .to_string()
}

fn validate_source_file(source_file: &Path) -> bool {
    runner::run_file(source_file, RunMode::Check { allow_missing_skills: false })
}

fn load_pipeline_task(path: &Path) -> Option<Task> {
    let source = fs::read_to_string(path).ok()?;
    let (tokens, lex_errors) = lex(&source);
    if !lex_errors.is_empty() {
        return None;
    }
    let (program, parse_errors) = parser::parse(&tokens, &source);
    if parse_errors.iter().any(|diag| diag.is_error()) {
        return None;
    }
    program.tasks.into_iter().find(|task| task.name == "CompilerPipeline")
}

fn describe_step_groups(task: &Task, source_file: &Path) -> Vec<String> {
    task.step_groups
        .iter()
        .map(|group| match group {
            StepGroup::Sequential(step) => {
                format!("{} — {}", step.name.bold(), describe_step(step, source_file))
            }
            StepGroup::Parallel(steps) => {
                format!("{} {}", "parallel".cyan(), steps.join(", "))
            }
            StepGroup::Retry { count, step } => {
                format!("{} {} ({count} attempts)", "retry".cyan(), step)
            }
            StepGroup::Fallback(steps) => {
                format!("{} {}", "fallback".cyan(), steps.join(" → "))
            }
        })
        .collect()
}

fn describe_step(step: &Step, source_file: &Path) -> String {
    if step.name == "EmitResults" {
        return "compute clean status, exit code, and diagnostic output".to_string();
    }

    if let Some(pass_name) = pass_name_from_calls(step) {
        if step.name.starts_with("Build") {
            return format!("build `{pass_name}` pass spec");
        }
        if step.name.starts_with("Run") {
            return format!("run `{pass_name}` pass on {}", source_file.display());
        }
    }

    if let Some(pass_name) = pass_name_from_step_name(&step.name) {
        if step.name.starts_with("Build") {
            return format!("build `{pass_name}` pass spec");
        }
        if step.name.starts_with("Run") {
            return format!("run `{pass_name}` pass on {}", source_file.display());
        }
    }

    let driver_calls = compiler_driver_calls(step);
    if !driver_calls.is_empty() {
        return format!("CompilerDriver calls: {}", driver_calls.join(", "));
    }

    "execute step".to_string()
}

fn pass_name_from_calls(step: &Step) -> Option<String> {
    for stmt in &step.body {
        match stmt {
            Stmt::Let { value, .. } | Stmt::Expr { value, .. } => {
                if let Some(pass) = pass_name_from_expr(value) {
                    return Some(pass);
                }
            }
        }
    }
    None
}

fn pass_name_from_expr(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Perform { skill, func, args, .. }
            if skill == "CompilerDriver" && func == "buildPassSpec" =>
        {
            match args.first() {
                Some(Expr::Str { value, .. }) => Some(value.clone()),
                _ => None,
            }
        }
        Expr::Await { expr, .. } => pass_name_from_expr(expr),
        Expr::Call { args, .. } => args.iter().find_map(pass_name_from_expr),
        Expr::Match { scrutinee, arms } => pass_name_from_expr(scrutinee)
            .or_else(|| arms.iter().find_map(|arm| pass_name_from_expr(&arm.body))),
        _ => None,
    }
}

fn compiler_driver_calls(step: &Step) -> Vec<String> {
    let mut calls = Vec::new();
    for stmt in &step.body {
        match stmt {
            Stmt::Let { value, .. } | Stmt::Expr { value, .. } => collect_driver_calls(value, &mut calls),
        }
    }
    calls
}

fn collect_driver_calls(expr: &Expr, calls: &mut Vec<String>) {
    match expr {
        Expr::Perform { skill, func, .. } if skill == "CompilerDriver" => calls.push(func.clone()),
        Expr::Perform { args, .. } | Expr::Call { args, .. } => {
            for arg in args {
                collect_driver_calls(arg, calls);
            }
        }
        Expr::Await { expr, .. } => collect_driver_calls(expr, calls),
        Expr::Match { scrutinee, arms } => {
            collect_driver_calls(scrutinee, calls);
            for arm in arms {
                collect_driver_calls(&arm.body, calls);
            }
        }
        _ => {}
    }
}

fn pass_name_from_step_name(step_name: &str) -> Option<&'static str> {
    match step_name {
        "BuildLexSpec" | "RunLex" => Some("lex"),
        "BuildParseSpec" | "RunParse" => Some("parse"),
        "RunCheck" => Some("check"),
        "RunHir" => Some("hir"),
        "RunFormat" => Some("fmt"),
        "RunFormatRecovery" => Some("fmt-recovery"),
        _ => None,
    }
}

fn run_cli(args: &[&str]) -> Option<CommandReport> {
    let output = match Command::new(std::env::current_exe().ok()?)
        .current_dir(repo_root())
        .args(args)
        .output()
    {
        Ok(output) => output,
        Err(err) => {
            eprintln!("{} failed to run `{}`: {}", "error:".red().bold(), args.join(" "), err);
            return None;
        }
    };

    let stderr = String::from_utf8_lossy(&output.stderr);
    Some(CommandReport {
        exit_code: output.status.code().unwrap_or(1),
        errors: count_diagnostics(&stderr, "error["),
        warnings: count_diagnostics(&stderr, "warning["),
    })
}

fn count_diagnostics(output: &str, marker: &str) -> usize {
    output.lines().filter(|line| line.starts_with(marker)).count()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static NEXT_FILE_ID: AtomicUsize = AtomicUsize::new(0);

    struct TestFile {
        path: PathBuf,
    }

    impl TestFile {
        fn path(&self) -> &Path {
            &self.path
        }
    }

    impl Drop for TestFile {
        fn drop(&mut self) {
            let _ = fs::remove_file(&self.path);
        }
    }

    fn example_path(name: &str) -> PathBuf {
        repo_root().join("examples").join(name)
    }

    fn write_test_file(stem: &str, source: &str) -> TestFile {
        let dir = repo_root().join("target/selfhosting-tests");
        fs::create_dir_all(&dir).expect("create selfhosting test dir");

        let id = NEXT_FILE_ID.fetch_add(1, Ordering::Relaxed);
        let path = dir.join(format!("{stem}-{id}.brief"));
        fs::write(&path, source).expect("write selfhosting test source");

        TestFile { path }
    }

    #[test]
    fn test_all_compiler_passes_check() {
        assert!(cmd_check());
    }

    #[test]
    fn test_compare_clean_file_matches() {
        let requested = example_path("01-book-flight.brief");
        let (resolved, _) = resolve_existing_source_path(&requested).expect("resolve 01-* example");

        assert!(validate_source_file(&resolved));
        assert!(cmd_run(&requested));
    }

    #[test]
    fn test_compare_broken_file_both_fail() {
        let bad_file = write_test_file("brief-test-bad", "task Broken {\n  ???\n}\n");

        assert!(!validate_source_file(bad_file.path()));
        assert!(!cmd_run(bad_file.path()));
    }

    #[test]
    fn test_format_recovered_is_degraded() {
        assert_eq!(COMPILER_BRIEFS.len(), 6);

        let fmt_pass = fs::read_to_string(repo_root().join("compiler/04-fmt-pass.brief"))
            .expect("read formatter pass spec");
        assert!(fmt_pass.contains("FormatRecovered = comment trivia lost"));
        assert!(fmt_pass.contains("degraded, not green"));
    }

    #[test]
    fn test_run_mode_does_not_panic() {
        let path = example_path("02-review-pr.brief");
        assert!(path.exists(), "expected canonical example to exist");
        assert!(cmd_run(&path));
    }
}
