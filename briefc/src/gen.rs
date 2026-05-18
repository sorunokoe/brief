/// `brief gen` — generates a `.brief` file from a natural language description.
///
/// Without BRIEF_LLM_API_KEY: produces a structured template (v0.0.1 behaviour).
/// With    BRIEF_LLM_API_KEY: calls the configured LLM, then runs `brief check`
///   on the output and feeds E-codes back for up to 3 iterations until clean.

use std::path::{Path, PathBuf};

use colored::Colorize;

pub fn gen(description: &str, output: Option<&Path>, force: bool) -> bool {
    println!("{} Generating brief from description...", "●".blue().bold());
    println!("  {}", description.italic());
    println!();

    let task_name = description_to_task_name(description);
    let output_path = output
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| format!("{task_name}.brief"));

    if !force && std::path::Path::new(&output_path).exists() {
        eprintln!(
            "{}: output file '{}' already exists — use {} to overwrite",
            "error".red().bold(), output_path, "--force".cyan()
        );
        return false;
    }

    let brief = if std::env::var("BRIEF_LLM_API_KEY").is_ok() {
        match gen_with_llm(description, &task_name) {
            Some(s) => s,
            None    => {
                eprintln!("{} LLM generation failed — using template fallback", "⚠".yellow().bold());
                render_brief_template(&task_name, description)
            }
        }
    } else {
        render_brief_template(&task_name, description)
    };

    match std::fs::write(&output_path, &brief) {
        Ok(_) => {
            println!("{} Generated: {}", "✅".green().bold(), output_path.green().bold());
            println!();
            if std::env::var("BRIEF_LLM_API_KEY").is_err() {
                println!("{} LLM-powered generation available — set BRIEF_LLM_API_KEY", "💡".yellow());
                println!("  Providers: {} (default), {} (set BRIEF_LLM_PROVIDER=openai)",
                    "Anthropic".cyan(), "OpenAI".cyan());
                println!();
            }
            println!("{}", "Next steps:".bold());
            println!("  1. Review the generated file");
            println!("  2. Run {} to validate", "`brief check`".cyan());
            println!("  3. Run {} to seal the contract", "`brief verify`".cyan());
            true
        }
        Err(e) => {
            eprintln!("{}: cannot write {}: {}", "error".red().bold(), output_path, e);
            false
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// LLM-powered generation with compiler feedback loop

fn gen_with_llm(description: &str, _task_name: &str) -> Option<String> {
    let provider = std::env::var("BRIEF_LLM_PROVIDER").unwrap_or_else(|_| "anthropic".into());
    // Probe for API key early — no point proceeding without it.
    std::env::var("BRIEF_LLM_API_KEY").ok()?;
    let model = std::env::var("BRIEF_LLM_MODEL").unwrap_or_else(|_| match provider.as_str() {
        "openai" => "gpt-4o-mini".into(),
        _        => "claude-3-5-haiku-20241022".into(),
    });

    const SYSTEM: &str = "\
You generate Brief DSL files for AI task workflows.\n\
\n\
Brief syntax:\n\
  import skill \"SkillName\"        // declare a skill dependency\n\
  task Name : TaskBrief uses [Skill1, Skill2] {\n\
      goal = \"brief description\"\n\
      step StepName {\n\
          let x = perform Skill1.functionName(arg)?;\n\
      }\n\
  }\n\
  test \"description\" {\n\
      // boundary tests\n\
  }\n\
\n\
Rules:\n\
- ALWAYS start with `import skill` for each skill used\n\
- task must have `uses [...]` listing all imported skills\n\
- step bodies use `perform SkillName.fnName(args)?` for skill calls\n\
- @once let handle = perform ... → handle can only be used once\n\
- Return ONLY the .brief file content, no markdown, no explanation.";

    let mut messages: Vec<serde_json::Value> = vec![
        serde_json::json!({ "role": "user", "content": description })
    ];

    let mut last_draft = None;

    for attempt in 0..3u8 {
        if attempt > 0 {
            println!("  {} Attempt {}/3 — iterating on compiler feedback...",
                "↻".yellow().bold(), attempt + 1);
        } else {
            println!("  {} Calling {} for generation...", "✦".cyan().bold(), provider);
        }

        let (body, req) = build_llm_request(&provider, &model, SYSTEM, &messages, 2048);
        let resp = match req.set("content-type", "application/json").send_json(&body) {
            Ok(r)  => r,
            Err(e) => {
                eprintln!("  {} LLM call failed: {e}", "⚠".yellow().bold());
                return last_draft;
            }
        };

        use std::io::Read;
        let mut body_str = String::new();
        let _ = resp.into_reader().take(64 * 1024).read_to_string(&mut body_str);

        let json: serde_json::Value = match serde_json::from_str(&body_str) {
            Ok(v) => v,
            Err(_) => return last_draft,
        };

        let text = json["content"][0]["text"].as_str()
            .or_else(|| json["choices"][0]["message"]["content"].as_str())
            .unwrap_or("")
            .to_string();

        let draft = strip_code_fences(&text);

        // Run brief check on the draft.
        let e_codes = check_draft(&draft);
        if e_codes.is_empty() {
            println!("  {} Draft passes all checks", "✅".green().bold());
            return Some(draft);
        }

        println!("  {} Found {} issue(s):", "⚠".yellow().bold(), e_codes.len());
        for code in &e_codes {
            println!("    {}", code.dimmed());
        }

        last_draft = Some(draft.clone());

        // Feed error codes back to the LLM.
        let assistant_msg = serde_json::json!({ "role": "assistant", "content": text });
        let errors_msg = serde_json::json!({
            "role": "user",
            "content": format!(
                "The generated .brief file has these compiler errors. Fix them:\n\n{}",
                e_codes.join("\n")
            )
        });
        messages.push(assistant_msg);
        messages.push(errors_msg);
    }

    // Return last draft even if it has errors — user can fix manually.
    println!("  {} Could not produce a clean file after 3 attempts — returning best draft",
        "⚠".yellow().bold());
    last_draft
}

fn build_llm_request(
    provider: &str,
    model:    &str,
    system:   &str,
    messages: &[serde_json::Value],
    max_tokens: u32,
) -> (serde_json::Value, ureq::Request) {
    let api_key = std::env::var("BRIEF_LLM_API_KEY").unwrap_or_default();
    let api_url = std::env::var("BRIEF_LLM_URL").unwrap_or_else(|_| match provider {
        "openai" => "https://api.openai.com/v1/chat/completions".into(),
        _        => "https://api.anthropic.com/v1/messages".into(),
    });

    if provider == "openai" {
        let mut all_msgs = vec![serde_json::json!({ "role": "system", "content": system })];
        all_msgs.extend_from_slice(messages);
        let body = serde_json::json!({
            "model": model, "max_tokens": max_tokens, "messages": all_msgs
        });
        let req = ureq::post(&api_url)
            .set("Authorization", &format!("Bearer {api_key}"));
        (body, req)
    } else {
        let body = serde_json::json!({
            "model": model, "max_tokens": max_tokens,
            "system": system,
            "messages": messages
        });
        let req = ureq::post(&api_url)
            .set("x-api-key", &api_key)
            .set("anthropic-version", "2023-06-01");
        (body, req)
    }
}

/// Run `brief check` in-process on the draft string; return error messages.
fn check_draft(draft: &str) -> Vec<String> {
    use crate::checker::{self, CheckContext};
    use crate::lexer::lex;
    use crate::parser::parse;

    let (tokens, lex_errs) = lex(draft);
    if !lex_errs.is_empty() {
        return lex_errs.iter().map(|(s, e)| format!("lex error at {s}..{e}")).collect();
    }

    let (program, parse_errs) = parse(&tokens, draft);
    let mut diags: Vec<String> = parse_errs.iter()
        .filter(|d| d.is_error())
        .map(|d| d.message.clone())
        .collect();

    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let ctx = CheckContext {
        file_dir:             &cwd,
        cwd:                  &cwd,
        manifest:             None,
        brief_path:           None,
        allow_missing_skills: true,
    };
    let check_diags = checker::check(&program, &ctx);
    diags.extend(check_diags.into_iter().filter(|d| d.is_error()).map(|d| d.message));
    diags
}

fn strip_code_fences(text: &str) -> String {
    let t = text.trim();
    let t = t.strip_prefix("```brief").unwrap_or(t);
    let t = t.strip_prefix("```").unwrap_or(t);
    let t = t.strip_suffix("```").unwrap_or(t);
    t.trim().to_string()
}

// ─────────────────────────────────────────────────────────────────────────────

fn description_to_task_name(desc: &str) -> String {
    desc.split_whitespace()
        .take(4)
        .map(|word| {
            let clean: String = word.chars().filter(|c| c.is_alphanumeric()).collect();
            let mut chars = clean.chars();
            match chars.next() {
                None    => String::new(),
                Some(c) => c.to_uppercase().to_string() + chars.as_str(),
            }
        })
        .collect::<String>()
}

fn render_brief_template(task_name: &str, description: &str) -> String {
    format!(
        r#"// Generated by `brief gen`
// Review and customise before use.

@BriefBuilder
task {task_name} : TaskBrief {{
    goal = "{description}"

    // TODO: add `import skill "SkillName"` for each skill you need
    // TODO: add steps to define the workflow

    // Example:
    // step FetchData {{
    //     let result = perform MySkill.operation(input)?;
    // }}
}}
"#
    )
}
