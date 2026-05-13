/// Semantic checker for Brief v0.1 + Phase 3 power types.
///
/// Phase 3 additions:
/// - Linear bindings: `@once let x = ...` — E104 if reused, E105 if dropped
/// - Effect function return linearity: `fn charge() -> @once Handle` auto-marks
///   the bound variable as linear at every call site
/// - Effect group aliases: `type AuthEffects = [Auth, Session]` expanded in `uses`
/// - Type alias validation: `type Email = @matches("...") String` resolved in structs

use std::collections::{HashMap, HashSet};
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

    // Build effect group map: group name → expanded member names.
    let groups: HashMap<&str, Vec<&str>> = program.effect_groups
        .iter()
        .map(|g| (g.name.as_str(), g.members.iter().map(|m| m.as_str()).collect()))
        .collect();

    // Build inline effect fn map: (effect_name, fn_name) → ret_attrs
    // Used to auto-detect @once return types without explicit annotation at call site.
    let inline_effects: HashMap<(&str, &str), &[Attribute]> = program.effects.iter()
        .flat_map(|e| e.fns.iter().map(move |f| ((e.name.as_str(), f.name.as_str()), f.ret_attrs.as_slice())))
        .collect();

    // 1. Validate each skill import (warn if no .briefskill found).
    for import in &program.imports {
        check_skill_import(import, ctx, &mut diags);
    }

    // 2. Validate effect declarations (duplicate fn names).
    for effect in &program.effects {
        check_effect_decl(effect, &mut diags);
    }

    // 3. Validate type aliases (base type must be a primitive or known type).
    for alias in &program.type_aliases {
        check_type_alias(alias, &mut diags);
    }

    // 4. Validate each task (with group expansion and linear binding tracking).
    for task in &program.tasks {
        check_task(task, &imported, &groups, &inline_effects, ctx, &mut diags);
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

fn check_type_alias(alias: &TypeAliasDecl, diags: &mut Vec<BriefError>) {
    // Basic validation: base must not be empty.
    if alias.base.name.is_empty() {
        diags.push(BriefError {
            code:    ErrorCode::UnknownType,
            message: format!("type alias '{}' has no base type", alias.name),
            span:    alias.span,
            hint:    Some(format!(r#"example: type {} = @nonEmpty String"#, alias.name)),
        });
    }
    // Known refinement attributes for type aliases.
    const KNOWN_ATTRS: &[&str] = &["url", "nonEmpty", "matches", "once", "affine", "mcp"];
    for attr in &alias.attrs {
        if !KNOWN_ATTRS.contains(&attr.name.as_str()) {
            diags.push(BriefError {
                code:    ErrorCode::AttributeConstraint,
                message: format!("unknown attribute '@{}' in type alias '{}'", attr.name, alias.name),
                span:    attr.span,
                hint:    Some(format!("known attributes: {}", KNOWN_ATTRS.join(", "))),
            });
        }
    }
}

fn check_task(
    task:           &Task,
    imported:       &HashSet<&str>,
    groups:         &HashMap<&str, Vec<&str>>,
    inline_effects: &HashMap<(&str, &str), &[Attribute]>,
    _ctx:           &CheckContext<'_>,
    diags:          &mut Vec<BriefError>,
) {
    // E101: goal is required.
    if task.goal.is_none() {
        diags.push(BriefError {
            code:    ErrorCode::MissingGoal,
            message: format!("task '{}' is missing a `goal` field", task.name),
            span:    task.span,
            hint:    Some(r#"add: goal = "describe what this task accomplishes""#.to_string()),
        });
    }

    // Expand effect group aliases in `uses [...]`.
    let mut expanded_uses: Vec<&str> = Vec::new();
    for skill in &task.uses {
        if let Some(members) = groups.get(skill.as_str()) {
            // Expand the group to its members.
            expanded_uses.extend(members.iter().copied());
        } else if groups.is_empty() || !groups.contains_key(skill.as_str()) {
            expanded_uses.push(skill.as_str());
        }
    }

    // E102: every skill in `uses [...]` must be imported (after expansion).
    for skill in &expanded_uses {
        if !imported.contains(skill) {
            diags.push(BriefError {
                code:    ErrorCode::UndeclaredSkillInUses,
                message: format!("skill '{skill}' is in `uses` but never imported"),
                span:    task.span,
                hint:    Some(format!(r#"add: import skill "{skill}" at the top of the file"#)),
            });
        }
    }

    // E103: validate perform calls inside steps.
    let uses_set: HashSet<&str> = expanded_uses.iter().copied().collect();
    for step in &task.steps {
        check_step(step, &uses_set, imported, inline_effects, diags);
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

fn check_step(
    step:           &Step,
    uses_set:       &HashSet<&str>,
    imported:       &HashSet<&str>,
    inline_effects: &HashMap<(&str, &str), &[Attribute]>,
    diags:          &mut Vec<BriefError>,
) {
    // Collect linear bindings: (name, span, use_count)
    // A binding is linear if:
    //   a) its let statement has `@once` in attrs, OR
    //   b) it is bound to a `perform Skill.fn()` whose inline effect declares `@once` return
    let mut linear: HashMap<String, (Span, usize)> = HashMap::new();

    for stmt in &step.body {
        check_expr_for_perform(stmt_value(stmt), uses_set, imported, diags);

        match stmt {
            Stmt::Let { attrs, name, value, span } => {
                // Explicit `@once let x = ...`
                let is_once = attrs.iter().any(|a| a == "once" || a == "affine");
                // Auto-infer from inline effect declaration.
                let auto_once = match value {
                    Expr::Perform { skill, func, .. } => {
                        let key = (skill.as_str(), func.as_str());
                        inline_effects.get(&key)
                            .map(|ret_attrs| ret_attrs.iter().any(|a| a.name == "once" || a.name == "affine"))
                            .unwrap_or(false)
                    }
                    _ => false,
                };
                if is_once || auto_once {
                    linear.insert(name.clone(), (*span, 0));
                }
            }
            Stmt::Expr { .. } => {}
        }
    }

    // Count usages of each linear binding across all expressions in the step.
    for stmt in &step.body {
        count_ident_uses(stmt_value(stmt), &mut linear);
    }

    // Emit E104 / E105 for linear binding violations.
    for (name, (span, uses)) in &linear {
        if *uses == 0 {
            diags.push(BriefError {
                code:    ErrorCode::LinearBindingDropped,
                message: format!("linear binding '{name}' is never consumed"),
                span:    *span,
                hint:    Some(format!("use '{name}' in a subsequent statement or remove the `@once` annotation")),
            });
        } else if *uses > 1 {
            diags.push(BriefError {
                code:    ErrorCode::LinearBindingReused,
                message: format!("linear binding '{name}' is consumed {uses} times (must be exactly once)"),
                span:    *span,
                hint:    Some(format!("a `@once` binding may only be used once — split into separate `perform` calls")),
            });
        }
    }
}

fn stmt_value(stmt: &Stmt) -> &Expr {
    match stmt {
        Stmt::Let { value, .. }  => value,
        Stmt::Expr { value, .. } => value,
    }
}

/// Recursively count how many times each linear binding name appears as an Ident.
fn count_ident_uses(expr: &Expr, linear: &mut HashMap<String, (Span, usize)>) {
    match expr {
        Expr::Ident { name, .. } => {
            if let Some((_, count)) = linear.get_mut(name) {
                *count += 1;
            }
        }
        Expr::Perform { args, .. } => {
            for a in args { count_ident_uses(a, linear); }
        }
        Expr::Call { args, .. } => {
            for a in args { count_ident_uses(a, linear); }
        }
        Expr::Await { expr: inner, .. } => count_ident_uses(inner, linear),
        Expr::Str { .. } => {}
    }
}

fn check_expr_for_perform(expr: &Expr, uses_set: &HashSet<&str>, _imported: &HashSet<&str>, diags: &mut Vec<BriefError>) {
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
        Expr::Await { expr: inner, .. } => {
            check_expr_for_perform(inner, uses_set, _imported, diags);
        }
        Expr::Call { args, .. } => {
            for arg in args {
                check_expr_for_perform(arg, uses_set, _imported, diags);
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

    // ── Phase 3: linear bindings ────────────────────────────────────────────

    #[test]
    fn linear_binding_dropped() {
        // @once let x = ... but x is never used → E105
        let diags = check_src(r#"
            import skill "Payment"
            task Pay : TaskBrief uses [Payment] {
                goal = "charge"
                step Charge {
                    @once let handle = perform Payment.charge(amount)?;
                }
            }
        "#);
        assert!(diags.iter().any(|d| d.code == ErrorCode::LinearBindingDropped), "{diags:?}");
    }

    #[test]
    fn linear_binding_reused() {
        // @once let x = ... but x is used twice → E104
        let diags = check_src(r#"
            import skill "Payment"
            task Pay : TaskBrief uses [Payment] {
                goal = "charge"
                step Charge {
                    @once let handle = perform Payment.charge(amount)?;
                    let a = perform Payment.confirm(handle)?;
                    let b = perform Payment.confirm(handle)?;
                }
            }
        "#);
        assert!(diags.iter().any(|d| d.code == ErrorCode::LinearBindingReused), "{diags:?}");
    }

    #[test]
    fn linear_binding_used_once_is_ok() {
        // @once let x = ... and x used exactly once → no linear error
        let diags = check_src(r#"
            import skill "Payment"
            task Pay : TaskBrief uses [Payment] {
                goal = "charge"
                step Charge {
                    @once let handle = perform Payment.charge(amount)?;
                    let receipt = perform Payment.confirm(handle)?;
                }
            }
        "#);
        let linear_errors: Vec<_> = diags.iter()
            .filter(|d| d.code == ErrorCode::LinearBindingDropped || d.code == ErrorCode::LinearBindingReused)
            .collect();
        assert!(linear_errors.is_empty(), "unexpected linear errors: {linear_errors:?}");
    }

    // ── Phase 3: effect group aliases ────────────────────────────────────────

    #[test]
    fn effect_group_expands_in_uses() {
        // `type AuthEffects = [Auth, Session]` — if Auth/Session imported, no E102
        let diags = check_src(r#"
            import skill "Auth"
            import skill "Session"
            type AuthEffects = [Auth, Session]
            task Login : TaskBrief uses [AuthEffects] {
                goal = "login"
                step Do {
                    let tok = perform Auth.login(user)?;
                    let s   = perform Session.create(tok)?;
                }
            }
        "#);
        let e102: Vec<_> = diags.iter().filter(|d| d.code == ErrorCode::UndeclaredSkillInUses).collect();
        assert!(e102.is_empty(), "unexpected E102: {e102:?}");
    }

    // ── Phase 3: type alias validation ──────────────────────────────────────

    #[test]
    fn type_alias_unknown_attr_is_warned() {
        let diags = check_src(r#"type Email = @bogusAttr String"#);
        assert!(diags.iter().any(|d| d.code == ErrorCode::AttributeConstraint), "{diags:?}");
    }

    #[test]
    fn type_alias_valid_no_error() {
        let diags = check_src(r#"type Email = @matches("[^@]+@[^@]+") String"#);
        let errors: Vec<_> = diags.iter().filter(|d| d.is_error()).collect();
        assert!(errors.is_empty(), "unexpected errors: {errors:?}");
    }

    // ── Effect fn @once return type auto-marks binding ───────────────────────

    #[test]
    fn inline_effect_once_return_auto_linear() {
        // Inline effect declares `fn charge() -> @once Handle`
        // → the bound variable is auto-linear without explicit `@once let`
        let diags = check_src(r#"
            import skill "Payment"
            effect Payment {
                fn charge(amount: String) -> @once Handle
                fn confirm(h: Handle) -> String
            }
            task Pay : TaskBrief uses [Payment] {
                goal = "pay"
                step Do {
                    let handle = perform Payment.charge(amount)?;
                    let r = perform Payment.confirm(handle)?;
                }
            }
        "#);
        let linear_errors: Vec<_> = diags.iter()
            .filter(|d| d.code == ErrorCode::LinearBindingDropped || d.code == ErrorCode::LinearBindingReused)
            .collect();
        assert!(linear_errors.is_empty(), "unexpected linear errors: {linear_errors:?}");
    }

    #[test]
    fn inline_effect_once_return_dropped_is_error() {
        let diags = check_src(r#"
            import skill "Payment"
            effect Payment {
                fn charge(amount: String) -> @once Handle
            }
            task Pay : TaskBrief uses [Payment] {
                goal = "pay"
                step Do {
                    let handle = perform Payment.charge(amount)?;
                }
            }
        "#);
        assert!(diags.iter().any(|d| d.code == ErrorCode::LinearBindingDropped), "{diags:?}");
    }

    // ── E103: effect performed but not in uses ───────────────────────────────

    #[test]
    fn e103_effect_performed_not_in_uses() {
        let diags = check_src(r#"
            import skill "GraphQL"
            import skill "Analytics"
            task T : TaskBrief uses [GraphQL] {
                goal = "fetch"
                step Do {
                    let _ = perform Analytics.track(event)?;
                }
            }
        "#);
        assert!(diags.iter().any(|d| d.code == ErrorCode::PerformWithoutUses), "{diags:?}");
    }

    #[test]
    fn e103_not_fired_when_skill_in_uses() {
        let diags = check_src(r#"
            import skill "GraphQL"
            task T : TaskBrief uses [GraphQL] {
                goal = "fetch"
                step Do {
                    let user = perform GraphQL.query(UserQuery)?;
                }
            }
        "#);
        let e103: Vec<_> = diags.iter().filter(|d| d.code == ErrorCode::PerformWithoutUses).collect();
        assert!(e103.is_empty(), "unexpected E103: {e103:?}");
    }

    // ── E101: missing goal ────────────────────────────────────────────────────

    #[test]
    fn e101_fires_on_task_without_goal() {
        let diags = check_src(r#"task T : TaskBrief { step S {} }"#);
        assert!(diags.iter().any(|d| d.code == ErrorCode::MissingGoal), "{diags:?}");
    }

    #[test]
    fn e101_not_fired_when_goal_present() {
        let diags = check_src(r#"task T : TaskBrief { goal = "do something" }"#);
        let e101: Vec<_> = diags.iter().filter(|d| d.code == ErrorCode::MissingGoal).collect();
        assert!(e101.is_empty(), "unexpected E101: {e101:?}");
    }

    // ── E102: skill in uses but not imported ──────────────────────────────────

    #[test]
    fn e102_fires_when_uses_without_import() {
        let diags = check_src(r#"task T : TaskBrief uses [GraphQL] { goal = "x" }"#);
        assert!(diags.iter().any(|d| d.code == ErrorCode::UndeclaredSkillInUses), "{diags:?}");
    }

    #[test]
    fn e102_not_fired_when_imported() {
        let diags = check_src(r#"
            import skill "GraphQL"
            task T : TaskBrief uses [GraphQL] { goal = "x" }
        "#);
        let e102: Vec<_> = diags.iter().filter(|d| d.code == ErrorCode::UndeclaredSkillInUses).collect();
        assert!(e102.is_empty(), "unexpected E102: {e102:?}");
    }

    // ── Multiple errors can coexist ───────────────────────────────────────────

    #[test]
    fn multiple_errors_on_bad_task() {
        // No goal + skill in uses but not imported
        let diags = check_src(r#"task T : TaskBrief uses [Missing] {}"#);
        let has_e101 = diags.iter().any(|d| d.code == ErrorCode::MissingGoal);
        let has_e102 = diags.iter().any(|d| d.code == ErrorCode::UndeclaredSkillInUses);
        assert!(has_e101, "expected E101: {diags:?}");
        assert!(has_e102, "expected E102: {diags:?}");
    }

    // ── Test blocks don't generate checker errors ─────────────────────────────

    #[test]
    fn test_blocks_do_not_generate_checker_errors() {
        // test { } blocks with mock/run/assert should be transparent to the checker
        let diags = check_src(r#"
            import skill "GraphQL"
            task FetchProfile : TaskBrief uses [GraphQL] {
                goal = "Fetch a user profile"
                step Load {
                    let user = perform GraphQL.query(UserProfileQuery)?;
                }
            }
            test "FetchProfile loads user via GraphQL" {
                mock GraphQL {
                    fn query(op) -> Ok(User { id: "u1", name: "Ada" })
                }
                run FetchProfile
                assert performed GraphQL.query
                assert result is Ok
            }
        "#);
        let hard_errors: Vec<_> = diags.iter().filter(|d| d.is_error()).collect();
        // W101 (no .briefskill) is ok; hard errors should be zero
        assert!(hard_errors.is_empty(), "unexpected hard errors: {hard_errors:?}");
    }

    #[test]
    fn test_block_does_not_require_mock_skills_to_be_imported_twice() {
        // The test block references skills only via mock — it should not
        // cause E102/E103 even if the mock refers to a skill not declared in uses
        let diags = check_src(r#"
            import skill "GraphQL"
            task T : TaskBrief uses [GraphQL] { goal = "x" }
            test "uses unimported mock" {
                mock SomeMockSkill { fn foo(x) -> Ok("y") }
                run T
                assert result is Ok
            }
        "#);
        // The only errors should be W101 (no .briefskill files) — no E102/E103
        let hard: Vec<_> = diags.iter().filter(|d| d.is_error()).collect();
        assert!(hard.is_empty(), "unexpected errors from test block mock: {hard:?}");
    }

    // ── W101: skill has no interface file ─────────────────────────────────────

    #[test]
    fn w101_fires_on_imported_skill_without_briefskill() {
        // When checking from "." (no .claude/skills/ dir), W101 should fire
        let diags = check_src(r#"
            import skill "GraphQL"
            task T : TaskBrief uses [GraphQL] { goal = "x" }
        "#);
        assert!(diags.iter().any(|d| d.code == ErrorCode::MissingSkillInterface), "{diags:?}");
    }

    // ── Effect group used in multiple tasks ───────────────────────────────────

    #[test]
    fn effect_group_works_across_multiple_tasks() {
        let diags = check_src(r#"
            import skill "Auth"
            import skill "Session"
            type AuthEffects = [Auth, Session]

            task Login : TaskBrief uses [AuthEffects] {
                goal = "login"
                step Do { let t = perform Auth.login(user)?; }
            }
            task Logout : TaskBrief uses [AuthEffects] {
                goal = "logout"
                step Do { let _ = perform Session.destroy(token)?; }
            }
        "#);
        let e102: Vec<_> = diags.iter().filter(|d| d.code == ErrorCode::UndeclaredSkillInUses).collect();
        assert!(e102.is_empty(), "unexpected E102: {e102:?}");
    }

    // ── @mcp attribute on type alias is valid ─────────────────────────────────

    #[test]
    fn mcp_attribute_on_type_alias_is_valid() {
        let diags = check_src(r#"type GitHubMCP = @mcp GitHub"#);
        let errors: Vec<_> = diags.iter().filter(|d| d.is_error()).collect();
        assert!(errors.is_empty(), "unexpected errors for @mcp alias: {errors:?}");
    }
}
