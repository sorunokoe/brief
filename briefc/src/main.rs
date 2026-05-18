mod ast;
mod checker;
mod ci;
mod codegen;
mod doc;
mod errors;
mod fmt;
mod gen;
mod hir;
mod init;
mod lexer;
mod lock;
mod lsp;
mod manifest;
mod mcp_schema;
mod parser;
mod registry;
mod repl;
mod runner;
mod serve;
mod skillgen;
mod tester;
mod typeck;
mod verify;
mod verifier;
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

        /// Suppress E107 (missing .briefskill) — useful when skills have not been generated yet
        #[arg(long)]
        allow_missing_skills: bool,
    },

    /// Verify dynamic annotations and write .brief.lock
    Verify {
        /// Path to the .brief file to verify
        file: PathBuf,
    },

    /// Start a verified MCP server for a .brief file (requires valid .brief.lock)
    Serve {
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

        /// Overwrite the output file if it already exists
        #[arg(long)]
        force: bool,
    },

    /// Run test { } blocks inside a .brief file
    Test {
        /// Path to the .brief file containing test blocks
        file: PathBuf,

        /// Show perform call log for each test
        #[arg(short, long)]
        verbose: bool,

        /// Make real MCP calls instead of using mocks (validates return types live)
        #[arg(long)]
        live: bool,
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

    /// Print the typed HIR for a .brief file
    Hir {
        /// Path to the .brief file
        file: PathBuf,
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

fn main() -> std::process::ExitCode {
    let cli = Cli::parse();

    match cli.command {
        Commands::Check { file, allow_missing_skills } => {
            print_brief_banner();
            let ok = runner::run_file(&file, runner::RunMode::Check { allow_missing_skills });
            if ok { std::process::ExitCode::SUCCESS } else { std::process::ExitCode::FAILURE }
        }

        Commands::Verify { file } => {
            print_brief_banner();
            let ok = verify::run_verify(&file);
            if ok { std::process::ExitCode::SUCCESS } else { std::process::ExitCode::FAILURE }
        }

        Commands::Serve { file } => {
            // No banner — MCP protocol on stdio.
            let ok = serve::run_serve(&file);
            if ok { std::process::ExitCode::SUCCESS } else { std::process::ExitCode::FAILURE }
        }

        Commands::Run { file } => {
            print_brief_banner();
            let ok = runner::run_file(&file, runner::RunMode::Run);
            if ok { std::process::ExitCode::SUCCESS } else { std::process::ExitCode::FAILURE }
        }

        Commands::Build { file, output, emit_ir, target } => {
            print_brief_banner();
            let ok = codegen::build_file(&file, output.as_deref(), emit_ir, target.as_deref());
            if ok { std::process::ExitCode::SUCCESS } else { std::process::ExitCode::FAILURE }
        }

        Commands::Repl => {
            print_brief_banner();
            repl::run_repl();
            std::process::ExitCode::SUCCESS
        }

        Commands::Lsp => {
            // No banner — LSP communicates over JSON-RPC on stdio.
            tokio::runtime::Runtime::new()
                .expect("tokio runtime")
                .block_on(lsp::run_lsp_server());
            std::process::ExitCode::SUCCESS
        }

        Commands::Skillgen { skill_path, check } => {
            print_brief_banner();
            let ok = if check {
                skillgen::skillgen_check(&skill_path)
            } else {
                skillgen::skillgen(&skill_path)
            };
            if ok { std::process::ExitCode::SUCCESS } else { std::process::ExitCode::FAILURE }
        }

        Commands::Gen { description, output, force } => {
            print_brief_banner();
            let ok = gen::gen(&description, output.as_deref(), force);
            if ok { std::process::ExitCode::SUCCESS } else { std::process::ExitCode::FAILURE }
        }

        Commands::Test { file, verbose, live } => {
            print_brief_banner();
            let ok = tester::test_file(&file, verbose, live);
            if ok { std::process::ExitCode::SUCCESS } else { std::process::ExitCode::FAILURE }
        }

        Commands::Fmt { file, write, check } => {
            print_brief_banner();
            let ok = fmt::fmt_file(&file, write, check);
            if ok { std::process::ExitCode::SUCCESS } else { std::process::ExitCode::FAILURE }
        }

        Commands::Add { resource } => {
            print_brief_banner();
            match resource {
                AddResource::Skill { name_or_path, list } => {
                    let ok = if list {
                        registry::list_registry_skills()
                    } else {
                        let root = registry::find_install_root();
                        registry::add_skill(&name_or_path, &root)
                    };
                    if ok { std::process::ExitCode::SUCCESS } else { std::process::ExitCode::FAILURE }
                }
            }
        }

        Commands::Doc { file, output } => {
            print_brief_banner();
            let ok = doc::doc_file(&file, output.as_deref());
            if ok { std::process::ExitCode::SUCCESS } else { std::process::ExitCode::FAILURE }
        }

        Commands::Hir { file } => {
            print_brief_banner();
            let ok = hir::print_hir_file(&file);
            if ok { std::process::ExitCode::SUCCESS } else { std::process::ExitCode::FAILURE }
        }

        Commands::Watch { path } => {
            print_brief_banner();
            let ok = watch::watch(&path);
            if ok { std::process::ExitCode::SUCCESS } else { std::process::ExitCode::FAILURE }
        }

        Commands::Init { name } => {
            print_brief_banner();
            let ok = init::init(&name);
            if ok { std::process::ExitCode::SUCCESS } else { std::process::ExitCode::FAILURE }
        }

        Commands::Completions { shell } => {
            generate(shell, &mut Cli::command(), "brief", &mut std::io::stdout());
            std::process::ExitCode::SUCCESS
        }

        Commands::Ci => {
            print_brief_banner();
            let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
            let ok  = ci::run_ci(&cwd);
            if ok { std::process::ExitCode::SUCCESS } else { std::process::ExitCode::FAILURE }
        }
    }
}

fn print_brief_banner() {
    eprintln!("{}", format!("brief v{}", env!("CARGO_PKG_VERSION")).dimmed());
}
