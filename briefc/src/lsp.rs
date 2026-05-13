/// `brief lsp` — Language Server Protocol server for Brief.
///
/// Implements a stdio LSP server that provides:
///   - Diagnostics: all E/W codes from the checker + type checker, published on
///     textDocument/didOpen, didChange, and didSave.
///   - Hover: shows the effect signature when hovering over a `perform` call.
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

/// State shared across LSP requests.
struct BriefLspState {
    /// Source text of each open document, keyed by URI string.
    documents: HashMap<String, String>,
}

/// The Brief LSP server.
pub struct BriefLsp {
    client: Client,
    state:  Arc<Mutex<BriefLspState>>,
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
                ..Default::default()
            },
            server_info: Some(ServerInfo {
                name:    "brief-lsp".to_string(),
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
        let uri  = params.text_document.uri.to_string();
        let text = params.text_document.text;

        {
            let mut state = self.state.lock().await;
            state.documents.insert(uri.clone(), text.clone());
        }

        self.publish_diagnostics(uri, text).await;
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

        {
            let mut state = self.state.lock().await;
            state.documents.insert(uri.clone(), text.clone());
        }

        self.publish_diagnostics(uri, text).await;
    }

    async fn did_save(&self, params: DidSaveTextDocumentParams) {
        let uri = params.text_document.uri.to_string();
        let state = self.state.lock().await;
        if let Some(text) = state.documents.get(&uri).cloned() {
            drop(state);
            self.publish_diagnostics(uri, text).await;
        }
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        let uri = params.text_document.uri.to_string();
        let mut state = self.state.lock().await;
        state.documents.remove(&uri);

        // Clear diagnostics for closed file.
        self.client
            .publish_diagnostics(
                params.text_document.uri,
                vec![],
                None,
            )
            .await;
    }

    // ── Hover ────────────────────────────────────────────────────────────

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let uri = params.text_document_position_params.text_document.uri.to_string();
        let pos = params.text_document_position_params.position;

        let state = self.state.lock().await;
        let text = match state.documents.get(&uri) {
            Some(t) => t.clone(),
            None    => return Ok(None),
        };
        drop(state);

        Ok(hover_at(&text, pos))
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Diagnostics
// ─────────────────────────────────────────────────────────────────────────────

impl BriefLsp {
    async fn publish_diagnostics(&self, uri_str: String, source: String) {
        let diags = validate_source(&source);
        let lsp_diags: Vec<Diagnostic> = diags
            .iter()
            .map(|e| brief_error_to_lsp(e, &source))
            .collect();

        let uri: tower_lsp::lsp_types::Url = match uri_str.parse() {
            Ok(u)  => u,
            Err(_) => return,
        };

        self.client
            .publish_diagnostics(uri, lsp_diags, None)
            .await;
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
                code:    crate::errors::ErrorCode::ParseError,
                message: format!("unrecognised character at byte {}–{}", start, end),
                span:    crate::ast::Span::new(*start, *end),
                hint:    None,
            })
            .collect();
    }

    let (program, parse_errors) = parse(&tokens, source);

    let file_dir = std::path::Path::new(".");
    let cwd      = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let ctx      = CheckContext { file_dir, cwd: &cwd };

    let mut diags: Vec<crate::errors::BriefError> = parse_errors;
    diags.extend(checker::check(&program, &ctx));
    diags.extend(typeck::type_check_with_skills(&program, std::collections::HashMap::new()));
    diags
}

/// Convert a `BriefError` (byte-span) to an LSP `Diagnostic` (line/char positions).
fn brief_error_to_lsp(err: &crate::errors::BriefError, source: &str) -> Diagnostic {
    let start = offset_to_position(source, err.span.start);
    let end   = offset_to_position(source, err.span.end);

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
// Hover
// ─────────────────────────────────────────────────────────────────────────────

/// Return hover information for the word at `pos` in `source`.
/// Currently surfaces `perform <Effect>.<fn>` call signatures.
fn hover_at(source: &str, pos: Position) -> Option<Hover> {
    let offset = position_to_offset(source, pos)?;

    // Find the word boundaries around the cursor.
    let bytes = source.as_bytes();
    let start = (0..offset).rev()
        .take_while(|&i| bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_' || bytes[i] == b'.')
        .last()
        .unwrap_or(offset);
    let end = (offset..bytes.len())
        .take_while(|&i| bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_')
        .last()
        .map(|i| i + 1)
        .unwrap_or(offset);

    let word = &source[start..end];
    if word.is_empty() { return None; }

    // Check if we're hovering inside a perform call: `perform Effect.fn`
    // Look backwards for `perform ` before the word.
    let before = source[..start].trim_end();
    if before.ends_with("perform") {
        // Parse the program to get effect signatures.
        use crate::lexer::lex;
        use crate::parser::parse;
        let (tokens, _)  = lex(source);
        let (program, _) = parse(&tokens, source);

        if let Some((effect_name, fn_name)) = word.split_once('.') {
            for effect in &program.effects {
                if effect.name == effect_name {
                    if let Some(sig) = effect.fns.iter().find(|f| f.name == fn_name) {
                        let params = sig.params.iter()
                            .map(|p| format!("{}: {}", p.name, p.ty.name))
                            .collect::<Vec<_>>()
                            .join(", ");
                        let text = format!(
                            "```brief\nfn {fn_name}({params}) -> {}\n```",
                            sig.ret.name
                        );
                        return Some(Hover {
                            contents: HoverContents::Markup(MarkupContent {
                                kind:  MarkupKind::Markdown,
                                value: text,
                            }),
                            range: Some(Range {
                                start: offset_to_position(source, start),
                                end:   offset_to_position(source, end),
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
// Position utilities
// ─────────────────────────────────────────────────────────────────────────────

/// Convert a byte offset to an LSP `Position` (0-indexed line + character).
pub fn offset_to_position(source: &str, offset: usize) -> Position {
    let safe_offset = offset.min(source.len());
    let before = &source[..safe_offset];
    let line = before.chars().filter(|&c| c == '\n').count() as u32;
    let character = before
        .rfind('\n')
        .map(|nl| safe_offset - nl - 1)
        .unwrap_or(safe_offset) as u32;
    Position { line, character }
}

/// Convert an LSP `Position` to a byte offset.
fn position_to_offset(source: &str, pos: Position) -> Option<usize> {
    let mut line = 0u32;
    let mut col  = 0u32;
    for (i, ch) in source.char_indices() {
        if line == pos.line && col == pos.character {
            return Some(i);
        }
        if ch == '\n' {
            if line == pos.line {
                return Some(i); // cursor at end of line
            }
            line += 1;
            col   = 0;
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
    let stdin  = tokio::io::stdin();
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
    fn test_offset_to_position_first_line() {
        let src = "hello world";
        assert_eq!(offset_to_position(src, 0),  Position { line: 0, character: 0 });
        assert_eq!(offset_to_position(src, 5),  Position { line: 0, character: 5 });
        assert_eq!(offset_to_position(src, 11), Position { line: 0, character: 11 });
    }

    #[test]
    fn test_offset_to_position_multiline() {
        let src = "line1\nline2\nline3";
        assert_eq!(offset_to_position(src, 0),  Position { line: 0, character: 0 });
        assert_eq!(offset_to_position(src, 6),  Position { line: 1, character: 0 });
        assert_eq!(offset_to_position(src, 10), Position { line: 1, character: 4 });
        assert_eq!(offset_to_position(src, 12), Position { line: 2, character: 0 });
    }

    #[test]
    fn test_position_to_offset_roundtrip() {
        let src = "line1\nline2\nline3";
        let offsets = [0, 3, 6, 9, 12];
        for &o in &offsets {
            let pos = offset_to_position(src, o);
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
}
