/// `brief skillgen` — generates a `.briefskill` typed interface from a skill directory.
///
/// Reads `<skill-dir>/README.md` and emits `<skill-dir>/<SkillName>.briefskill`.
/// Supports two paths:
///   - Offline: structured markdown parsing (looks for "## Interface", "## Functions", etc.)
///   - Online:  LLM-enhanced (requires BRIEF_LLM_API_KEY env var — coming in v0.1)

use std::path::Path;
use colored::Colorize;

pub fn skillgen(skill_path: &Path) {
    let skill_name = skill_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("UnknownSkill");

    let readme_path = skill_path.join("README.md");
    if !readme_path.exists() {
        eprintln!(
            "{}: no README.md found in {}",
            "error".red().bold(),
            skill_path.display()
        );
        eprintln!("  {} Create a README.md that describes the skill's capabilities.", "hint:".cyan().bold());
        std::process::exit(1);
    }

    let readme = match std::fs::read_to_string(&readme_path) {
        Ok(s)  => s,
        Err(e) => {
            eprintln!("{}: cannot read {}: {}", "error".red().bold(), readme_path.display(), e);
            std::process::exit(1);
        }
    };

    println!("{} Generating interface for skill '{}'...", "●".blue().bold(), skill_name.bold());
    println!("  {} reading {}", "→".dimmed(), readme_path.display());

    // Compute a simple checksum of the README for staleness detection.
    let checksum = simple_checksum(readme.as_bytes());

    // Extract interface (offline structured path).
    let functions = extract_functions_from_markdown(&readme, skill_name);

    // Emit .briefskill file.
    let output_path = skill_path.join(format!("{skill_name}.briefskill"));
    let content = render_briefskill(skill_name, &functions, checksum, &readme_path);

    match std::fs::write(&output_path, &content) {
        Ok(_)  => {
            println!("  {} {}", "✅".green(), output_path.display().to_string().green());
            println!();
            println!("{}", "Generated interface:".bold());
            println!("{}", content.dimmed());
        }
        Err(e) => {
            eprintln!("{}: cannot write {}: {}", "error".red().bold(), output_path.display(), e);
            std::process::exit(1);
        }
    }

    if functions.is_empty() {
        println!();
        println!("{} No typed functions extracted — the interface is empty.", "⚠".yellow().bold());
        println!("  {} Add an `## Interface` section to your README.md with function signatures.", "hint:".cyan().bold());
        println!("  {} For richer interfaces, set BRIEF_LLM_API_KEY to enable LLM extraction (coming in v0.1).", "hint:".cyan().bold());
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Structured markdown extraction (offline path)
// ─────────────────────────────────────────────────────────────────────────────

/// A function signature extracted from the skill README.
#[derive(Debug)]
struct FnSig {
    name:        String,
    params:      Vec<(String, String)>,  // (param_name, type)
    return_type: String,
    #[allow(dead_code)]
    doc:         Option<String>,
}

/// Look for sections like:
///
/// ```markdown
/// ## Interface
/// - `fn query(op: Operation) -> Result<T, QueryError>`
/// - `fn components(url: FigmaURL) -> [Component]`
/// ```
///
/// Also parses simple `### fn_name` sub-sections.
fn extract_functions_from_markdown(markdown: &str, _skill_name: &str) -> Vec<FnSig> {
    let mut fns = Vec::new();
    let mut in_interface = false;

    for line in markdown.lines() {
        let trimmed = line.trim();

        // Detect interface section headers.
        if trimmed.starts_with("## Interface")
            || trimmed.starts_with("## Functions")
            || trimmed.starts_with("## API")
            || trimmed.starts_with("## Capabilities")
        {
            in_interface = true;
            continue;
        }

        // Leave the interface section on a new `##` header.
        if trimmed.starts_with("## ") && in_interface {
            in_interface = false;
        }

        if !in_interface { continue; }

        // Match lines like:  - `fn name(param: Type) -> ReturnType`
        // or code-fence lines: fn name(param: Type) -> ReturnType
        let sig_line = if trimmed.starts_with("- `fn ") || trimmed.starts_with("* `fn ") {
            // strip leading `- ` and backticks
            trimmed.trim_start_matches("- ").trim_start_matches("* ")
                   .trim_matches('`')
        } else if trimmed.starts_with("fn ") {
            trimmed
        } else {
            continue;
        };

        if let Some(sig) = parse_fn_sig(sig_line) {
            fns.push(sig);
        }
    }

    fns
}

/// Parse `fn name(p1: T1, p2: T2) -> ReturnType`
fn parse_fn_sig(sig: &str) -> Option<FnSig> {
    // Strip `fn `
    let rest = sig.strip_prefix("fn ")?;

    // Split at `(`
    let (name, rest) = rest.split_once('(')?;
    let name = name.trim().to_string();

    // Split at `)` to get params and return
    let (params_str, rest) = rest.split_once(')')?;
    let return_type = rest.trim()
        .strip_prefix("->").unwrap_or("Void")
        .trim()
        .to_string();

    let mut params = Vec::new();
    for param in params_str.split(',') {
        let param = param.trim();
        if param.is_empty() { continue; }
        if let Some((pname, ptype)) = param.split_once(':') {
            params.push((pname.trim().to_string(), ptype.trim().to_string()));
        } else {
            params.push((param.to_string(), "Any".to_string()));
        }
    }

    Some(FnSig { name, params, return_type, doc: None })
}

// ─────────────────────────────────────────────────────────────────────────────
// .briefskill rendering
// ─────────────────────────────────────────────────────────────────────────────

fn render_briefskill(skill_name: &str, fns: &[FnSig], checksum: u32, source_path: &Path) -> String {
    let mut out = String::new();

    out.push_str(&format!(
        "// Auto-generated by `brief skillgen v0.0.1`\n\
         // Source: {} (crc32: {:08x})\n\
         // Regenerate: brief skillgen {}\n\
         // Do not edit manually.\n\n",
        source_path.display(),
        checksum,
        source_path.parent().map(|p| p.display().to_string()).unwrap_or_default()
    ));

    out.push_str(&format!("interface {skill_name} {{\n"));

    if fns.is_empty() {
        out.push_str("    // No typed functions extracted.\n");
        out.push_str("    // Add an `## Interface` section to README.md with function signatures,\n");
        out.push_str("    // or set BRIEF_LLM_API_KEY for AI-assisted extraction (coming in v0.1).\n");
    } else {
        for f in fns {
            let params = f.params.iter()
                .map(|(n, t)| format!("{n}: {t}"))
                .collect::<Vec<_>>()
                .join(", ");
            out.push_str(&format!("    fn {}({}) -> {}\n", f.name, params, f.return_type));
        }
    }

    out.push_str("}\n");
    out
}

// ─────────────────────────────────────────────────────────────────────────────

/// A deterministic CRC32-like checksum (simple, dependency-free).
fn simple_checksum(data: &[u8]) -> u32 {
    let mut crc: u32 = 0xFFFF_FFFF;
    for &byte in data {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            if crc & 1 != 0 { crc = (crc >> 1) ^ 0xEDB8_8320; } else { crc >>= 1; }
        }
    }
    !crc
}
