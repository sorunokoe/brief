/// brief init <name> — scaffold a new Brief project
///
/// Creates a new directory with:
///   <name>/
///   ├── brief.toml           ← project manifest
///   ├── hello.brief          ← starter task
///   ├── .claude/skills/      ← skill directory (empty, with .gitkeep)
///   ├── docs/                ← documentation
///   └── README.md            ← project README

use std::path::Path;
use colored::Colorize;

/// Scaffold a new Brief project at `<cwd>/<name>/`.
pub fn init(name: &str) -> bool {
    let root = Path::new(name);

    if root.exists() {
        eprintln!(
            "{} directory '{}' already exists",
            "error:".red().bold(), name
        );
        return false;
    }

    println!("{}", format!("brief init — scaffolding '{name}'").dimmed());

    // Create directory structure
    let dirs = [
        format!("{name}/.claude/skills"),
        format!("{name}/docs"),
    ];

    for dir in &dirs {
        if let Err(e) = std::fs::create_dir_all(dir) {
            eprintln!("{} could not create {dir}: {e}", "error:".red().bold());
            return false;
        }
    }

    // Write each file
    let files: &[(&str, &str)] = &[
        (
            &format!("{name}/brief.toml"),
            &brief_toml(name),
        ),
        (
            &format!("{name}/hello.brief"),
            &hello_brief(name),
        ),
        (
            &format!("{name}/.claude/skills/.gitkeep"),
            "",
        ),
        (
            &format!("{name}/docs/getting-started.md"),
            &getting_started_md(name),
        ),
        (
            &format!("{name}/README.md"),
            &readme_md(name),
        ),
    ];

    for (path, content) in files {
        if let Err(e) = std::fs::write(path, content) {
            eprintln!("{} could not write {path}: {e}", "error:".red().bold());
            return false;
        }
    }

    // Print success tree
    println!("{} {}", "✅  Created:".green().bold(), name);
    println!("   {name}/");
    println!("   ├── brief.toml");
    println!("   ├── hello.brief");
    println!("   ├── docs/getting-started.md");
    println!("   ├── README.md");
    println!("   └── .claude/skills/");
    println!();
    println!("{}", "Next steps:".bold());
    println!("   cd {name}");
    println!("   brief check hello.brief");
    println!("   brief run   hello.brief");
    println!("   brief watch hello.brief   # live re-check on save");
    println!();

    true
}

// ── File templates ────────────────────────────────────────────────────────────

fn brief_toml(name: &str) -> String {
    format!(r#"# brief.toml — project manifest
# https://github.com/yourusername/brief

[project]
name    = "{name}"
version = "0.1.0"
authors = []

# Skills to resolve (name → path relative to this file)
[skills]
# GraphQL = ".claude/skills/GraphQL"

# Examples to check in CI
[ci]
examples = ["hello.brief"]
"#)
}

fn hello_brief(name: &str) -> String {
    format!(r#"// hello.brief — starter task for the '{name}' project
//
// Run:  brief check hello.brief   → type-checks this file
// Run:  brief run   hello.brief   → executes the task
// Run:  brief watch hello.brief   → re-checks on every save

task Hello : TaskBrief {{
    goal = "Say hello from the {name} project"
}}
"#)
}

fn getting_started_md(name: &str) -> String {
    format!(r#"# Getting Started with {name}

## Install Brief

```bash
curl -sSf https://install.brieftool.io | sh
# Or: cargo install briefc
```

## Run the starter task

```bash
brief check hello.brief   # type-check
brief run   hello.brief   # execute
brief watch hello.brief   # live re-check on save
```

## Add a skill

```bash
mkdir -p .claude/skills/GraphQL
# Create .claude/skills/GraphQL/README.md with an ## Interface section
brief skillgen .claude/skills/GraphQL/
```

Then import it in your `.brief` files:

```brief
import skill "GraphQL"

@BriefBuilder
task FetchData : TaskBrief uses [GraphQL] {{
    goal = "Fetch data via GraphQL"

    step Fetch {{
        let result = perform GraphQL.query(MyQuery)?;
    }}
}}
```

## Next steps

- See the [Brief language docs](https://github.com/yourusername/brief/tree/main/docs)
- Browse [32 examples](https://github.com/yourusername/brief/tree/main/examples)
"#)
}

fn readme_md(name: &str) -> String {
    format!(r#"# {name}

A Brief project.

> *"If it compiles, the AI has everything it needs."*

## Quick start

```bash
brief check hello.brief
brief run   hello.brief
```

## Structure

```
.
├── brief.toml          ← project manifest
├── hello.brief         ← starter task
├── docs/               ← documentation
└── .claude/skills/     ← skill interfaces (.briefskill files)
```

## Learn more

- [Brief language](https://github.com/yourusername/brief)
- [Getting started](docs/getting-started.md)
"#)
}
