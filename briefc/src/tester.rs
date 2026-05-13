/// `brief test` — test runner for Brief files.
///
/// Looks for `test { }` blocks in `.brief` files. Each test block can declare
/// `mock { }` overrides for skills, then call `perform` and assert results.
///
/// ## Syntax
///
/// ```brief
/// test "fetches user and renders card" {
///     mock GraphQL {
///         fn query(op) -> Ok(User { id: "u1", name: "Ada" })
///     }
///     mock DesignSystem {
///         fn profileCard(user) -> Ok(Component { id: "card" })
///     }
///
///     run ProfileScreen.FetchData
///     assert performed GraphQL.query
///     assert performed DesignSystem.profileCard
/// }
/// ```
///
/// ## Test-only syntax additions (parsed in test mode)
///
/// - `test "<name>" { ... }` — a named test case
/// - `mock <SkillName> { fn <name>(...) -> <expr> }` — skill stub override
/// - `run <TaskName>` or `run <TaskName>.<StepName>` — execute a task/step
/// - `assert performed <Skill>.<fn>` — verify a skill call was made
/// - `assert not performed <Skill>.<fn>` — verify a skill call was NOT made
/// - `assert result is Ok` / `assert result is Err` — check last return value

use std::collections::HashMap;
use std::path::Path;

use colored::Colorize;

use crate::ast::*;
use crate::checker::{self, CheckContext};
use crate::errors::{print_diagnostics, BriefError};
use crate::lexer::lex;
use crate::parser::parse;
use crate::skillgen;
use crate::typeck;

// ─────────────────────────────────────────────────────────────────────────────
// Test AST (lightweight, parsed from the source file)
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct TestBlock {
    pub name:     String,
    pub mocks:    Vec<MockDecl>,
    pub commands: Vec<TestCmd>,
    #[allow(dead_code)]
    pub span:     Span,
}

#[derive(Debug, Clone)]
pub struct MockDecl {
    pub skill: String,
    pub fns:   Vec<MockFn>,
}

#[derive(Debug, Clone)]
pub struct MockFn {
    pub name:        String,
    pub return_expr: String, // simplified: raw string value
}

#[derive(Debug, Clone)]
pub enum TestCmd {
    Run  { task: String, step: Option<String> },
    AssertPerformed     { skill: String, func: String },
    AssertNotPerformed  { skill: String, func: String },
    AssertResultOk,
    AssertResultErr,
}

// ─────────────────────────────────────────────────────────────────────────────
// Runtime state
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Default)]
struct TestRuntime {
    /// Record of all `perform Skill.fn(args)` calls made during this test.
    performed: Vec<(String, String)>, // (skill, func)
    /// Mock skill return values: (skill, func) → return string
    mocks: HashMap<(String, String), String>,
    /// Last result of a perform call.
    last_result: Option<String>,
}

impl TestRuntime {
    fn install_mocks(&mut self, mocks: &[MockDecl]) {
        for m in mocks {
            for f in &m.fns {
                self.mocks.insert((m.skill.clone(), f.name.clone()), f.return_expr.clone());
            }
        }
    }

    fn perform(&mut self, skill: &str, func: &str) -> String {
        self.performed.push((skill.to_string(), func.to_string()));
        let key = (skill.to_string(), func.to_string());
        let result = self.mocks.get(&key)
            .cloned()
            .unwrap_or_else(|| format!("Ok(<mock {skill}.{func}>)"));
        self.last_result = Some(result.clone());
        result
    }

    fn was_performed(&self, skill: &str, func: &str) -> bool {
        self.performed.iter().any(|(s, f)| s == skill && f == func)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Test extraction from Brief source
// ─────────────────────────────────────────────────────────────────────────────

/// Extract `test "..." { }` blocks from the raw source using a simple line-based parser.
/// Full parser integration is Phase 2; this covers the common patterns.
fn extract_tests(source: &str) -> Vec<TestBlock> {
    let mut tests = Vec::new();
    // Collect into a Vec so we can use a manual cursor (avoids double borrow in nested loops).
    let all_lines: Vec<(usize, &str)> = source.lines().enumerate().collect();
    let mut cursor = 0usize;

    while cursor < all_lines.len() {
        let (line_idx, line) = all_lines[cursor];
        let trimmed = line.trim();
        cursor += 1;

        // Match: test "name" {
        if !trimmed.starts_with("test ") || !trimmed.contains('"') { continue; }
        let after_test = trimmed.trim_start_matches("test").trim();
        if !after_test.starts_with('"') { continue; }
        let end_quote = after_test[1..].find('"').map(|i| i + 1);
        let name = match end_quote {
            Some(eq) => after_test[1..eq].to_string(),
            None     => continue,
        };

        let mut mocks: Vec<MockDecl> = Vec::new();
        let mut commands: Vec<TestCmd> = Vec::new();
        let mut depth = if trimmed.ends_with('{') { 1u32 } else { 0 };

        while cursor < all_lines.len() {
            let (_, inner) = all_lines[cursor];
            let t = inner.trim();
            cursor += 1;

            if t.contains('{') { depth += t.chars().filter(|&c| c == '{').count() as u32; }
            if t.contains('}') {
                let close = t.chars().filter(|&c| c == '}').count() as u32;
                if close >= depth { break; }
                depth -= close;
            }

            // mock <Skill> { ... } — collect until closing brace
            if t.starts_with("mock ") && t.ends_with('{') {
                let skill = t["mock ".len()..].trim_end_matches('{').trim().to_string();
                let mut fns = Vec::new();
                while cursor < all_lines.len() {
                    let (_, mline) = all_lines[cursor];
                    let mt = mline.trim();
                    cursor += 1;
                    if mt == "}" { break; }
                    // fn name(...) -> value
                    if mt.starts_with("fn ") {
                        if let Some((sig, ret)) = mt.split_once("->") {
                            let fname = sig["fn ".len()..]
                                .split('(').next().unwrap_or("").trim().to_string();
                            fns.push(MockFn {
                                name:        fname,
                                return_expr: ret.trim().to_string(),
                            });
                        }
                    }
                }
                mocks.push(MockDecl { skill, fns });
                depth -= 1; // the mock block consumed its own }
                continue;
            }

            // run <Task> or run <Task>.<Step>
            if t.starts_with("run ") {
                let target = t["run ".len()..].trim();
                if let Some((task, step)) = target.split_once('.') {
                    commands.push(TestCmd::Run { task: task.to_string(), step: Some(step.to_string()) });
                } else {
                    commands.push(TestCmd::Run { task: target.to_string(), step: None });
                }
                continue;
            }

            // assert performed / assert not performed
            if t.starts_with("assert performed ") {
                let callsite = t["assert performed ".len()..].trim();
                if let Some((sk, fn_)) = callsite.split_once('.') {
                    commands.push(TestCmd::AssertPerformed { skill: sk.to_string(), func: fn_.to_string() });
                }
                continue;
            }
            if t.starts_with("assert not performed ") {
                let callsite = t["assert not performed ".len()..].trim();
                if let Some((sk, fn_)) = callsite.split_once('.') {
                    commands.push(TestCmd::AssertNotPerformed { skill: sk.to_string(), func: fn_.to_string() });
                }
                continue;
            }
            if t == "assert result is Ok"  { commands.push(TestCmd::AssertResultOk);  continue; }
            if t == "assert result is Err" { commands.push(TestCmd::AssertResultErr); continue; }
        }

        tests.push(TestBlock {
            name,
            mocks,
            commands,
            span: Span::new(line_idx * 80, (line_idx + 1) * 80), // approximate
        });
    }

    tests
}

// ─────────────────────────────────────────────────────────────────────────────
// Test execution
// ─────────────────────────────────────────────────────────────────────────────

fn run_test(test: &TestBlock, program: &Program) -> (bool, Vec<String>) {
    let mut rt  = TestRuntime::default();
    rt.install_mocks(&test.mocks);
    let mut failures = Vec::new();

    for cmd in &test.commands {
        match cmd {
            TestCmd::Run { task, step } => {
                if let Some(t) = program.tasks.iter().find(|t| &t.name == task) {
                    let steps_to_run: Vec<&Step> = if let Some(sname) = step {
                        t.steps.iter().filter(|s| &s.name == sname).collect()
                    } else {
                        t.steps.iter().collect()
                    };
                    for s in steps_to_run {
                        run_step(s, &mut rt);
                    }
                } else {
                    failures.push(format!("run: task '{}' not found in brief", task));
                }
            }

            TestCmd::AssertPerformed { skill, func } => {
                if !rt.was_performed(skill, func) {
                    failures.push(format!(
                        "assert performed {skill}.{func} — FAILED (never called)"
                    ));
                }
            }

            TestCmd::AssertNotPerformed { skill, func } => {
                if rt.was_performed(skill, func) {
                    failures.push(format!(
                        "assert not performed {skill}.{func} — FAILED (was called)"
                    ));
                }
            }

            TestCmd::AssertResultOk => {
                match rt.last_result.as_deref() {
                    Some(r) if r.starts_with("Ok") => {}
                    Some(r) => failures.push(format!("assert result is Ok — FAILED (got {r})")),
                    None    => failures.push("assert result is Ok — FAILED (no perform was called)".into()),
                }
            }

            TestCmd::AssertResultErr => {
                match rt.last_result.as_deref() {
                    Some(r) if r.starts_with("Err") => {}
                    Some(r) => failures.push(format!("assert result is Err — FAILED (got {r})")),
                    None    => failures.push("assert result is Err — FAILED (no perform was called)".into()),
                }
            }
        }
    }

    (failures.is_empty(), failures)
}

fn run_step(step: &Step, rt: &mut TestRuntime) {
    for stmt in &step.body {
        let expr = match stmt {
            Stmt::Let  { value, .. } => value,
            Stmt::Expr { value, .. } => value,
        };
        run_expr(expr, rt);
    }
}

fn run_expr(expr: &Expr, rt: &mut TestRuntime) {
    match expr {
        Expr::Perform { skill, func, .. } => { rt.perform(skill, func); }
        Expr::Await   { expr: inner, .. } => run_expr(inner, rt),
        _ => {}
    }
}

/// Strip `test "..." { ... }` blocks from source so the parser (which doesn't
/// know about test syntax) only sees the task declarations.
/// Replaces each test block with blank lines (preserving line numbers for errors).
fn strip_test_blocks(source: &str) -> String {
    let lines: Vec<&str> = source.lines().collect();
    let mut out = Vec::with_capacity(lines.len());
    let mut cursor = 0usize;

    while cursor < lines.len() {
        let t = lines[cursor].trim();
        if t.starts_with("test ") && t.contains('"') {
            // Emit a blank line for the opening `test "..." {`
            out.push(String::new());
            cursor += 1;
            let mut depth = if t.ends_with('{') { 1u32 } else { 0 };
            while cursor < lines.len() {
                let inner = lines[cursor].trim();
                if inner.contains('{') {
                    depth += inner.chars().filter(|&c| c == '{').count() as u32;
                }
                if inner.contains('}') {
                    let close = inner.chars().filter(|&c| c == '}').count() as u32;
                    out.push(String::new()); // blank line in place of test body
                    cursor += 1;
                    if close >= depth { break; }
                    depth -= close;
                    continue;
                }
                out.push(String::new());
                cursor += 1;
            }
        } else {
            out.push(lines[cursor].to_string());
            cursor += 1;
        }
    }

    out.join("\n")
}


/// Run all `test { }` blocks found in a `.brief` file.
/// Returns `true` if all tests passed (or there were no tests).
pub fn test_file(path: &Path, verbose: bool) -> bool {
    // ── 1. Read source ────────────────────────────────────────────────────
    let source = match std::fs::read_to_string(path) {
        Ok(s)  => s,
        Err(e) => {
            eprintln!("{}: cannot read {}: {e}", "error".red().bold(), path.display());
            return false;
        }
    };

    let file_str = path.to_string_lossy().to_string();

    // ── 2. Strip test blocks so the parser only sees tasks/types ──────────
    let parse_source = strip_test_blocks(&source);

    // ── 3. Lex + Parse ────────────────────────────────────────────────────
    let (tokens, _)            = lex(&parse_source);
    let (program, parse_errors) = parse(&tokens, &parse_source);

    if parse_errors.iter().any(|e| e.is_error()) {
        print_diagnostics(&parse_errors, &parse_source, &file_str);
        eprintln!("{} Cannot run tests — fix parse errors first.", "error".red().bold());
        return false;
    }

    // ── 3. Type check ─────────────────────────────────────────────────────
    let file_dir = path.parent().unwrap_or(Path::new("."));
    let cwd      = std::env::current_dir().unwrap_or_default();
    let ctx      = CheckContext { file_dir, cwd: &cwd };
    let mut diags: Vec<BriefError> = parse_errors;
    diags.extend(checker::check(&program, &ctx));
    let skill_ifaces = {
        let mut ifaces = HashMap::new();
        for import in &program.imports {
            let name = &import.name;
            let rel  = format!(".claude/skills/{name}/{name}.briefskill");
            for base in &[file_dir, cwd.as_path()] {
                let p = base.join(&rel);
                if p.exists() {
                    if let Ok(content) = std::fs::read_to_string(&p) {
                        if let Some(iface) = skillgen::parse_briefskill(&content) {
                            ifaces.insert(name.clone(), iface);
                            break;
                        }
                    }
                }
            }
        }
        ifaces
    };
    diags.extend(typeck::type_check_with_skills(&program, skill_ifaces));
    if diags.iter().any(|d| d.is_error()) {
        print_diagnostics(&diags, &parse_source, &file_str);
        eprintln!("{} Cannot run tests — fix type errors first.", "error".red().bold());
        return false;
    }

    // ── 4. Extract tests ──────────────────────────────────────────────────
    let tests = extract_tests(&source);
    if tests.is_empty() {
        println!("{} No tests found in {}.", "⚠".yellow().bold(), path.display());
        println!("  Add test blocks:");
        println!("  {}", r#"test "my test" {"#.cyan());
        println!("  {}",  "    run MyTask".cyan());
        println!("  {}",  "    assert performed Skill.fn".cyan());
        println!("  {}", "}".cyan());
        return true;
    }

    // ── 5. Run tests ──────────────────────────────────────────────────────
    let total  = tests.len();
    let mut passed = 0usize;
    let mut failed = 0usize;

    println!();
    println!("{} {} in {}", "Running".bold(), pluralize(total, "test"), path.display());
    println!();

    for test in &tests {
        let (ok, failures) = run_test(test, &program);
        if ok {
            println!("  {} {}", "✅".green(), test.name);
            passed += 1;
        } else {
            println!("  {} {}", "❌".red(), test.name);
            for f in &failures {
                println!("       {}", f.red());
            }
            failed += 1;
        }
        if verbose {
            // Show the perform call log
            let mut rt = TestRuntime::default();
            rt.install_mocks(&test.mocks);
            for cmd in &test.commands {
                if let TestCmd::Run { task, step } = cmd {
                    if let Some(t) = program.tasks.iter().find(|t| &t.name == task) {
                        let steps: Vec<&Step> = if let Some(sn) = step {
                            t.steps.iter().filter(|s| &s.name == sn).collect()
                        } else {
                            t.steps.iter().collect()
                        };
                        for s in steps { run_step(s, &mut rt); }
                    }
                }
            }
            if !rt.performed.is_empty() {
                println!("       {} calls: {}",
                    "perform".dimmed(),
                    rt.performed.iter().map(|(s,f)| format!("{s}.{f}")).collect::<Vec<_>>().join(", ").dimmed()
                );
            }
        }
    }

    // ── 6. Summary ────────────────────────────────────────────────────────
    println!();
    let summary = format!(
        "{} tests, {} passed, {} failed",
        total, passed, failed
    );
    if failed == 0 {
        println!("{} {}", "✅".green(), summary.bold());
    } else {
        println!("{} {}", "❌".red(), summary.bold());
    }

    failed == 0
}

fn pluralize(n: usize, singular: &str) -> String {
    if n == 1 { format!("{n} {singular}") } else { format!("{n} {singular}s") }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::lex;
    use crate::parser::parse;

    fn parse_program(src: &str) -> Program {
        let (tokens, _) = lex(src);
        let (prog, _)   = parse(&tokens, src);
        prog
    }

    const FIXTURE: &str = r#"
import skill "GraphQL"
import skill "DesignSystem"

task ProfileScreen : TaskBrief uses [GraphQL, DesignSystem] {
    goal = "show profile"
    step FetchData {
        let user = perform GraphQL.query(Q)?;
    }
    step Render {
        let card = perform DesignSystem.profileCard(user)?;
    }
}

test "fetch step calls GraphQL.query" {
    mock GraphQL {
        fn query(op) -> Ok(User)
    }
    run ProfileScreen.FetchData
    assert performed GraphQL.query
    assert not performed DesignSystem.profileCard
}

test "full task calls both skills" {
    mock GraphQL {
        fn query(op) -> Ok(User)
    }
    mock DesignSystem {
        fn profileCard(user) -> Ok(Card)
    }
    run ProfileScreen
    assert performed GraphQL.query
    assert performed DesignSystem.profileCard
}

test "result check" {
    mock GraphQL {
        fn query(op) -> Ok(User)
    }
    run ProfileScreen.FetchData
    assert result is Ok
}
"#;

    #[test]
    fn test_extract_three_tests() {
        let tests = extract_tests(FIXTURE);
        assert_eq!(tests.len(), 3);
        assert_eq!(tests[0].name, "fetch step calls GraphQL.query");
        assert_eq!(tests[1].name, "full task calls both skills");
        assert_eq!(tests[2].name, "result check");
    }

    #[test]
    fn test_mock_decl_extracted() {
        let tests = extract_tests(FIXTURE);
        assert_eq!(tests[0].mocks.len(), 1);
        assert_eq!(tests[0].mocks[0].skill, "GraphQL");
        assert_eq!(tests[0].mocks[0].fns[0].name, "query");
        assert_eq!(tests[0].mocks[0].fns[0].return_expr, "Ok(User)");
    }

    #[test]
    fn test_run_cmd_step_only() {
        let tests = extract_tests(FIXTURE);
        assert!(matches!(&tests[0].commands[0], TestCmd::Run { task, step: Some(s) }
            if task == "ProfileScreen" && s == "FetchData"));
    }

    #[test]
    fn test_assert_performed_pass() {
        let prog  = parse_program(FIXTURE);
        let tests = extract_tests(FIXTURE);
        let (ok, _) = run_test(&tests[0], &prog);
        assert!(ok, "fetch step should call GraphQL.query");
    }

    #[test]
    fn test_assert_not_performed_pass() {
        let prog  = parse_program(FIXTURE);
        let tests = extract_tests(FIXTURE);
        let (ok, _) = run_test(&tests[0], &prog);
        assert!(ok, "fetch step should NOT call DesignSystem.profileCard");
    }

    #[test]
    fn test_full_task_both_skills() {
        let prog  = parse_program(FIXTURE);
        let tests = extract_tests(FIXTURE);
        let (ok, failures) = run_test(&tests[1], &prog);
        assert!(ok, "failures: {:?}", failures);
    }

    #[test]
    fn test_result_ok_assertion() {
        let prog  = parse_program(FIXTURE);
        let tests = extract_tests(FIXTURE);
        let (ok, failures) = run_test(&tests[2], &prog);
        assert!(ok, "failures: {:?}", failures);
    }

    #[test]
    fn test_no_tests_empty_source() {
        let tests = extract_tests("task Hello : TaskBrief { goal = \"hi\" }");
        assert!(tests.is_empty());
    }
}
