/// Recursive-descent parser for Brief v0.0.1.
///
/// Consumes the flat token list from `lexer::lex` and produces an `ast::Program`.
/// Errors are non-fatal where possible — the parser collects them and keeps going
/// so that a single pass reveals as many issues as possible.

use crate::ast::*;
use crate::errors::{BriefError, ErrorCode};
use crate::lexer::{Spanned, Token};

// ─────────────────────────────────────────────────────────────────────────────
// Parser state
// ─────────────────────────────────────────────────────────────────────────────

pub struct Parser<'a> {
    tokens:  &'a [Spanned],
    pos:     usize,
    #[allow(dead_code)]
    source:  &'a str,
    pub errors: Vec<BriefError>,
}

impl<'a> Parser<'a> {
    pub fn new(tokens: &'a [Spanned], source: &'a str) -> Self {
        Self { tokens, pos: 0, source, errors: Vec::new() }
    }

    // ── Peeking / advancing ──────────────────────────────────────────────

    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos).map(|s| &s.token)
    }

    #[allow(dead_code)]
    fn peek_spanned(&self) -> Option<&Spanned> {
        self.tokens.get(self.pos)
    }

    fn advance(&mut self) -> Option<&Spanned> {
        let s = self.tokens.get(self.pos);
        if s.is_some() { self.pos += 1; }
        s
    }

    fn current_span(&self) -> Span {
        self.tokens.get(self.pos)
            .map(|s| Span::new(s.start, s.end))
            .unwrap_or_default()
    }

    fn prev_span(&self) -> Span {
        if self.pos == 0 { return Span::default(); }
        self.tokens.get(self.pos - 1)
            .map(|s| Span::new(s.start, s.end))
            .unwrap_or_default()
    }

    /// Consume the next token if it matches `expected`, returning its span.
    /// Records a parse error and returns `None` on mismatch.
    fn expect(&mut self, expected: &Token) -> Option<Span> {
        if self.peek() == Some(expected) {
            let s = self.advance().unwrap();
            Some(Span::new(s.start, s.end))
        } else {
            let got  = self.peek().cloned();
            let span = self.current_span();
            self.errors.push(BriefError {
                code:    ErrorCode::ParseError,
                message: format!("expected `{expected:?}`, got `{got:?}`"),
                span,
                hint:    None,
            });
            None
        }
    }

    /// Consume an `Ident` token, returning its string value.
    fn expect_ident(&mut self) -> Option<(String, Span)> {
        match self.peek().cloned() {
            Some(Token::Ident(name)) => {
                let s = self.advance().unwrap();
                Some((name, Span::new(s.start, s.end)))
            }
            got => {
                let span = self.current_span();
                self.errors.push(BriefError {
                    code:    ErrorCode::ParseError,
                    message: format!("expected identifier, got `{got:?}`"),
                    span,
                    hint:    None,
                });
                None
            }
        }
    }

    /// Consume a `Str` token, returning its (already-unquoted) value.
    fn expect_str(&mut self) -> Option<(String, Span)> {
        match self.peek().cloned() {
            Some(Token::Str(val)) => {
                let s = self.advance().unwrap();
                Some((val, Span::new(s.start, s.end)))
            }
            got => {
                let span = self.current_span();
                self.errors.push(BriefError {
                    code:    ErrorCode::ParseError,
                    message: format!("expected string literal, got `{got:?}`"),
                    span,
                    hint:    None,
                });
                None
            }
        }
    }

    fn at_end(&self) -> bool { self.pos >= self.tokens.len() }
}

// ─────────────────────────────────────────────────────────────────────────────
// Top-level
// ─────────────────────────────────────────────────────────────────────────────

pub fn parse(tokens: &[Spanned], source: &str) -> (Program, Vec<BriefError>) {
    let mut p = Parser::new(tokens, source);
    let program = p.parse_program();
    (program, p.errors)
}

impl<'a> Parser<'a> {
    fn parse_program(&mut self) -> Program {
        let mut imports = Vec::new();
        let mut tasks   = Vec::new();

        while !self.at_end() {
            match self.peek() {
                Some(Token::Import)       => { if let Some(i) = self.parse_import() { imports.push(i); } }
                Some(Token::Task)
                | Some(Token::BriefBuilder) => { if let Some(t) = self.parse_task() { tasks.push(t); } }
                _ => {
                    // Skip unexpected tokens at the top level (error recovery)
                    let span = self.current_span();
                    let got  = self.peek().cloned();
                    self.errors.push(BriefError {
                        code:    ErrorCode::ParseError,
                        message: format!("unexpected token `{got:?}` at top level"),
                        span,
                        hint:    Some("expected `import skill` or `task`".to_string()),
                    });
                    self.advance();
                }
            }
        }

        Program { imports, tasks }
    }

    // ── import skill "Name" ──────────────────────────────────────────────

    fn parse_import(&mut self) -> Option<SkillImport> {
        let start = self.current_span().start;
        self.expect(&Token::Import)?;
        self.expect(&Token::Skill)?;
        let (name, name_span) = self.expect_str()?;
        Some(SkillImport { name, span: Span::new(start, name_span.end) })
    }

    // ── task declaration ─────────────────────────────────────────────────

    fn parse_task(&mut self) -> Option<Task> {
        let start = self.current_span().start;

        // optional @BriefBuilder decorator
        let has_builder = if self.peek() == Some(&Token::BriefBuilder) {
            self.advance();
            true
        } else {
            false
        };

        self.expect(&Token::Task)?;
        let (name, _) = self.expect_ident()?;
        self.expect(&Token::Colon)?;

        // Expect `TaskBrief` (as an identifier)
        match self.peek().cloned() {
            Some(Token::Ident(ref s)) if s == "TaskBrief" => { self.advance(); }
            got => {
                let span = self.current_span();
                self.errors.push(BriefError {
                    code:    ErrorCode::ParseError,
                    message: format!("expected `TaskBrief` after `:`, got `{got:?}`"),
                    span,
                    hint:    Some("task declarations must be `task Name : TaskBrief { ... }`".to_string()),
                });
            }
        }

        // optional `uses [Skill1, Skill2]`
        let uses = if self.peek() == Some(&Token::Uses) {
            self.advance(); // consume `uses`
            self.expect(&Token::LBracket)?;
            let mut skills = Vec::new();
            while self.peek() != Some(&Token::RBracket) && !self.at_end() {
                if let Some((name, _)) = self.expect_ident() {
                    skills.push(name);
                }
                if self.peek() == Some(&Token::Comma) { self.advance(); }
            }
            self.expect(&Token::RBracket)?;
            skills
        } else {
            Vec::new()
        };

        self.expect(&Token::LBrace)?;

        let mut goal   = None;
        let mut extras = Vec::new();
        let mut steps  = Vec::new();

        while self.peek() != Some(&Token::RBrace) && !self.at_end() {
            match self.peek().cloned() {
                Some(Token::Ident(ref s)) if s == "goal" => {
                    self.advance(); // `goal`
                    self.expect(&Token::Eq)?;
                    if let Some((val, _)) = self.expect_str() {
                        goal = Some(val);
                    }
                }
                Some(Token::Ident(ref s)) if s == "extras" => {
                    self.advance(); // `extras`
                    self.expect(&Token::Eq)?;
                    extras = self.parse_extras_map();
                }
                Some(Token::Step) => {
                    if let Some(step) = self.parse_step() {
                        steps.push(step);
                    }
                }
                got => {
                    let span = self.current_span();
                    self.errors.push(BriefError {
                        code:    ErrorCode::ParseError,
                        message: format!("unexpected token `{got:?}` inside task body"),
                        span,
                        hint:    Some("expected `goal`, `extras`, or `step`".to_string()),
                    });
                    self.advance();
                }
            }
        }

        let end = self.current_span().end;
        self.expect(&Token::RBrace);

        Some(Task { has_builder, name, uses, goal, extras, steps, span: Span::new(start, end) })
    }

    // ── extras = ["key": "value", ...] ──────────────────────────────────

    fn parse_extras_map(&mut self) -> Vec<(String, String)> {
        let mut pairs = Vec::new();
        if self.expect(&Token::LBracket).is_none() { return pairs; }

        while self.peek() != Some(&Token::RBracket) && !self.at_end() {
            let key = match self.expect_str() { Some((k, _)) => k, None => break };
            if self.expect(&Token::Colon).is_none() { break; }
            let val = match self.expect_str() { Some((v, _)) => v, None => break };
            pairs.push((key, val));
            if self.peek() == Some(&Token::Comma) { self.advance(); }
        }

        self.expect(&Token::RBracket);
        pairs
    }

    // ── step FetchData { ... } ───────────────────────────────────────────

    fn parse_step(&mut self) -> Option<Step> {
        let start = self.current_span().start;
        self.expect(&Token::Step)?;
        let (name, _) = self.expect_ident()?;
        self.expect(&Token::LBrace)?;

        let mut body = Vec::new();
        while self.peek() != Some(&Token::RBrace) && !self.at_end() {
            if let Some(stmt) = self.parse_stmt() {
                body.push(stmt);
            }
        }

        let end = self.current_span().end;
        self.expect(&Token::RBrace);

        Some(Step { name, body, span: Span::new(start, end) })
    }

    // ── statement ────────────────────────────────────────────────────────

    fn parse_stmt(&mut self) -> Option<Stmt> {
        let start = self.current_span().start;

        match self.peek().cloned() {
            Some(Token::Let) => {
                self.advance(); // consume `let`
                let (name, _) = self.expect_ident()?;
                self.expect(&Token::Eq)?;
                let value = self.parse_expr()?;
                self.expect(&Token::Semi);
                let end = self.prev_span().end;
                Some(Stmt::Let { name, value, span: Span::new(start, end) })
            }
            _ => {
                let value = self.parse_expr()?;
                self.expect(&Token::Semi);
                let end = self.prev_span().end;
                Some(Stmt::Expr { value, span: Span::new(start, end) })
            }
        }
    }

    // ── expression ───────────────────────────────────────────────────────

    fn parse_expr(&mut self) -> Option<Expr> {
        let start = self.current_span().start;

        match self.peek().cloned() {
            // `perform Skill.fn(args)?`
            Some(Token::Perform) => {
                self.advance(); // consume `perform`
                let (skill, _) = self.expect_ident()?;
                self.expect(&Token::Dot)?;
                let (func, _) = self.expect_ident()?;
                self.expect(&Token::LParen)?;
                let args = self.parse_arg_list()?;
                self.expect(&Token::RParen)?;
                let propagate = if self.peek() == Some(&Token::Question) {
                    self.advance();
                    true
                } else {
                    false
                };
                let end = self.prev_span().end;
                Some(Expr::Perform { skill, func, args, propagate, span: Span::new(start, end) })
            }

            // `Ident` — could be `foo.bar(args)`, `foo(args)`, or just `foo`
            Some(Token::Ident(name)) => {
                self.advance();

                if self.peek() == Some(&Token::Dot) {
                    self.advance(); // consume `.`
                    let (func, _) = self.expect_ident()?;
                    self.expect(&Token::LParen)?;
                    let args = self.parse_arg_list()?;
                    self.expect(&Token::RParen)?;
                    let end = self.prev_span().end;
                    Some(Expr::Call { receiver: Some(name), func, args, span: Span::new(start, end) })
                } else if self.peek() == Some(&Token::LParen) {
                    self.advance(); // consume `(`
                    let args = self.parse_arg_list()?;
                    self.expect(&Token::RParen)?;
                    let end = self.prev_span().end;
                    Some(Expr::Call { receiver: None, func: name, args, span: Span::new(start, end) })
                } else {
                    let end = self.prev_span().end;
                    Some(Expr::Ident { name, span: Span::new(start, end) })
                }
            }

            // String literal as expression
            Some(Token::Str(val)) => {
                self.advance();
                let end = self.prev_span().end;
                Some(Expr::Str { value: val, span: Span::new(start, end) })
            }

            got => {
                let span = self.current_span();
                self.errors.push(BriefError {
                    code:    ErrorCode::ParseError,
                    message: format!("expected expression, got `{got:?}`"),
                    span,
                    hint:    None,
                });
                None
            }
        }
    }

    // ── argument list: (expr (',' expr)*)? ───────────────────────────────

    fn parse_arg_list(&mut self) -> Option<Vec<Expr>> {
        let mut args = Vec::new();
        while self.peek() != Some(&Token::RParen) && !self.at_end() {
            let expr = self.parse_expr()?;
            args.push(expr);
            if self.peek() == Some(&Token::Comma) { self.advance(); }
        }
        Some(args)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::lex;

    fn parse_src(src: &str) -> (Program, Vec<BriefError>) {
        let (tokens, _) = lex(src);
        parse(&tokens, src)
    }

    #[test]
    fn parse_minimal_task() {
        let (prog, errs) = parse_src(r#"task Hello : TaskBrief { goal = "hi" }"#);
        assert!(errs.is_empty(), "{errs:?}");
        assert_eq!(prog.tasks.len(), 1);
        assert_eq!(prog.tasks[0].name, "Hello");
        assert_eq!(prog.tasks[0].goal, Some("hi".to_string()));
    }

    #[test]
    fn parse_import_and_uses() {
        let src = r#"
            import skill "GraphQL"
            task T : TaskBrief uses [GraphQL] { goal = "x" }
        "#;
        let (prog, errs) = parse_src(src);
        assert!(errs.is_empty(), "{errs:?}");
        assert_eq!(prog.imports[0].name, "GraphQL");
        assert_eq!(prog.tasks[0].uses, vec!["GraphQL"]);
    }

    #[test]
    fn parse_step_with_perform() {
        let src = r#"
            import skill "GQL"
            task T : TaskBrief uses [GQL] {
                goal = "test"
                step Fetch {
                    let user = perform GQL.query(UserQuery)?;
                }
            }
        "#;
        let (prog, errs) = parse_src(src);
        assert!(errs.is_empty(), "{errs:?}");
        let step = &prog.tasks[0].steps[0];
        assert_eq!(step.name, "Fetch");
        assert!(matches!(&step.body[0], Stmt::Let { name, value: Expr::Perform { propagate: true, .. }, .. } if name == "user"));
    }
}
