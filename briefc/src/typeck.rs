/// Type checker for Brief v0.1.
///
/// Builds a type environment from declarations (sealed types, structs, protocols,
/// effects) and validates:
///   - E201: type references that point to unknown names
///   - E202: perform calls with wrong argument count (inline and cross-file via .briefskill)
///   - E203: struct field attribute constraints (basic regex/format checks)
///
/// This is a structural type checker — not full HM inference. Full HM with
/// generics is deferred to v0.2.
use std::collections::{HashMap, HashSet};

use crate::ast::*;
use crate::errors::{BriefError, ErrorCode};
use crate::skillgen::SkillInterface;

// ─────────────────────────────────────────────────────────────────────────────
// Built-in types (always in scope)
// ─────────────────────────────────────────────────────────────────────────────

/// Domain-specific built-in types that are always in scope.
fn builtin_types() -> &'static HashSet<&'static str> {
    use std::sync::OnceLock;
    static SET: OnceLock<HashSet<&'static str>> = OnceLock::new();
    SET.get_or_init(|| {
        [
            "String",
            "Bool",
            "Int",
            "Float",
            "Unit",
            "Result",
            "Option",
            "Component",
            "Theme",
            "Color",
            "Schema",
            "Operation",
            "Mutation",
            "Handle",
            "IOError",
            "AsyncError",
            "QueryError",
            "MutationError",
            "DesignError",
            "TokenError",
            "FetchError",
            "ButtonStyle",
            "User",
            "UserProfile",
        ]
        .into_iter()
        .collect()
    })
}

fn is_builtin_type(name: &str) -> bool {
    builtin_types().contains(name)
}

fn is_local_generic(name: &str, local_generics: &[String]) -> bool {
    local_generics.iter().any(|generic| generic == name)
}

// ─────────────────────────────────────────────────────────────────────────────
// Type environment
// ─────────────────────────────────────────────────────────────────────────────

/// Recursively collect all named types from a `TypeRef` tree.
fn collect_type_names(ty: &TypeRef, local_generics: &[String], out: &mut HashSet<String>) {
    if !ty.name.is_empty()
        && !is_builtin_type(&ty.name)
        && !is_local_generic(&ty.name, local_generics)
    {
        out.insert(ty.name.clone());
    }
    for arg in &ty.args {
        collect_type_names(arg, local_generics, out);
    }
}

/// The type environment built from a Program's declarations.
pub struct TypeEnv<'a> {
    sealed_types: HashMap<&'a str, &'a SealedTypeDecl>,
    structs: HashMap<&'a str, &'a StructDecl>,
    protocols: HashMap<&'a str, &'a ProtocolDecl>,
    effects: HashMap<&'a str, &'a EffectDecl>,
    skill_ifaces: HashMap<String, SkillInterface>,
    /// Opaque types declared as return types of inline `effect` fn signatures.
    /// These don't need explicit struct/sealed type declarations.
    effect_ret_types: HashSet<String>,
}

impl<'a> TypeEnv<'a> {
    #[allow(dead_code)]
    pub fn from_program(program: &'a Program) -> Self {
        Self::from_program_with_skills(program, HashMap::new())
    }

    pub fn from_program_with_skills(
        program: &'a Program,
        skill_ifaces: HashMap<String, SkillInterface>,
    ) -> Self {
        let mut sealed_types = HashMap::new();
        let mut structs = HashMap::new();
        let mut protocols = HashMap::new();
        let mut effects = HashMap::new();

        for t in &program.types {
            sealed_types.insert(t.name.as_str(), t);
        }
        for s in &program.structs {
            structs.insert(s.name.as_str(), s);
        }
        for p in &program.protocols {
            protocols.insert(p.name.as_str(), p);
        }
        for e in &program.effects {
            effects.insert(e.name.as_str(), e);
        }

        // Collect all type names from effect return types — treated as opaque.
        let mut effect_ret_types = HashSet::new();
        for e in &program.effects {
            for f in &e.fns {
                let local_generics = merged_generics(&e.params, &f.type_params);
                collect_type_names(&f.ret, &local_generics, &mut effect_ret_types);
            }
        }

        Self {
            sealed_types,
            structs,
            protocols,
            effects,
            skill_ifaces,
            effect_ret_types,
        }
    }

    fn declared_type_exists(&self, name: &str) -> bool {
        self.sealed_types.contains_key(name)
            || self.structs.contains_key(name)
            || self.protocols.contains_key(name)
            || self.effect_ret_types.contains(name)
    }

    fn shadowed_type_description(&self, name: &str) -> Option<String> {
        if is_builtin_type(name) {
            Some(format!("the built-in type '{name}'"))
        } else if self.declared_type_exists(name) {
            Some(format!("the declared type '{name}'"))
        } else {
            None
        }
    }

    /// Check whether a type name is declared (built-in, scoped generic, or user-defined).
    pub fn type_exists(&self, name: &str, local_generics: &[String]) -> bool {
        is_local_generic(name, local_generics)
            || is_builtin_type(name)
            || self.declared_type_exists(name)
    }

    /// Look up a function in a named effect (inline declarations or skill interfaces).
    pub fn effect_fn_arg_count(&self, effect_name: &str, fn_name: &str) -> Option<usize> {
        // 1. Check inline effect declarations first.
        if let Some(decl) = self.effects.get(effect_name) {
            if let Some(f) = decl.fns.iter().find(|f| f.name == fn_name) {
                return Some(f.params.len());
            }
        }
        // 2. Fall back to .briefskill interface.
        if let Some(iface) = self.skill_ifaces.get(effect_name) {
            if let Some(f) = iface.funcs.iter().find(|f| f.name == fn_name) {
                return Some(f.arg_count);
            }
        }
        None
    }

    /// Look up a function in a named inline effect declaration.
    pub fn effect_fn(&self, effect_name: &str, fn_name: &str) -> Option<&FnSignature> {
        self.effects
            .get(effect_name)
            .and_then(|e| e.fns.iter().find(|f| f.name == fn_name))
    }

    /// Look up a declared effect.
    #[allow(dead_code)]
    pub fn effect(&self, name: &str) -> Option<&&'a EffectDecl> {
        self.effects.get(name)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Entry point
// ─────────────────────────────────────────────────────────────────────────────

#[allow(dead_code)]
pub fn type_check(program: &Program) -> Vec<BriefError> {
    type_check_with_skills(program, HashMap::new())
}

pub fn type_check_with_skills(
    program: &Program,
    skill_ifaces: HashMap<String, SkillInterface>,
) -> Vec<BriefError> {
    let env = TypeEnv::from_program_with_skills(program, skill_ifaces);
    let mut diags = Vec::new();

    // Validate type references in struct field types and attribute types.
    for s in &program.structs {
        check_struct(s, &env, &mut diags);
    }

    // Validate type references in protocol method signatures.
    for p in &program.protocols {
        check_protocol(p, &env, &mut diags);
    }

    // Validate type references in effect function signatures.
    for e in &program.effects {
        check_effect(e, &env, &mut diags);
    }

    // Validate type references in sealed type variants.
    for t in &program.types {
        check_sealed_type(t, &env, &mut diags);
    }

    // Validate perform calls in task steps against inline effect declarations.
    for task in &program.tasks {
        check_task_types(task, &env, &mut diags);
    }

    diags
}

// ─────────────────────────────────────────────────────────────────────────────
// Declaration checkers
// ─────────────────────────────────────────────────────────────────────────────

fn merged_generics(outer: &[String], inner: &[String]) -> Vec<String> {
    let mut generics = Vec::with_capacity(outer.len() + inner.len());
    generics.extend(outer.iter().cloned());
    generics.extend(inner.iter().cloned());
    generics
}

fn generic_param_hint(name: &str) -> String {
    let short = name
        .chars()
        .next()
        .map(|c| c.to_ascii_uppercase().to_string())
        .unwrap_or_else(|| "T".to_string());
    let alt: String = if name.len() <= 3 {
        format!("{name}T")
    } else {
        name.chars().take(3).collect()
    };
    format!("rename the type parameter to avoid shadowing (e.g., '{short}' or '{alt}')")
}

fn check_generic_param_conflicts(
    params: &[String],
    env: &TypeEnv<'_>,
    span: Span,
    diags: &mut Vec<BriefError>,
) {
    for param in params {
        if let Some(shadowed) = env.shadowed_type_description(param) {
            diags.push(BriefError {
                code: ErrorCode::ScopedGenericConflict,
                message: format!("generic type parameter '{param}' shadows {shadowed}"),
                span,
                hint: Some(generic_param_hint(param)),
            });
        }
    }
}

fn check_sealed_type(t: &SealedTypeDecl, env: &TypeEnv<'_>, diags: &mut Vec<BriefError>) {
    check_generic_param_conflicts(&t.params, env, t.span, diags);
    for variant in &t.variants {
        for field_ty in &variant.fields {
            check_type_ref(field_ty, env, &t.params, diags);
        }
    }
}

fn check_struct(s: &StructDecl, env: &TypeEnv<'_>, diags: &mut Vec<BriefError>) {
    check_generic_param_conflicts(&s.params, env, s.span, diags);
    for field in &s.fields {
        check_type_ref(&field.ty, env, &s.params, diags);
        for attr in &field.attrs {
            check_attribute_constraint(attr, &field.ty, diags);
        }
    }
}

fn check_protocol(p: &ProtocolDecl, env: &TypeEnv<'_>, diags: &mut Vec<BriefError>) {
    check_generic_param_conflicts(&p.params, env, p.span, diags);
    for method in &p.methods {
        let local_generics = merged_generics(&p.params, &method.type_params);
        check_fn_sig(method, env, &local_generics, diags);
    }
}

fn check_effect(e: &EffectDecl, env: &TypeEnv<'_>, diags: &mut Vec<BriefError>) {
    check_generic_param_conflicts(&e.params, env, e.span, diags);
    for f in &e.fns {
        let local_generics = merged_generics(&e.params, &f.type_params);
        check_fn_sig(f, env, &local_generics, diags);
    }
}

fn check_fn_sig(
    f: &FnSignature,
    env: &TypeEnv<'_>,
    local_generics: &[String],
    diags: &mut Vec<BriefError>,
) {
    check_generic_param_conflicts(&f.type_params, env, f.span, diags);
    for param in &f.params {
        check_type_ref(&param.ty, env, local_generics, diags);
    }
    check_type_ref(&f.ret, env, local_generics, diags);
}

// ─────────────────────────────────────────────────────────────────────────────
// Task type checking
// ─────────────────────────────────────────────────────────────────────────────

fn check_task_types(task: &Task, env: &TypeEnv<'_>, diags: &mut Vec<BriefError>) {
    for step in &task.steps {
        check_step_types(step, env, diags);
    }
}

fn check_step_types(step: &Step, env: &TypeEnv<'_>, diags: &mut Vec<BriefError>) {
    for stmt in &step.body {
        let expr = match stmt {
            Stmt::Let { value, .. } => value,
            Stmt::Expr { value, .. } => value,
        };
        check_expr_types(expr, env, diags);
    }
}

fn check_expr_types(expr: &Expr, env: &TypeEnv<'_>, diags: &mut Vec<BriefError>) {
    match expr {
        Expr::Perform {
            skill,
            func,
            args,
            span,
            ..
        } => {
            // Check arg count against inline effect or .briefskill interface.
            if let Some(expected) = env.effect_fn_arg_count(skill, func) {
                let got = args.len();
                if expected != got {
                    // Build a human-readable hint from inline sig if available.
                    let hint = env
                        .effect_fn(skill, func)
                        .map(|sig| {
                            format!(
                                "signature: fn {}({}) -> {}",
                                sig.name,
                                sig.params
                                    .iter()
                                    .map(|p| format!("{}: {}", p.name, p.ty.name))
                                    .collect::<Vec<_>>()
                                    .join(", "),
                                sig.ret.name
                            )
                        })
                        .or_else(|| Some(format!("expected {expected} argument(s)")));

                    diags.push(BriefError {
                        code: ErrorCode::WrongArgCount,
                        message: format!(
                            "{skill}.{func}() expects {expected} argument(s), got {got}"
                        ),
                        span: *span,
                        hint,
                    });
                }
            }
            // Recurse into args.
            for arg in args {
                check_expr_types(arg, env, diags);
            }
        }
        Expr::Await { expr: inner, .. } => check_expr_types(inner, env, diags),
        Expr::Call { args, .. } => {
            for arg in args {
                check_expr_types(arg, env, diags);
            }
        }
        Expr::Match { scrutinee, arms } => {
            check_expr_types(scrutinee, env, diags);
            for arm in arms {
                check_expr_types(&arm.body, env, diags);
            }
        }
        Expr::Ident { .. } | Expr::Str { .. } | Expr::Int { .. } => {}
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Type reference resolution
// ─────────────────────────────────────────────────────────────────────────────

fn check_type_ref(
    ty: &TypeRef,
    env: &TypeEnv<'_>,
    local_generics: &[String],
    diags: &mut Vec<BriefError>,
) {
    if !env.type_exists(&ty.name, local_generics) {
        diags.push(BriefError {
            code: ErrorCode::UnknownType,
            message: format!("unknown type `{}`", ty.name),
            span: ty.span,
            hint: Some(format!(
                "declare it with `sealed type {}` or `struct {}`",
                ty.name, ty.name
            )),
        });
    }
    // Recurse into generic arguments.
    for arg in &ty.args {
        check_type_ref(arg, env, local_generics, diags);
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Attribute constraint checking
// ─────────────────────────────────────────────────────────────────────────────

fn check_attribute_constraint(attr: &Attribute, ty: &TypeRef, diags: &mut Vec<BriefError>) {
    match attr.name.as_str() {
        "url" | "nonEmpty" | "matches" => {
            // These attributes are only valid on String fields.
            if ty.name != "String" {
                diags.push(BriefError {
                    code: ErrorCode::AttributeConstraint,
                    message: format!(
                        "@{} can only be applied to `String` fields, not `{}`",
                        attr.name, ty.name
                    ),
                    span: attr.span,
                    hint: Some(format!("change the field type to `String`")),
                });
            }
        }
        _ => {} // Unknown attributes are ignored (extensible)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::lex;
    use crate::parser::parse;

    fn typeck_src(src: &str) -> Vec<BriefError> {
        let (tokens, _) = lex(src);
        let (program, _) = parse(&tokens, src);
        type_check(&program)
    }

    #[test]
    fn no_errors_on_builtin_types() {
        let diags = typeck_src(
            r#"
            struct Profile {
                name: @nonEmpty String
                email: @matches(".*@.*") String
            }
        "#,
        );
        assert!(diags.is_empty(), "{diags:?}");
    }

    #[test]
    fn error_on_unknown_type() {
        let diags = typeck_src(
            r#"
            struct Foo {
                x: NonexistentType
            }
        "#,
        );
        assert!(
            diags.iter().any(|d| d.code == ErrorCode::UnknownType),
            "{diags:?}"
        );
    }

    #[test]
    fn no_error_on_declared_type() {
        let diags = typeck_src(
            r#"
            sealed type Status = Active | Done
            struct Item {
                status: Status
            }
        "#,
        );
        assert!(diags.is_empty(), "{diags:?}");
    }

    #[test]
    fn error_on_attribute_wrong_type() {
        let diags = typeck_src(
            r#"
            struct Foo {
                count: @nonEmpty Int
            }
        "#,
        );
        assert!(
            diags
                .iter()
                .any(|d| d.code == ErrorCode::AttributeConstraint),
            "{diags:?}"
        );
    }

    #[test]
    fn error_on_wrong_arg_count() {
        let diags = typeck_src(
            r#"
            effect MyEffect {
                fn doSomething(a: String, b: String) -> String
            }
            import skill "MyEffect"
            task T : TaskBrief uses [MyEffect] {
                goal = "test"
                step S {
                    let r = perform MyEffect.doSomething(onlyOneArg)?;
                }
            }
        "#,
        );
        assert!(
            diags.iter().any(|d| d.code == ErrorCode::WrongArgCount),
            "{diags:?}"
        );
    }

    #[test]
    fn no_error_on_correct_arg_count() {
        let diags = typeck_src(
            r#"
            effect MyEffect {
                fn doSomething(a: String, b: String) -> String
            }
            import skill "MyEffect"
            task T : TaskBrief uses [MyEffect] {
                goal = "test"
                step S {
                    let r = perform MyEffect.doSomething(arg1, arg2)?;
                }
            }
        "#,
        );
        // Only possible diag is W101 (missing .briefskill) — no type errors.
        assert!(
            diags
                .iter()
                .all(|d| !d.is_error() || d.code != ErrorCode::WrongArgCount),
            "{diags:?}"
        );
    }

    #[test]
    fn no_errors_on_sealed_type_variants() {
        let diags = typeck_src(
            r#"
            sealed type Platform = iOS | Android | Web
        "#,
        );
        assert!(diags.is_empty(), "{diags:?}");
    }

    #[test]
    fn no_errors_on_effect_with_builtins() {
        let diags = typeck_src(
            r#"
            effect GraphQL {
                fn query(op: Operation) -> Result
                fn schema(name: String) -> Schema
            }
        "#,
        );
        assert!(diags.is_empty(), "{diags:?}");
    }
}
