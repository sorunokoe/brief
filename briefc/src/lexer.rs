use logos::Logos;

/// Unescape a Brief string literal body (content between the quotes).
/// Converts `\\` → `\` and `\"` → `"`. Other escape sequences are passed through.
fn unescape_str(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('\\') => out.push('\\'),
                Some('"')  => out.push('"'),
                Some('n')  => out.push('\n'),
                Some('t')  => out.push('\t'),
                Some(other) => { out.push('\\'); out.push(other); }
                None        => out.push('\\'),
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// All tokens in the Brief language (v0.1).
/// Keywords take priority over Ident because they are declared first in the enum
/// and logos applies the longest-then-first-declared rule.
#[derive(Logos, Debug, PartialEq, Clone)]
#[logos(skip r"[ \t\n\r]+")] // skip whitespace
pub enum Token {
    // ── Keywords ──────────────────────────────────────────────────────────
    #[token("task")]     Task,
    #[token("step")]     Step,
    #[token("import")]   Import,
    #[token("skill")]    Skill,
    #[token("uses")]     Uses,
    #[token("perform")]  Perform,
    #[token("let")]      Let,
    #[token("sealed")]   Sealed,
    #[token("opaque")]   Opaque,
    #[token("type")]     Type,
    #[token("struct")]   Struct,
    #[token("protocol")] Protocol,
    #[token("effect")]   Effect,
    #[token("fn")]       Fn,
    #[token("async")]    Async,
    #[token("await")]    Await,
    #[token("return")]   Return,
    #[token("match")]    Match,
    #[token("test")]     Test,

    // ── Literals ──────────────────────────────────────────────────────────
    /// String literal — logos captures the full `"..."` slice; we strip quotes and unescape below.
    #[regex(r#""([^"\\]|\\.)*""#, |lex| {
        let s = lex.slice();
        unescape_str(&s[1..s.len()-1])
    })]
    Str(String),

    /// Integer literal, e.g. `0`, `100`, `-42`.
    #[regex(r"-?[0-9]+", |lex| lex.slice().parse::<i64>().ok())]
    Int(i64),

    #[regex(r"//[^\n]*", |lex| lex.slice().to_string())]
    LineComment(String),

    // ── Identifiers ───────────────────────────────────────────────────────
    /// Any word not matched by a keyword. Includes `TaskBrief`, `goal`, `extras`, etc.
    #[regex(r"[a-zA-Z_][a-zA-Z0-9_-]*", |lex| lex.slice().to_string())]
    Ident(String),

    // ── Operators & Punctuation ───────────────────────────────────────────
    #[token("->")] Arrow,   // function return type
    #[token("{")] LBrace,
    #[token("}")] RBrace,
    #[token("[")] LBracket,
    #[token("]")] RBracket,
    #[token("(")] LParen,
    #[token(")")] RParen,
    #[token("<")]  LAngle,  // generic type arguments
    #[token(">")]  RAngle,
    #[token("|")]  Pipe,    // sealed type variant separator
    #[token("@")]  At,      // decorator / attribute prefix
    #[token(":")] Colon,
    #[token(",")] Comma,
    #[token(".")] Dot,
    #[token("=>")] FatArrow,
    #[token("=")] Eq,
    #[token("?")] Question,
    #[token(";")] Semi,
    #[token("*")] Star,
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
            Ok(Token::LineComment(_)) => {}
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
    fn lex_decorator() {
        let src = "@BriefBuilder\ntask";
        let (toks, errs) = lex(src);
        assert!(errs.is_empty(), "unexpected lex errors: {:?}", errs);
        // @BriefBuilder lexes as: At, Ident("BriefBuilder"), Task
        assert_eq!(toks[0].token, Token::At);
        assert!(matches!(&toks[1].token, Token::Ident(s) if s == "BriefBuilder"));
        assert_eq!(toks[2].token, Token::Task);
    }

    #[test]
    fn lex_sealed_type() {
        let src = "sealed type Platform = iOS | Android";
        let (toks, errs) = lex(src);
        assert!(errs.is_empty(), "unexpected lex errors: {:?}", errs);
        assert_eq!(toks[0].token, Token::Sealed);
        assert_eq!(toks[1].token, Token::Type);
        assert!(matches!(&toks[2].token, Token::Ident(s) if s == "Platform"));
        assert!(toks.iter().any(|t| t.token == Token::Pipe));
    }

    #[test]
    fn lex_opaque_type() {
        let src = "opaque type TokenStream";
        let (toks, errs) = lex(src);
        assert!(errs.is_empty(), "unexpected lex errors: {:?}", errs);
        assert_eq!(toks[0].token, Token::Opaque);
        assert_eq!(toks[1].token, Token::Type);
        assert!(matches!(&toks[2].token, Token::Ident(s) if s == "TokenStream"));
    }

    #[test]
    fn lex_fn_signature() {
        let src = "fn query(op: Operation) -> Result";
        let (toks, errs) = lex(src);
        assert!(errs.is_empty(), "unexpected lex errors: {:?}", errs);
        assert_eq!(toks[0].token, Token::Fn);
        assert_eq!(toks[toks.len()-1].token, Token::Ident("Result".to_string()));
        assert!(toks.iter().any(|t| t.token == Token::Arrow));
    }

    #[test]
    fn lex_generics() {
        let src = "Result<T, E>";
        let (toks, errs) = lex(src);
        assert!(errs.is_empty(), "unexpected lex errors: {:?}", errs);
        assert_eq!(toks[1].token, Token::LAngle);
        assert_eq!(toks[toks.len()-1].token, Token::RAngle);
    }

    #[test]
    fn lex_integer_literal() {
        let (toks, errs) = lex("0 100 -42");
        assert!(errs.is_empty());
        assert_eq!(toks[0].token, Token::Int(0));
        assert_eq!(toks[1].token, Token::Int(100));
        assert_eq!(toks[2].token, Token::Int(-42));
    }

    #[test]
    fn lex_match_tokens() {
        let (toks, errs) = lex("match value { Ok(v) => v _ => other }");
        assert!(errs.is_empty(), "unexpected lex errors: {:?}", errs);
        assert_eq!(toks[0].token, Token::Match);
        assert!(toks.iter().any(|t| t.token == Token::FatArrow));
        assert!(toks.iter().any(|t| matches!(&t.token, Token::Ident(name) if name == "_")));
    }

    #[test]
    fn logos_emits_line_comment_tokens() {
        let mut lexer = Token::lexer("// keep me\ntask");
        assert_eq!(lexer.next(), Some(Ok(Token::LineComment("// keep me".to_string()))));
    }

    #[test]
    fn lex_skips_line_comments_for_parser() {
        let src = "// keep me\ntask Hello : TaskBrief { goal = \"hi\" }";
        let (toks, errs) = lex(src);
        assert!(errs.is_empty());
        assert!(toks.iter().all(|t| !matches!(&t.token, Token::LineComment(_))));
        assert_eq!(toks[0].token, Token::Task);
    }
}
