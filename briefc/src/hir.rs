//! Typed HIR — a typed, desugared view of the Brief AST produced after type-checking.
#![allow(dead_code)]

use std::collections::HashMap;
use std::path::Path;

use colored::Colorize;

use crate::ast::{self, Expr, ExtrasNode, Pattern, Span, Stmt, Task, TypeRef};
use crate::errors::print_diagnostics;
use crate::lexer::lex;
use crate::parser::parse;
use crate::typeck::TypeEnv;

/// A Brief type as resolved by the checker.
#[derive(Debug, Clone, PartialEq)]
pub enum Ty {
    /// A primitive scalar type.
    String,
    Bool,
    Int,
    Float,
    Unit,
    /// A declared sealed type (e.g. Platform).
    Sealed(String),
    /// A result wrapper: Ok(T) | Err(String).
    Result(Box<Ty>),
    /// An unknown/inferred type (used when checker cannot resolve).
    Unknown,
}

/// A HIR expression — similar to ast::Expr but with type annotations.
#[derive(Debug, Clone)]
pub struct HirExpr {
    pub kind: HirExprKind,
    pub ty: Ty,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum HirExprKind {
    Lit(HirLit),
    Var(String),
    Perform {
        skill: String,
        method: String,
        args: Vec<HirExpr>,
    },
    Match {
        scrutinee: Box<HirExpr>,
        arms: Vec<HirArm>,
    },
    Field(Box<HirExpr>, String),
    Call(String, Vec<HirExpr>),
}

#[derive(Debug, Clone)]
pub enum HirLit {
    String(String),
    Bool(bool),
    Int(i64),
    Float(f64),
    Variant(String),
}

#[derive(Debug, Clone)]
pub struct HirArm {
    pub pattern: Pattern,
    pub bindings: Vec<(String, Ty)>,
    pub body: HirExpr,
    pub span: Span,
}

/// A HIR step — a desugared step body with typed let-bindings.
#[derive(Debug, Clone)]
pub struct HirStep {
    pub name: String,
    pub bindings: Vec<HirBinding>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct HirBinding {
    pub name: String,
    pub ty: Ty,
    pub value: HirExpr,
    pub is_linear: bool,
    pub span: Span,
}

/// A HIR task — the typed representation of a Task.
#[derive(Debug, Clone)]
pub struct HirTask {
    pub name: String,
    pub skills: Vec<String>,
    pub extras: Vec<HirField>,
    pub provides: Vec<HirField>,
    pub steps: Vec<HirStep>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct HirField {
    pub name: String,
    pub ty: Ty,
}

/// A HIR module — the top-level typed representation of a Brief file.
#[derive(Debug, Clone)]
pub struct HirModule {
    pub tasks: Vec<HirTask>,
    pub sealed_types: Vec<HirSealedType>,
}

#[derive(Debug, Clone)]
pub struct HirSealedType {
    pub name: String,
    pub variants: Vec<String>,
}

type LocalTypes = HashMap<String, TypeRef>;

pub fn lower(program: &ast::Program) -> HirModule {
    let env = TypeEnv::from_program(program);

    HirModule {
        tasks: program
            .tasks
            .iter()
            .map(|task| lower_task(task, &env))
            .collect(),
        sealed_types: program
            .types
            .iter()
            .map(|sealed| HirSealedType {
                name: sealed.name.clone(),
                variants: sealed
                    .variants
                    .iter()
                    .map(|variant| variant.name.clone())
                    .collect(),
            })
            .collect(),
    }
}

fn lower_task(task: &Task, env: &TypeEnv<'_>) -> HirTask {
    let base_locals = task_base_locals(task);

    HirTask {
        name: task.name.clone(),
        skills: task.uses.clone(),
        extras: lower_extras(task, env),
        provides: lower_provides(task, env),
        steps: task
            .steps
            .iter()
            .map(|step| lower_step(step, env, &base_locals))
            .collect(),
        span: task.span,
    }
}

fn lower_extras(task: &Task, env: &TypeEnv<'_>) -> Vec<HirField> {
    match &task.extras {
        Some(ExtrasNode::TypedRecord(fields)) => {
            fields.iter().map(|field| lower_field(field, env)).collect()
        }
        Some(ExtrasNode::StringMap(_)) | None => Vec::new(),
    }
}

fn lower_provides(task: &Task, env: &TypeEnv<'_>) -> Vec<HirField> {
    task.provides
        .as_deref()
        .unwrap_or(&[])
        .iter()
        .map(|field| lower_field(field, env))
        .collect()
}

fn lower_field(field: &ast::ExtrasField, env: &TypeEnv<'_>) -> HirField {
    HirField {
        name: field.name.clone(),
        ty: lower_type_ref(&field.type_ref, env),
    }
}

fn task_base_locals(task: &Task) -> LocalTypes {
    let mut locals = HashMap::new();
    if let Some(ExtrasNode::TypedRecord(fields)) = &task.extras {
        for field in fields {
            locals.insert(field.name.clone(), field.type_ref.clone());
        }
    }
    locals
}

fn lower_step(step: &ast::Step, env: &TypeEnv<'_>, base_locals: &LocalTypes) -> HirStep {
    let mut locals = base_locals.clone();
    let mut bindings = Vec::new();

    for stmt in &step.body {
        if let Some(binding) = lower_binding(stmt, env, &mut locals) {
            bindings.push(binding);
        }
    }

    HirStep {
        name: step.name.clone(),
        bindings,
        span: step.span,
    }
}

fn lower_binding(stmt: &Stmt, env: &TypeEnv<'_>, locals: &mut LocalTypes) -> Option<HirBinding> {
    let Stmt::Let {
        attrs,
        name,
        value,
        span,
    } = stmt
    else {
        return None;
    };

    let value = lower_expr(value, env, locals);
    let ty = value.ty.clone();
    let is_linear = binding_is_linear(attrs, value_ref(stmt), env);

    if let Some(value_ty) = infer_expr_type_ref(value_ref(stmt), env, locals) {
        locals.insert(name.clone(), value_ty);
    }

    Some(HirBinding {
        name: name.clone(),
        ty,
        value,
        is_linear,
        span: *span,
    })
}

fn value_ref(stmt: &Stmt) -> &Expr {
    match stmt {
        Stmt::Let { value, .. } | Stmt::Expr { value, .. } => value,
    }
}

fn binding_is_linear(attrs: &[String], value: &Expr, env: &TypeEnv<'_>) -> bool {
    attrs.iter().any(|attr| attr == "once" || attr == "affine")
        || matches!(
            value,
            Expr::Perform { skill, func, .. }
                if env
                    .effect_fn(skill, func)
                    .map(|sig| {
                        sig.ret_attrs
                            .iter()
                            .any(|attr| attr.name == "once" || attr.name == "affine")
                    })
                    .unwrap_or(false)
        )
}

fn lower_expr(expr: &Expr, env: &TypeEnv<'_>, locals: &LocalTypes) -> HirExpr {
    match expr {
        Expr::Perform {
            skill,
            func,
            args,
            span,
            ..
        } => HirExpr {
            kind: HirExprKind::Perform {
                skill: skill.clone(),
                method: func.clone(),
                args: args
                    .iter()
                    .map(|arg| lower_expr(arg, env, locals))
                    .collect(),
            },
            ty: infer_expr_ty(expr, env, locals),
            span: *span,
        },
        Expr::Await { expr: inner, span } => {
            let mut lowered = lower_expr(inner, env, locals);
            lowered.span = *span;
            lowered
        }
        Expr::Call {
            receiver,
            func,
            args,
            span,
        } => {
            let name = receiver
                .as_ref()
                .map(|receiver| format!("{receiver}.{func}"))
                .unwrap_or_else(|| func.clone());
            HirExpr {
                kind: HirExprKind::Call(
                    name,
                    args.iter()
                        .map(|arg| lower_expr(arg, env, locals))
                        .collect(),
                ),
                ty: infer_expr_ty(expr, env, locals),
                span: *span,
            }
        }
        Expr::Match { scrutinee, arms } => {
            let lowered_scrutinee = lower_expr(scrutinee, env, locals);
            let scrutinee_ty = infer_expr_type_ref(scrutinee, env, locals);
            let lowered_arms: Vec<_> = arms
                .iter()
                .map(|arm| lower_arm(arm, scrutinee_ty.as_ref(), env, locals))
                .collect();
            let ty = shared_arm_ty(&lowered_arms).unwrap_or(Ty::Unknown);

            HirExpr {
                kind: HirExprKind::Match {
                    scrutinee: Box::new(lowered_scrutinee),
                    arms: lowered_arms,
                },
                ty,
                span: expr.span(),
            }
        }
        Expr::Ident { name, span } => HirExpr {
            kind: HirExprKind::Var(name.clone()),
            ty: infer_expr_ty(expr, env, locals),
            span: *span,
        },
        Expr::Str { value, span } => HirExpr {
            kind: HirExprKind::Lit(HirLit::String(value.clone())),
            ty: Ty::String,
            span: *span,
        },
        Expr::Int { value, span } => HirExpr {
            kind: HirExprKind::Lit(HirLit::Int(*value)),
            ty: Ty::Int,
            span: *span,
        },
    }
}

fn lower_arm(
    arm: &ast::MatchArm,
    scrutinee_ty: Option<&TypeRef>,
    env: &TypeEnv<'_>,
    locals: &LocalTypes,
) -> HirArm {
    let mut arm_locals = locals.clone();
    let bindings = match &arm.pattern {
        Pattern::Variant1(variant, binding) => {
            let ty = pattern_binding_type(variant, scrutinee_ty, env);
            if let Some(binding_ty) = type_ref_from_ty(&ty, arm.span) {
                arm_locals.insert(binding.clone(), binding_ty);
            }
            vec![(binding.clone(), ty)]
        }
        Pattern::Variant(_) | Pattern::Wildcard => Vec::new(),
    };

    HirArm {
        pattern: arm.pattern.clone(),
        bindings,
        body: lower_expr(&arm.body, env, &arm_locals),
        span: arm.span,
    }
}

fn shared_arm_ty(arms: &[HirArm]) -> Option<Ty> {
    let first = arms.first()?.body.ty.clone();
    if arms.iter().all(|arm| arm.body.ty == first) {
        Some(first)
    } else {
        None
    }
}

fn infer_expr_ty(expr: &Expr, env: &TypeEnv<'_>, locals: &LocalTypes) -> Ty {
    infer_expr_type_ref(expr, env, locals)
        .map(|ty| lower_type_ref(&ty, env))
        .unwrap_or(Ty::Unknown)
}

fn infer_expr_type_ref(expr: &Expr, env: &TypeEnv<'_>, locals: &LocalTypes) -> Option<TypeRef> {
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
        Expr::Await { expr: inner, .. } => infer_expr_type_ref(inner, env, locals),
        Expr::Call { .. } | Expr::Match { .. } => None,
    }
}

fn pattern_binding_type(
    variant_name: &str,
    scrutinee_ty: Option<&TypeRef>,
    env: &TypeEnv<'_>,
) -> Ty {
    builtin_variant_binding_type(scrutinee_ty, variant_name)
        .or_else(|| {
            scrutinee_ty.and_then(|ty| env.sealed_variant_field_type(&ty.name, variant_name))
        })
        .map(|ty| lower_type_ref(&ty, env))
        .unwrap_or(Ty::Unknown)
}

fn builtin_variant_binding_type(
    scrutinee_ty: Option<&TypeRef>,
    variant_name: &str,
) -> Option<TypeRef> {
    let scrutinee_ty = scrutinee_ty?;
    match (scrutinee_ty.name.as_str(), variant_name) {
        ("Result", "Ok") if !scrutinee_ty.args.is_empty() => Some(scrutinee_ty.args[0].clone()),
        ("Result", "Err") if scrutinee_ty.args.len() >= 2 => Some(scrutinee_ty.args[1].clone()),
        ("Option", "Some") if !scrutinee_ty.args.is_empty() => Some(scrutinee_ty.args[0].clone()),
        _ => None,
    }
}

fn lower_type_ref(ty: &TypeRef, env: &TypeEnv<'_>) -> Ty {
    if ty.optional {
        return Ty::Unknown;
    }

    match ty.name.as_str() {
        "String" => Ty::String,
        "Bool" => Ty::Bool,
        "Int" => Ty::Int,
        "Float" => Ty::Float,
        "Unit" => Ty::Unit,
        "Result" => ty
            .args
            .first()
            .map(|inner| Ty::Result(Box::new(lower_type_ref(inner, env))))
            .unwrap_or(Ty::Unknown),
        name if env.sealed_type(name).is_some() => Ty::Sealed(name.to_string()),
        _ => Ty::Unknown,
    }
}

fn type_ref_from_ty(ty: &Ty, span: Span) -> Option<TypeRef> {
    match ty {
        Ty::String => Some(TypeRef {
            name: "String".to_string(),
            args: Vec::new(),
            optional: false,
            span,
        }),
        Ty::Bool => Some(TypeRef {
            name: "Bool".to_string(),
            args: Vec::new(),
            optional: false,
            span,
        }),
        Ty::Int => Some(TypeRef {
            name: "Int".to_string(),
            args: Vec::new(),
            optional: false,
            span,
        }),
        Ty::Float => Some(TypeRef {
            name: "Float".to_string(),
            args: Vec::new(),
            optional: false,
            span,
        }),
        Ty::Unit => Some(TypeRef {
            name: "Unit".to_string(),
            args: Vec::new(),
            optional: false,
            span,
        }),
        Ty::Sealed(name) => Some(TypeRef {
            name: name.clone(),
            args: Vec::new(),
            optional: false,
            span,
        }),
        Ty::Result(inner) => Some(TypeRef {
            name: "Result".to_string(),
            args: type_ref_from_ty(inner, span).into_iter().collect(),
            optional: false,
            span,
        }),
        Ty::Unknown => None,
    }
}

pub fn print_hir_file(path: &Path) -> bool {
    let source = match std::fs::read_to_string(path) {
        Ok(source) => source,
        Err(err) => {
            eprintln!(
                "{}: cannot read {}: {err}",
                "error".red().bold(),
                path.display()
            );
            return false;
        }
    };

    let (tokens, lex_errors) = lex(&source);
    if !lex_errors.is_empty() {
        for (start, end) in &lex_errors {
            eprintln!(
                "{}: unrecognised character at byte offset {}–{}",
                "error[E000]".red().bold(),
                start,
                end
            );
        }
        return false;
    }

    let file_str = path.to_string_lossy().to_string();
    let (program, parse_errors) = parse(&tokens, &source);
    if !parse_errors.is_empty() {
        print_diagnostics(&parse_errors, &source, &file_str);
        if parse_errors.iter().any(|diag| diag.is_error()) {
            return false;
        }
    }

    println!("{:#?}", lower(&program));
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::lex;
    use crate::parser::parse;

    fn lower_src(src: &str) -> HirModule {
        let (tokens, lex_errors) = lex(src);
        assert!(lex_errors.is_empty(), "{lex_errors:?}");
        let (program, parse_errors) = parse(&tokens, src);
        assert!(parse_errors.is_empty(), "{parse_errors:?}");
        lower(&program)
    }

    #[test]
    fn lowers_simple_task() {
        let hir = lower_src(
            r#"
            task Hello : TaskBrief {
                goal = "hi"
                step Fetch {
                    let title = "Home";
                }
            }
        "#,
        );

        assert_eq!(hir.tasks.len(), 1);
        assert_eq!(hir.tasks[0].steps.len(), 1);
        assert_eq!(hir.tasks[0].steps[0].bindings.len(), 1);
        assert_eq!(hir.tasks[0].steps[0].bindings[0].ty, Ty::String);
        assert!(matches!(
            &hir.tasks[0].steps[0].bindings[0].value.kind,
            HirExprKind::Lit(HirLit::String(value)) if value == "Home"
        ));
    }

    #[test]
    fn lowers_sealed_types() {
        let hir = lower_src(
            r#"
            sealed type Platform = iOS | Android | Web

            task Hello : TaskBrief {
                goal = "hi"
            }
        "#,
        );

        assert_eq!(hir.sealed_types.len(), 1);
        assert_eq!(hir.sealed_types[0].name, "Platform");
        assert_eq!(hir.sealed_types[0].variants, vec!["iOS", "Android", "Web"]);
    }
}
