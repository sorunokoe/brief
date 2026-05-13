use logos::Logos;

/// All tokens in the Brief language (v0.0.1).
/// Keywords take priority over Ident because they are declared first in the enum
/// and logos applies the longest-then-first-declared rule.
#[derive(Logos, Debug, PartialEq, Clone)]
#[logos(skip r"[ \t\n\r]+")] // skip whitespace
#[logos(skip r"//[^\n]*")]   // skip line comments
pub enum Token {
    // ── Keywords ──────────────────────────────────────────────────────────
    #[token("task")]    Task,
    #[token("step")]    Step,
    #[token("import")]  Import,
    #[token("skill")]   Skill,
    #[token("uses")]    Uses,
    #[token("perform")] Perform,
    #[token("let")]     Let,

    // ── Decorator ─────────────────────────────────────────────────────────
    #[token("@BriefBuilder")] BriefBuilder,

    // ── Literals ──────────────────────────────────────────────────────────
    /// String literal — logos captures the full `"..."` slice; we strip quotes below.
    #[regex(r#""([^"\\]|\\.)*""#, |lex| {
        let s = lex.slice();
        s[1..s.len()-1].to_string()
    })]
    Str(String),

    // ── Identifiers ───────────────────────────────────────────────────────
    /// Any word not matched by a keyword. Includes `TaskBrief`, `goal`, `extras`, etc.
    #[regex(r"[a-zA-Z_][a-zA-Z0-9_]*", |lex| lex.slice().to_string())]
    Ident(String),

    // ── Punctuation ───────────────────────────────────────────────────────
    #[token("{")] LBrace,
    #[token("}")] RBrace,
    #[token("[")] LBracket,
    #[token("]")] RBracket,
    #[token("(")] LParen,
    #[token(")")] RParen,
    #[token(":")] Colon,
    #[token(",")] Comma,
    #[token(".")] Dot,
    #[token("=")] Eq,
    #[token("?")] Question,
    #[token(";")] Semi,
}

/// A token paired with its byte-offset span in the source.
#[derive(Debug, Clone)]
pub struct Spanned {
    pub token: Token,
    pub start: usize,
    pub end:   usize,
}

/// Lex `source` into a flat token list, returning any unrecognised character positions.
pub fn lex(source: &str) -> (Vec<Spanned>, Vec<(usize, usize)>) {
    let mut tokens  = Vec::new();
    let mut errors  = Vec::new();

    let mut lexer = Token::lexer(source);
    while let Some(result) = lexer.next() {
        let span = lexer.span();
        match result {
            Ok(tok) => tokens.push(Spanned { token: tok, start: span.start, end: span.end }),
            Err(_)  => errors.push((span.start, span.end)),
        }
    }

    (tokens, errors)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lex_hello_brief() {
        let src = r#"task Hello : TaskBrief { goal = "Say hello" }"#;
        let (toks, errs) = lex(src);
        assert!(errs.is_empty(), "unexpected lex errors: {:?}", errs);
        assert!(toks.iter().any(|t| t.token == Token::Task));
        assert!(toks.iter().any(|t| matches!(&t.token, Token::Ident(s) if s == "Hello")));
    }

    #[test]
    fn lex_strips_string_quotes() {
        let src = r#""hello world""#;
        let (toks, _) = lex(src);
        assert_eq!(toks[0].token, Token::Str("hello world".to_string()));
    }

    #[test]
    fn lex_skips_comment() {
        let src = "// this is a comment\ntask";
        let (toks, _) = lex(src);
        assert_eq!(toks.len(), 1);
        assert_eq!(toks[0].token, Token::Task);
    }
}
