mod ast;
mod checker;
mod errors;
mod gen;
mod lexer;
mod lsp;
mod parser;
mod repl;
mod runner;
mod skillgen;
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
    }
}

fn print_brief_banner() {
    println!("{}", format!("brief v{}", env!("CARGO_PKG_VERSION")).dimmed());
}
