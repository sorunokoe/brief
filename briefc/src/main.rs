mod ast;
mod checker;
mod ci;
mod codegen;
mod doc;
mod errors;
mod fmt;
mod gen;
mod init;
mod lexer;
mod lsp;
mod manifest;
mod parser;
mod registry;
mod repl;
mod runner;
mod skillgen;
mod tester;
mod typeck;
mod watch;

use std::path::PathBuf;
use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::generate;
use colored::Colorize;

// ─────────────────────────────────────────────────────────────────────────────

#[derive(Parser)]
#[command(name = "brief")]
#[command(version = env!("CARGO_PKG_VERSION"))]
#[command(about = "If it compiles, the AI has everything it needs.")]
#[command(long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Type-check a .brief file — fast, CI-friendly
    Check {
        /// Path to the .brief file
        file: PathBuf,
    },

    /// Compile and execute a .brief file
    Run {
        /// Path to the .brief file
        file: PathBuf,
    },

    /// Compile a .brief file to a native binary (requires --features llvm-backend + LLVM 18)
    Build {
        /// Path to the .brief file
        file: PathBuf,

        /// Output binary path (defaults to ./<stem> or ./<stem>.wasm for wasm targets)
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// Emit LLVM IR to a .ll file instead of a binary
        #[arg(long)]
        emit_ir: bool,

        /// Compilation target triple (e.g. wasm32-unknown-unknown, arm64-apple-macos)
        #[arg(long)]
        target: Option<String>,
    },

    /// Interactive REPL (coming in v0.2)
    Repl,

    /// Start the LSP server (communicates over stdin/stdout)
    Lsp,

    /// Generate a .briefskill interface file from a skill directory
    Skillgen {
        /// Path to the skill directory (must contain README.md)
        skill_path: PathBuf,

        /// Check if the existing .briefskill is up-to-date (CI mode).
        /// Exits 0 if fresh, 1 if stale or missing.
        #[arg(long)]
        check: bool,
    },

    /// Generate a .brief file from a natural language description (LLM in v0.1)
    Gen {
        /// Natural language description of the task
        description: String,

        /// Output file path (defaults to <TaskName>.brief)
        #[arg(short, long)]
        output: Option<PathBuf>,
    },

    /// Run test { } blocks inside a .brief file
    Test {
        /// Path to the .brief file containing test blocks
        file: PathBuf,

        /// Show perform call log for each test
        #[arg(short, long)]
        verbose: bool,
    },

    /// Format a .brief file (canonical style)
    Fmt {
        /// Path to the .brief file to format
        file: PathBuf,

        /// Write formatted output back to the file in-place
        #[arg(short, long)]
        write: bool,

        /// Check formatting without changing anything (CI mode — exits 1 if unformatted)
        #[arg(long)]
        check: bool,
    },

    /// Add a skill from the registry or a local path
    Add {
        #[command(subcommand)]
        resource: AddResource,
    },

    /// Generate Markdown documentation from a .brief file
    Doc {
        /// Path to the .brief file
        file: PathBuf,

        /// Write output to a file instead of stdout
        #[arg(short, long)]
        output: Option<PathBuf>,
    },

    /// Watch a .brief file (or directory) for changes and re-check on every save
    Watch {
        /// Path to the .brief file or directory to watch
        path: PathBuf,
    },

    /// Scaffold a new Brief project in a new directory
    Init {
        /// Project name (also used as the new directory name)
        name: String,
    },

    /// Generate shell completion scripts (bash, zsh, fish, powershell)
    Completions {
        /// Shell to generate completions for
        #[arg(value_enum)]
        shell: clap_complete::Shell,
    },

    /// Run all checks listed in brief.toml [ci] examples
    Ci,
}

#[derive(Subcommand)]
enum AddResource {
    /// Install a skill from the Brief registry or a local directory
    Skill {
        /// Skill name (e.g. GraphQL) or local path (e.g. ./my-skills/GraphQL)
        name_or_path: String,

        /// List all available skills in the registry instead of installing
        #[arg(long)]
        list: bool,
    },
}

// ─────────────────────────────────────────────────────────────────────────────

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Check { file } => {
            print_brief_banner();
            let ok = runner::run_file(&file, runner::RunMode::Check);
            std::process::exit(if ok { 0 } else { 1 });
        }

        Commands::Run { file } => {
            print_brief_banner();
            let ok = runner::run_file(&file, runner::RunMode::Run);
            std::process::exit(if ok { 0 } else { 1 });
        }

        Commands::Build { file, output, emit_ir, target } => {
            print_brief_banner();
            let ok = codegen::build_file(&file, output.as_deref(), emit_ir, target.as_deref());
            std::process::exit(if ok { 0 } else { 1 });
        }

        Commands::Repl => {
            print_brief_banner();
            repl::run_repl();
        }

        Commands::Lsp => {
            // No banner — LSP communicates over JSON-RPC on stdio.
            tokio::runtime::Runtime::new()
                .expect("tokio runtime")
                .block_on(lsp::run_lsp_server());
        }

        Commands::Skillgen { skill_path, check } => {
            print_brief_banner();
            if check {
                skillgen::skillgen_check(&skill_path);
            } else {
                skillgen::skillgen(&skill_path);
            }
        }

        Commands::Gen { description, output } => {
            print_brief_banner();
            gen::gen(&description, output.as_deref());
        }

        Commands::Test { file, verbose } => {
            print_brief_banner();
            let ok = tester::test_file(&file, verbose);
            std::process::exit(if ok { 0 } else { 1 });
        }

        Commands::Fmt { file, write, check } => {
            print_brief_banner();
            let ok = fmt::fmt_file(&file, write, check);
            std::process::exit(if ok { 0 } else { 1 });
        }

        Commands::Add { resource } => {
            print_brief_banner();
            match resource {
                AddResource::Skill { name_or_path, list } => {
                    if list {
                        let ok = registry::list_registry_skills();
                        std::process::exit(if ok { 0 } else { 1 });
                    } else {
                        let root = registry::find_install_root();
                        let ok   = registry::add_skill(&name_or_path, &root);
                        std::process::exit(if ok { 0 } else { 1 });
                    }
                }
            }
        }

        Commands::Doc { file, output } => {
            print_brief_banner();
            let ok = doc::doc_file(&file, output.as_deref());
            std::process::exit(if ok { 0 } else { 1 });
        }

        Commands::Watch { path } => {
            print_brief_banner();
            let ok = watch::watch(&path);
            std::process::exit(if ok { 0 } else { 1 });
        }

        Commands::Init { name } => {
            print_brief_banner();
            let ok = init::init(&name);
            std::process::exit(if ok { 0 } else { 1 });
        }
        Commands::Completions { shell } => {
            generate(shell, &mut Cli::command(), "brief", &mut std::io::stdout());
        }
        Commands::Ci => {
            print_brief_banner();
            let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
            let ok  = ci::run_ci(&cwd);
            std::process::exit(if ok { 0 } else { 1 });
        }
    }
}

fn print_brief_banner() {
    println!("{}", format!("brief v{}", env!("CARGO_PKG_VERSION")).dimmed());
}
