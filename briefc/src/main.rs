mod ast;
mod checker;
mod codegen;
mod errors;
mod fmt;
mod gen;
mod lexer;
mod lsp;
mod parser;
mod repl;
mod runner;
mod skillgen;
mod tester;
mod typeck;

use std::path::PathBuf;
use clap::{Parser, Subcommand};
use colored::Colorize;

// ─────────────────────────────────────────────────────────────────────────────

#[derive(Parser)]
#[command(name = "brief")]
#[command(version = "0.1.0")]
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

        /// Output binary path (defaults to ./<stem>)
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// Emit LLVM IR to a .ll file instead of a binary
        #[arg(long)]
        emit_ir: bool,
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

        Commands::Build { file, output, emit_ir } => {
            print_brief_banner();
            let ok = codegen::build_file(&file, output.as_deref(), emit_ir);
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
    }
}

fn print_brief_banner() {
    println!("{}", format!("brief v{}", env!("CARGO_PKG_VERSION")).dimmed());
}
