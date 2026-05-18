/// `brief fmt` — canonical source formatter for Brief files.
///
/// Reads a `.brief` file, parses it, then regenerates the source from the AST
/// in a canonical style. Optionally writes in-place or prints to stdout.
///
/// ## Rules
///
/// - 4-space indentation (no tabs)
/// - One blank line between top-level declarations
/// - `goal` and `extras` always on their own indented lines
/// - `step` blocks: name + space + `{`, body indented 8 spaces
/// - `let` and `perform` each on their own line
/// - String literals: double quotes
/// - `extras` entries: sorted alphabetically by key
/// - Trailing newline, no trailing whitespace
/// - `brief fmt --write` refuses to overwrite files that contain `//` comments

use std::path::Path;
use colored::Colorize;
use logos::Logos;

use crate::ast::*;
use crate::lexer::{lex, Token};
use crate::parser::parse;

// ─────────────────────────────────────────────────────────────────────────────
// Formatter
// ─────────────────────────────────────────────────────────────────────────────

pub struct Formatter {
    out:    String,
    indent: usize,
}

impl Formatter {
    fn new() -> Self { Formatter { out: String::new(), indent: 0 } }

    fn line(&mut self, s: &str) {
        if s.is_empty() {
            self.out.push('\n');
        } else {
            let pad = "    ".repeat(self.indent);
            self.out.push_str(&pad);
            self.out.push_str(s);
            self.out.push('\n');
        }
    }

    fn blank(&mut self) { self.out.push('\n'); }

    fn indented<F: FnOnce(&mut Self)>(&mut self, f: F) {
        self.indent += 1;
        f(self);
        self.indent -= 1;
    }

    // ── Type references ──────────────────────────────────────────────────

    fn fmt_type_ref(ty: &TypeRef) -> String {
        let name = if ty.args.is_empty() {
            ty.name.clone()
        } else {
            let args = ty.args.iter().map(Self::fmt_type_ref).collect::<Vec<_>>().join(", ");
            format!("{}<{}>", ty.name, args)
        };
        if ty.optional { format!("{name}?") } else { name }
    }

    // ── Attributes ────────────────────────────────────────────────────────

    fn fmt_attrs(attrs: &[Attribute]) -> String {
        attrs.iter()
            .map(|a| match &a.arg {
                Some(arg) => format!("@{}({:?})", a.name, arg),
                None      => format!("@{}", a.name),
            })
            .collect::<Vec<_>>()
            .join(" ")
    }

    fn fmt_pattern(pattern: &Pattern) -> String {
        match pattern {
            Pattern::Variant(name) => name.clone(),
            Pattern::Variant1(name, binding) => format!("{name}({binding})"),
            Pattern::Wildcard => "_".to_string(),
        }
    }

    // ── Expressions ───────────────────────────────────────────────────────

    fn fmt_expr(expr: &Expr) -> String {
        match expr {
            Expr::Perform { skill, func, args, propagate, .. } => {
                let arg_str = args.iter().map(Self::fmt_expr).collect::<Vec<_>>().join(", ");
                let call    = format!("perform {skill}.{func}({arg_str})");
                if *propagate { format!("{call}?") } else { call }
            }
            Expr::Await { expr: inner, .. } => {
                format!("await {}", Self::fmt_expr(inner))
            }
            Expr::Call { receiver, func, args, .. } => {
                let arg_str = args.iter().map(Self::fmt_expr).collect::<Vec<_>>().join(", ");
                match receiver {
                    Some(r) if args.is_empty() => format!("{r}.{func}"),
                    Some(r) => format!("{r}.{func}({arg_str})"),
                    None    => format!("{func}({arg_str})"),
                }
            }
            Expr::Match { scrutinee, arms } => {
                let scrutinee = Self::fmt_expr(scrutinee);
                let width = arms
                    .iter()
                    .map(|arm| Self::fmt_pattern(&arm.pattern).len())
                    .max()
                    .unwrap_or(0);
                let mut out = format!("match {scrutinee} {{\n");
                for arm in arms {
                    let pattern = Self::fmt_pattern(&arm.pattern);
                    let body = Self::fmt_expr(&arm.body);
                    out.push_str(&format!("    {pattern:<width$} => {body}\n"));
                }
                out.push('}');
                out
            }
            Expr::Ident { name, .. } => name.clone(),
            Expr::Str   { value, .. } => format!("{value:?}"),
            Expr::Int   { value, .. } => value.to_string(),
        }
    }

    // ── Statements ────────────────────────────────────────────────────────

    fn fmt_expr_stmt(&mut self, prefix: &str, expr: &Expr, suffix: &str) {
        let rendered = Self::fmt_expr(expr);
        if !rendered.contains('\n') {
            self.line(&format!("{prefix}{rendered}{suffix}"));
            return;
        }

        let lines = rendered.lines().collect::<Vec<_>>();
        self.line(&format!("{prefix}{}", lines[0]));
        for line in &lines[1..lines.len().saturating_sub(1)] {
            self.line(line);
        }
        if let Some(last) = lines.last() {
            self.line(&format!("{last}{suffix}"));
        }
    }

    fn fmt_stmt(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::Let { name, value, attrs, .. } => {
                let prefix = if attrs.is_empty() {
                    String::new()
                } else {
                    format!("{} ", attrs.iter().map(|a| format!("@{a}")).collect::<Vec<_>>().join(" "))
                };
                self.fmt_expr_stmt(&format!("{prefix}let {name} = "), value, ";");
            }
            Stmt::Expr { value, .. } => {
                self.fmt_expr_stmt("", value, ";");
            }
        }
    }

    // ── Steps ─────────────────────────────────────────────────────────────

    fn fmt_typed_fields(f: &mut Formatter, keyword: &str, fields: &[ExtrasField]) {
        if fields.len() == 1 {
            let field = &fields[0];
            f.line(&format!("{keyword} {{ {}: {} }}", field.name, Self::fmt_type_ref(&field.type_ref)));
        } else if !fields.is_empty() {
            f.line(&format!("{keyword} {{"));
            f.indented(|g| {
                for (i, field) in fields.iter().enumerate() {
                    let comma = if i + 1 < fields.len() { "," } else { "" };
                    g.line(&format!("{}: {}{}", field.name, Self::fmt_type_ref(&field.type_ref), comma));
                }
            });
            f.line("}");
        }
    }

    fn fmt_step(&mut self, step: &Step) {
        self.line(&format!("step {} {{", step.name));
        self.indented(|f| {
            if !step.pre_conditions.is_empty() {
                f.line(&format!("pre {{ {} }}", step.pre_conditions.join(", ")));
            }
            if !step.post_conditions.is_empty() {
                f.line(&format!("post {{ {} }}", step.post_conditions.join(", ")));
            }
            if (!step.pre_conditions.is_empty() || !step.post_conditions.is_empty()) && !step.body.is_empty() {
                f.blank();
            }
            for stmt in &step.body {
                f.fmt_stmt(stmt);
            }
        });
        self.line("}");
    }

    // ── FnSignature ───────────────────────────────────────────────────────

    fn fmt_fn_sig(sig: &FnSignature) -> String {
        let tp = if sig.type_params.is_empty() {
            String::new()
        } else {
            format!("<{}>", sig.type_params.join(", "))
        };
        let params = sig.params.iter().map(|p| {
            let attrs = Self::fmt_attrs(&p.attrs);
            let ty    = Self::fmt_type_ref(&p.ty);
            if attrs.is_empty() { format!("{}: {ty}", p.name) }
            else                { format!("{}: {attrs} {ty}", p.name) }
        }).collect::<Vec<_>>().join(", ");
        let ret_attrs = Self::fmt_attrs(&sig.ret_attrs);
        let ret_ty    = Self::fmt_type_ref(&sig.ret);
        let ret = if ret_attrs.is_empty() { ret_ty } else { format!("{ret_attrs} {ret_ty}") };
        format!("fn {}{tp}({params}) -> {ret}", sig.name)
    }

    // ── Sealed types ──────────────────────────────────────────────────────

    fn fmt_sealed_type(&mut self, ty: &SealedTypeDecl) {
        let name = &ty.name;
        let params = if ty.params.is_empty() {
            String::new()
        } else {
            format!("<{}>", ty.params.join(", "))
        };
        let variants = ty.variants.iter().map(|v| {
            if v.fields.is_empty() {
                v.name.clone()
            } else {
                let fs = v.fields.iter().map(Self::fmt_type_ref).collect::<Vec<_>>().join(", ");
                format!("{}({})", v.name, fs)
            }
        }).collect::<Vec<_>>().join(" | ");
        self.line(&format!("sealed type {name}{params} = {variants}"));
    }

    // ── Type alias ────────────────────────────────────────────────────────

    fn fmt_type_alias(&mut self, a: &TypeAliasDecl) {
        let attrs = Self::fmt_attrs(&a.attrs);
        let base  = Self::fmt_type_ref(&a.base);
        if attrs.is_empty() {
            self.line(&format!("type {} = {base}", a.name));
        } else {
            self.line(&format!("type {} = {attrs} {base}", a.name));
        }
    }

    // ── Effect group ──────────────────────────────────────────────────────

    fn fmt_effect_group(&mut self, g: &EffectGroupDecl) {
        self.line(&format!("type {} = [{}]", g.name, g.members.join(", ")));
    }

    // ── Structs ───────────────────────────────────────────────────────────

    fn fmt_struct(&mut self, s: &StructDecl) {
        let name = &s.name;
        let params = if s.params.is_empty() {
            String::new()
        } else {
            format!("<{}>", s.params.join(", "))
        };
        self.line(&format!("struct {name}{params} {{"));
        self.indented(|f| {
            for field in &s.fields {
                let attrs = Self::fmt_attrs(&field.attrs);
                let ty    = Self::fmt_type_ref(&field.ty);
                if attrs.is_empty() {
                    f.line(&format!("{}: {ty}", field.name));
                } else {
                    f.line(&format!("{}: {attrs} {ty}", field.name));
                }
            }
        });
        self.line("}");
    }

    // ── Protocols ─────────────────────────────────────────────────────────

    fn fmt_protocol(&mut self, p: &ProtocolDecl) {
        let name = &p.name;
        let params = if p.params.is_empty() {
            String::new()
        } else {
            format!("<{}>", p.params.join(", "))
        };
        self.line(&format!("protocol {name}{params} {{"));
        self.indented(|f| {
            for m in &p.methods {
                f.line(&format!("{}", Self::fmt_fn_sig(m)));
            }
        });
        self.line("}");
    }

    // ── Effects ───────────────────────────────────────────────────────────

    fn fmt_effect(&mut self, e: &EffectDecl) {
        let name = &e.name;
        let params = if e.params.is_empty() {
            String::new()
        } else {
            format!("<{}>", e.params.join(", "))
        };
        self.line(&format!("effect {name}{params} {{"));
        self.indented(|f| {
            for fn_ in &e.fns {
                f.line(&format!("{}", Self::fmt_fn_sig(fn_)));
            }
        });
        self.line("}");
    }

    // ── Tasks ─────────────────────────────────────────────────────────────

    fn fmt_task(&mut self, task: &Task) {
        for d in &task.decorators {
            match &d.arg {
                Some(a) => self.line(&format!("@{}({:?})", d.name, a)),
                None    => self.line(&format!("@{}", d.name)),
            }
        }

        let uses = if task.uses.is_empty() {
            String::new()
        } else {
            format!(" uses [{}]", task.uses.join(", "))
        };
        self.line(&format!("task {} : TaskBrief{uses} {{", task.name));

        self.indented(|f| {
            if let Some(goal) = &task.goal {
                f.line(&format!("goal   = {goal:?}"));
            }
            if !task.effects.is_empty() {
                f.line(&format!("effects [{}]", task.effects.join(", ")));
            }

            // Format extras if present
            if let Some(extras_node) = &task.extras {
                match extras_node {
                    ExtrasNode::StringMap(pairs) => {
                        let mut sorted = pairs.clone();
                        sorted.sort_by(|a, b| a.0.cmp(&b.0));
                        if sorted.len() == 1 {
                            f.line(&format!("extras = [{}]", sorted.iter()
                                .map(|(k,v)| format!("{k:?}: {v:?}"))
                                .collect::<Vec<_>>().join(", ")));
                        } else if !sorted.is_empty() {
                            f.line("extras = [");
                            f.indented(|g| {
                                for (i, (k, v)) in sorted.iter().enumerate() {
                                    let comma = if i + 1 < sorted.len() { "," } else { "" };
                                    g.line(&format!("{k:?}: {v:?}{comma}"));
                                }
                            });
                            f.line("]");
                        }
                    }
                    ExtrasNode::TypedRecord(fields) => {
                        Self::fmt_typed_fields(f, "extras", fields);
                    }
                }
            }

            if let Some(provides) = &task.provides {
                Self::fmt_typed_fields(f, "provides", provides);
            }

            for (i, step) in task.steps.iter().enumerate() {
                if i > 0
                    || task.goal.is_some()
                    || !task.effects.is_empty()
                    || task.extras.is_some()
                    || task.provides.is_some()
                {
                    f.blank();
                }
                f.fmt_step(step);
            }
        });

        self.line("}");
    }

    // ── Test block ────────────────────────────────────────────────────────

    fn fmt_test(&mut self, test: &TestDecl) {
        self.line(&format!("test {:?} {{", test.name));
        self.indented(|f| {
            for stmt in &test.body {
                f.fmt_stmt(stmt);
            }
        });
        self.line("}");
    }

    // ── Program ───────────────────────────────────────────────────────────

    pub fn format_program(program: &Program) -> String {
        let mut f = Formatter::new();
        let mut first = true;

        let sep = |f: &mut Formatter, first: &mut bool| {
            if !*first { f.blank(); }
            *first = false;
        };

        // Imports
        for import in &program.imports {
            sep(&mut f, &mut first);
            f.line(&format!("import skill {:?}", import.name));
        }

        // Sealed types
        for ty in &program.types {
            sep(&mut f, &mut first);
            f.fmt_sealed_type(ty);
        }

        // Type aliases (refinements)
        for a in &program.type_aliases {
            sep(&mut f, &mut first);
            f.fmt_type_alias(a);
        }

        // Effect group aliases
        for g in &program.effect_groups {
            sep(&mut f, &mut first);
            f.fmt_effect_group(g);
        }

        // Structs
        for s in &program.structs {
            sep(&mut f, &mut first);
            f.fmt_struct(s);
        }

        // Protocols
        for p in &program.protocols {
            sep(&mut f, &mut first);
            f.fmt_protocol(p);
        }

        // Effects
        for e in &program.effects {
            sep(&mut f, &mut first);
            f.fmt_effect(e);
        }

        // Tasks
        for task in &program.tasks {
            sep(&mut f, &mut first);
            f.fmt_task(task);
        }

        // Test blocks
        for test in &program.tests {
            sep(&mut f, &mut first);
            f.fmt_test(test);
        }

        // Ensure trailing newline
        if !f.out.ends_with('\n') { f.out.push('\n'); }
        f.out
    }
}

fn format_source(source: &str) -> String {
    let (tokens, _)  = lex(source);
    let (program, _) = parse(&tokens, source);
    Formatter::format_program(&program)
}

fn offset_to_line(source: &str, offset: usize) -> usize {
    source[..offset].bytes().filter(|b| *b == b'\n').count() + 1
}

fn line_comment_lines(source: &str) -> Vec<usize> {
    let mut lines = Vec::new();
    let mut lexer = Token::lexer(source);

    while let Some(result) = lexer.next() {
        if let Ok(Token::LineComment(_)) = result {
            lines.push(offset_to_line(source, lexer.span().start));
        }
    }

    lines.dedup();
    lines
}

fn format_for_write(source: &str) -> Result<String, Vec<usize>> {
    let formatted = format_source(source);
    let comment_lines = line_comment_lines(source);
    if comment_lines.is_empty() { Ok(formatted) } else { Err(comment_lines) }
}

// ─────────────────────────────────────────────────────────────────────────────
// Public entry point
// ─────────────────────────────────────────────────────────────────────────────

/// Format a `.brief` file.
///
/// - `write`: if true, overwrites the file in-place; otherwise prints to stdout.
/// - `check`: if true, exits non-zero if the file is not already formatted (CI mode).
pub fn fmt_file(path: &Path, write: bool, check: bool) -> bool {
    let source = match std::fs::read_to_string(path) {
        Ok(s)  => s,
        Err(e) => {
            eprintln!("{}: cannot read {}: {e}", "error".red().bold(), path.display());
            return false;
        }
    };

    let formatted = format_source(&source);

    if check {
        if formatted == source {
            println!("{} {} is already formatted.", "✅".green(), path.display());
            return true;
        } else {
            println!("{} {} is not formatted — run `brief fmt {}`",
                "❌".red(), path.display(), path.display());
            return false;
        }
    }

    if write {
        let formatted = match format_for_write(&source) {
            Ok(formatted) => formatted,
            Err(lines) => {
                let line_list = lines.iter().map(|line| line.to_string()).collect::<Vec<_>>().join(", ");
                eprintln!("{}: comments detected — formatted output would drop comments on lines {line_list}", "error".red().bold());
                eprintln!("fix: remove comments before running brief fmt --write, or use brief fmt (without --write) to preview");
                return false;
            }
        };

        if formatted == source {
            println!("{} {} (unchanged)", "✅".green(), path.display());
            return true;
        }
        match std::fs::write(path, &formatted) {
            Ok(_)  => println!("{} {}", "✅ formatted:".green(), path.display()),
            Err(e) => {
                eprintln!("{}: cannot write {}: {e}", "error".red().bold(), path.display());
                return false;
            }
        }
    } else {
        print!("{}", formatted);
    }

    true
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::lex;
    use crate::parser::parse;

    fn roundtrip(src: &str) -> String {
        let (tokens, _) = lex(src);
        let (prog, _)   = parse(&tokens, src);
        Formatter::format_program(&prog)
    }

    #[test]
    fn fmt_minimal_task() {
        let src = r#"task Hello : TaskBrief { goal = "Say hello" }"#;
        let out = roundtrip(src);
        assert!(out.contains("task Hello : TaskBrief {"));
        assert!(out.contains(r#"goal   = "Say hello""#));
        assert!(out.ends_with('\n'));
    }

    #[test]
    fn fmt_sealed_type() {
        let src = "sealed type Platform = iOS | Android | Web";
        let out = roundtrip(src);
        assert!(out.contains("sealed type Platform = iOS | Android | Web"));
    }

    #[test]
    fn fmt_sealed_type_with_param() {
        let src = "sealed type Result<T> = Ok(T) | Err(String)";
        let out = roundtrip(src);
        assert!(out.contains("sealed type Result<T> = Ok(T) | Err(String)"));
    }

    #[test]
    fn fmt_struct() {
        let src = r#"struct User { id: @nonEmpty String }"#;
        let out = roundtrip(src);
        assert!(out.contains("struct User {"));
        assert!(out.contains("id: @nonEmpty String"));
    }

    #[test]
    fn fmt_effect() {
        let src = r#"effect Storage { fn save(key: String) -> Result }"#;
        let out = roundtrip(src);
        assert!(out.contains("effect Storage {"));
        assert!(out.contains("fn save(key: String) -> Result"));
    }

    #[test]
    fn fmt_task_with_steps() {
        let src = r#"
            import skill "GraphQL"
            task T : TaskBrief uses [GraphQL] {
                goal = "g"
                step Fetch { let x = perform GraphQL.query(Q)?; }
            }
        "#;
        let out = roundtrip(src);
        assert!(out.contains(r#"import skill "GraphQL""#));
        assert!(out.contains("task T : TaskBrief uses [GraphQL] {"));
        assert!(out.contains("step Fetch {"));
        assert!(out.contains("let x = perform GraphQL.query(Q)?;"));
    }

    #[test]
    fn fmt_task_effects() {
        let src = r#"
            task T : TaskBrief {
                goal = "g"
                effects [network, cache-read]
            }
        "#;
        let out = roundtrip(src);
        assert!(out.contains("effects [network, cache-read]"));
    }

    #[test]
    fn fmt_step_phase_contracts() {
        let src = r#"
            task T : TaskBrief {
                goal = "g"
                step Charge {
                    pre { amount > 0 }
                    post { receipt.isValid }
                    let receipt = "ok";
                }
            }
        "#;
        let out = roundtrip(src);
        assert!(out.contains("pre { amount > 0 }"));
        assert!(out.contains("post { receipt.isValid }"));
        assert!(out.contains("\n\n        let receipt = \"ok\";"));
    }

    #[test]
    fn fmt_extras_sorted() {
        let src = r#"
            task T : TaskBrief {
                goal   = "g"
                extras = ["zzz": "3", "aaa": "1", "mmm": "2"]
            }
        "#;
        let out = roundtrip(src);
        let aaa = out.find("\"aaa\"").unwrap();
        let mmm = out.find("\"mmm\"").unwrap();
        let zzz = out.find("\"zzz\"").unwrap();
        assert!(aaa < mmm && mmm < zzz, "extras should be sorted: {out}");
    }

    #[test]
    fn fmt_idempotent() {
        let src = r#"
            import skill "GraphQL"
            sealed type S = A | B
            task T : TaskBrief uses [GraphQL] {
                goal = "test"
                step Do { let x = perform GraphQL.query(Q)?; }
            }
        "#;
        let first  = roundtrip(src);
        let second = roundtrip(&first);
        assert_eq!(first, second, "formatter should be idempotent");
    }

    #[test]
    fn fmt_trailing_newline() {
        let src = "task H : TaskBrief { goal = \"hi\" }";
        assert!(roundtrip(src).ends_with('\n'));
    }

    #[test]
    fn fmt_does_not_silently_drop_comments() {
        let src = "// This is a task comment\ntask Hello : TaskBrief { goal = \"hi\" }";
        let result = format_for_write(src);
        assert!(
            matches!(&result, Ok(out) if out.contains("// This is a task comment")) || result.is_err(),
            "fmt silently dropped a comment"
        );
        assert!(matches!(result, Err(lines) if lines == vec![1]));
    }
}
