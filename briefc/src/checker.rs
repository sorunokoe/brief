/// Semantic checker for Brief v0.1.
///
/// Validates a parsed `Program` and returns a list of errors/warnings.
/// Phase-1 additions: checks effect decls for duplicate fn names, validates
/// `perform` inside `await`, and validates new declaration forms.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::ast::*;
use crate::errors::{BriefError, ErrorCode};

// ─────────────────────────────────────────────────────────────────────────────

pub struct CheckContext<'a> {
    /// Directory of the `.brief` file being checked — used for skill lookups.
    pub file_dir: &'a Path,
    /// Current working directory — fallback for skill lookups.
    pub cwd:      &'a Path,
}

/// Check a program and return all diagnostics.
pub fn check(program: &Program, ctx: &CheckContext<'_>) -> Vec<BriefError> {
    let mut diags = Vec::new();

    // Build the set of imported skill names for cross-checks.
    let imported: HashSet<&str> = program.imports.iter().map(|i| i.name.as_str()).collect();

    // 1. Validate each skill import (warn if no .briefskill found).
    for import in &program.imports {
        check_skill_import(import, ctx, &mut diags);
    }

    // 2. Validate effect declarations (duplicate fn names).
    for effect in &program.effects {
        check_effect_decl(effect, &mut diags);
    }

    // 3. Validate each task.
    for task in &program.tasks {
        check_task(task, &imported, ctx, &mut diags);
    }

    diags
}

// ─────────────────────────────────────────────────────────────────────────────

fn check_skill_import(import: &SkillImport, ctx: &CheckContext<'_>, diags: &mut Vec<BriefError>) {
    if find_skill_interface(&import.name, ctx).is_none() {
        let skill_path = skill_interface_path_display(&import.name);
        diags.push(BriefError {
            code:    ErrorCode::MissingSkillInterface,
            message: format!("skill '{}' has no interface file", import.name),
            span:    import.span,
            hint:    Some(format!(
                "{skill_path} not found — run: brief skillgen .claude/skills/{}/",
                import.name
            )),
        });
    }
}

fn check_task(task: &Task, imported: &HashSet<&str>, _ctx: &CheckContext<'_>, diags: &mut Vec<BriefError>) {
    // E101: goal is required.
    if task.goal.is_none() {
        diags.push(BriefError {
            code:    ErrorCode::MissingGoal,
            message: format!("task '{}' is missing a `goal` field", task.name),
            span:    task.span,
            hint:    Some(r#"add: goal = "describe what this task accomplishes""#.to_string()),
        });
    }

    // E102: every skill in `uses [...]` must be imported.
    for skill in &task.uses {
        if !imported.contains(skill.as_str()) {
            diags.push(BriefError {
                code:    ErrorCode::UndeclaredSkillInUses,
                message: format!("skill '{}' is in `uses` but never imported", skill),
                span:    task.span,
                hint:    Some(format!(r#"add: import skill "{skill}" at the top of the file"#)),
            });
        }
    }

    // E103/W: validate perform calls inside steps.
    let uses_set: HashSet<&str> = task.uses.iter().map(|s| s.as_str()).collect();
    for step in &task.steps {
        check_step(step, &uses_set, imported, diags);
    }
}

fn check_effect_decl(effect: &EffectDecl, diags: &mut Vec<BriefError>) {
    let mut seen = HashSet::new();
    for f in &effect.fns {
        if !seen.insert(f.name.as_str()) {
            diags.push(BriefError {
                code:    ErrorCode::ParseError,
                message: format!("effect '{}' has duplicate function '{}'", effect.name, f.name),
                span:    f.span,
                hint:    Some("remove the duplicate declaration".to_string()),
            });
        }
    }
}

fn check_step(step: &Step, uses_set: &HashSet<&str>, imported: &HashSet<&str>, diags: &mut Vec<BriefError>) {
    for stmt in &step.body {
        let expr = match stmt {
            Stmt::Let { value, .. }  => value,
            Stmt::Expr { value, .. } => value,
        };
        check_expr_for_perform(expr, uses_set, imported, diags);
    }
}

fn check_expr_for_perform(expr: &Expr, uses_set: &HashSet<&str>, imported: &HashSet<&str>, diags: &mut Vec<BriefError>) {
    match expr {
        Expr::Perform { skill, span, .. } => {
            if !uses_set.contains(skill.as_str()) {
                diags.push(BriefError {
                    code:    ErrorCode::PerformWithoutUses,
                    message: format!("effect '{}' is performed but not declared in `uses [...]`", skill),
                    span:    *span,
                    hint:    Some(format!("add '{skill}' to the task's `uses` clause")),
                });
            }
        }
        // Recurse into `await expr`
        Expr::Await { expr: inner, .. } => {
            check_expr_for_perform(inner, uses_set, imported, diags);
        }
        Expr::Call { args, .. } => {
            for arg in args {
                check_expr_for_perform(arg, uses_set, imported, diags);
            }
        }
        Expr::Ident { .. } | Expr::Str { .. } => {}
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Skill interface resolution
// ─────────────────────────────────────────────────────────────────────────────

/// Returns the path of the `.briefskill` file if it exists, searching in order:
/// 1. `<file_dir>/.claude/skills/<name>/<name>.briefskill`
/// 2. `<cwd>/.claude/skills/<name>/<name>.briefskill`
/// 3. `~/.brief/skills/<name>.briefskill`  (user-global, future)
pub fn find_skill_interface(name: &str, ctx: &CheckContext<'_>) -> Option<PathBuf> {
    let relative = format!(".claude/skills/{name}/{name}.briefskill");

    let candidates = [
        ctx.file_dir.join(&relative),
        ctx.cwd.join(&relative),
    ];

    candidates.into_iter().find(|p| p.exists())
}

fn skill_interface_path_display(name: &str) -> String {
    format!(".claude/skills/{name}/{name}.briefskill")
}

// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::lex;
    use crate::parser::parse;

    fn check_src(src: &str) -> Vec<BriefError> {
        let (tokens, _) = lex(src);
        let (program, _) = parse(&tokens, src);
        let ctx = CheckContext { file_dir: Path::new("."), cwd: Path::new(".") };
        check(&program, &ctx)
    }

    #[test]
    fn no_errors_on_minimal_task() {
        let diags = check_src(r#"task Hello : TaskBrief { goal = "hi" }"#);
        // Only possible warning: no skill imports to check → no errors
        assert!(diags.iter().all(|d| !d.is_error()), "{diags:?}");
    }

    #[test]
    fn error_on_missing_goal() {
        let diags = check_src("task T : TaskBrief {}");
        assert!(diags.iter().any(|d| d.code == ErrorCode::MissingGoal), "{diags:?}");
    }

    #[test]
    fn error_on_undeclared_uses() {
        let diags = check_src(r#"task T : TaskBrief uses [GraphQL] { goal = "x" }"#);
        assert!(diags.iter().any(|d| d.code == ErrorCode::UndeclaredSkillInUses), "{diags:?}");
    }
}
