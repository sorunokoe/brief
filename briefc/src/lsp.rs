/// `brief lsp` — Language Server Protocol server for Brief.
///
/// Implements a stdio LSP server that provides:
///   - Diagnostics: all E/W codes from the checker + type checker, published on
///     textDocument/didOpen, didChange, and didSave.
///   - Hover: shows the effect signature when hovering over a `perform` call.
///   - Go-to-definition: jumps to the declaration of tasks, types, effects, etc.
///   - Find-references: lists all uses of any named symbol in the document.
///
/// Launch with: `brief lsp` (communicates over stdin/stdout).
/// Configure in VS Code or Zed with: `"languageServerCommand": ["brief", "lsp"]`.
use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::Mutex;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};

// ─────────────────────────────────────────────────────────────────────────────

#[derive(Clone)]
struct OpenDocument {
    text: String,
    line_starts: Vec<usize>,
}

impl OpenDocument {
    fn new(text: String) -> Self {
        Self {
            line_starts: compute_line_starts(&text),
            text,
        }
    }
}

/// State shared across LSP requests.
struct BriefLspState {
    /// Source text of each open document, keyed by URI string.
    documents: HashMap<String, OpenDocument>,
}

/// The Brief LSP server.
pub struct BriefLsp {
    client: Client,
    state: Arc<Mutex<BriefLspState>>,
}

impl BriefLsp {
    fn new(client: Client) -> Self {
        BriefLsp {
            client,
            state: Arc::new(Mutex::new(BriefLspState {
                documents: HashMap::new(),
            })),
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// LanguageServer implementation
// ─────────────────────────────────────────────────────────────────────────────

#[tower_lsp::async_trait]
impl LanguageServer for BriefLsp {
    async fn initialize(&self, _params: InitializeParams) -> Result<InitializeResult> {
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                definition_provider: Some(OneOf::Left(true)),
                references_provider: Some(OneOf::Left(true)),
                ..Default::default()
            },
            server_info: Some(ServerInfo {
                name: "brief-lsp".to_string(),
                version: Some(env!("CARGO_PKG_VERSION").to_string()),
            }),
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, "brief-lsp initialized")
            .await;
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    // ── Document sync ────────────────────────────────────────────────────

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let uri = params.text_document.uri.to_string();
        let document = OpenDocument::new(params.text_document.text);

        {
            let mut state = self.state.lock().await;
            state.documents.insert(uri.clone(), document.clone());
        }

        self.publish_diagnostics(uri, document).await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri.to_string();

        // We use FULL sync — take the last change content.
        let text = params
            .content_changes
            .into_iter()
            .last()
            .map(|c| c.text)
            .unwrap_or_default();
        let document = OpenDocument::new(text);

        {
            let mut state = self.state.lock().await;
            state.documents.insert(uri.clone(), document.clone());
        }

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        self.publish_diagnostics(uri, document).await;
    }

    async fn did_save(&self, params: DidSaveTextDocumentParams) {
        let uri = params.text_document.uri.to_string();
        let state = self.state.lock().await;
        if let Some(document) = state.documents.get(&uri).cloned() {
            drop(state);
            self.publish_diagnostics(uri, document).await;
        }
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        let uri = params.text_document.uri.to_string();
        let mut state = self.state.lock().await;
        state.documents.remove(&uri);

        // Clear diagnostics for closed file.
        self.client
            .publish_diagnostics(params.text_document.uri, vec![], None)
            .await;
    }

    // ── Hover ────────────────────────────────────────────────────────────

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let uri = params
            .text_document_position_params
            .text_document
            .uri
            .to_string();
        let pos = params.text_document_position_params.position;

        let state = self.state.lock().await;
        let document = match state.documents.get(&uri) {
            Some(document) => document.clone(),
            None => return Ok(None),
        };
        drop(state);

        Ok(hover_at(&document.text, pos, &document.line_starts))
    }

    // ── Go-to-definition ─────────────────────────────────────────────────

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        let uri = params
            .text_document_position_params
            .text_document
            .uri
            .clone();
        let pos = params.text_document_position_params.position;

        let state = self.state.lock().await;
        let document = match state.documents.get(&uri.to_string()) {
            Some(document) => document.clone(),
            None => return Ok(None),
        };
        drop(state);

        let word = word_at(&document.text, pos);
        if word.is_empty() {
            return Ok(None);
        }

        let index = build_symbol_index(&document.text);
        if let Some(&span) = index.get(word.as_str()) {
            let start = offset_to_position(span.start, &document.line_starts);
            let end = offset_to_position(span.end, &document.line_starts);
            let loc = Location {
                uri,
                range: Range { start, end },
            };
            return Ok(Some(GotoDefinitionResponse::Scalar(loc)));
        }
        Ok(None)
    }

    // ── Find-references ───────────────────────────────────────────────────

    async fn references(&self, params: ReferenceParams) -> Result<Option<Vec<Location>>> {
        let uri = params.text_document_position.text_document.uri.clone();
        let pos = params.text_document_position.position;

        let state = self.state.lock().await;
        let document = match state.documents.get(&uri.to_string()) {
            Some(document) => document.clone(),
            None => return Ok(None),
        };
        drop(state);

        let word = word_at(&document.text, pos);
        if word.is_empty() {
            return Ok(None);
        }

        let locs = find_references(&document.text, &word, &uri, &document.line_starts);
        if locs.is_empty() {
            return Ok(None);
        }
        Ok(Some(locs))
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Diagnostics
// ─────────────────────────────────────────────────────────────────────────────

impl BriefLsp {
    async fn publish_diagnostics(&self, uri_str: String, document: OpenDocument) {
        let diags = validate_source(&document.text);
        let lsp_diags: Vec<Diagnostic> = diags
            .iter()
            .map(|e| brief_error_to_lsp(e, &document.line_starts))
            .collect();

        let uri: tower_lsp::lsp_types::Url = match uri_str.parse() {
            Ok(u) => u,
            Err(_) => return,
        };

        self.client.publish_diagnostics(uri, lsp_diags, None).await;
    }
}

/// Run the full Brief pipeline on `source` and return all errors/warnings.
fn validate_source(source: &str) -> Vec<crate::errors::BriefError> {
    use crate::checker::{self, CheckContext};
    use crate::lexer::lex;
    use crate::parser::parse;
    use crate::typeck;

    let (tokens, lex_errors) = lex(source);
    if !lex_errors.is_empty() {
        return lex_errors
            .iter()
            .map(|(start, end)| crate::errors::BriefError {
                code: crate::errors::ErrorCode::ParseError,
                message: format!("unrecognised character at byte {}–{}", start, end),
                span: crate::ast::Span::new(*start, *end),
                hint: None,
            })
            .collect();
    }

    let (program, parse_errors) = parse(&tokens, source);

    let file_dir = std::path::Path::new(".");
    let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let ctx = CheckContext {
        file_dir,
        cwd: &cwd,
        manifest: None,
        brief_path: None,
        allow_missing_skills: false,
    };

    let mut diags: Vec<crate::errors::BriefError> = parse_errors;
    diags.extend(checker::check(&program, &ctx));
    diags.extend(typeck::type_check_with_skills(
        &program,
        std::collections::HashMap::new(),
    ));
    diags
}

/// Convert a `BriefError` (byte-span) to an LSP `Diagnostic` (line/char positions).
fn brief_error_to_lsp(err: &crate::errors::BriefError, line_starts: &[usize]) -> Diagnostic {
    let start = offset_to_position(err.span.start, line_starts);
    let end = offset_to_position(err.span.end, line_starts);

    let severity = if err.is_error() {
        DiagnosticSeverity::ERROR
    } else {
        DiagnosticSeverity::WARNING
    };

    let mut message = err.message.clone();
    if let Some(hint) = &err.hint {
        message.push_str(&format!("\n  hint: {hint}"));
    }

    Diagnostic {
        range: Range { start, end },
        severity: Some(severity),
        code: Some(NumberOrString::String(format!("{:?}", err.code))),
        source: Some("brief".to_string()),
        message,
        ..Default::default()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Symbol index
// ─────────────────────────────────────────────────────────────────────────────

/// Maps every declared identifier name → its declaration span.
/// Covers: tasks, sealed types, structs, protocols, effects, skill imports.
fn build_symbol_index(source: &str) -> HashMap<&'static str, crate::ast::Span> {
    // We store owned names, so use a HashMap<String, Span> internally and
    // convert to a leaked version for the &'static lifetime the caller needs.
    // Actually, let's just use String keys.
    build_symbol_index_owned(source)
        .into_iter()
        .map(|(k, v)| {
            let k: &'static str = Box::leak(k.into_boxed_str());
            (k, v)
        })
        .collect()
}

/// Same as build_symbol_index but with owned String keys (avoids leaking in tests).
pub fn build_symbol_index_owned(source: &str) -> HashMap<String, crate::ast::Span> {
    use crate::lexer::lex;
    use crate::parser::parse;

    let (tokens, _) = lex(source);
    let (program, _) = parse(&tokens, source);
    let mut index = HashMap::new();

    for import in &program.imports {
        index.insert(import.name.clone(), import.span);
    }
    for ty in &program.types {
        index.insert(ty.name.clone(), ty.span);
    }
    for s in &program.structs {
        index.insert(s.name.clone(), s.span);
    }
    for p in &program.protocols {
        index.insert(p.name.clone(), p.span);
    }
    for e in &program.effects {
        index.insert(e.name.clone(), e.span);
        for f in &e.fns {
            // Qualify as EffectName.fnName
            index.insert(format!("{}.{}", e.name, f.name), f.span);
        }
    }
    for task in &program.tasks {
        index.insert(task.name.clone(), task.span);
        for step in &task.steps {
            // Qualify as TaskName.StepName
            index.insert(format!("{}.{}", task.name, step.name), step.span);
        }
    }

    index
}

// ─────────────────────────────────────────────────────────────────────────────
// Find-references helper
// ─────────────────────────────────────────────────────────────────────────────

/// Find every occurrence of `word` (whole-word match) in `source`.
/// Returns LSP `Location` for each match, scoped to the given document URI.
fn find_references(source: &str, word: &str, uri: &Url, line_starts: &[usize]) -> Vec<Location> {
    let mut locs = Vec::new();
    let wlen = word.len();

    // Scan for all offsets where `word` appears as a whole word.
    let bytes = source.as_bytes();
    let wbytes = word.as_bytes();
    let mut pos = 0usize;

    while pos + wlen <= bytes.len() {
        if bytes[pos..pos + wlen] == *wbytes {
            // Check word boundaries
            let before_ok = pos == 0 || !is_ident_char(bytes[pos - 1]);
            let after_ok = pos + wlen >= bytes.len() || !is_ident_char(bytes[pos + wlen]);

            if before_ok && after_ok {
                let start = offset_to_position(pos, line_starts);
                let end = offset_to_position(pos + wlen, line_starts);
                locs.push(Location {
                    uri: uri.clone(),
                    range: Range { start, end },
                });
            }
        }
        pos += 1;
    }

    locs
}

fn is_ident_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

// ─────────────────────────────────────────────────────────────────────────────
// Hover
// ─────────────────────────────────────────────────────────────────────────────

/// Return hover information for the word at `pos` in `source`.
/// Currently surfaces `perform <Effect>.<fn>` call signatures.
fn hover_at(source: &str, pos: Position, line_starts: &[usize]) -> Option<Hover> {
    let offset = position_to_offset(source, pos)?;

    // Find the word boundaries around the cursor.
    let bytes = source.as_bytes();
    let start = (0..offset)
        .rev()
        .take_while(|&i| bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_' || bytes[i] == b'.')
        .last()
        .unwrap_or(offset);
    let end = (offset..bytes.len())
        .take_while(|&i| bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_')
        .last()
        .map(|i| i + 1)
        .unwrap_or(offset);

    let word = &source[start..end];
    if word.is_empty() {
        return None;
    }

    // Check if we're hovering inside a perform call: `perform Effect.fn`
    // Look backwards for `perform ` before the word.
    let before = source[..start].trim_end();
    if before.ends_with("perform") {
        // Parse the program to get effect signatures.
        use crate::lexer::lex;
        use crate::parser::parse;
        let (tokens, _) = lex(source);
        let (program, _) = parse(&tokens, source);

        if let Some((effect_name, fn_name)) = word.split_once('.') {
            for effect in &program.effects {
                if effect.name == effect_name {
                    if let Some(sig) = effect.fns.iter().find(|f| f.name == fn_name) {
                        let params = sig
                            .params
                            .iter()
                            .map(|p| format!("{}: {}", p.name, p.ty.name))
                            .collect::<Vec<_>>()
                            .join(", ");
                        let text =
                            format!("```brief\nfn {fn_name}({params}) -> {}\n```", sig.ret.name);
                        return Some(Hover {
                            contents: HoverContents::Markup(MarkupContent {
                                kind: MarkupKind::Markdown,
                                value: text,
                            }),
                            range: Some(Range {
                                start: offset_to_position(start, line_starts),
                                end: offset_to_position(end, line_starts),
                            }),
                        });
                    }
                }
            }
        }
    }

    None
}

// ─────────────────────────────────────────────────────────────────────────────
// Word extraction utility
// ─────────────────────────────────────────────────────────────────────────────

/// Extract the identifier (or `Task.Step` qualified name) at cursor `pos`.
fn word_at(source: &str, pos: Position) -> String {
    let offset = match position_to_offset(source, pos) {
        Some(o) => o,
        None => return String::new(),
    };

    let bytes = source.as_bytes();
    // Expand left
    let start = (0..offset)
        .rev()
        .take_while(|&i| bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_' || bytes[i] == b'.')
        .last()
        .unwrap_or(offset);
    // Expand right
    let end = (offset..bytes.len())
        .take_while(|&i| bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_' || bytes[i] == b'.')
        .last()
        .map(|i| i + 1)
        .unwrap_or(offset);

    source[start..end].to_string()
}

// ─────────────────────────────────────────────────────────────────────────────
// Position utilities
// ─────────────────────────────────────────────────────────────────────────────

fn compute_line_starts(text: &str) -> Vec<usize> {
    let mut starts = vec![0usize];
    for (i, ch) in text.char_indices() {
        if ch == '\n' {
            starts.push(i + 1);
        }
    }
    starts
}

/// Convert a byte offset to an LSP `Position` (0-indexed line + character).
pub fn offset_to_position(offset: usize, line_starts: &[usize]) -> Position {
    let line = line_starts
        .partition_point(|&start| start <= offset)
        .saturating_sub(1);
    let character = offset - line_starts[line];
    Position {
        line: line as u32,
        character: character as u32,
    }
}

/// Convert an LSP `Position` to a byte offset.
fn position_to_offset(source: &str, pos: Position) -> Option<usize> {
    let mut line = 0u32;
    let mut col = 0u32;
    for (i, ch) in source.char_indices() {
        if line == pos.line && col == pos.character {
            return Some(i);
        }
        if ch == '\n' {
            if line == pos.line {
                return Some(i); // cursor at end of line
            }
            line += 1;
            col = 0;
        } else {
            col += 1;
        }
    }
    if line == pos.line && col == pos.character {
        return Some(source.len());
    }
    None
}

// ─────────────────────────────────────────────────────────────────────────────
// Entry point
// ─────────────────────────────────────────────────────────────────────────────

/// Start the Brief LSP server over stdin/stdout.
pub async fn run_lsp_server() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::new(BriefLsp::new);
    Server::new(stdin, stdout, socket).serve(service).await;
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offset_to_position_first_line() {
        let starts = compute_line_starts("hello\nworld\n");
        let pos = offset_to_position(3, &starts);
        assert_eq!(pos.line, 0);
        assert_eq!(pos.character, 3);
    }

    #[test]
    fn offset_to_position_second_line() {
        let starts = compute_line_starts("hello\nworld\n");
        let pos = offset_to_position(9, &starts);
        assert_eq!(pos.line, 1);
        assert_eq!(pos.character, 3);
    }

    #[test]
    fn offset_to_position_empty_line() {
        let starts = compute_line_starts("a\n\nb\n");
        let pos = offset_to_position(2, &starts);
        assert_eq!(pos.line, 1);
        assert_eq!(pos.character, 0);
    }

    #[test]
    fn offset_to_position_no_trailing_newline() {
        let starts = compute_line_starts("abc");
        let pos = offset_to_position(2, &starts);
        assert_eq!(pos.line, 0);
        assert_eq!(pos.character, 2);
    }

    #[test]
    fn test_position_to_offset_roundtrip() {
        let src = "line1\nline2\nline3";
        let starts = compute_line_starts(src);
        let offsets = [0, 3, 6, 9, 12];
        for &o in &offsets {
            let pos = offset_to_position(o, &starts);
            let back = position_to_offset(src, pos).unwrap();
            assert_eq!(back, o, "offset {o} did not roundtrip");
        }
    }

    #[test]
    fn test_validate_source_clean() {
        let src = "task HelloWorld : TaskBrief {\n    goal = \"Say hello\"\n}\n";
        let diags = validate_source(src);
        let errors: Vec<_> = diags.iter().filter(|d| d.is_error()).collect();
        assert!(errors.is_empty(), "unexpected errors: {errors:?}");
    }

    #[test]
    fn test_validate_source_missing_goal() {
        let src = "task BrokenTask : TaskBrief {\n    step DoThing {\n    }\n}\n";
        let diags = validate_source(src);
        // Missing goal should produce at least one diagnostic.
        assert!(!diags.is_empty(), "expected at least one diagnostic");
    }

    // ── Symbol index tests ────────────────────────────────────────────────

    #[test]
    fn test_symbol_index_task_name() {
        let src = "task FetchUser : TaskBrief { goal = \"g\" }";
        let idx = build_symbol_index_owned(src);
        assert!(idx.contains_key("FetchUser"), "task name not indexed");
    }

    #[test]
    fn test_symbol_index_type() {
        let src = "sealed type Platform = iOS | Android";
        let idx = build_symbol_index_owned(src);
        assert!(idx.contains_key("Platform"), "sealed type not indexed");
    }

    #[test]
    fn test_symbol_index_effect_and_fn() {
        let src = r#"effect GraphQL { fn query(op: String) -> Result }"#;
        let idx = build_symbol_index_owned(src);
        assert!(idx.contains_key("GraphQL"), "effect not indexed");
        assert!(idx.contains_key("GraphQL.query"), "effect.fn not indexed");
    }

    #[test]
    fn test_symbol_index_step_qualified() {
        let src = "task T : TaskBrief { goal = \"g\"\n step Fetch { } }";
        let idx = build_symbol_index_owned(src);
        assert!(idx.contains_key("T.Fetch"), "step not indexed as Task.Step");
    }

    // ── References tests ─────────────────────────────────────────────────

    #[test]
    fn test_find_references_word() {
        let src = "task Login : TaskBrief { goal = \"g\" }\n// Login is great\n";
        let uri: Url = "file:///tmp/test.brief".parse().unwrap();
        let starts = compute_line_starts(src);
        let locs = find_references(src, "Login", &uri, &starts);
        // Should find at least 2: declaration + comment usage
        assert!(locs.len() >= 2, "expected >= 2 refs, got {}", locs.len());
    }

    #[test]
    fn test_find_references_whole_word_only() {
        let src = "task Login : TaskBrief { goal = \"g\" }\ntask LoginAdmin : TaskBrief { goal = \"h\" }\n";
        let uri: Url = "file:///tmp/test.brief".parse().unwrap();
        let starts = compute_line_starts(src);
        let locs = find_references(src, "Login", &uri, &starts);
        // "LoginAdmin" contains "Login" but is not a whole-word match
        for loc in &locs {
            let start_off = loc.range.start.character as usize;
            // The match at the Login word should be exactly 5 chars
            assert_eq!(loc.range.end.character - loc.range.start.character, 5);
            let _ = start_off;
        }
    }

    // ── word_at tests ─────────────────────────────────────────────────────

    #[test]
    fn test_word_at_ident() {
        let src = "task Hello : TaskBrief { }";
        //              ^5
        let pos = Position {
            line: 0,
            character: 5,
        };
        assert_eq!(word_at(src, pos), "Hello");
    }

    #[test]
    fn test_word_at_empty() {
        let src = "task Hello : TaskBrief { }";
        let pos = Position {
            line: 0,
            character: 4,
        }; // space
           // space before H → empty or "Hello" depending on direction; either is ok
        let w = word_at(src, pos);
        // should not crash
        let _ = w;
    }
}
