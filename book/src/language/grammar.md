# Full Grammar (EBNF)

```ebnf
program       ::= import_decl* type_decl* sealed_decl* struct_decl*
                  protocol_decl* effect_decl* task_decl* test_decl*

import_decl   ::= "import" "skill" STRING_LIT

type_decl     ::= "type" IDENT "=" type_rhs
type_rhs      ::= attr* type_name
                | "[" IDENT ("," IDENT)* "]"

sealed_decl   ::= "sealed" IDENT "{" variant+ "}"
variant       ::= IDENT ("{" field_list "}")?

struct_decl   ::= "struct" IDENT "{" field_list "}"
field_list    ::= field+
field         ::= attr* IDENT ":" type_name

protocol_decl ::= "protocol" IDENT "{" proto_fn+ "}"
proto_fn      ::= "fn" IDENT "(" param_list? ")" "->" type_name

effect_decl   ::= "effect" IDENT "{" effect_fn+ "}"
effect_fn     ::= "fn" IDENT "(" param_list? ")" "->" (attr* type_name)

param_list    ::= param ("," param)*
param         ::= IDENT ":" type_name

task_decl     ::= decorator* "task" IDENT ":" IDENT
                  ("uses" "[" IDENT ("," IDENT)* "]")?
                  "{" task_body "}"
task_body     ::= goal_field step_decl*
goal_field    ::= "goal" "=" STRING_LIT

step_decl     ::= "step" IDENT "{" stmt* "}"
stmt          ::= let_stmt | perform_stmt
let_stmt      ::= ("@once")? "let" IDENT "=" perform_expr ";"?
perform_stmt  ::= "perform" perform_expr ";"?
perform_expr  ::= IDENT "." IDENT "(" arg_list? ")" "?"?

test_decl     ::= "test" STRING_LIT "{" <opaque body> "}"

attr          ::= "@" IDENT ("(" STRING_LIT ")")?
decorator     ::= "@" IDENT (IDENT)?

IDENT         ::= [a-zA-Z_][a-zA-Z0-9_]*
STRING_LIT    ::= '"' [^"]* '"'
```

> **Note:** `test { }` bodies are parsed opaquely by the main parser. The `tester.rs` module has its own line-based parser for `mock`/`run`/`assert` syntax.
