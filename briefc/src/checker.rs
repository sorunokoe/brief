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
use crate::lock::{check_lock, lock_path, read_lock, LockState};
use crate::manifest::BriefManifest;
use crate::skillgen::{parse_briefskill, SkillInterface, StaticConstraint};
use crate::typeck::TypeEnv;

// ─────────────────────────────────────────────────────────────────────────────

pub struct CheckContext<'a> {
    /// Directory of the `.brief` file being checked — used for skill lookups.
    pub file_dir: &'a Path,
    /// Current working directory — fallback for skill lookups.
    pub cwd: &'a Path,
    /// Optional parsed `brief.toml` — used for manifest-defined skill path overrides.
    pub manifest: Option<&'a BriefManifest>,
    /// Path to the `.brief` source file — used for lock gate (E303).
    /// `None` in unit tests that don't need lock checking.
    pub brief_path: Option<&'a Path>,
    /// When `true`, missing `.briefskill` files produce a warning instead of E101.
    /// Set by `brief check --allow-missing-skills` or by unit tests that run without real skill dirs.
    pub allow_missing_skills: bool,
}

/// Check a program and return all diagnostics.
pub fn check(program: &Program, ctx: &CheckContext<'_>) -> Vec<BriefError> {
    let mut diags = Vec::new();

    // Build the set of imported skill names for cross-checks.
    let imported: HashSet<&str> = program.imports.iter().map(|i| i.name.as_str()).collect();

    // Build effect group map: group name → expanded member names.
    let groups: HashMap<&str, Vec<&str>> = program
        .effect_groups
        .iter()
        .map(|g| {
            (
                g.name.as_str(),
                g.members.iter().map(|m| m.as_str()).collect(),
            )
        })
        .collect();

    // Build inline effect fn map: (effect_name, fn_name) → ret_attrs
    // Used to auto-detect @once return types without explicit annotation at call site.
    let inline_effects: HashMap<(&str, &str), &[Attribute]> = program
        .effects
        .iter()
        .flat_map(|e| {
            e.fns
                .iter()
                .map(move |f| ((e.name.as_str(), f.name.as_str()), f.ret_attrs.as_slice()))
        })
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

    let skill_ifaces = load_skill_interfaces(program, ctx);
    let type_env = TypeEnv::from_program(program);

    // 4. Validate each task (with group expansion and linear binding tracking).
    for task in &program.tasks {
        check_task(
            task,
            &imported,
            &groups,
            &inline_effects,
            &skill_ifaces,
            &type_env,
            ctx,
            &mut diags,
        );
    }

    // 5. Spec coverage: load skill interfaces, then check test block coverage.
    for test in &program.tests {
        check_test_spec_coverage(test, &skill_ifaces, &mut diags);
    }

    // 6. Lock gate (E303): if any dynamic annotations exist, require a valid lock.
    check_lock_gate(program, &skill_ifaces, ctx, &mut diags);

    // 7. E309: dynamic annotations without a configured verifier.
    check_unconfigured_verifiers(program, &skill_ifaces, ctx, &mut diags);

    diags
}

// ─────────────────────────────────────────────────────────────────────────────

fn check_skill_import(import: &SkillImport, ctx: &CheckContext<'_>, diags: &mut Vec<BriefError>) {
    if find_skill_interface(&import.name, ctx).is_none() {
        if ctx.allow_missing_skills {
            return; // suppressed by --allow-missing-skills
        }
        let skill_path = skill_interface_path_display(&import.name);
        diags.push(BriefError {
            code: ErrorCode::MissingSkillInterface,
            message: format!("skill '{}' has no interface file", import.name),
            span: import.span,
            hint: Some(format!(
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
            code: ErrorCode::UnknownType,
            message: format!("type alias '{}' has no base type", alias.name),
            span: alias.span,
            hint: Some(format!(
                r#"example: type {} = @nonEmpty String"#,
                alias.name
            )),
        });
    }
    // Known refinement attributes for type aliases.
    const KNOWN_ATTRS: &[&str] = &["url", "nonEmpty", "matches", "once", "affine", "mcp"];
    for attr in &alias.attrs {
        if !KNOWN_ATTRS.contains(&attr.name.as_str()) {
            diags.push(BriefError {
                code: ErrorCode::AttributeConstraint,
                message: format!(
                    "unknown attribute '@{}' in type alias '{}'",
                    attr.name, alias.name
                ),
                span: attr.span,
                hint: Some(format!("known attributes: {}", KNOWN_ATTRS.join(", "))),
            });
        }
    }
}

fn check_task(
    task: &Task,
    imported: &HashSet<&str>,
    groups: &HashMap<&str, Vec<&str>>,
    inline_effects: &HashMap<(&str, &str), &[Attribute]>,
    skill_ifaces: &HashMap<String, SkillInterface>,
    type_env: &TypeEnv<'_>,
    _ctx: &CheckContext<'_>,
    diags: &mut Vec<BriefError>,
) {
    // E101: goal is required.
    if task.goal.is_none() {
        diags.push(BriefError {
            code: ErrorCode::MissingGoal,
            message: format!("task '{}' is missing a `goal` field", task.name),
            span: task.span,
            hint: Some(r#"add: goal = "describe what this task accomplishes""#.to_string()),
        });
    }

    check_task_extras(task, type_env, diags);
    check_brief_builder_provides(task, diags);

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
                code: ErrorCode::UndeclaredSkillInUses,
                message: format!("skill '{skill}' is in `uses` but never imported"),
                span: task.span,
                hint: Some(format!(
                    r#"add: import skill "{skill}" at the top of the file"#
                )),
            });
        }
    }

    // E103: validate perform calls inside steps.
    // Two-level linear tracking (TRIZ P5 Merging):
    //   task_linear = bindings declared @once in a previous step and not yet consumed.
    //   step_linear = bindings declared @once in the current step.
    // This lets us detect cross-step E104/E105 violations.
    let uses_set: HashSet<&str> = expanded_uses.iter().copied().collect();
    let task_effects: HashSet<&str> = task.effects.iter().map(String::as_str).collect();
    let base_locals = task_base_locals(task);
    let mut task_linear: HashMap<String, (Span, usize)> = HashMap::new();

    for step in &task.steps {
        check_step(
            task,
            step,
            &uses_set,
            &task_effects,
            imported,
            inline_effects,
            skill_ifaces,
            type_env,
            &base_locals,
            &mut task_linear,
            diags,
        );
    }

    // After all steps: any remaining task_linear binding was never consumed → E105.
    for (name, (span, _)) in &task_linear {
        diags.push(BriefError {
            code: ErrorCode::LinearBindingDropped,
            message: format!(
                "linear binding '{name}' is declared but never consumed across all task steps"
            ),
            span: *span,
            hint: Some(format!(
                "use '{name}' in a step, or remove the `@once` annotation"
            )),
        });
    }
}

fn type_ref_str(ty: &TypeRef) -> String {
    let mut s = ty.name.clone();
    if !ty.args.is_empty() {
        let args = ty
            .args
            .iter()
            .map(type_ref_str)
            .collect::<Vec<_>>()
            .join(", ");
        s.push_str(&format!("<{args}>"));
    }
    if ty.optional {
        s.push('?');
    }
    s
}

fn check_task_extras(task: &Task, env: &TypeEnv<'_>, diags: &mut Vec<BriefError>) {
    let Some(extras) = &task.extras else {
        return;
    };
    let extras_span = task.extras_span.unwrap_or(task.span);

    match extras {
        ExtrasNode::StringMap(_) => {
            diags.push(BriefError {
                code: ErrorCode::DeprecatedStringExtras,
                message: "string extras are deprecated — use typed extras { } instead".to_string(),
                span: extras_span,
                hint: Some(
                    "replace `extras = [\"key\": \"val\", ...]` with `extras { field: Type }`"
                        .to_string(),
                ),
            });
        }
        ExtrasNode::TypedRecord(fields) => {
            for field in fields {
                check_extras_field_type(&field.name, &field.type_ref, field.span, env, diags);
            }
        }
    }
}

fn check_brief_builder_provides(task: &Task, diags: &mut Vec<BriefError>) {
    if task.has_builder && task.provides.is_none() {
        diags.push(BriefError {
            code: ErrorCode::BriefBuilderProvidesMissing,
            message: format!(
                "task '{}' is annotated @BriefBuilder but has no provides {{ }} block",
                task.name
            ),
            span: task.span,
            hint: Some("add: provides { field: Type }".to_string()),
        });
    }
}

type LocalTypes = HashMap<String, TypeRef>;

fn task_base_locals(task: &Task) -> LocalTypes {
    let mut locals = HashMap::new();
    if let Some(ExtrasNode::TypedRecord(fields)) = &task.extras {
        for field in fields {
            locals.insert(field.name.clone(), field.type_ref.clone());
        }
    }
    locals
}

fn check_extras_field_type(
    field_name: &str,
    ty: &TypeRef,
    span: Span,
    env: &TypeEnv<'_>,
    diags: &mut Vec<BriefError>,
) {
    if !env.type_exists(&ty.name, &[]) {
        let ty_name = type_ref_str(ty);
        diags.push(BriefError {
            code: ErrorCode::UnknownExtrasField,
            message: format!(
                "extras field '{}' has unknown type '{}' — declare it as a sealed type or struct",
                field_name, ty_name
            ),
            span,
            hint: Some(format!(
                "declare `sealed type {}` or `struct {}` before using it in extras",
                ty_name, ty_name
            )),
        });
    }

    for arg in &ty.args {
        check_extras_field_type(field_name, arg, span, env, diags);
    }
}

fn check_effect_decl(effect: &EffectDecl, diags: &mut Vec<BriefError>) {
    let mut seen = HashSet::new();
    for f in &effect.fns {
        if !seen.insert(f.name.as_str()) {
            diags.push(BriefError {
                code: ErrorCode::ParseError,
                message: format!(
                    "effect '{}' has duplicate function '{}'",
                    effect.name, f.name
                ),
                span: f.span,
                hint: Some("remove the duplicate declaration".to_string()),
            });
        }
    }
}

fn check_step(
    task: &Task,
    step: &Step,
    uses_set: &HashSet<&str>,
    task_effects: &HashSet<&str>,
    imported: &HashSet<&str>,
    inline_effects: &HashMap<(&str, &str), &[Attribute]>,
    skill_ifaces: &HashMap<String, SkillInterface>,
    env: &TypeEnv<'_>,
    base_locals: &LocalTypes,
    task_linear: &mut HashMap<String, (Span, usize)>,
    diags: &mut Vec<BriefError>,
) {
    record_phase_contracts(step);

    // step_linear: @once bindings introduced in this step.
    let mut step_linear: HashMap<String, (Span, usize)> = HashMap::new();
    let mut locals = base_locals.clone();

    // First pass: validate performs, match exhaustiveness, and collect step_linear bindings.
    for stmt in &step.body {
        let expr = stmt_value(stmt);
        check_expr_semantics(
            expr,
            &task.name,
            uses_set,
            task_effects,
            imported,
            skill_ifaces,
            env,
            &locals,
            diags,
        );

        if let Stmt::Let {
            attrs,
            name,
            value,
            span,
        } = stmt
        {
            let is_once = attrs.iter().any(|a| a == "once" || a == "affine");
            let auto_once = match value {
                Expr::Perform { skill, func, .. } => {
                    let key = (skill.as_str(), func.as_str());
                    inline_effects
                        .get(&key)
                        .map(|ret_attrs| {
                            ret_attrs
                                .iter()
                                .any(|a| a.name == "once" || a.name == "affine")
                        })
                        .unwrap_or(false)
                }
                _ => false,
            };

            if is_once || auto_once {
                // Shadowing: a new @once x while task_linear still holds an earlier x.
                if let Some((old_span, _)) = task_linear.remove(name) {
                    diags.push(BriefError {
                        code: ErrorCode::LinearBindingDropped,
                        message: format!(
                            "linear binding '{name}' is shadowed before being consumed"
                        ),
                        span: old_span,
                        hint: Some(format!("use '{name}' before re-binding it with `@once`")),
                    });
                }
                step_linear.insert(name.clone(), (*span, 0));
            }

            if let Some(ty) = infer_expr_type(value, env, &locals) {
                locals.insert(name.clone(), ty);
            }
        }
    }

    // Second pass: count usages of all linear bindings across every expression in this step.
    for stmt in &step.body {
        count_ident_uses(stmt_value(stmt), &mut step_linear);
        count_ident_uses(stmt_value(stmt), task_linear);
    }

    // Resolve step_linear:
    //   used once  → consumed, discard
    //   not used   → promote to task_linear (will be consumed in a later step)
    //   used > 1   → E104
    for (name, (span, uses)) in step_linear {
        match uses {
            0 => {
                task_linear.insert(name, (span, 0));
            }
            1 => { /* consumed exactly once — done */ }
            n => {
                diags.push(BriefError {
                    code:    ErrorCode::LinearBindingReused,
                    message: format!("linear binding '{name}' is consumed {n} times (must be exactly once)"),
                    span,
                    hint:    Some("a `@once` binding may only be used once — split into separate `perform` calls".to_string()),
                });
            }
        }
    }

    // Resolve task_linear (bindings from earlier steps):
    //   used once  → consumed, remove
    //   not used   → leave for future steps (reset count to 0)
    //   used > 1   → E104, then remove
    let mut to_remove = Vec::new();
    for (name, (span, uses)) in task_linear.iter() {
        match *uses {
            0 => { /* not used in this step — leave */ }
            1 => {
                to_remove.push(name.clone());
            }
            n => {
                diags.push(BriefError {
                    code:    ErrorCode::LinearBindingReused,
                    message: format!("linear binding '{name}' is consumed {n} times in one step (must be exactly once total)"),
                    span:    *span,
                    hint:    Some("a `@once` binding from a previous step may only be used once".to_string()),
                });
                to_remove.push(name.clone());
            }
        }
    }
    for name in &to_remove {
        task_linear.remove(name);
    }
    // Reset use-counts on remaining task_linear entries for the next step.
    for (_, (_, uses)) in task_linear.iter_mut() {
        *uses = 0;
    }
}

fn record_phase_contracts(step: &Step) {
    if step.pre_conditions.is_empty() && step.post_conditions.is_empty() {
        return;
    }

    // H2 stores phase contracts and makes the checker aware of them.
    // H3 will type-check and evaluate these assertions.
    let _ = (&step.pre_conditions, &step.post_conditions);
}

fn stmt_value(stmt: &Stmt) -> &Expr {
    match stmt {
        Stmt::Let { value, .. } => value,
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
            for a in args {
                count_ident_uses(a, linear);
            }
        }
        Expr::Call { args, .. } => {
            for a in args {
                count_ident_uses(a, linear);
            }
        }
        Expr::Match { scrutinee, arms } => {
            count_ident_uses(scrutinee, linear);

            let template: HashMap<String, (Span, usize)> = linear
                .iter()
                .map(|(name, (span, _))| (name.clone(), (*span, 0)))
                .collect();
            let mut max_arm_uses: HashMap<String, usize> =
                linear.keys().cloned().map(|name| (name, 0)).collect();

            for arm in arms {
                let mut arm_linear = template.clone();
                if let Pattern::Variant1(_, binding) = &arm.pattern {
                    arm_linear.remove(binding);
                }
                count_ident_uses(&arm.body, &mut arm_linear);
                for (name, (_, count)) in arm_linear {
                    if let Some(max_count) = max_arm_uses.get_mut(&name) {
                        *max_count = (*max_count).max(count);
                    }
                }
            }

            for (name, count) in max_arm_uses {
                if let Some((_, total)) = linear.get_mut(&name) {
                    *total += count;
                }
            }
        }
        Expr::Await { expr: inner, .. } => count_ident_uses(inner, linear),
        Expr::Str { .. } | Expr::Int { .. } => {}
    }
}

fn check_expr_semantics(
    expr: &Expr,
    task_name: &str,
    uses_set: &HashSet<&str>,
    task_effects: &HashSet<&str>,
    _imported: &HashSet<&str>,
    skill_ifaces: &HashMap<String, SkillInterface>,
    env: &TypeEnv<'_>,
    locals: &LocalTypes,
    diags: &mut Vec<BriefError>,
) {
    match expr {
        Expr::Perform {
            skill, func, span, args, ..
        } => {
            if !uses_set.contains(skill.as_str()) {
                diags.push(BriefError {
                    code: ErrorCode::PerformWithoutUses,
                    message: format!(
                        "effect '{}' is performed but not declared in `uses [...]`",
                        skill
                    ),
                    span: *span,
                    hint: Some(format!("add '{skill}' to the task's `uses` clause")),
                });
            }
            if let Some(iface) = skill_ifaces.get(skill) {
                for effect in &iface.effects {
                    if !task_effects.contains(effect.as_str()) {
                        diags.push(BriefError {
                            code: ErrorCode::EffectContractViolation,
                            message: format!(
                                "task '{}' uses skill '{}.{}' which produces effect '{}', but '{}' is not declared in task effects",
                                task_name, skill, func, effect, effect
                            ),
                            span: *span,
                            hint: Some(format!(
                                "add '{}' to task '{}' `effects [...]`",
                                effect, task_name
                            )),
                        });
                    }
                }
            }
            for arg in args {
                check_expr_semantics(
                    arg,
                    task_name,
                    uses_set,
                    task_effects,
                    _imported,
                    skill_ifaces,
                    env,
                    locals,
                    diags,
                );
            }
        }
        Expr::Await { expr: inner, .. } => {
            check_expr_semantics(
                inner,
                task_name,
                uses_set,
                task_effects,
                _imported,
                skill_ifaces,
                env,
                locals,
                diags,
            );
        }
        Expr::Call { args, .. } => {
            for arg in args {
                check_expr_semantics(
                    arg,
                    task_name,
                    uses_set,
                    task_effects,
                    _imported,
                    skill_ifaces,
                    env,
                    locals,
                    diags,
                );
            }
        }
        Expr::Match { scrutinee, arms } => {
            check_expr_semantics(
                scrutinee,
                task_name,
                uses_set,
                task_effects,
                _imported,
                skill_ifaces,
                env,
                locals,
                diags,
            );
            let scrutinee_ty = infer_expr_type(scrutinee, env, locals);
            for arm in arms {
                let mut arm_locals = locals.clone();
                if let Some((binding, binding_ty)) =
                    match_pattern_binding(&arm.pattern, scrutinee_ty.as_ref(), env, arm.span)
                {
                    arm_locals.insert(binding, binding_ty);
                }
                check_expr_semantics(
                    &arm.body,
                    task_name,
                    uses_set,
                    task_effects,
                    _imported,
                    skill_ifaces,
                    env,
                    &arm_locals,
                    diags,
                );
            }
            check_match_exhaustiveness(expr, scrutinee_ty.as_ref(), arms, env, diags);
        }
        Expr::Ident { .. } | Expr::Str { .. } | Expr::Int { .. } => {}
    }
}

fn check_match_exhaustiveness(
    expr: &Expr,
    scrutinee_ty: Option<&TypeRef>,
    arms: &[MatchArm],
    env: &TypeEnv<'_>,
    diags: &mut Vec<BriefError>,
) {
    let Some(scrutinee_ty) = scrutinee_ty else {
        return;
    };
    let Some(sealed) = env.sealed_type(&scrutinee_ty.name) else {
        return;
    };
    if arms
        .iter()
        .any(|arm| matches!(arm.pattern, Pattern::Wildcard))
    {
        return;
    }

    let covered: HashSet<&str> = arms
        .iter()
        .filter_map(|arm| match &arm.pattern {
            Pattern::Variant(name) | Pattern::Variant1(name, _) => Some(name.as_str()),
            Pattern::Wildcard => None,
        })
        .collect();

    let missing: Vec<String> = sealed
        .variants
        .iter()
        .filter(|variant| !covered.contains(variant.name.as_str()))
        .map(|variant| variant.name.clone())
        .collect();

    if missing.is_empty() {
        return;
    }

    diags.push(BriefError {
        code: ErrorCode::NonExhaustiveMatch,
        message: format!(
            "non-exhaustive match on '{}': missing variants [{}]",
            sealed.name,
            missing.join(", ")
        ),
        span: expr.span(),
        hint: Some(format!(
            "add arms for {} or add a wildcard `_` arm",
            missing.join(", ")
        )),
    });
}

fn infer_expr_type(expr: &Expr, env: &TypeEnv<'_>, locals: &LocalTypes) -> Option<TypeRef> {
    match expr {
        Expr::Ident { name, .. } => locals.get(name).cloned(),
        Expr::Str { span, .. } => Some(TypeRef {
            name: "String".to_string(),
            args: Vec::new(),
            optional: false,
            span: *span,
        }),
        Expr::Int { span, .. } => Some(TypeRef {
            name: "Int".to_string(),
            args: Vec::new(),
            optional: false,
            span: *span,
        }),
        Expr::Perform { skill, func, .. } => env.effect_fn(skill, func).map(|sig| sig.ret.clone()),
        Expr::Await { expr: inner, .. } => infer_expr_type(inner, env, locals),
        Expr::Call { .. } | Expr::Match { .. } => None,
    }
}

fn match_pattern_binding(
    pattern: &Pattern,
    scrutinee_ty: Option<&TypeRef>,
    env: &TypeEnv<'_>,
    span: Span,
) -> Option<(String, TypeRef)> {
    match pattern {
        Pattern::Variant1(variant, binding) => Some((
            binding.clone(),
            pattern_binding_type(variant, scrutinee_ty, env, span),
        )),
        Pattern::Variant(_) | Pattern::Wildcard => None,
    }
}

fn pattern_binding_type(
    variant_name: &str,
    scrutinee_ty: Option<&TypeRef>,
    env: &TypeEnv<'_>,
    span: Span,
) -> TypeRef {
    scrutinee_ty
        .and_then(|ty| builtin_variant_binding_type(ty, variant_name))
        .or_else(|| {
            scrutinee_ty.and_then(|ty| env.sealed_variant_field_type(&ty.name, variant_name))
        })
        .unwrap_or_else(|| default_binding_type(span))
}

fn builtin_variant_binding_type(scrutinee_ty: &TypeRef, variant_name: &str) -> Option<TypeRef> {
    match (scrutinee_ty.name.as_str(), variant_name) {
        ("Result", "Ok") if !scrutinee_ty.args.is_empty() => Some(scrutinee_ty.args[0].clone()),
        ("Result", "Err") if scrutinee_ty.args.len() >= 2 => Some(scrutinee_ty.args[1].clone()),
        ("Option", "Some") if !scrutinee_ty.args.is_empty() => Some(scrutinee_ty.args[0].clone()),
        _ => None,
    }
}

fn default_binding_type(span: Span) -> TypeRef {
    TypeRef {
        name: "String".to_string(),
        args: Vec::new(),
        optional: false,
        span,
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Skill interface resolution
// ─────────────────────────────────────────────────────────────────────────────

/// Returns the path of the `.briefskill` file if it exists, searching in order:
/// 1. `brief.toml` `[skills]` manifest override (if manifest present)
/// 2. `<file_dir>/.claude/skills/<name>/<name>.briefskill`
/// 3. `<cwd>/.claude/skills/<name>/<name>.briefskill`
/// 4. `~/.brief/skills/<name>.briefskill`  (user-global, future)
pub fn find_skill_interface(name: &str, ctx: &CheckContext<'_>) -> Option<PathBuf> {
    // 1. Manifest-defined path override
    if let Some(manifest) = ctx.manifest {
        if let Some(p) = manifest.resolve_skill(name) {
            return Some(p);
        }
    }

    // 2 & 3. Default discovery: .claude/skills/<name>/<name>.briefskill
    let relative = format!(".claude/skills/{name}/{name}.briefskill");

    let candidates = [ctx.file_dir.join(&relative), ctx.cwd.join(&relative)];

    candidates.into_iter().find(|p| p.exists())
}

fn skill_interface_path_display(name: &str) -> String {
    format!(".claude/skills/{name}/{name}.briefskill")
}

// ─────────────────────────────────────────────────────────────────────────────
// Spec coverage (E301 / E302)
// ─────────────────────────────────────────────────────────────────────────────

/// Load parsed `SkillInterface` for every imported skill that has a `.briefskill` file.
fn load_skill_interfaces(
    program: &Program,
    ctx: &CheckContext<'_>,
) -> HashMap<String, SkillInterface> {
    let mut map = HashMap::new();
    for import in &program.imports {
        if let Some(path) = find_skill_interface(&import.name, ctx) {
            if let Ok(content) = std::fs::read_to_string(&path) {
                if let Some(iface) = parse_briefskill(&content) {
                    map.insert(import.name.clone(), iface);
                }
            }
        }
    }
    map
}

/// Check that every `@range(min,max)` param has both boundary literals covered,
/// and every `@enum(vals)` param has all enum values covered, across all
/// `perform Skill.fn(args)` calls in the test body.
fn check_test_spec_coverage(
    test: &TestDecl,
    ifaces: &HashMap<String, SkillInterface>,
    diags: &mut Vec<BriefError>,
) {
    // Collect all perform calls: (skill, fn_name, args_vec, call_span)
    let mut performs: Vec<(&str, &str, &[Expr], Span)> = Vec::new();
    for stmt in &test.body {
        collect_performs(stmt_value(stmt), &mut performs);
    }

    // Group calls by (skill, fn): (skill_name, fn_name) → Vec<&[Expr]>
    let mut grouped: HashMap<(&str, &str), Vec<&[Expr]>> = HashMap::new();
    for (skill, func, args, _span) in &performs {
        grouped.entry((skill, func)).or_default().push(args);
    }

    // For each group, look up constraints and check coverage.
    for ((skill_name, fn_name), all_call_args) in &grouped {
        let iface = match ifaces.get(*skill_name) {
            Some(i) => i,
            None => continue, // no interface loaded — silently skip
        };
        let skill_fn = match iface.funcs.iter().find(|f| f.name == *fn_name) {
            Some(f) => f,
            None => continue, // fn not in interface — type checker handles this
        };

        for (param_idx, param) in skill_fn.params.iter().enumerate() {
            for constraint in &param.static_constraints {
                match constraint {
                    StaticConstraint::Range(min, max) => {
                        check_range_coverage(
                            skill_name,
                            fn_name,
                            param_idx,
                            &param.name,
                            *min,
                            *max,
                            all_call_args,
                            test.span,
                            diags,
                        );
                    }
                    StaticConstraint::Enum(vals) => {
                        check_enum_coverage(
                            skill_name,
                            fn_name,
                            param_idx,
                            &param.name,
                            vals,
                            all_call_args,
                            test.span,
                            diags,
                        );
                    }
                    // @matches / @nonEmpty are not coverage obligations on test args.
                    _ => {}
                }
            }
        }
    }
}

fn collect_performs<'a>(expr: &'a Expr, out: &mut Vec<(&'a str, &'a str, &'a [Expr], Span)>) {
    match expr {
        Expr::Perform {
            skill,
            func,
            args,
            span,
            ..
        } => {
            out.push((skill.as_str(), func.as_str(), args.as_slice(), *span));
            for arg in args {
                collect_performs(arg, out);
            }
        }
        Expr::Await { expr: inner, .. } => collect_performs(inner, out),
        Expr::Call { args, .. } => {
            for arg in args {
                collect_performs(arg, out);
            }
        }
        Expr::Match { scrutinee, arms } => {
            collect_performs(scrutinee, out);
            for arm in arms {
                collect_performs(&arm.body, out);
            }
        }
        Expr::Ident { .. } | Expr::Str { .. } | Expr::Int { .. } => {}
    }
}

/// Check that both `min` and `max` appear as literal int args at `param_idx`.
fn check_range_coverage(
    skill: &str,
    fn_name: &str,
    idx: usize,
    pname: &str,
    min: i64,
    max: i64,
    all_args: &[&[Expr]],
    span: Span,
    diags: &mut Vec<BriefError>,
) {
    let mut saw_min = false;
    let mut saw_max = false;
    for args in all_args {
        if let Some(Expr::Int { value, .. }) = args.get(idx) {
            if *value == min {
                saw_min = true;
            }
            if *value == max {
                saw_max = true;
            }
        }
    }

    if !saw_min {
        diags.push(BriefError {
            code: ErrorCode::RangeBoundaryMissing,
            message: format!(
                "test block missing lower boundary for `{skill}.{fn_name}` param `{pname}`: \
                 @range({min}, {max}) requires a call with arg = {min}"
            ),
            span,
            hint: Some(format!(
                "add: perform {skill}.{fn_name}({min}, ...) to cover the lower boundary"
            )),
        });
    }
    if !saw_max {
        diags.push(BriefError {
            code: ErrorCode::RangeBoundaryMissing,
            message: format!(
                "test block missing upper boundary for `{skill}.{fn_name}` param `{pname}`: \
                 @range({min}, {max}) requires a call with arg = {max}"
            ),
            span,
            hint: Some(format!(
                "add: perform {skill}.{fn_name}({max}, ...) to cover the upper boundary"
            )),
        });
    }
}

/// Check that each enum value appears as a literal string arg at `param_idx`.
fn check_enum_coverage(
    skill: &str,
    fn_name: &str,
    idx: usize,
    pname: &str,
    vals: &[String],
    all_args: &[&[Expr]],
    span: Span,
    diags: &mut Vec<BriefError>,
) {
    let covered: HashSet<&str> = all_args
        .iter()
        .filter_map(|args| args.get(idx))
        .filter_map(|expr| {
            if let Expr::Str { value, .. } = expr {
                Some(value.as_str())
            } else {
                None
            }
        })
        .collect();

    for val in vals {
        if !covered.contains(val.as_str()) {
            diags.push(BriefError {
                code:    ErrorCode::EnumValueMissing,
                message: format!(
                    "test block missing enum value \"{val}\" for `{skill}.{fn_name}` param `{pname}`"
                ),
                span,
                hint: Some(format!(
                    "add: perform {skill}.{fn_name}(\"{val}\", ...) to cover all enum variants"
                )),
            });
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Lock gate (E303)
// ─────────────────────────────────────────────────────────────────────────────

/// Emit E303 if the program has dynamic annotations and the lock file is
/// missing, stale, or the source has changed since it was written.
///
/// "Dynamic annotations" = any `@xyz` in `.briefskill` params that is not in
/// the static list (`@range`, `@enum`, `@matches`, `@nonEmpty`).
fn check_lock_gate(
    program: &Program,
    ifaces: &HashMap<String, SkillInterface>,
    ctx: &CheckContext<'_>,
    diags: &mut Vec<BriefError>,
) {
    // Only run the gate if `require_lock` is enabled (default: true).
    let (require_lock, max_age_hours) = if let Some(mf) = ctx.manifest {
        (mf.verify.require_lock, mf.verify.max_lock_age_hours)
    } else {
        (true, 24) // defaults when no brief.toml
    };

    if !require_lock {
        return;
    }

    // Check if there are any dynamic annotations across all loaded interfaces.
    let has_dynamic = ifaces
        .values()
        .flat_map(|iface| iface.funcs.iter())
        .flat_map(|f| f.params.iter())
        .any(|p| !p.dynamic_attrs.is_empty());

    if !has_dynamic {
        return;
    }

    // Dynamic annotations present — need a valid lock.
    let brief_path = match ctx.brief_path {
        Some(p) => p,
        None => return, // no path in context (unit tests) — skip
    };

    let lp = lock_path(brief_path);

    let lock = match read_lock(&lp) {
        Some(l) => l,
        None => {
            diags.push(BriefError {
                code: ErrorCode::LockRequired,
                message: format!(
                    "dynamic annotations require a verified lock file (expected: {})",
                    lp.display()
                ),
                span: program.imports.first().map(|i| i.span).unwrap_or_default(),
                hint: Some("run: brief verify".to_string()),
            });
            return;
        }
    };

    // Read source bytes for hash comparison.
    let source_bytes = match std::fs::read(brief_path) {
        Ok(b) => b,
        Err(_) => return, // can't read — skip gate
    };

    match check_lock(&lock, &source_bytes, max_age_hours) {
        LockState::Fresh => {}                // all good
        LockState::SourceChanged => {
            diags.push(BriefError {
                code: ErrorCode::LockRequired,
                message: format!(
                    "lock file {} is stale — source has changed since last verify",
                    lp.display()
                ),
                span: program.imports.first().map(|i| i.span).unwrap_or_default(),
                hint: Some("run: brief verify to re-seal the contract".to_string()),
            });
        }
        LockState::Stale => {
            diags.push(BriefError {
                code: ErrorCode::LockRequired,
                message: format!(
                    "lock file {} is older than {max_age_hours}h — re-verification required",
                    lp.display()
                ),
                span: program.imports.first().map(|i| i.span).unwrap_or_default(),
                hint: Some("run: brief verify to refresh the verification seal".to_string()),
            });
        }
    }
}

/// E309: dynamic annotations that have no configured verifier in `brief.toml`.
///
/// A dynamic annotation is any `@xyz` in a `.briefskill` param that is not
/// handled statically (`@range`, `@enum`, `@matches`, `@nonEmpty`).
/// If the manifest has no `[verifiers."@xyz"]` entry, emit E309.
fn check_unconfigured_verifiers(
    program: &Program,
    skill_ifaces: &HashMap<String, SkillInterface>,
    ctx: &CheckContext<'_>,
    diags: &mut Vec<BriefError>,
) {
    let verifiers = ctx
        .manifest
        .map(|m| &m.verifiers)
        .cloned()
        .unwrap_or_default();

    // Collect (annotation, span) for all dynamic annotations that appear in
    // actual `perform` calls (not just imported but unused functions).
    let mut perform_list: Vec<(&str, &str, &[Expr], Span)> = Vec::new();
    for task in &program.tasks {
        for step in &task.steps {
            for stmt in &step.body {
                let expr = match stmt {
                    Stmt::Expr { value, .. } | Stmt::Let { value, .. } => value,
                };
                collect_performs(expr, &mut perform_list);
            }
        }
    }
    for test in &program.tests {
        for stmt in &test.body {
            let expr = match stmt {
                Stmt::Expr { value, .. } | Stmt::Let { value, .. } => value,
            };
            collect_performs(expr, &mut perform_list);
        }
    }

    let mut seen: HashSet<String> = HashSet::new();
    for (skill_name, fn_name, _args, span) in &perform_list {
        let iface = match skill_ifaces.get(*skill_name) {
            Some(i) => i,
            None => continue,
        };
        let skill_fn = match iface.funcs.iter().find(|f| f.name == *fn_name) {
            Some(f) => f,
            None => continue,
        };
        for param in &skill_fn.params {
            for dyn_attr in &param.dynamic_attrs {
                if seen.contains(dyn_attr) {
                    continue;
                }
                let key_with = dyn_attr.as_str();
                let key_without = dyn_attr.strip_prefix('@').unwrap_or(dyn_attr);
                if !verifiers.contains_key(key_with) && !verifiers.contains_key(key_without) {
                    seen.insert(dyn_attr.clone());
                    diags.push(BriefError {
                        code:    ErrorCode::UnconfiguredVerifier,
                        message: format!(
                            "error[E309]: annotation `{dyn_attr}` on {}::{} has no configured verifier — \
                             add [verifiers.\"{dyn_attr}\"] to brief.toml",
                            skill_name, fn_name
                        ),
                        span:    *span,
                        hint:    Some(format!(
                            "use a static constraint like @matches(\"pattern\") if format validation is sufficient, \
                             or add [verifiers.\"{dyn_attr}\"] with mcp_command or mcp_url"
                        )),
                    });
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::lex;
    use crate::parser::parse;
    use crate::typeck;

    fn check_src(src: &str) -> Vec<BriefError> {
        let (tokens, _) = lex(src);
        let (program, _) = parse(&tokens, src);
        let ctx = CheckContext {
            file_dir: Path::new("."),
            cwd: Path::new("."),
            manifest: None,
            brief_path: None,
            allow_missing_skills: true,
        };
        check(&program, &ctx)
    }

    fn check_str(src: &str) -> Vec<BriefError> {
        check_src(src)
    }

    fn check_all_src(src: &str) -> Vec<BriefError> {
        let (tokens, _) = lex(src);
        let (program, _) = parse(&tokens, src);
        let ctx = CheckContext {
            file_dir: Path::new("."),
            cwd: Path::new("."),
            manifest: None,
            brief_path: None,
            allow_missing_skills: true,
        };
        let mut diags = check(&program, &ctx);
        diags.extend(typeck::type_check(&program));
        diags
    }

    #[test]
    fn scoped_generic_t_in_two_effects_does_not_conflict() {
        let src = r#"
            effect Foo { fn a<T>(x: T) -> T }
            effect Bar { fn b<T>(x: T) -> T }
            task X : TaskBrief { goal = "x" }
        "#;
        let diags = check_all_src(src);
        assert!(diags.is_empty(), "unexpected diags: {diags:?}");
    }

    #[test]
    fn generic_param_shadowing_builtin_is_e206() {
        let src = r#"
            effect Bad { fn foo<String>(x: String) -> String }
            task X : TaskBrief { goal = "x" }
        "#;
        let diags = check_all_src(src);
        assert!(
            diags
                .iter()
                .any(|d| matches!(d.code, ErrorCode::ScopedGenericConflict)),
            "unexpected diags: {diags:?}"
        );
    }

    #[test]
    fn no_errors_on_minimal_task() {
        let diags = check_src(r#"task Hello : TaskBrief { goal = "hi" }"#);
        assert!(diags.iter().all(|d| !d.is_error()), "{diags:?}");
    }

    #[test]
    fn match_platform_parses_and_checks_without_errors() {
        let src = r#"
            task Render : TaskBrief {
                goal = "render"
                step Render {
                    let view = match platform {
                        iOS => "mobile"
                        Android => "desktop"
                        _ => "other"
                    };
                }
            }
        "#;
        let (tokens, lex_errs) = lex(src);
        assert!(lex_errs.is_empty(), "lex errors: {lex_errs:?}");
        let (program, parse_errs) = parse(&tokens, src);
        assert!(parse_errs.is_empty(), "parse errors: {parse_errs:?}");
        let ctx = CheckContext {
            file_dir: Path::new("."),
            cwd: Path::new("."),
            manifest: None,
            brief_path: None,
            allow_missing_skills: true,
        };
        let diags = check(&program, &ctx);
        let hard_errors: Vec<_> = diags.iter().filter(|d| d.is_error()).collect();
        assert!(hard_errors.is_empty(), "unexpected errors: {hard_errors:?}");
    }

    #[test]
    fn match_result_parses_and_checks_without_errors() {
        let src = r#"
            task HandleResult : TaskBrief {
                goal = "handle"
                step Handle {
                    let name = match result {
                        Ok(v) => v
                        Err(e) => "fail"
                    };
                }
            }
        "#;
        let (tokens, lex_errs) = lex(src);
        assert!(lex_errs.is_empty(), "lex errors: {lex_errs:?}");
        let (program, parse_errs) = parse(&tokens, src);
        assert!(parse_errs.is_empty(), "parse errors: {parse_errs:?}");
        let ctx = CheckContext {
            file_dir: Path::new("."),
            cwd: Path::new("."),
            manifest: None,
            brief_path: None,
            allow_missing_skills: true,
        };
        let diags = check(&program, &ctx);
        let hard_errors: Vec<_> = diags.iter().filter(|d| d.is_error()).collect();
        assert!(hard_errors.is_empty(), "unexpected errors: {hard_errors:?}");
    }

    #[test]
    fn match_variant_bindings_are_scoped_to_their_arms() {
        let diags = check_all_src(
            r#"
            import skill "Auth"
            effect Auth {
                fn load(user_id: String) -> Result<String, String>
            }
            task HandleResult : TaskBrief uses [Auth] {
                goal = "handle"
                step Handle {
                    let result = perform Auth.load(user_id)?;
                    let value = match result {
                        Ok(user) => user
                        Err(e) => e
                    };
                }
            }
        "#,
        );
        let errors: Vec<_> = diags.iter().filter(|d| d.is_error()).collect();
        assert!(errors.is_empty(), "unexpected errors: {errors:?}");
    }

    #[test]
    fn match_wildcard_arm_checks_successfully() {
        let diags = check_all_src(
            r#"
            sealed type Platform = iOS | Android | Web
            task Render : TaskBrief {
                goal = "render"
                extras { platform: Platform }
                step Render {
                    let view = match platform {
                        iOS => "a"
                        _ => "b"
                    };
                }
            }
        "#,
        );
        let errors: Vec<_> = diags.iter().filter(|d| d.is_error()).collect();
        assert!(errors.is_empty(), "unexpected errors: {errors:?}");
    }

    #[test]
    fn exhaustive_match_with_wildcard_has_no_e207() {
        let diags = check_src(
            r#"
            sealed type Platform = iOS | Android | Web
            task Render : TaskBrief {
                goal = "render"
                extras { platform: Platform }
                step Render {
                    let view = match platform {
                        iOS => "a"
                        _ => "b"
                    };
                }
            }
        "#,
        );
        assert!(
            diags
                .iter()
                .all(|d| d.code != ErrorCode::NonExhaustiveMatch),
            "unexpected diags: {diags:?}"
        );
    }

    #[test]
    fn exhaustive_match_covering_all_sealed_variants_has_no_e207() {
        let diags = check_src(
            r#"
            sealed type Platform = iOS | Android | Web
            task Render : TaskBrief {
                goal = "render"
                extras { platform: Platform }
                step Render {
                    let view = match platform {
                        iOS => "a"
                        Android => "b"
                        Web => "c"
                    };
                }
            }
        "#,
        );
        assert!(
            diags
                .iter()
                .all(|d| d.code != ErrorCode::NonExhaustiveMatch),
            "unexpected diags: {diags:?}"
        );
    }

    #[test]
    fn non_exhaustive_match_emits_e207_with_missing_variants() {
        let diags = check_src(
            r#"
            sealed type Platform = iOS | Android | Web
            task Render : TaskBrief {
                goal = "render"
                extras { platform: Platform }
                step Render {
                    let view = match platform {
                        iOS => "a"
                        Android => "b"
                    };
                }
            }
        "#,
        );
        let diag = diags
            .iter()
            .find(|d| d.code == ErrorCode::NonExhaustiveMatch)
            .expect("missing E207 diagnostic");
        assert_eq!(
            diag.message,
            "non-exhaustive match on 'Platform': missing variants [Web]"
        );
    }

    #[test]
    fn nested_perform_in_match_arm_checks_successfully() {
        let diags = check_all_src(
            r#"
            import skill "Skill"
            sealed type Platform = iOS | Android | Web
            effect Skill {
                fn mobile() -> String
                fn web() -> String
            }
            task Render : TaskBrief uses [Skill] {
                goal = "render"
                extras { platform: Platform }
                step Render {
                    let view = match platform {
                        iOS => perform Skill.mobile()?
                        _ => perform Skill.web()?
                    };
                }
            }
        "#,
        );
        let errors: Vec<_> = diags.iter().filter(|d| d.is_error()).collect();
        assert!(errors.is_empty(), "unexpected errors: {errors:?}");
    }

    #[test]
    fn linear_binding_used_in_multiple_match_arms_is_not_reused() {
        let diags = check_src(
            r#"
            import skill "Payment"
            sealed type Platform = iOS | Android | Web
            effect Payment {
                fn charge(amount: String) -> @once Handle
                fn confirm(handle: Handle) -> String
            }
            task Pay : TaskBrief uses [Payment] {
                goal = "pay"
                extras { platform: Platform }
                step Decide {
                    let handle = perform Payment.charge(amount)?;
                    let receipt = match platform {
                        iOS => perform Payment.confirm(handle)?
                        Android => perform Payment.confirm(handle)?
                        _ => "skip"
                    };
                }
            }
        "#,
        );
        let linear_errors: Vec<_> = diags
            .iter()
            .filter(|d| {
                d.code == ErrorCode::LinearBindingDropped
                    || d.code == ErrorCode::LinearBindingReused
            })
            .collect();
        assert!(
            linear_errors.is_empty(),
            "unexpected linear errors: {linear_errors:?}"
        );
    }

    #[test]
    fn error_on_missing_goal() {
        let diags = check_src("task T : TaskBrief {}");
        assert!(
            diags.iter().any(|d| d.code == ErrorCode::MissingGoal),
            "{diags:?}"
        );
    }

    #[test]
    fn error_on_undeclared_uses() {
        let diags = check_src(r#"task T : TaskBrief uses [GraphQL] { goal = "x" }"#);
        assert!(
            diags
                .iter()
                .any(|d| d.code == ErrorCode::UndeclaredSkillInUses),
            "{diags:?}"
        );
    }

    // ── Phase 3: linear bindings ────────────────────────────────────────────

    #[test]
    fn linear_binding_dropped() {
        // @once let x = ... but x is never used → E105
        let diags = check_src(
            r#"
            import skill "Payment"
            task Pay : TaskBrief uses [Payment] {
                goal = "charge"
                step Charge {
                    @once let handle = perform Payment.charge(amount)?;
                }
            }
        "#,
        );
        assert!(
            diags
                .iter()
                .any(|d| d.code == ErrorCode::LinearBindingDropped),
            "{diags:?}"
        );
    }

    #[test]
    fn linear_binding_reused() {
        // @once let x = ... but x is used twice → E104
        let diags = check_src(
            r#"
            import skill "Payment"
            task Pay : TaskBrief uses [Payment] {
                goal = "charge"
                step Charge {
                    @once let handle = perform Payment.charge(amount)?;
                    let a = perform Payment.confirm(handle)?;
                    let b = perform Payment.confirm(handle)?;
                }
            }
        "#,
        );
        assert!(
            diags
                .iter()
                .any(|d| d.code == ErrorCode::LinearBindingReused),
            "{diags:?}"
        );
    }

    #[test]
    fn linear_binding_used_once_is_ok() {
        // @once let x = ... and x used exactly once → no linear error
        let diags = check_src(
            r#"
            import skill "Payment"
            task Pay : TaskBrief uses [Payment] {
                goal = "charge"
                step Charge {
                    @once let handle = perform Payment.charge(amount)?;
                    let receipt = perform Payment.confirm(handle)?;
                }
            }
        "#,
        );
        let linear_errors: Vec<_> = diags
            .iter()
            .filter(|d| {
                d.code == ErrorCode::LinearBindingDropped
                    || d.code == ErrorCode::LinearBindingReused
            })
            .collect();
        assert!(
            linear_errors.is_empty(),
            "unexpected linear errors: {linear_errors:?}"
        );
    }

    // ── Phase 3: effect group aliases ────────────────────────────────────────

    #[test]
    fn effect_group_expands_in_uses() {
        // `type AuthEffects = [Auth, Session]` — if Auth/Session imported, no E102
        let diags = check_src(
            r#"
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
        "#,
        );
        let e102: Vec<_> = diags
            .iter()
            .filter(|d| d.code == ErrorCode::UndeclaredSkillInUses)
            .collect();
        assert!(e102.is_empty(), "unexpected E102: {e102:?}");
    }

    // ── Phase 3: type alias validation ──────────────────────────────────────

    #[test]
    fn type_alias_unknown_attr_is_warned() {
        let diags = check_src(r#"type Email = @bogusAttr String"#);
        assert!(
            diags
                .iter()
                .any(|d| d.code == ErrorCode::AttributeConstraint),
            "{diags:?}"
        );
    }

    #[test]
    fn type_alias_valid_no_error() {
        let diags = check_src(r#"type Email = @matches("[^@]+@[^@]+") String"#);
        let errors: Vec<_> = diags.iter().filter(|d| d.is_error()).collect();
        assert!(errors.is_empty(), "unexpected errors: {errors:?}");
    }

    #[test]
    fn typed_extras_with_known_type_is_ok() {
        let src = r#"
            sealed type Platform = iOS | Android | Web
            task X : TaskBrief {
                goal = "x"
                extras { platform: Platform }
            }
        "#;
        let diags = check_str(src);
        assert!(
            diags
                .iter()
                .all(|d| d.code != ErrorCode::UnknownExtrasField),
            "{diags:?}"
        );
    }

    #[test]
    fn typed_extras_with_unknown_type_is_e208() {
        let src = r#"
            task X : TaskBrief {
                goal = "x"
                extras { figmaURL: FigmaUrl }
            }
        "#;
        let diags = check_str(src);
        assert!(
            diags
                .iter()
                .any(|d| d.code == ErrorCode::UnknownExtrasField),
            "{diags:?}"
        );
    }

    #[test]
    fn string_extras_emits_w103_deprecation() {
        let src = r#"
            task X : TaskBrief {
                goal = "x"
                extras = ["platform": "iOS"]
            }
        "#;
        let diags = check_str(src);
        assert!(
            diags
                .iter()
                .any(|d| d.code == ErrorCode::DeprecatedStringExtras),
            "{diags:?}"
        );
    }

    #[test]
    fn typed_extras_sealed_type_has_no_e208() {
        let src = r#"
            sealed type Platform = iOS | Android | Web
            task DeploymentBrief : TaskBrief {
                goal = "deploy"
                extras { platform: Platform }
            }
        "#;
        let diags = check_str(src);
        assert!(
            diags
                .iter()
                .all(|d| d.code != ErrorCode::UnknownExtrasField),
            "{diags:?}"
        );
    }

    #[test]
    fn typed_extras_undeclared_type_emits_e208() {
        let src = r#"
            task DeploymentBrief : TaskBrief {
                goal = "deploy"
                extras { platform: UndeclaredType }
            }
        "#;
        let diags = check_str(src);
        let diag = diags
            .iter()
            .find(|d| d.code == ErrorCode::UnknownExtrasField)
            .expect("missing E208 diagnostic");
        assert_eq!(
            diag.message,
            "extras field 'platform' has unknown type 'UndeclaredType' — declare it as a sealed type or struct"
        );
    }

    #[test]
    fn brief_builder_without_provides_emits_w104() {
        let src = r#"
            @BriefBuilder
            task DeploymentBrief : TaskBrief {
                goal = "deploy"
            }
        "#;
        let diags = check_str(src);
        assert!(
            diags
                .iter()
                .any(|d| d.code == ErrorCode::BriefBuilderProvidesMissing),
            "{diags:?}"
        );
    }

    #[test]
    fn brief_builder_with_provides_has_no_w104() {
        let src = r#"
            @BriefBuilder
            task DeploymentBrief : TaskBrief {
                goal = "deploy"
                provides { deploymentUrl: String }
            }
        "#;
        let diags = check_str(src);
        assert!(
            diags
                .iter()
                .all(|d| d.code != ErrorCode::BriefBuilderProvidesMissing),
            "{diags:?}"
        );
    }

    #[test]
    fn effect_contract_allows_declared_skill_effects() {
        let skill_content = r#"
        effects [network]
        interface NetworkService {
            fn fetch(url: String) -> Response
        }
        "#;
        let src = r#"
import skill "NetworkService"
task Fetch : TaskBrief uses [NetworkService] {
    goal = "fetch"
    effects [network]
    step Run {
        let result = perform NetworkService.fetch("https://api.example.com")?;
    }
}
"#;
        let diags = check_src_with_skills(src, &[("NetworkService", skill_content)]);
        assert!(
            diags
                .iter()
                .all(|d| d.code != ErrorCode::EffectContractViolation),
            "unexpected E209: {diags:?}"
        );
    }

    #[test]
    fn effect_contract_rejects_undeclared_skill_effects() {
        let skill_content = r#"
        effects [network]
        interface NetworkService {
            fn fetch(url: String) -> Response
        }
        "#;
        let src = r#"
import skill "NetworkService"
task Fetch : TaskBrief uses [NetworkService] {
    goal = "fetch"
    step Run {
        let result = perform NetworkService.fetch("https://api.example.com")?;
    }
}
"#;
        let diags = check_src_with_skills(src, &[("NetworkService", skill_content)]);
        assert!(
            diags
                .iter()
                .any(|d| d.code == ErrorCode::EffectContractViolation),
            "expected E209: {diags:?}"
        );
    }

    // ── Effect fn @once return type auto-marks binding ───────────────────────

    #[test]
    fn inline_effect_once_return_auto_linear() {
        // Inline effect declares `fn charge() -> @once Handle`
        // → the bound variable is auto-linear without explicit `@once let`
        let diags = check_src(
            r#"
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
        "#,
        );
        let linear_errors: Vec<_> = diags
            .iter()
            .filter(|d| {
                d.code == ErrorCode::LinearBindingDropped
                    || d.code == ErrorCode::LinearBindingReused
            })
            .collect();
        assert!(
            linear_errors.is_empty(),
            "unexpected linear errors: {linear_errors:?}"
        );
    }

    #[test]
    fn inline_effect_once_return_dropped_is_error() {
        let diags = check_src(
            r#"
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
        "#,
        );
        assert!(
            diags
                .iter()
                .any(|d| d.code == ErrorCode::LinearBindingDropped),
            "{diags:?}"
        );
    }

    // ── E103: effect performed but not in uses ───────────────────────────────

    #[test]
    fn e103_effect_performed_not_in_uses() {
        let diags = check_src(
            r#"
            import skill "GraphQL"
            import skill "Analytics"
            task T : TaskBrief uses [GraphQL] {
                goal = "fetch"
                step Do {
                    let _ = perform Analytics.track(event)?;
                }
            }
        "#,
        );
        assert!(
            diags
                .iter()
                .any(|d| d.code == ErrorCode::PerformWithoutUses),
            "{diags:?}"
        );
    }

    #[test]
    fn e103_not_fired_when_skill_in_uses() {
        let diags = check_src(
            r#"
            import skill "GraphQL"
            task T : TaskBrief uses [GraphQL] {
                goal = "fetch"
                step Do {
                    let user = perform GraphQL.query(UserQuery)?;
                }
            }
        "#,
        );
        let e103: Vec<_> = diags
            .iter()
            .filter(|d| d.code == ErrorCode::PerformWithoutUses)
            .collect();
        assert!(e103.is_empty(), "unexpected E103: {e103:?}");
    }

    // ── E101: missing goal ────────────────────────────────────────────────────

    #[test]
    fn e101_fires_on_task_without_goal() {
        let diags = check_src(r#"task T : TaskBrief { step S {} }"#);
        assert!(
            diags.iter().any(|d| d.code == ErrorCode::MissingGoal),
            "{diags:?}"
        );
    }

    #[test]
    fn e101_not_fired_when_goal_present() {
        let diags = check_src(r#"task T : TaskBrief { goal = "do something" }"#);
        let e101: Vec<_> = diags
            .iter()
            .filter(|d| d.code == ErrorCode::MissingGoal)
            .collect();
        assert!(e101.is_empty(), "unexpected E101: {e101:?}");
    }

    // ── E102: skill in uses but not imported ──────────────────────────────────

    #[test]
    fn e102_fires_when_uses_without_import() {
        let diags = check_src(r#"task T : TaskBrief uses [GraphQL] { goal = "x" }"#);
        assert!(
            diags
                .iter()
                .any(|d| d.code == ErrorCode::UndeclaredSkillInUses),
            "{diags:?}"
        );
    }

    #[test]
    fn e102_not_fired_when_imported() {
        let diags = check_src(
            r#"
            import skill "GraphQL"
            task T : TaskBrief uses [GraphQL] { goal = "x" }
        "#,
        );
        let e102: Vec<_> = diags
            .iter()
            .filter(|d| d.code == ErrorCode::UndeclaredSkillInUses)
            .collect();
        assert!(e102.is_empty(), "unexpected E102: {e102:?}");
    }

    // ── Multiple errors can coexist ───────────────────────────────────────────

    #[test]
    fn multiple_errors_on_bad_task() {
        // No goal + skill in uses but not imported
        let diags = check_src(r#"task T : TaskBrief uses [Missing] {}"#);
        let has_e101 = diags.iter().any(|d| d.code == ErrorCode::MissingGoal);
        let has_e102 = diags
            .iter()
            .any(|d| d.code == ErrorCode::UndeclaredSkillInUses);
        assert!(has_e101, "expected E101: {diags:?}");
        assert!(has_e102, "expected E102: {diags:?}");
    }

    // ── Test blocks don't generate checker errors ─────────────────────────────

    #[test]
    fn test_blocks_do_not_generate_checker_errors() {
        // test { } blocks with mock/run/assert should be transparent to the checker
        let diags = check_src(
            r#"
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
        "#,
        );
        let hard_errors: Vec<_> = diags.iter().filter(|d| d.is_error()).collect();
        // check_src uses allow_missing_skills=true so E107 is suppressed; only structural errors fire
        assert!(
            hard_errors.is_empty(),
            "unexpected hard errors: {hard_errors:?}"
        );
    }

    #[test]
    fn test_block_does_not_require_mock_skills_to_be_imported_twice() {
        // The test block references skills only via mock — it should not
        // cause E102/E103 even if the mock refers to a skill not declared in uses
        let diags = check_src(
            r#"
            import skill "GraphQL"
            task T : TaskBrief uses [GraphQL] { goal = "x" }
            test "uses unimported mock" {
                mock SomeMockSkill { fn foo(x) -> Ok("y") }
                run T
                assert result is Ok
            }
        "#,
        );
        // check_src uses allow_missing_skills=true so E107 is suppressed — only E102/E103 would appear
        let hard: Vec<_> = diags.iter().filter(|d| d.is_error()).collect();
        assert!(
            hard.is_empty(),
            "unexpected errors from test block mock: {hard:?}"
        );
    }

    // ── E107: skill has no interface file ─────────────────────────────────────

    #[test]
    fn e107_fires_on_imported_skill_without_briefskill() {
        // Use strict mode (allow_missing_skills=false) — E107 should fire
        let (tokens, _) = lex(r#"
            import skill "GraphQL"
            task T : TaskBrief uses [GraphQL] { goal = "x" }
        "#);
        let (program, _) = parse(
            &tokens,
            r#"
            import skill "GraphQL"
            task T : TaskBrief uses [GraphQL] { goal = "x" }
        "#,
        );
        let ctx = CheckContext {
            file_dir: Path::new("."),
            cwd: Path::new("."),
            manifest: None,
            brief_path: None,
            allow_missing_skills: false,
        };
        let diags = check(&program, &ctx);
        assert!(
            diags
                .iter()
                .any(|d| d.code == ErrorCode::MissingSkillInterface),
            "{diags:?}"
        );
        assert!(
            diags.iter().any(|d| d.is_error()),
            "E107 must be an error: {diags:?}"
        );
    }

    #[test]
    fn e107_suppressed_by_allow_missing_skills() {
        // allow_missing_skills=true → no E107 emitted at all
        let diags = check_src(
            r#"
            import skill "GraphQL"
            task T : TaskBrief uses [GraphQL] { goal = "x" }
        "#,
        );
        let e107: Vec<_> = diags
            .iter()
            .filter(|d| d.code == ErrorCode::MissingSkillInterface)
            .collect();
        assert!(
            e107.is_empty(),
            "E107 should be suppressed by allow_missing_skills: {e107:?}"
        );
    }

    // ── Effect group used in multiple tasks ───────────────────────────────────

    #[test]
    fn effect_group_works_across_multiple_tasks() {
        let diags = check_src(
            r#"
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
        "#,
        );
        let e102: Vec<_> = diags
            .iter()
            .filter(|d| d.code == ErrorCode::UndeclaredSkillInUses)
            .collect();
        assert!(e102.is_empty(), "unexpected E102: {e102:?}");
    }

    // ── @mcp attribute on type alias is valid ─────────────────────────────────

    #[test]
    fn mcp_attribute_on_type_alias_is_valid() {
        let diags = check_src(r#"type GitHubMCP = @mcp GitHub"#);
        let errors: Vec<_> = diags.iter().filter(|d| d.is_error()).collect();
        assert!(
            errors.is_empty(),
            "unexpected errors for @mcp alias: {errors:?}"
        );
    }

    // ── Cross-step linear tracking (two-level task_linear) ────────────────────

    #[test]
    fn linear_binding_consumed_in_later_step_is_ok() {
        // @once let handle declared in step 1, consumed in step 2 — no error.
        let diags = check_src(
            r#"
            import skill "Payment"
            task Pay : TaskBrief uses [Payment] {
                goal = "charge"
                step Charge {
                    @once let handle = perform Payment.charge(amount)?;
                }
                step Confirm {
                    let r = perform Payment.confirm(handle)?;
                }
            }
        "#,
        );
        let linear_errors: Vec<_> = diags
            .iter()
            .filter(|d| {
                d.code == ErrorCode::LinearBindingDropped
                    || d.code == ErrorCode::LinearBindingReused
            })
            .collect();
        assert!(
            linear_errors.is_empty(),
            "unexpected linear errors: {linear_errors:?}"
        );
    }

    #[test]
    fn linear_binding_never_consumed_across_steps_is_e105() {
        // @once let handle declared in step 1, never used in any step → E105.
        let diags = check_src(
            r#"
            import skill "Payment"
            task Pay : TaskBrief uses [Payment] {
                goal = "charge"
                step Charge {
                    @once let handle = perform Payment.charge(amount)?;
                }
                step Done {
                    let x = perform Payment.finalize(amount)?;
                }
            }
        "#,
        );
        assert!(
            diags
                .iter()
                .any(|d| d.code == ErrorCode::LinearBindingDropped),
            "{diags:?}"
        );
    }

    #[test]
    fn linear_binding_reused_across_steps_is_e104() {
        // @once handle promoted to task_linear (step 1 declares but doesn't use it).
        // Step 2 uses it twice in one step → E104.
        let diags = check_src(
            r#"
            import skill "Payment"
            task Pay : TaskBrief uses [Payment] {
                goal = "charge"
                step Charge {
                    @once let handle = perform Payment.charge(amount)?;
                }
                step Confirm {
                    let a = perform Payment.confirm(handle)?;
                    let b = perform Payment.confirm(handle)?;
                }
            }
        "#,
        );
        assert!(
            diags
                .iter()
                .any(|d| d.code == ErrorCode::LinearBindingReused),
            "expected E104 for cross-step reuse: {diags:?}"
        );
    }

    #[test]
    fn linear_binding_shadowed_before_consumed_is_e105() {
        // @once x declared in step 1, then re-declared @once in step 2 → E105 for old x.
        let diags = check_src(
            r#"
            import skill "Payment"
            task Pay : TaskBrief uses [Payment] {
                goal = "charge"
                step A {
                    @once let handle = perform Payment.charge(amount)?;
                }
                step B {
                    @once let handle = perform Payment.charge(other)?;
                    let _ = perform Payment.confirm(handle)?;
                }
            }
        "#,
        );
        assert!(
            diags
                .iter()
                .any(|d| d.code == ErrorCode::LinearBindingDropped),
            "expected E105 for shadow-before-consume: {diags:?}"
        );
    }

    // ── Spec coverage (E301/E302) ───────────────────────────────────────────

    /// Build a CheckContext that points at a temp dir containing `.briefskill` files.
    fn check_src_with_skills(src: &str, skills: &[(&str, &str)]) -> Vec<BriefError> {
        use std::fs;
        let dir = tempfile::tempdir().expect("tempdir");
        let skills_root = dir.path().join(".claude").join("skills");
        for (name, content) in skills {
            let skill_dir = skills_root.join(name);
            fs::create_dir_all(&skill_dir).unwrap();
            fs::write(skill_dir.join(format!("{name}.briefskill")), content).unwrap();
        }
        let (tokens, _) = lex(src);
        let (program, _) = parse(&tokens, src);
        let ctx = CheckContext {
            file_dir: dir.path(),
            cwd: dir.path(),
            manifest: None,
            brief_path: None,
            allow_missing_skills: false,
        };
        check(&program, &ctx)
    }

    #[test]
    fn e301_range_lower_boundary_missing() {
        let skill_content = r#"interface Mapper {
    fn mapValue(input: @range(0, 100) Int) -> Code
}"#;
        // test block only covers max=100, not min=0
        let src = r#"
import skill "Mapper"
test "coverage" {
    perform Mapper.mapValue(100);
}
"#;
        let diags = check_src_with_skills(src, &[("Mapper", skill_content)]);
        assert!(
            diags
                .iter()
                .any(|d| d.code == ErrorCode::RangeBoundaryMissing),
            "{diags:?}"
        );
        // Ensure the lower boundary (0) is the one flagged
        let e301: Vec<_> = diags
            .iter()
            .filter(|d| d.code == ErrorCode::RangeBoundaryMissing)
            .collect();
        assert!(
            e301.iter().any(|d| d.message.contains("arg = 0")),
            "{e301:?}"
        );
    }

    #[test]
    fn e301_range_upper_boundary_missing() {
        let skill_content = r#"interface Mapper {
    fn mapValue(input: @range(0, 100) Int) -> Code
}"#;
        let src = r#"
import skill "Mapper"
test "coverage" {
    perform Mapper.mapValue(0);
}
"#;
        let diags = check_src_with_skills(src, &[("Mapper", skill_content)]);
        let e301: Vec<_> = diags
            .iter()
            .filter(|d| d.code == ErrorCode::RangeBoundaryMissing)
            .collect();
        assert!(
            !e301.is_empty(),
            "expected E301 for missing upper boundary: {diags:?}"
        );
        assert!(
            e301.iter().any(|d| d.message.contains("arg = 100")),
            "{e301:?}"
        );
    }

    #[test]
    fn e301_range_both_boundaries_present_no_error() {
        let skill_content = r#"interface Mapper {
    fn mapValue(input: @range(0, 100) Int) -> Code
}"#;
        let src = r#"
import skill "Mapper"
test "coverage" {
    perform Mapper.mapValue(0);
    perform Mapper.mapValue(100);
}
"#;
        let diags = check_src_with_skills(src, &[("Mapper", skill_content)]);
        let e301: Vec<_> = diags
            .iter()
            .filter(|d| d.code == ErrorCode::RangeBoundaryMissing)
            .collect();
        assert!(e301.is_empty(), "unexpected E301: {e301:?}");
    }

    #[test]
    fn e302_enum_value_missing() {
        let skill_content = r#"interface Classifier {
    fn classify(cat: @enum("ok", "warn", "err") String) -> Status
}"#;
        let src = r#"
import skill "Classifier"
test "coverage" {
    perform Classifier.classify("ok");
    perform Classifier.classify("warn");
}
"#;
        // "err" is missing
        let diags = check_src_with_skills(src, &[("Classifier", skill_content)]);
        let e302: Vec<_> = diags
            .iter()
            .filter(|d| d.code == ErrorCode::EnumValueMissing)
            .collect();
        assert!(
            !e302.is_empty(),
            "expected E302 for missing \"err\": {diags:?}"
        );
        assert!(
            e302.iter().any(|d| d.message.contains("\"err\"")),
            "{e302:?}"
        );
    }

    #[test]
    fn e302_enum_all_values_covered_no_error() {
        let skill_content = r#"interface Classifier {
    fn classify(cat: @enum("ok", "warn", "err") String) -> Status
}"#;
        let src = r#"
import skill "Classifier"
test "coverage" {
    perform Classifier.classify("ok");
    perform Classifier.classify("warn");
    perform Classifier.classify("err");
}
"#;
        let diags = check_src_with_skills(src, &[("Classifier", skill_content)]);
        let e302: Vec<_> = diags
            .iter()
            .filter(|d| d.code == ErrorCode::EnumValueMissing)
            .collect();
        assert!(e302.is_empty(), "unexpected E302: {e302:?}");
    }

    // ── Lock gate (E303) ────────────────────────────────────────────────────

    fn check_src_with_skills_and_path(
        src: &str,
        skills: &[(&str, &str)],
        brief_path: Option<&Path>,
    ) -> Vec<BriefError> {
        use std::fs;
        let dir = tempfile::tempdir().expect("tempdir");
        let skills_root = dir.path().join(".claude").join("skills");
        for (name, content) in skills {
            let skill_dir = skills_root.join(name);
            fs::create_dir_all(&skill_dir).unwrap();
            fs::write(skill_dir.join(format!("{name}.briefskill")), content).unwrap();
        }
        let (tokens, _) = lex(src);
        let (program, _) = parse(&tokens, src);
        let ctx = CheckContext {
            file_dir: dir.path(),
            cwd: dir.path(),
            manifest: None,
            brief_path,
            allow_missing_skills: false,
        };
        check(&program, &ctx)
    }

    #[test]
    fn e303_missing_lock_for_dynamic_annotation() {
        let skill_content = r#"interface Payment {
    fn charge(amount: Int, webhook: @stripe-hook String) -> Handle
}"#;
        let src = r#"
import skill "Payment"
task Pay : TaskBrief uses [Payment] { goal = "charge" }
"#;
        // brief_path points to a nonexistent file → lock also nonexistent
        let brief_path = Path::new("./nonexistent_for_test.brief");
        let diags =
            check_src_with_skills_and_path(src, &[("Payment", skill_content)], Some(brief_path));
        assert!(
            diags.iter().any(|d| d.code == ErrorCode::LockRequired),
            "expected E303 for missing lock: {diags:?}"
        );
    }

    #[test]
    fn e303_no_error_when_only_static_constraints() {
        let skill_content = r#"interface Mapper {
    fn mapValue(input: @range(0, 100) Int) -> Code
}"#;
        let src = r#"
import skill "Mapper"
task T : TaskBrief uses [Mapper] { goal = "map" }
"#;
        // No dynamic annotations → lock gate should NOT fire
        let diags = check_src_with_skills(src, &[("Mapper", skill_content)]);
        let e303: Vec<_> = diags
            .iter()
            .filter(|d| d.code == ErrorCode::LockRequired)
            .collect();
        assert!(e303.is_empty(), "unexpected E303: {e303:?}");
    }

    #[test]
    fn e303_no_error_when_brief_path_is_none() {
        let skill_content = r#"interface Payment {
    fn charge(amount: Int, webhook: @stripe-hook String) -> Handle
}"#;
        let src = r#"
import skill "Payment"
task Pay : TaskBrief uses [Payment] { goal = "charge" }
"#;
        // brief_path = None → lock gate skips gracefully
        let diags = check_src_with_skills_and_path(src, &[("Payment", skill_content)], None);
        let e303: Vec<_> = diags
            .iter()
            .filter(|d| d.code == ErrorCode::LockRequired)
            .collect();
        assert!(
            e303.is_empty(),
            "unexpected E303 when brief_path=None: {e303:?}"
        );
    }
}
