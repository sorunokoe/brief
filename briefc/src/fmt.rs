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
/// - Comments are NOT preserved (parse → regenerate)

use std::path::Path;
use colored::Colorize;

use crate::ast::*;
use crate::lexer::lex;
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
                    Some(r) => format!("{r}.{func}({arg_str})"),
                    None    => format!("{func}({arg_str})"),
                }
            }
            Expr::Ident { name, .. } => name.clone(),
            Expr::Str   { value, .. } => format!("{value:?}"),
        }
    }

    // ── Statements ────────────────────────────────────────────────────────

    fn fmt_stmt(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::Let { name, value, attrs, .. } => {
                let prefix = if attrs.is_empty() {
                    String::new()
                } else {
                    format!("{} ", attrs.iter().map(|a| format!("@{a}")).collect::<Vec<_>>().join(" "))
                };
                self.line(&format!("{prefix}let {name} = {};", Self::fmt_expr(value)));
            }
            Stmt::Expr { value, .. } => {
                self.line(&format!("{};", Self::fmt_expr(value)));
            }
        }
    }

    // ── Steps ─────────────────────────────────────────────────────────────

    fn fmt_step(&mut self, step: &Step) {
        self.line(&format!("step {} {{", step.name));
        self.indented(|f| {
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
        self.line(&format!("type {} = {attrs} {base}", a.name));
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

            // Sort extras by key
            let mut extras = task.extras.clone();
            extras.sort_by(|a, b| a.0.cmp(&b.0));
            if !extras.is_empty() {
                if extras.len() == 1 {
                    f.line(&format!("extras = [{}]", extras.iter()
                        .map(|(k,v)| format!("{k:?}: {v:?}"))
                        .collect::<Vec<_>>().join(", ")));
                } else {
                    f.line("extras = [");
                    f.indented(|g| {
                        for (i, (k, v)) in extras.iter().enumerate() {
                            let comma = if i + 1 < extras.len() { "," } else { "" };
                            g.line(&format!("{k:?}: {v:?}{comma}"));
                        }
                    });
                    f.line("]");
                }
            }

            for (i, step) in task.steps.iter().enumerate() {
                if i > 0 || task.goal.is_some() || !task.extras.is_empty() {
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

    let (tokens, _)  = lex(&source);
    let (program, _) = parse(&tokens, &source);

    let formatted = Formatter::format_program(&program);

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
}
