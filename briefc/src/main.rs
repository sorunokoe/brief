mod ast;
mod audit;
mod checker;
mod models;
mod ci;
mod codegen;
mod doc;
mod enforcer;
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
mod policy_check;
mod policy_suggest;
mod registry;
mod repl;
mod runner;
mod serve;
mod skill_backends;
mod skill_loader;
mod skillgen;
mod skillsync;
mod selfhosting;
mod suggest;
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

        /// Output machine-readable JSON report to stdout instead of human-readable diagnostics
        #[arg(long)]
        report: bool,
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

        /// Start without requiring a verified .brief.lock (exploratory mode).
        /// Still enforces uses[], forbids{}, and needs{} env vars.
        /// Dynamic annotations (@url, @local-path, etc.) are NOT verified in draft mode.
        #[arg(long)]
        draft: bool,

        /// Record all tool calls (allowed and blocked) to a JSONL trace file
        #[arg(long)]
        record: Option<PathBuf>,

        /// Trace privacy level: 'schema' (arg names+types, no values) or 'full' (all values)
        #[arg(long, default_value = "schema")]
        record_args: String,
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

    /// Validate and inspect the Brief-defined compiler pipeline
    SelfHosting {
        #[command(subcommand)]
        command: Option<SelfHostingCommand>,
    },

    /// Run all checks listed in brief.toml [ci] examples
    Ci,

    /// Test allow/deny policy against a simulated tool call (no MCP server needed)
    PolicyCheck {
        /// Path to the .brief file (defaults to auto-detecting in current directory)
        #[arg(long)]
        file: Option<PathBuf>,

        /// Task name to check policy for
        #[arg(long)]
        task: String,

        /// Tool call in Skill.function format (e.g. FileSystem.write_file)
        #[arg(long)]
        tool: String,

        /// Tool arguments as a JSON object (e.g. '{"path":"./src/main.rs"}')
        #[arg(long, default_value = "{}")]
        args: String,
    },

    /// Summarize a JSONL trace file from `brief serve --record`
    Audit {
        /// Path to the JSONL trace file
        trace: PathBuf,

        /// Exit with code 1 if any blocked (DENY) calls are found
        #[arg(long)]
        fail_on_deny: bool,
    },

    /// Auto-generate .briefskill interface files from live MCP servers
    Skillsync {
        /// Skip confirmation when overwriting existing .briefskill files
        #[arg(long, short = 'y')]
        yes: bool,
    },

    /// Read a trace file and suggest spec improvements
    Suggest {
        /// Path to the JSONL trace file (from `brief serve --record`)
        trace: PathBuf,

        /// Path to the .brief file (defaults to auto-detecting in current directory)
        #[arg(long)]
        file: Option<PathBuf>,

        /// Auto-apply safe additions (missing uses[], needs{}) — never applies allow{} changes
        #[arg(long)]
        apply: bool,
    },

    /// Manage local AI models for policy generation
    Models {
        #[command(subcommand)]
        command: ModelsCommand,
    },

    /// Generate an allow{}/deny{} spec from a task goal using heuristics (+ AI if model installed)
    PolicySuggest {
        /// Task name to generate policy for
        #[arg(long)]
        task: String,

        /// Path to the .brief file (defaults to auto-detecting in current directory)
        #[arg(long)]
        file: Option<PathBuf>,

        /// Auto-apply the suggested allow{}/deny{} blocks to the .brief file
        #[arg(long)]
        apply: bool,
    },
}

#[derive(Subcommand)]
enum ModelsCommand {
    /// Download a model to ~/.brief/models/
    Install {
        /// Model name to install (default: smollm2)
        model: Option<String>,
    },
    /// List installed models
    List,
}

#[derive(Subcommand)]
enum SelfHostingCommand {
    /// Type-check all compiler pass Brief files
    Check,

    /// Show the Stage 1 self-hosting execution plan for a source file
    Run {
        /// Path to the source .brief file
        file: PathBuf,
    },

    /// Compare Rust-native vs Brief-mediated pipeline behavior for a file
    Compare {
        /// Path to the source .brief file
        file: PathBuf,
    },
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
        Commands::Check { file, allow_missing_skills, report } => {
            if !report { print_brief_banner(); }
            let ok = runner::run_file(&file, runner::RunMode::Check { allow_missing_skills, report });
            if ok { std::process::ExitCode::SUCCESS } else { std::process::ExitCode::FAILURE }
        }

        Commands::Verify { file } => {
            print_brief_banner();
            let ok = verify::run_verify(&file);
            if ok { std::process::ExitCode::SUCCESS } else { std::process::ExitCode::FAILURE }
        }

        Commands::Serve { file, draft, record, record_args } => {
            // No banner — MCP protocol on stdio.
            let record_mode = serve::RecordMode::from_str(&record_args);
            let ok = serve::run_serve(&file, draft, record.as_deref(), record_mode);
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

        Commands::SelfHosting { command } => {
            print_brief_banner();
            let ok = match command.unwrap_or(SelfHostingCommand::Check) {
                SelfHostingCommand::Check => selfhosting::cmd_check(),
                SelfHostingCommand::Run { file } => selfhosting::cmd_run(&file),
                SelfHostingCommand::Compare { file } => selfhosting::cmd_compare(&file),
            };
            if ok { std::process::ExitCode::SUCCESS } else { std::process::ExitCode::FAILURE }
        }

        Commands::Ci => {
            print_brief_banner();
            let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
            let ok  = ci::run_ci(&cwd);
            if ok { std::process::ExitCode::SUCCESS } else { std::process::ExitCode::FAILURE }
        }

        Commands::PolicyCheck { file, task, tool, args } => {
            print_brief_banner();
            let ok = policy_check::run_policy_check(file.as_deref(), &task, &tool, &args);
            if ok { std::process::ExitCode::SUCCESS } else { std::process::ExitCode::FAILURE }
        }

        Commands::Audit { trace, fail_on_deny } => {
            print_brief_banner();
            let ok = audit::run_audit(&trace, fail_on_deny);
            if ok { std::process::ExitCode::SUCCESS } else { std::process::ExitCode::FAILURE }
        }

        Commands::Skillsync { yes } => {
            print_brief_banner();
            let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
            let ok = skillsync::run_skillsync(&cwd, yes);
            if ok { std::process::ExitCode::SUCCESS } else { std::process::ExitCode::FAILURE }
        }

        Commands::Suggest { trace, file, apply } => {
            print_brief_banner();
            let ok = suggest::run_suggest(&trace, file.as_deref(), apply);
            if ok { std::process::ExitCode::SUCCESS } else { std::process::ExitCode::FAILURE }
        }

        Commands::Models { command } => {
            print_brief_banner();
            let ok = match command {
                ModelsCommand::Install { model } => models::run_models_install(model.as_deref()),
                ModelsCommand::List => models::run_models_list(),
            };
            if ok { std::process::ExitCode::SUCCESS } else { std::process::ExitCode::FAILURE }
        }

        Commands::PolicySuggest { task, file, apply } => {
            print_brief_banner();
            let ok = policy_suggest::run_policy_suggest(&task, file.as_deref(), apply);
            if ok { std::process::ExitCode::SUCCESS } else { std::process::ExitCode::FAILURE }
        }
    }
}

fn print_brief_banner() {
    eprintln!("{}", format!("brief v{}", env!("CARGO_PKG_VERSION")).dimmed());
}
