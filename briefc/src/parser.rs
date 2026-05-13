/// Recursive-descent parser for Brief v0.1.
///
/// Consumes the flat token list from `lexer::lex` and produces an `ast::Program`.
/// Errors are non-fatal where possible — the parser collects them and keeps going
/// so that a single pass reveals as many issues as possible.
///
/// Phase-1 additions over v0.0.1:
///   - Decorators: `@BriefBuilder`, `@deprecated("msg")` etc.
///   - `sealed type Name = Variant | ...`
///   - `struct Name { field: @attr Type }`
///   - `protocol Name { fn sig }`
///   - `effect Name { fn sig }`
///   - Full `TypeRef`: generics (`Result<T, E>`), optional (`T?`)
///   - `await expr` expression

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

    fn peek2(&self) -> Option<&Token> {
        self.tokens.get(self.pos + 1).map(|s| &s.token)
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

    /// Consume an identifier OR a contextual keyword used as an identifier.
    /// Some keywords (`goal`, `extras`) appear as identifiers in task bodies.
    #[allow(dead_code)]
    fn expect_ident_or_kw(&mut self) -> Option<(String, Span)> {
        let span = self.current_span();
        let name = match self.peek().cloned() {
            Some(Token::Ident(n)) => n,
            Some(Token::Type)     => "type".to_string(),
            Some(Token::Struct)   => "struct".to_string(),
            _ => return self.expect_ident(), // will emit error
        };
        self.advance();
        Some((name, span))
    }

    fn at_end(&self) -> bool { self.pos >= self.tokens.len() }

    fn _source(&self) -> &str { self.source }
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
        let mut imports   = Vec::new();
        let mut types     = Vec::new();
        let mut structs   = Vec::new();
        let mut protocols = Vec::new();
        let mut effects   = Vec::new();
        let mut tasks     = Vec::new();

        while !self.at_end() {
            match self.peek() {
                Some(Token::Import)   => { if let Some(i) = self.parse_import() { imports.push(i); } }
                Some(Token::Sealed)   => { if let Some(t) = self.parse_sealed_type() { types.push(t); } }
                Some(Token::Struct)   => { if let Some(s) = self.parse_struct() { structs.push(s); } }
                Some(Token::Protocol) => { if let Some(p) = self.parse_protocol() { protocols.push(p); } }
                Some(Token::Effect)   => { if let Some(e) = self.parse_effect() { effects.push(e); } }
                Some(Token::Task)
                | Some(Token::At)     => { if let Some(t) = self.parse_task() { tasks.push(t); } }
                _ => {
                    let span = self.current_span();
                    let got  = self.peek().cloned();
                    self.errors.push(BriefError {
                        code:    ErrorCode::ParseError,
                        message: format!("unexpected token `{got:?}` at top level"),
                        span,
                        hint:    Some("expected `import skill`, `sealed type`, `struct`, `protocol`, `effect`, or `task`".to_string()),
                    });
                    self.advance();
                }
            }
        }

        Program { imports, types, structs, protocols, effects, tasks }
    }

    // ── import skill "Name" ──────────────────────────────────────────────

    fn parse_import(&mut self) -> Option<SkillImport> {
        let start = self.current_span().start;
        self.expect(&Token::Import)?;
        self.expect(&Token::Skill)?;
        let (name, name_span) = self.expect_str()?;
        Some(SkillImport { name, span: Span::new(start, name_span.end) })
    }

    // ── @Decorator ───────────────────────────────────────────────────────

    fn parse_decorator(&mut self) -> Option<Decorator> {
        let start = self.current_span().start;
        self.expect(&Token::At)?;
        let (name, _) = self.expect_ident()?;
        // optional argument: @deprecated("reason")
        let arg = if self.peek() == Some(&Token::LParen) {
            self.advance();
            let a = self.expect_str().map(|(s, _)| s);
            self.expect(&Token::RParen);
            a
        } else {
            None
        };
        let end = self.prev_span().end;
        Some(Decorator { name, arg, span: Span::new(start, end) })
    }

    // ── @attr on struct fields ────────────────────────────────────────────

    fn parse_attribute(&mut self) -> Option<Attribute> {
        let start = self.current_span().start;
        self.expect(&Token::At)?;
        let (name, _) = self.expect_ident()?;
        let arg = if self.peek() == Some(&Token::LParen) {
            self.advance();
            let a = self.expect_str().map(|(s, _)| s);
            self.expect(&Token::RParen);
            a
        } else {
            None
        };
        let end = self.prev_span().end;
        Some(Attribute { name, arg, span: Span::new(start, end) })
    }

    // ── TypeRef ───────────────────────────────────────────────────────────

    /// Parse a type reference: `String`, `Result<T, E>`, `Theme?`
    fn parse_type_ref(&mut self) -> Option<TypeRef> {
        let start = self.current_span().start;
        let (name, _) = self.expect_ident()?;

        // optional generic arguments: `<T, E>`
        let args = if self.peek() == Some(&Token::LAngle) {
            self.advance();
            let mut args = Vec::new();
            while self.peek() != Some(&Token::RAngle) && !self.at_end() {
                if let Some(arg) = self.parse_type_ref() { args.push(arg); }
                if self.peek() == Some(&Token::Comma) { self.advance(); }
            }
            self.expect(&Token::RAngle);
            args
        } else {
            Vec::new()
        };

        // optional `?` shorthand for `Option<T>`
        let optional = if self.peek() == Some(&Token::Question) {
            self.advance();
            true
        } else {
            false
        };

        let end = self.prev_span().end;
        Some(TypeRef { name, args, optional, span: Span::new(start, end) })
    }

    /// Parse a type parameter list: `<T>` or `<T, U>`
    fn parse_type_params(&mut self) -> Vec<String> {
        if self.peek() != Some(&Token::LAngle) { return Vec::new(); }
        self.advance();
        let mut params = Vec::new();
        while self.peek() != Some(&Token::RAngle) && !self.at_end() {
            if let Some((name, _)) = self.expect_ident() { params.push(name); }
            if self.peek() == Some(&Token::Comma) { self.advance(); }
        }
        self.expect(&Token::RAngle);
        params
    }

    // ── fn signature ─────────────────────────────────────────────────────

    fn parse_fn_sig(&mut self) -> Option<FnSignature> {
        let start = self.current_span().start;
        self.expect(&Token::Fn)?;
        let (name, _) = self.expect_ident()?;
        let type_params = self.parse_type_params();
        self.expect(&Token::LParen)?;
        let params = self.parse_param_list();
        self.expect(&Token::RParen)?;
        self.expect(&Token::Arrow)?;
        let ret = self.parse_type_ref()?;
        let end = self.prev_span().end;
        Some(FnSignature { name, type_params, params, ret, doc: None, span: Span::new(start, end) })
    }

    fn parse_param_list(&mut self) -> Vec<Param> {
        let mut params = Vec::new();
        while self.peek() != Some(&Token::RParen) && !self.at_end() {
            if let Some(p) = self.parse_param() { params.push(p); }
            if self.peek() == Some(&Token::Comma) { self.advance(); }
        }
        params
    }

    fn parse_param(&mut self) -> Option<Param> {
        let start = self.current_span().start;
        // collect leading attributes like `@nonEmpty`
        let mut attrs = Vec::new();
        while self.peek() == Some(&Token::At) {
            if let Some(a) = self.parse_attribute() { attrs.push(a); }
        }
        let (name, _) = self.expect_ident()?;
        self.expect(&Token::Colon)?;
        // attributes may also appear between `:` and the type name
        while self.peek() == Some(&Token::At) {
            if let Some(a) = self.parse_attribute() { attrs.push(a); }
        }
        let ty = self.parse_type_ref()?;
        let end = self.prev_span().end;
        Some(Param { name, attrs, ty, span: Span::new(start, end) })
    }

    // ── sealed type ──────────────────────────────────────────────────────

    fn parse_sealed_type(&mut self) -> Option<SealedTypeDecl> {
        let start = self.current_span().start;
        self.expect(&Token::Sealed)?;
        self.expect(&Token::Type)?;
        let (name, _) = self.expect_ident()?;
        let params = self.parse_type_params();
        self.expect(&Token::Eq)?;

        let mut variants = Vec::new();
        loop {
            if let Some(v) = self.parse_type_variant() { variants.push(v); }
            if self.peek() == Some(&Token::Pipe) {
                self.advance();
            } else {
                break;
            }
        }

        let end = self.prev_span().end;
        Some(SealedTypeDecl { name, params, variants, span: Span::new(start, end) })
    }

    fn parse_type_variant(&mut self) -> Option<TypeVariant> {
        let start = self.current_span().start;
        let (name, _) = self.expect_ident()?;
        let fields = if self.peek() == Some(&Token::LParen) {
            self.advance();
            let mut fields = Vec::new();
            while self.peek() != Some(&Token::RParen) && !self.at_end() {
                if let Some(t) = self.parse_type_ref() { fields.push(t); }
                if self.peek() == Some(&Token::Comma) { self.advance(); }
            }
            self.expect(&Token::RParen);
            fields
        } else {
            Vec::new()
        };
        let end = self.prev_span().end;
        Some(TypeVariant { name, fields, span: Span::new(start, end) })
    }

    // ── struct ───────────────────────────────────────────────────────────

    fn parse_struct(&mut self) -> Option<StructDecl> {
        let start = self.current_span().start;
        self.expect(&Token::Struct)?;
        let (name, _) = self.expect_ident()?;
        let params = self.parse_type_params();
        self.expect(&Token::LBrace)?;

        let mut fields = Vec::new();
        while self.peek() != Some(&Token::RBrace) && !self.at_end() {
            if let Some(f) = self.parse_struct_field() { fields.push(f); }
        }

        let end = self.current_span().end;
        self.expect(&Token::RBrace);
        Some(StructDecl { name, params, fields, span: Span::new(start, end) })
    }

    fn parse_struct_field(&mut self) -> Option<StructField> {
        let start = self.current_span().start;
        let (name, _) = self.expect_ident()?;
        self.expect(&Token::Colon)?;

        let mut attrs = Vec::new();
        while self.peek() == Some(&Token::At) {
            if let Some(a) = self.parse_attribute() { attrs.push(a); }
        }

        let ty = self.parse_type_ref()?;
        let end = self.prev_span().end;
        Some(StructField { name, attrs, ty, span: Span::new(start, end) })
    }

    // ── protocol ─────────────────────────────────────────────────────────

    fn parse_protocol(&mut self) -> Option<ProtocolDecl> {
        let start = self.current_span().start;
        self.expect(&Token::Protocol)?;
        let (name, _) = self.expect_ident()?;
        let params = self.parse_type_params();
        self.expect(&Token::LBrace)?;

        let mut methods = Vec::new();
        while self.peek() != Some(&Token::RBrace) && !self.at_end() {
            if let Some(f) = self.parse_fn_sig() { methods.push(f); }
        }

        let end = self.current_span().end;
        self.expect(&Token::RBrace);
        Some(ProtocolDecl { name, params, methods, span: Span::new(start, end) })
    }

    // ── effect ───────────────────────────────────────────────────────────

    fn parse_effect(&mut self) -> Option<EffectDecl> {
        let start = self.current_span().start;
        self.expect(&Token::Effect)?;
        let (name, _) = self.expect_ident()?;
        let params = self.parse_type_params();
        self.expect(&Token::LBrace)?;

        let mut fns = Vec::new();
        while self.peek() != Some(&Token::RBrace) && !self.at_end() {
            if let Some(f) = self.parse_fn_sig() { fns.push(f); }
        }

        let end = self.current_span().end;
        self.expect(&Token::RBrace);
        Some(EffectDecl { name, params, fns, span: Span::new(start, end) })
    }

    // ── task declaration ─────────────────────────────────────────────────

    fn parse_task(&mut self) -> Option<Task> {
        let start = self.current_span().start;

        // Collect leading decorators: `@BriefBuilder`, `@deprecated(...)` etc.
        let mut decorators = Vec::new();
        while self.peek() == Some(&Token::At) {
            if let Some(d) = self.parse_decorator() { decorators.push(d); }
        }

        let has_builder = decorators.iter().any(|d| d.name == "BriefBuilder");

        self.expect(&Token::Task)?;
        let (name, _) = self.expect_ident()?;
        self.expect(&Token::Colon)?;

        // Expect `TaskBrief` (identifier)
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
            self.advance();
            self.expect(&Token::LBracket)?;
            let mut skills = Vec::new();
            while self.peek() != Some(&Token::RBracket) && !self.at_end() {
                if let Some((name, _)) = self.expect_ident() { skills.push(name); }
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
            let pos_before = self.pos;
            match self.peek().cloned() {
                Some(Token::Ident(ref s)) if s == "goal" => {
                    self.advance();
                    self.expect(&Token::Eq)?;
                    if let Some((val, _)) = self.expect_str() { goal = Some(val); }
                }
                Some(Token::Ident(ref s)) if s == "extras" => {
                    self.advance();
                    self.expect(&Token::Eq)?;
                    extras = self.parse_extras_map();
                }
                Some(Token::Step) => {
                    if let Some(step) = self.parse_step() { steps.push(step); }
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
            // Recovery guard: if nothing was consumed and we aren't at `}`, skip.
            if self.pos == pos_before && self.peek() != Some(&Token::RBrace) {
                self.advance();
            }
        }

        let end = self.current_span().end;
        self.expect(&Token::RBrace);
        Some(Task { decorators, has_builder, name, uses, goal, extras, steps, span: Span::new(start, end) })
    }

    // ── extras = ["key": "value", ...] ───────────────────────────────────

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

    // ── step FetchData { ... } ────────────────────────────────────────────

    fn parse_step(&mut self) -> Option<Step> {
        let start = self.current_span().start;
        self.expect(&Token::Step)?;
        let (name, _) = self.expect_ident()?;
        self.expect(&Token::LBrace)?;

        let mut body = Vec::new();
        while self.peek() != Some(&Token::RBrace) && !self.at_end() {
            let pos_before = self.pos;
            if let Some(stmt) = self.parse_stmt() {
                body.push(stmt);
            } else if self.pos == pos_before {
                // Error recovery: parse_stmt failed without advancing — skip one token.
                self.advance();
            }
        }

        let end = self.current_span().end;
        self.expect(&Token::RBrace);
        Some(Step { name, body, span: Span::new(start, end) })
    }

    // ── statement ─────────────────────────────────────────────────────────

    fn parse_stmt(&mut self) -> Option<Stmt> {
        let start = self.current_span().start;

        match self.peek().cloned() {
            Some(Token::Let) => {
                self.advance();
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

    // ── expression ────────────────────────────────────────────────────────

    fn parse_expr(&mut self) -> Option<Expr> {
        let start = self.current_span().start;

        match self.peek().cloned() {
            // `perform Skill.fn(args)?`
            Some(Token::Perform) => {
                self.advance();
                let (skill, _) = self.expect_ident()?;
                self.expect(&Token::Dot)?;
                let (func, _) = self.expect_ident()?;
                // optional generic type args on the call: perform GQL.query<User>(...)
                if self.peek() == Some(&Token::LAngle) {
                    self.advance();
                    while self.peek() != Some(&Token::RAngle) && !self.at_end() {
                        self.advance(); // skip type args (not yet in AST)
                    }
                    self.expect(&Token::RAngle);
                }
                self.expect(&Token::LParen)?;
                let args = self.parse_arg_list()?;
                self.expect(&Token::RParen)?;
                let propagate = if self.peek() == Some(&Token::Question) {
                    self.advance(); true
                } else { false };
                let end = self.prev_span().end;
                Some(Expr::Perform { skill, func, args, propagate, span: Span::new(start, end) })
            }

            // `await expr`
            Some(Token::Await) => {
                self.advance();
                let inner = self.parse_expr()?;
                let end   = inner.span().end;
                Some(Expr::Await { expr: Box::new(inner), span: Span::new(start, end) })
            }

            // `Ident` — could be `foo.bar(args)`, `foo(args)`, or just `foo`
            Some(Token::Ident(name)) => {
                self.advance();

                if self.peek() == Some(&Token::Dot) {
                    // check it's not `foo.` followed by a non-ident (parse error)
                    if !matches!(self.peek2(), Some(Token::Ident(_))) {
                        let span = self.current_span();
                        self.errors.push(BriefError {
                            code:    ErrorCode::ParseError,
                            message: "expected method name after `.`".to_string(),
                            span,
                            hint:    None,
                        });
                        return None;
                    }
                    self.advance(); // `.`
                    let (func, _) = self.expect_ident()?;
                    self.expect(&Token::LParen)?;
                    let args = self.parse_arg_list()?;
                    self.expect(&Token::RParen)?;
                    let end = self.prev_span().end;
                    Some(Expr::Call { receiver: Some(name), func, args, span: Span::new(start, end) })
                } else if self.peek() == Some(&Token::LParen) {
                    self.advance();
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

    // ── argument list: (expr (',' expr)*)? ────────────────────────────────

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

    #[test]
    fn parse_brief_builder_decorator() {
        let src = r#"
            @BriefBuilder
            task T : TaskBrief { goal = "x" }
        "#;
        let (prog, errs) = parse_src(src);
        assert!(errs.is_empty(), "{errs:?}");
        assert!(prog.tasks[0].has_builder);
        assert_eq!(prog.tasks[0].decorators[0].name, "BriefBuilder");
    }

    #[test]
    fn parse_sealed_type() {
        let src = "sealed type Platform = iOS | Android | Web";
        let (prog, errs) = parse_src(src);
        assert!(errs.is_empty(), "{errs:?}");
        assert_eq!(prog.types.len(), 1);
        assert_eq!(prog.types[0].name, "Platform");
        assert_eq!(prog.types[0].variants.len(), 3);
    }

    #[test]
    fn parse_struct_with_attrs() {
        let src = r#"
            struct UserProfile {
                id: @nonEmpty String
                email: @matches(".*@.*") String
            }
        "#;
        let (prog, errs) = parse_src(src);
        assert!(errs.is_empty(), "{errs:?}");
        assert_eq!(prog.structs.len(), 1);
        let s = &prog.structs[0];
        assert_eq!(s.fields[0].attrs[0].name, "nonEmpty");
        assert_eq!(s.fields[1].attrs[0].name, "matches");
        assert_eq!(s.fields[1].attrs[0].arg, Some(".*@.*".to_string()));
    }

    #[test]
    fn parse_effect_decl() {
        let src = r#"
            effect GraphQL {
                fn query(op: Operation) -> Result
                fn schema(name: String) -> Schema
            }
        "#;
        let (prog, errs) = parse_src(src);
        assert!(errs.is_empty(), "{errs:?}");
        assert_eq!(prog.effects.len(), 1);
        assert_eq!(prog.effects[0].fns.len(), 2);
        assert_eq!(prog.effects[0].fns[0].name, "query");
    }

    #[test]
    fn parse_protocol_decl() {
        let src = r#"
            protocol Renderable {
                fn render() -> Component
            }
        "#;
        let (prog, errs) = parse_src(src);
        assert!(errs.is_empty(), "{errs:?}");
        assert_eq!(prog.protocols.len(), 1);
        assert_eq!(prog.protocols[0].methods[0].name, "render");
    }

    #[test]
    fn parse_await_expr() {
        let src = r#"
            task T : TaskBrief {
                goal = "test"
                step S {
                    let r = await perform GQL.query(Q)?;
                }
            }
        "#;
        // `await perform ...` — await wraps the perform
        // Parse `await` followed by `perform ...`
        let (prog, errs) = parse_src(src);
        assert!(errs.is_empty(), "{errs:?}");
        assert!(matches!(&prog.tasks[0].steps[0].body[0],
            Stmt::Let { value: Expr::Await { .. }, .. }));
    }
}
