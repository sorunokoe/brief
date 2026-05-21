use std::any::Any;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

use serde_json::Value;

use crate::ast::{Program, Span};
use crate::checker::{self, CheckContext};
use crate::errors::{BriefError, ErrorCode};
use crate::fmt::Formatter;
use crate::hir::{self, HirModule};
use crate::lexer::{self, Spanned};
use crate::parser;
use crate::runner::{self, RunMode};
use crate::typeck;

pub type SkillFn = Box<dyn Fn(&[SkillValue]) -> SkillValue + Send + Sync>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SkillValue {
    Str(String),
    Int(i64),
    Bool(bool),
    Opaque(String, Vec<u8>),
    Unit,
}

pub struct SkillBackend {
    pub skill_name: String,
    pub functions: HashMap<String, SkillFn>,
}

#[derive(Debug, Clone)]
struct TokenStreamArtifact {
    tokens: Vec<Spanned>,
    source: String,
    file_path: String,
    diagnostics: Vec<BriefError>,
}

#[derive(Debug, Clone)]
struct AstArtifact {
    program: Program,
    source: String,
    file_path: String,
    diagnostics: Vec<BriefError>,
}

#[derive(Debug, Clone)]
struct TypedAstArtifact {
    program: Program,
    source: String,
    file_path: String,
    diagnostics: Vec<BriefError>,
}

#[derive(Debug, Clone)]
struct DiagnosticSetArtifact {
    diagnostics: Vec<BriefError>,
    source: Option<String>,
    file_path: Option<String>,
}

#[derive(Debug, Clone)]
struct OutcomeValue {
    variant: String,
    payload: Option<SkillValue>,
}

#[derive(Debug, Clone)]
struct PassSpecArtifact {
    pass_name: String,
    strict: bool,
}

#[derive(Debug, Clone)]
struct PassResultArtifact {
    pass_name: String,
    source_file: String,
    clean: bool,
}

struct StoredOpaque {
    value: Box<dyn Any + Send + Sync>,
}

static NEXT_OPAQUE_ID: AtomicU64 = AtomicU64::new(1);

fn opaque_store() -> &'static Mutex<HashMap<u64, StoredOpaque>> {
    static STORE: OnceLock<Mutex<HashMap<u64, StoredOpaque>>> = OnceLock::new();
    STORE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn backend_registry() -> &'static HashMap<String, SkillBackend> {
    static REGISTRY: OnceLock<HashMap<String, SkillBackend>> = OnceLock::new();
    REGISTRY.get_or_init(|| {
        register_all_backends()
            .into_iter()
            .map(|backend| (backend.skill_name.clone(), backend))
            .collect()
    })
}

fn store_opaque<T>(type_name: &str, value: T) -> SkillValue
where
    T: Any + Send + Sync + 'static,
{
    let id = NEXT_OPAQUE_ID.fetch_add(1, Ordering::Relaxed);
    opaque_store()
        .lock()
        .expect("opaque store poisoned")
        .insert(id, StoredOpaque { value: Box::new(value) });
    SkillValue::Opaque(type_name.to_string(), id.to_le_bytes().to_vec())
}

fn opaque_id(value: &SkillValue) -> Option<u64> {
    match value {
        SkillValue::Opaque(_, blob) if blob.len() == 8 => {
            let mut bytes = [0u8; 8];
            bytes.copy_from_slice(blob);
            Some(u64::from_le_bytes(bytes))
        }
        _ => None,
    }
}

fn with_opaque_any<T, R>(value: &SkillValue, f: impl FnOnce(&T) -> R) -> Option<R>
where
    T: Any + Send + Sync + 'static,
{
    let id = opaque_id(value)?;
    let store = opaque_store().lock().ok()?;
    let stored = store.get(&id)?;
    let typed = stored.value.downcast_ref::<T>()?;
    Some(f(typed))
}

fn clone_opaque<T>(value: &SkillValue) -> Option<T>
where
    T: Any + Send + Sync + Clone + 'static,
{
    with_opaque_any(value, Clone::clone)
}

fn as_str(value: Option<&SkillValue>) -> Option<String> {
    match value? {
        SkillValue::Str(s) => Some(s.clone()),
        _ => None,
    }
}

fn as_bool(value: Option<&SkillValue>) -> Option<bool> {
    match value? {
        SkillValue::Bool(v) => Some(*v),
        _ => None,
    }
}

fn outcome(type_name: &str, variant: &str, payload: Option<SkillValue>) -> SkillValue {
    store_opaque(
        type_name,
        OutcomeValue {
            variant: variant.to_string(),
            payload,
        },
    )
}

fn lex_diagnostics(source: &str, file_path: &str, errors: &[(usize, usize)]) -> DiagnosticSetArtifact {
    DiagnosticSetArtifact {
        diagnostics: errors
            .iter()
            .map(|(start, end)| BriefError {
                code: ErrorCode::ParseError,
                message: format!("unrecognised character at byte offset {}–{}", start, end),
                span: Span::new(*start, *end),
                hint: None,
            })
            .collect(),
        source: Some(source.to_string()),
        file_path: Some(file_path.to_string()),
    }
}

fn current_dirs(file_path: &str) -> (PathBuf, PathBuf) {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let file_dir = Path::new(file_path)
        .parent()
        .map(Path::to_path_buf)
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| cwd.clone());
    (cwd, file_dir)
}

fn direct_token_stream(value: &SkillValue) -> Option<TokenStreamArtifact> {
    match value {
        SkillValue::Opaque(type_name, _) if type_name == "TokenStream" => clone_opaque(value),
        _ => None,
    }
}

fn direct_ast(value: &SkillValue) -> Option<AstArtifact> {
    match value {
        SkillValue::Opaque(type_name, _) if type_name == "Ast" => clone_opaque(value),
        _ => None,
    }
}

fn direct_typed_ast(value: &SkillValue) -> Option<TypedAstArtifact> {
    match value {
        SkillValue::Opaque(type_name, _) if type_name == "TypedAst" => clone_opaque(value),
        _ => None,
    }
}

fn direct_diagnostics(value: &SkillValue) -> Option<DiagnosticSetArtifact> {
    match value {
        SkillValue::Opaque(type_name, _) if type_name == "DiagnosticSet" => clone_opaque(value),
        _ => None,
    }
}

fn direct_hir(value: &SkillValue) -> Option<HirModule> {
    match value {
        SkillValue::Opaque(type_name, _) if type_name == "HirModule" => clone_opaque(value),
        _ => None,
    }
}

fn direct_pass_spec(value: &SkillValue) -> Option<PassSpecArtifact> {
    match value {
        SkillValue::Opaque(type_name, _) if type_name == "PassSpec" => clone_opaque(value),
        _ => None,
    }
}

fn direct_pass_result(value: &SkillValue) -> Option<PassResultArtifact> {
    match value {
        SkillValue::Opaque(type_name, _) if type_name == "PassResult" => clone_opaque(value),
        _ => None,
    }
}

pub fn match_outcome(value: &SkillValue) -> Option<(String, Option<SkillValue>)> {
    clone_opaque::<OutcomeValue>(value).map(|outcome| (outcome.variant, outcome.payload))
}

fn outcome_payload(value: &SkillValue, variants: &[&str]) -> Option<SkillValue> {
    match_outcome(value).and_then(|(variant, payload)| {
        variants
            .iter()
            .any(|candidate| *candidate == variant)
            .then_some(payload)
            .flatten()
    })
}

fn token_stream_from_value(value: &SkillValue) -> Option<TokenStreamArtifact> {
    direct_token_stream(value)
        .or_else(|| outcome_payload(value, &["Lexed"]).and_then(|payload| token_stream_from_value(&payload)))
}

fn ast_from_value(value: &SkillValue) -> Option<AstArtifact> {
    direct_ast(value)
        .or_else(|| {
            direct_typed_ast(value).map(|typed| AstArtifact {
                program: typed.program,
                source: typed.source,
                file_path: typed.file_path,
                diagnostics: typed.diagnostics,
            })
        })
        .or_else(|| outcome_payload(value, &["Parsed", "Checked"]).and_then(|payload| ast_from_value(&payload)))
}

fn typed_ast_from_value(value: &SkillValue) -> Option<TypedAstArtifact> {
    direct_typed_ast(value)
        .or_else(|| {
            direct_ast(value).map(|ast| TypedAstArtifact {
                program: ast.program,
                source: ast.source,
                file_path: ast.file_path,
                diagnostics: ast.diagnostics,
            })
        })
        .or_else(|| outcome_payload(value, &["Checked"]).and_then(|payload| typed_ast_from_value(&payload)))
}

fn diagnostics_from_value(value: &SkillValue) -> Option<DiagnosticSetArtifact> {
    direct_diagnostics(value)
        .or_else(|| {
            direct_ast(value).map(|ast| DiagnosticSetArtifact {
                diagnostics: ast.diagnostics,
                source: Some(ast.source),
                file_path: Some(ast.file_path),
            })
        })
        .or_else(|| {
            direct_typed_ast(value).map(|typed| DiagnosticSetArtifact {
                diagnostics: typed.diagnostics,
                source: Some(typed.source),
                file_path: Some(typed.file_path),
            })
        })
        .or_else(|| outcome_payload(value, &["LexFailed", "ParseFailed", "CheckFailed", "FormatFailed", "HirFailed", "PassFailed"]).and_then(|payload| diagnostics_from_value(&payload)))
}

fn hir_from_value(value: &SkillValue) -> Option<HirModule> {
    direct_hir(value)
        .or_else(|| outcome_payload(value, &["Lowered"]).and_then(|payload| hir_from_value(&payload)))
}

fn task_only_program(program: &Program) -> Program {
    Program {
        imports: program.imports.clone(),
        opaque_types: program.opaque_types.clone(),
        types: program.types.clone(),
        type_aliases: program.type_aliases.clone(),
        effect_groups: program.effect_groups.clone(),
        structs: program.structs.clone(),
        protocols: program.protocols.clone(),
        effects: program.effects.clone(),
        tasks: program.tasks.iter().take(1).cloned().collect(),
        tests: Vec::new(),
    }
}

fn parse_with_tokens(tokens: TokenStreamArtifact, task_only: bool) -> SkillValue {
    let (program, diagnostics) = parser::parse(&tokens.tokens, &tokens.source);
    let program = if task_only { task_only_program(&program) } else { program };
    if diagnostics.iter().any(BriefError::is_error) {
        return outcome(
            "ParseOutcome",
            "ParseFailed",
            Some(store_opaque(
                "DiagnosticSet",
                DiagnosticSetArtifact {
                    diagnostics,
                    source: Some(tokens.source),
                    file_path: Some(tokens.file_path),
                },
            )),
        );
    }

    outcome(
        "ParseOutcome",
        "Parsed",
        Some(store_opaque(
            "Ast",
            AstArtifact {
                program,
                source: tokens.source,
                file_path: tokens.file_path,
                diagnostics,
            },
        )),
    )
}

fn run_semantic_checks(ast: AstArtifact) -> SkillValue {
    let (cwd, file_dir) = current_dirs(&ast.file_path);
    let ctx = CheckContext {
        file_dir: &file_dir,
        cwd: &cwd,
        manifest: None,
        brief_path: None,
        allow_missing_skills: true,
    };

    let mut diagnostics = checker::check(&ast.program, &ctx);
    diagnostics.extend(typeck::type_check(&ast.program));

    if diagnostics.iter().any(BriefError::is_error) {
        return outcome(
            "CheckOutcome",
            "CheckFailed",
            Some(store_opaque(
                "DiagnosticSet",
                DiagnosticSetArtifact {
                    diagnostics,
                    source: Some(ast.source),
                    file_path: Some(ast.file_path),
                },
            )),
        );
    }

    outcome(
        "CheckOutcome",
        "Checked",
        Some(store_opaque(
            "TypedAst",
            TypedAstArtifact {
                program: ast.program,
                source: ast.source,
                file_path: ast.file_path,
                diagnostics,
            },
        )),
    )
}

fn diagnostics_text(diags: &DiagnosticSetArtifact) -> String {
    let _ = (&diags.source, &diags.file_path);
    diags
        .diagnostics
        .iter()
        .map(|diag| format!("{}: {}", diag.code, diag.message))
        .collect::<Vec<_>>()
        .join("\n")
}

fn count_matching_diagnostics(value: Option<&SkillValue>, predicate: impl Fn(&BriefError) -> bool) -> i64 {
    diagnostics_from_value(value.unwrap_or(&SkillValue::Unit))
        .map(|diags| diags.diagnostics.iter().filter(|diag| predicate(diag)).count() as i64)
        .unwrap_or(0)
}

fn format_program_outcome(program: Program, _source: String, recovered: bool) -> SkillValue {
    let formatted = Formatter::format_program(&program);
    let variant = if recovered { "FormatRecovered" } else { "Formatted" };
    outcome(variant_to_type(variant), variant, Some(SkillValue::Str(formatted)))
}

fn variant_to_type(variant: &str) -> &'static str {
    match variant {
        "Lexed" | "LexFailed" => "LexOutcome",
        "Parsed" | "ParseFailed" => "ParseOutcome",
        "Checked" | "CheckFailed" => "CheckOutcome",
        "Formatted" | "FormatRecovered" | "FormatFailed" => "FormatOutcome",
        "Lowered" | "HirFailed" => "HirOutcome",
        "PassSuccess" | "PassFailed" => "RunOutcome",
        _ => "Outcome",
    }
}

fn run_outcome(clean: bool, pass_name: String, source_file: String) -> SkillValue {
    if clean {
        outcome(
            "RunOutcome",
            "PassSuccess",
            Some(store_opaque(
                "PassResult",
                PassResultArtifact {
                    pass_name,
                    source_file,
                    clean,
                },
            )),
        )
    } else {
        outcome(
            "RunOutcome",
            "PassFailed",
            Some(store_opaque(
                "DiagnosticSet",
                DiagnosticSetArtifact {
                    diagnostics: vec![BriefError {
                        code: ErrorCode::LockRequired,
                        message: "pass failed".to_string(),
                        span: Span::default(),
                        hint: Some("see emitted diagnostics above".to_string()),
                    }],
                    source: None,
                    file_path: Some(source_file),
                },
            )),
        )
    }
}

pub fn register_all_backends() -> Vec<SkillBackend> {
    vec![
        lexer_backend(),
        parser_backend(),
        checker_backend(),
        formatter_backend(),
        hir_backend(),
        driver_backend(),
    ]
}

pub fn lexer_backend() -> SkillBackend {
    let mut functions: HashMap<String, SkillFn> = HashMap::new();
    functions.insert(
        "tokenize".to_string(),
        Box::new(|args| {
            let source = as_str(args.first()).unwrap_or_default();
            let file_path = as_str(args.get(1)).unwrap_or_default();
            let (tokens, errors) = lexer::lex(&source);
            if errors.is_empty() {
                outcome(
                    "LexOutcome",
                    "Lexed",
                    Some(store_opaque(
                        "TokenStream",
                        TokenStreamArtifact {
                            tokens,
                            source,
                            file_path,
                            diagnostics: Vec::new(),
                        },
                    )),
                )
            } else {
                outcome(
                    "LexOutcome",
                    "LexFailed",
                    Some(store_opaque(
                        "DiagnosticSet",
                        lex_diagnostics(&source, &file_path, &errors),
                    )),
                )
            }
        }),
    );
    functions.insert(
        "classify".to_string(),
        Box::new(|args| {
            if let Some((variant, _)) = args.first().and_then(match_outcome) {
                if variant == "LexFailed" {
                    return args.first().cloned().unwrap_or(SkillValue::Unit);
                }
            }
            token_stream_from_value(args.first().unwrap_or(&SkillValue::Unit))
                .map(|tokens| {
                    if tokens.diagnostics.iter().any(BriefError::is_error) {
                        outcome(
                            "LexOutcome",
                            "LexFailed",
                            Some(store_opaque(
                                "DiagnosticSet",
                                DiagnosticSetArtifact {
                                    diagnostics: tokens.diagnostics,
                                    source: Some(tokens.source),
                                    file_path: Some(tokens.file_path),
                                },
                            )),
                        )
                    } else {
                        outcome(
                            "LexOutcome",
                            "Lexed",
                            Some(store_opaque("TokenStream", tokens)),
                        )
                    }
                })
                .unwrap_or_else(|| outcome("LexOutcome", "LexFailed", Some(store_opaque("DiagnosticSet", DiagnosticSetArtifact {
                    diagnostics: vec![BriefError {
                        code: ErrorCode::UnknownType,
                        message: "expected TokenStream or LexOutcome".to_string(),
                        span: Span::default(),
                        hint: None,
                    }],
                    source: None,
                    file_path: None,
                }))))
        }),
    );
    functions.insert(
        "tokenCount".to_string(),
        Box::new(|args| SkillValue::Int(
            token_stream_from_value(args.first().unwrap_or(&SkillValue::Unit))
                .map(|tokens| tokens.tokens.len() as i64)
                .unwrap_or(0),
        )),
    );

    SkillBackend {
        skill_name: "LexerPrimitives".to_string(),
        functions,
    }
}

pub fn parser_backend() -> SkillBackend {
    let mut functions: HashMap<String, SkillFn> = HashMap::new();
    functions.insert(
        "parseSource".to_string(),
        Box::new(|args| {
            token_stream_from_value(args.first().unwrap_or(&SkillValue::Unit))
                .map(|tokens| parse_with_tokens(tokens, false))
                .unwrap_or_else(|| outcome("ParseOutcome", "ParseFailed", Some(store_opaque("DiagnosticSet", DiagnosticSetArtifact {
                    diagnostics: vec![BriefError {
                        code: ErrorCode::UnknownType,
                        message: "expected TokenStream or LexOutcome".to_string(),
                        span: Span::default(),
                        hint: None,
                    }],
                    source: None,
                    file_path: None,
                }))))
        }),
    );
    functions.insert(
        "parseTask".to_string(),
        Box::new(|args| {
            token_stream_from_value(args.first().unwrap_or(&SkillValue::Unit))
                .map(|tokens| parse_with_tokens(tokens, true))
                .unwrap_or_else(|| outcome("ParseOutcome", "ParseFailed", Some(store_opaque("DiagnosticSet", DiagnosticSetArtifact {
                    diagnostics: vec![BriefError {
                        code: ErrorCode::UnknownType,
                        message: "expected TokenStream or LexOutcome".to_string(),
                        span: Span::default(),
                        hint: None,
                    }],
                    source: None,
                    file_path: None,
                }))))
        }),
    );
    functions.insert(
        "formatErrors".to_string(),
        Box::new(|args| {
            diagnostics_from_value(args.first().unwrap_or(&SkillValue::Unit))
                .map(|diags| SkillValue::Str(diagnostics_text(&diags)))
                .unwrap_or_else(|| SkillValue::Str(String::new()))
        }),
    );
    functions.insert(
        "taskCount".to_string(),
        Box::new(|args| SkillValue::Int(
            ast_from_value(args.first().unwrap_or(&SkillValue::Unit))
                .map(|ast| ast.program.tasks.len() as i64)
                .unwrap_or(0),
        )),
    );

    SkillBackend {
        skill_name: "ParserPrimitives".to_string(),
        functions,
    }
}

pub fn checker_backend() -> SkillBackend {
    let mut functions: HashMap<String, SkillFn> = HashMap::new();
    for name in ["typeCheck", "checkLinear", "checkEffects", "checkExhaustiveness"] {
        functions.insert(
            name.to_string(),
            Box::new(|args| {
                ast_from_value(args.first().unwrap_or(&SkillValue::Unit))
                    .map(run_semantic_checks)
                    .unwrap_or_else(|| outcome("CheckOutcome", "CheckFailed", Some(store_opaque("DiagnosticSet", DiagnosticSetArtifact {
                        diagnostics: vec![BriefError {
                            code: ErrorCode::UnknownType,
                            message: "expected Ast or ParseOutcome".to_string(),
                            span: Span::default(),
                            hint: None,
                        }],
                        source: None,
                        file_path: None,
                    }))))
            }),
        );
    }
    functions.insert(
        "errorCount".to_string(),
        Box::new(|args| SkillValue::Int(count_matching_diagnostics(args.first(), BriefError::is_error))),
    );
    functions.insert(
        "warningCount".to_string(),
        Box::new(|args| SkillValue::Int(count_matching_diagnostics(args.first(), BriefError::is_warning))),
    );

    SkillBackend {
        skill_name: "CheckerPrimitives".to_string(),
        functions,
    }
}

pub fn formatter_backend() -> SkillBackend {
    let mut functions: HashMap<String, SkillFn> = HashMap::new();
    functions.insert(
        "formatFull".to_string(),
        Box::new(|args| {
            ast_from_value(args.first().unwrap_or(&SkillValue::Unit))
                .map(|ast| format_program_outcome(ast.program, ast.source.clone(), ast.source.contains("//")))
                .unwrap_or_else(|| outcome("FormatOutcome", "FormatFailed", Some(store_opaque("DiagnosticSet", DiagnosticSetArtifact {
                    diagnostics: vec![BriefError {
                        code: ErrorCode::UnknownType,
                        message: "expected Ast or TypedAst".to_string(),
                        span: Span::default(),
                        hint: None,
                    }],
                    source: None,
                    file_path: None,
                }))))
        }),
    );
    functions.insert(
        "formatTask".to_string(),
        Box::new(|args| {
            ast_from_value(args.first().unwrap_or(&SkillValue::Unit))
                .map(|ast| format_program_outcome(task_only_program(&ast.program), ast.source.clone(), ast.source.contains("//")))
                .unwrap_or_else(|| outcome("FormatOutcome", "FormatFailed", Some(store_opaque("DiagnosticSet", DiagnosticSetArtifact {
                    diagnostics: vec![BriefError {
                        code: ErrorCode::UnknownType,
                        message: "expected Ast or TypedAst".to_string(),
                        span: Span::default(),
                        hint: None,
                    }],
                    source: None,
                    file_path: None,
                }))))
        }),
    );
    functions.insert(
        "checkFormat".to_string(),
        Box::new(|args| {
            let Some(ast) = ast_from_value(args.first().unwrap_or(&SkillValue::Unit)) else {
                return outcome("FormatOutcome", "FormatFailed", Some(store_opaque("DiagnosticSet", DiagnosticSetArtifact {
                    diagnostics: vec![BriefError {
                        code: ErrorCode::UnknownType,
                        message: "expected Ast or TypedAst".to_string(),
                        span: Span::default(),
                        hint: None,
                    }],
                    source: None,
                    file_path: None,
                })));
            };
            let original = as_str(args.get(1)).unwrap_or_default();
            let formatted = Formatter::format_program(&ast.program);
            if formatted == original {
                outcome("FormatOutcome", "Formatted", Some(SkillValue::Str(formatted)))
            } else {
                outcome("FormatOutcome", "FormatFailed", Some(store_opaque("DiagnosticSet", DiagnosticSetArtifact {
                    diagnostics: vec![BriefError {
                        code: ErrorCode::AttributeConstraint,
                        message: "source is not canonically formatted".to_string(),
                        span: Span::default(),
                        hint: Some("run `brief fmt` to rewrite the source".to_string()),
                    }],
                    source: Some(original),
                    file_path: Some(ast.file_path),
                })))
            }
        }),
    );
    functions.insert(
        "extractFormatted".to_string(),
        Box::new(|args| {
            match args.first() {
                Some(SkillValue::Str(s)) => SkillValue::Str(s.clone()),
                Some(value) => match_outcome(value)
                    .and_then(|(_, payload)| payload)
                    .and_then(|payload| match payload {
                        SkillValue::Str(s) => Some(SkillValue::Str(s)),
                        _ => None,
                    })
                    .unwrap_or_else(|| SkillValue::Str(String::new())),
                None => SkillValue::Str(String::new()),
            }
        }),
    );

    SkillBackend {
        skill_name: "FormatterPrimitives".to_string(),
        functions,
    }
}

pub fn hir_backend() -> SkillBackend {
    let mut functions: HashMap<String, SkillFn> = HashMap::new();
    functions.insert(
        "lowerModule".to_string(),
        Box::new(|args| {
            typed_ast_from_value(args.first().unwrap_or(&SkillValue::Unit))
                .map(|ast| outcome("HirOutcome", "Lowered", Some(store_opaque("HirModule", hir::lower(&ast.program)))))
                .unwrap_or_else(|| outcome("HirOutcome", "HirFailed", Some(store_opaque("DiagnosticSet", DiagnosticSetArtifact {
                    diagnostics: vec![BriefError {
                        code: ErrorCode::UnknownType,
                        message: "expected TypedAst or CheckOutcome".to_string(),
                        span: Span::default(),
                        hint: None,
                    }],
                    source: None,
                    file_path: None,
                }))))
        }),
    );
    functions.insert(
        "lowerTask".to_string(),
        Box::new(|args| {
            typed_ast_from_value(args.first().unwrap_or(&SkillValue::Unit))
                .map(|ast| {
                    let program = task_only_program(&ast.program);
                    outcome("HirOutcome", "Lowered", Some(store_opaque("HirModule", hir::lower(&program))))
                })
                .unwrap_or_else(|| outcome("HirOutcome", "HirFailed", Some(store_opaque("DiagnosticSet", DiagnosticSetArtifact {
                    diagnostics: vec![BriefError {
                        code: ErrorCode::UnknownType,
                        message: "expected TypedAst or CheckOutcome".to_string(),
                        span: Span::default(),
                        hint: None,
                    }],
                    source: None,
                    file_path: None,
                }))))
        }),
    );
    functions.insert(
        "validateHir".to_string(),
        Box::new(|args| {
            hir_from_value(args.first().unwrap_or(&SkillValue::Unit))
                .map(|hir_module| {
                    if hir_module.tasks.iter().all(|task| !task.name.trim().is_empty()) {
                        outcome("HirOutcome", "Lowered", Some(store_opaque("HirModule", hir_module)))
                    } else {
                        outcome("HirOutcome", "HirFailed", Some(store_opaque("DiagnosticSet", DiagnosticSetArtifact {
                            diagnostics: vec![BriefError {
                                code: ErrorCode::UnknownType,
                                message: "HIR validation failed".to_string(),
                                span: Span::default(),
                                hint: None,
                            }],
                            source: None,
                            file_path: None,
                        })))
                    }
                })
                .unwrap_or_else(|| outcome("HirOutcome", "HirFailed", Some(store_opaque("DiagnosticSet", DiagnosticSetArtifact {
                    diagnostics: vec![BriefError {
                        code: ErrorCode::UnknownType,
                        message: "expected HirModule or HirOutcome".to_string(),
                        span: Span::default(),
                        hint: None,
                    }],
                    source: None,
                    file_path: None,
                }))))
        }),
    );
    functions.insert(
        "taskCount".to_string(),
        Box::new(|args| SkillValue::Int(
            hir_from_value(args.first().unwrap_or(&SkillValue::Unit))
                .map(|hir_module| hir_module.tasks.len() as i64)
                .unwrap_or(0),
        )),
    );

    SkillBackend {
        skill_name: "HirPrimitives".to_string(),
        functions,
    }
}

pub fn driver_backend() -> SkillBackend {
    let mut functions: HashMap<String, SkillFn> = HashMap::new();
    functions.insert(
        "buildPassSpec".to_string(),
        Box::new(|args| {
            let pass_name = as_str(args.first()).unwrap_or_else(|| "unknown".to_string());
            let strict = as_bool(args.get(1)).unwrap_or(true);
            store_opaque("PassSpec", PassSpecArtifact { pass_name, strict })
        }),
    );
    functions.insert(
        "runPass".to_string(),
        Box::new(|args| {
            let spec = args
                .first()
                .and_then(direct_pass_spec)
                .unwrap_or_else(|| PassSpecArtifact {
                    pass_name: as_str(args.first()).unwrap_or_else(|| "unknown".to_string()),
                    strict: true,
                });
            let source_file = as_str(args.get(1)).unwrap_or_default();
            let clean = runner::run_file(
                Path::new(&source_file),
                RunMode::Check {
                    allow_missing_skills: !spec.strict,
                },
            );
            run_outcome(clean, spec.pass_name, source_file)
        }),
    );
    functions.insert(
        "emitDiagnostics".to_string(),
        Box::new(|args| {
            diagnostics_from_value(args.first().unwrap_or(&SkillValue::Unit))
                .map(|diags| SkillValue::Str(diagnostics_text(&diags)))
                .unwrap_or_else(|| SkillValue::Str(String::new()))
        }),
    );
    functions.insert(
        "isClean".to_string(),
        Box::new(|args| {
            let value = args.first().unwrap_or(&SkillValue::Unit);
            let clean = direct_pass_result(value)
                .map(|result| result.clean)
                .or_else(|| {
                    match_outcome(value).map(|(variant, payload)| {
                        variant == "PassSuccess"
                            && payload
                                .and_then(|inner| direct_pass_result(&inner))
                                .map(|result| result.clean)
                                .unwrap_or(true)
                    })
                })
                .unwrap_or_else(|| {
                    diagnostics_from_value(value)
                        .map(|diags| diags.diagnostics.is_empty())
                        .unwrap_or(false)
                });
            SkillValue::Bool(clean)
        }),
    );
    functions.insert(
        "exitCode".to_string(),
        Box::new(|args| {
            let is_clean = match driver_backend().functions.get("isClean") {
                Some(check) => matches!(check(args), SkillValue::Bool(true)),
                None => false,
            };
            SkillValue::Int(if is_clean { 0 } else { 1 })
        }),
    );

    SkillBackend {
        skill_name: "CompilerDriver".to_string(),
        functions,
    }
}

pub fn dispatch(skill: &str, func: &str, args: &[SkillValue]) -> Option<SkillValue> {
    backend_registry()
        .get(skill)
        .and_then(|backend| backend.functions.get(func))
        .map(|function| function(args))
}

pub fn to_json(value: &SkillValue) -> Value {
    match value {
        SkillValue::Str(s) => Value::String(s.clone()),
        SkillValue::Int(i) => Value::from(*i),
        SkillValue::Bool(b) => Value::from(*b),
        SkillValue::Opaque(type_name, blob) => {
            if let Some((variant, payload)) = match_outcome(value) {
                let mut object = serde_json::Map::new();
                object.insert("type".to_string(), Value::String(type_name.clone()));
                object.insert("variant".to_string(), Value::String(variant));
                if let Some(payload) = payload {
                    object.insert("payload".to_string(), to_json(&payload));
                }
                Value::Object(object)
            } else {
                Value::String(format!("<{type_name}:{}>", blob.len()))
            }
        }
        SkillValue::Unit => Value::Null,
    }
}

pub fn from_json(value: &Value) -> SkillValue {
    match value {
        Value::String(s) => SkillValue::Str(s.clone()),
        Value::Bool(b) => SkillValue::Bool(*b),
        Value::Number(number) => number
            .as_i64()
            .map(SkillValue::Int)
            .unwrap_or_else(|| SkillValue::Str(number.to_string())),
        Value::Null => SkillValue::Unit,
        _ => SkillValue::Str(value.to_string()),
    }
}

pub fn display_value(value: &SkillValue) -> String {
    match value {
        SkillValue::Str(s) => s.clone(),
        SkillValue::Int(i) => i.to_string(),
        SkillValue::Bool(b) => b.to_string(),
        SkillValue::Opaque(type_name, _) => match_outcome(value)
            .map(|(variant, _)| format!("{variant}<{type_name}>"))
            .or_else(|| direct_pass_result(value).map(|result| format!("{}({})", result.pass_name, result.source_file)))
            .unwrap_or_else(|| format!("Opaque({type_name})")),
        SkillValue::Unit => "()".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lexer_backend_registered() {
        let backend = lexer_backend();
        assert!(backend.functions.contains_key("tokenize"));
    }

    #[test]
    fn test_all_backends_registered() {
        assert_eq!(register_all_backends().len(), 6);
    }

    #[test]
    fn test_opaque_value_roundtrip() {
        let value = SkillValue::Opaque("TokenStream".to_string(), vec![1, 2, 3]);
        match value {
            SkillValue::Opaque(name, blob) => {
                assert_eq!(name, "TokenStream");
                assert_eq!(blob, vec![1, 2, 3]);
            }
            _ => panic!("expected opaque value"),
        }
    }
}
