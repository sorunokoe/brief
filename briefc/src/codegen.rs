/// `brief build` — LLVM IR emission backend for Brief.
///
/// Compiles `.brief` files to native code via LLVM IR using the `inkwell` crate.
///
/// ## Setup (required before enabling `llvm-backend` feature)
///
///   macOS:   brew install llvm@18
///   Ubuntu:  apt-get install llvm-18 llvm-18-dev
///
/// Then add inkwell to briefc/Cargo.toml [dependencies]:
///   inkwell = { git = "https://github.com/TheDan64/inkwell", branch = "master",
///               features = ["llvm18-0"], optional = true }
///   # and update the feature:
///   llvm-backend = ["inkwell"]
///
/// Set the LLVM path (macOS):
///   export LLVM_SYS_180_PREFIX=$(brew --prefix llvm@18)
///
/// Build with the feature:
///   cargo build --features llvm-backend
///
/// ## Usage
///
///   brief build hello.brief            # compile → ./hello (native binary)
///   brief build hello.brief -o myapp   # custom output path
///   brief build hello.brief --emit-ir  # emit LLVM IR to hello.ll
///
/// ## Architecture (for when LLVM is available)
///
/// Pipeline: .brief → AST → type-checked Program → LLVM IR Module → object → binary
///
/// Each `task` becomes an LLVM function.
/// Each `step` becomes a labelled basic block.
/// `perform Skill.fn(args)` emits a call to `brief_rt_perform(skill, fn, argc, ...)`.
/// Brief runtime stubs: `brief_rt_print`, `brief_rt_perform`, `brief_rt_exit`.

// ─────────────────────────────────────────────────────────────────────────────
// When LLVM is installed + inkwell added to Cargo.toml, replace this block
// with the full backend implementation (see git history / docs/llvm-setup.md).
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(feature = "llvm-backend")]
compile_error!(
    "The `llvm-backend` feature requires `inkwell` in Cargo.toml and LLVM 18 installed.\n\
     See briefc/src/codegen.rs or docs/llvm-setup.md for setup instructions."
);

// ─────────────────────────────────────────────────────────────────────────────
// Public stub — always compiled. Shows a helpful setup message.
// ─────────────────────────────────────────────────────────────────────────────

/// Build a `.brief` file to a native binary.
///
/// Requires the `llvm-backend` feature + LLVM 18 + inkwell added to Cargo.toml.
/// See `docs/llvm-setup.md` for the full setup guide.
pub fn build_file(
    source_path: &std::path::Path,
    output:      Option<&std::path::Path>,
    emit_ir:     bool,
) -> bool {
    use colored::Colorize;
    let _ = (source_path, output, emit_ir);
    eprintln!("{} `brief build` requires the LLVM backend.", "error".red().bold());
    eprintln!();
    eprintln!("{}", "Setup steps:".yellow().bold());
    eprintln!(
        "  1. Install LLVM 18:  {}",
        "brew install llvm@18".cyan()
    );
    eprintln!(
        "  2. Add inkwell to Cargo.toml and set LLVM_SYS_180_PREFIX"
    );
    eprintln!(
        "  3. Rebuild:          {}",
        "cargo build --features llvm-backend".cyan()
    );
    eprintln!();
    eprintln!("See {} for details.", "docs/llvm-setup.md".underline());
    false
}
