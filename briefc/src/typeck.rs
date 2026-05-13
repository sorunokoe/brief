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

use std::collections::HashMap;

use crate::ast::*;
use crate::errors::{BriefError, ErrorCode};
use crate::skillgen::SkillInterface;

// ─────────────────────────────────────────────────────────────────────────────
// Built-in types (always in scope)
// ─────────────────────────────────────────────────────────────────────────────

const BUILTIN_TYPES: &[&str] = &[
    "String", "Bool", "Int", "Float", "Unit",
    "Result", "Option",         // generic built-ins
    "Component", "Theme", "Color", "Schema", "Operation", "Mutation",
    "Handle", "IOError", "AsyncError", "QueryError", "MutationError",
    "DesignError", "TokenError", "FetchError", "ButtonStyle", "User",
    "UserProfile",
    // Allow T, E, U etc. as unconstrained generics
    "T", "E", "U", "V", "A", "B",
];

// ─────────────────────────────────────────────────────────────────────────────
// Type environment
// ─────────────────────────────────────────────────────────────────────────────

/// The type environment built from a Program's declarations.
pub struct TypeEnv<'a> {
    sealed_types:  HashMap<&'a str, &'a SealedTypeDecl>,
    structs:       HashMap<&'a str, &'a StructDecl>,
    protocols:     HashMap<&'a str, &'a ProtocolDecl>,
    effects:       HashMap<&'a str, &'a EffectDecl>,
    skill_ifaces:  HashMap<String, SkillInterface>,
}

impl<'a> TypeEnv<'a> {
    #[allow(dead_code)]
    pub fn from_program(program: &'a Program) -> Self {
        Self::from_program_with_skills(program, HashMap::new())
    }

    pub fn from_program_with_skills(program: &'a Program, skill_ifaces: HashMap<String, SkillInterface>) -> Self {
        let mut sealed_types = HashMap::new();
        let mut structs      = HashMap::new();
        let mut protocols    = HashMap::new();
        let mut effects      = HashMap::new();

        for t in &program.types     { sealed_types.insert(t.name.as_str(), t); }
        for s in &program.structs   { structs.insert(s.name.as_str(), s); }
        for p in &program.protocols { protocols.insert(p.name.as_str(), p); }
        for e in &program.effects   { effects.insert(e.name.as_str(), e); }

        Self { sealed_types, structs, protocols, effects, skill_ifaces }
    }

    /// Check whether a type name is declared (built-in or user-defined).
    pub fn type_exists(&self, name: &str) -> bool {
        BUILTIN_TYPES.contains(&name)
            || self.sealed_types.contains_key(name)
            || self.structs.contains_key(name)
            || self.protocols.contains_key(name)
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
        self.effects.get(effect_name)
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

pub fn type_check_with_skills(program: &Program, skill_ifaces: HashMap<String, SkillInterface>) -> Vec<BriefError> {
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

fn check_sealed_type(t: &SealedTypeDecl, env: &TypeEnv<'_>, diags: &mut Vec<BriefError>) {
    for variant in &t.variants {
        for field_ty in &variant.fields {
            check_type_ref(field_ty, env, diags);
        }
    }
}

fn check_struct(s: &StructDecl, env: &TypeEnv<'_>, diags: &mut Vec<BriefError>) {
    for field in &s.fields {
        check_type_ref(&field.ty, env, diags);
        for attr in &field.attrs {
            check_attribute_constraint(attr, &field.ty, diags);
        }
    }
}

fn check_protocol(p: &ProtocolDecl, env: &TypeEnv<'_>, diags: &mut Vec<BriefError>) {
    for method in &p.methods {
        check_fn_sig(method, env, diags);
    }
}

fn check_effect(e: &EffectDecl, env: &TypeEnv<'_>, diags: &mut Vec<BriefError>) {
    for f in &e.fns {
        check_fn_sig(f, env, diags);
    }
}

fn check_fn_sig(f: &FnSignature, env: &TypeEnv<'_>, diags: &mut Vec<BriefError>) {
    for param in &f.params {
        check_type_ref(&param.ty, env, diags);
    }
    check_type_ref(&f.ret, env, diags);
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
            Stmt::Let  { value, .. } => value,
            Stmt::Expr { value, .. } => value,
        };
        check_expr_types(expr, env, diags);
    }
}

fn check_expr_types(expr: &Expr, env: &TypeEnv<'_>, diags: &mut Vec<BriefError>) {
    match expr {
        Expr::Perform { skill, func, args, span, .. } => {
            // Check arg count against inline effect or .briefskill interface.
            if let Some(expected) = env.effect_fn_arg_count(skill, func) {
                let got = args.len();
                if expected != got {
                    // Build a human-readable hint from inline sig if available.
                    let hint = env.effect_fn(skill, func).map(|sig| format!(
                        "signature: fn {}({}) -> {}",
                        sig.name,
                        sig.params.iter().map(|p| format!("{}: {}", p.name, p.ty.name)).collect::<Vec<_>>().join(", "),
                        sig.ret.name
                    )).or_else(|| Some(format!("expected {expected} argument(s)")));

                    diags.push(BriefError {
                        code:    ErrorCode::WrongArgCount,
                        message: format!(
                            "{skill}.{func}() expects {expected} argument(s), got {got}"
                        ),
                        span:    *span,
                        hint,
                    });
                }
            }
            // Recurse into args.
            for arg in args { check_expr_types(arg, env, diags); }
        }
        Expr::Await { expr: inner, .. } => check_expr_types(inner, env, diags),
        Expr::Call  { args, .. }         => {
            for arg in args { check_expr_types(arg, env, diags); }
        }
        Expr::Ident { .. } | Expr::Str { .. } => {}
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Type reference resolution
// ─────────────────────────────────────────────────────────────────────────────

fn check_type_ref(ty: &TypeRef, env: &TypeEnv<'_>, diags: &mut Vec<BriefError>) {
    if !env.type_exists(&ty.name) {
        diags.push(BriefError {
            code:    ErrorCode::UnknownType,
            message: format!("unknown type `{}`", ty.name),
            span:    ty.span,
            hint:    Some(format!(
                "declare it with `sealed type {}` or `struct {}`",
                ty.name, ty.name
            )),
        });
    }
    // Recurse into generic arguments.
    for arg in &ty.args {
        check_type_ref(arg, env, diags);
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
                    code:    ErrorCode::AttributeConstraint,
                    message: format!(
                        "@{} can only be applied to `String` fields, not `{}`",
                        attr.name, ty.name
                    ),
                    span:    attr.span,
                    hint:    Some(format!("change the field type to `String`")),
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
        let diags = typeck_src(r#"
            struct Profile {
                name: @nonEmpty String
                email: @matches(".*@.*") String
            }
        "#);
        assert!(diags.is_empty(), "{diags:?}");
    }

    #[test]
    fn error_on_unknown_type() {
        let diags = typeck_src(r#"
            struct Foo {
                x: NonexistentType
            }
        "#);
        assert!(diags.iter().any(|d| d.code == ErrorCode::UnknownType), "{diags:?}");
    }

    #[test]
    fn no_error_on_declared_type() {
        let diags = typeck_src(r#"
            sealed type Status = Active | Done
            struct Item {
                status: Status
            }
        "#);
        assert!(diags.is_empty(), "{diags:?}");
    }

    #[test]
    fn error_on_attribute_wrong_type() {
        let diags = typeck_src(r#"
            struct Foo {
                count: @nonEmpty Int
            }
        "#);
        assert!(diags.iter().any(|d| d.code == ErrorCode::AttributeConstraint), "{diags:?}");
    }

    #[test]
    fn error_on_wrong_arg_count() {
        let diags = typeck_src(r#"
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
        "#);
        assert!(diags.iter().any(|d| d.code == ErrorCode::WrongArgCount), "{diags:?}");
    }

    #[test]
    fn no_error_on_correct_arg_count() {
        let diags = typeck_src(r#"
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
        "#);
        // Only possible diag is W101 (missing .briefskill) — no type errors.
        assert!(diags.iter().all(|d| !d.is_error() || d.code != ErrorCode::WrongArgCount),
            "{diags:?}");
    }

    #[test]
    fn no_errors_on_sealed_type_variants() {
        let diags = typeck_src(r#"
            sealed type Platform = iOS | Android | Web
        "#);
        assert!(diags.is_empty(), "{diags:?}");
    }

    #[test]
    fn no_errors_on_effect_with_builtins() {
        let diags = typeck_src(r#"
            effect GraphQL {
                fn query(op: Operation) -> Result
                fn schema(name: String) -> Schema
            }
        "#);
        assert!(diags.is_empty(), "{diags:?}");
    }
}
