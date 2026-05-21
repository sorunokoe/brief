/// `brief models` — manage local AI models for policy suggestion.
///
/// Commands:
///   brief models install [smollm2]   — download SmolLM2-135M to ~/.brief/models/
///   brief models list                — list installed models
use colored::Colorize;
use std::path::PathBuf;

/// Default model: SmolLM2-135M GGUF from Hugging Face Hub.
const SMOLLM2_URL: &str =
    "https://huggingface.co/HuggingFaceTB/SmolLM2-135M-Instruct-GGUF/resolve/main/smollm2-135m-instruct-q4_k_m.gguf";
const SMOLLM2_FILENAME: &str = "smollm2-135m.gguf";
const SMOLLM2_EXPECTED_SIZE_MB: u64 = 80; // ~80MB

pub fn models_dir() -> PathBuf {
    dirs_next()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".brief")
        .join("models")
}

fn dirs_next() -> Option<PathBuf> {
    // Use home dir.
    std::env::var("HOME").ok().map(PathBuf::from)
        .or_else(|| std::env::var("USERPROFILE").ok().map(PathBuf::from))
}

pub fn model_path(name: &str) -> PathBuf {
    models_dir().join(name)
}

pub fn is_model_installed(filename: &str) -> bool {
    model_path(filename).exists()
}

pub fn run_models_install(model_name: Option<&str>) -> bool {
    let (url, filename) = match model_name.unwrap_or("smollm2") {
        "smollm2" | "smollm2-135m" => (SMOLLM2_URL, SMOLLM2_FILENAME),
        other => {
            eprintln!("{}: unknown model '{}'. Available: smollm2",
                "error".red().bold(), other);
            return false;
        }
    };

    let dest = model_path(filename);

    if dest.exists() {
        eprintln!("{} Model already installed: {}", "✓".green(), dest.display());
        return true;
    }

    // Ensure models dir exists.
    if let Err(e) = std::fs::create_dir_all(models_dir()) {
        eprintln!("{}: cannot create models directory: {e}", "error".red().bold());
        return false;
    }

    eprintln!("{} Downloading {} (~{}MB) ...",
        "→".cyan(), filename, SMOLLM2_EXPECTED_SIZE_MB);
    eprintln!("  Source: {}", url.dimmed());
    eprintln!("  Dest:   {}", dest.display().to_string().dimmed());

    match download_with_progress(url, &dest) {
        Ok(bytes) => {
            let mb = bytes / (1024 * 1024);
            eprintln!("{} Downloaded {} ({mb}MB)", "✓".green().bold(), filename);
            eprintln!("  Run `brief policy suggest` to use AI-powered policy generation.");
            true
        }
        Err(e) => {
            // Clean up partial download.
            let _ = std::fs::remove_file(&dest);
            eprintln!("{}: download failed: {e}", "error".red().bold());
            false
        }
    }
}

pub fn run_models_list() -> bool {
    let dir = models_dir();
    eprintln!("{}", "Installed models:".bold());
    if !dir.exists() {
        eprintln!("  (none — run `brief models install` to download SmolLM2-135M)");
        return true;
    }
    let mut found = false;
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("gguf") {
                let size = std::fs::metadata(&path)
                    .map(|m| m.len() / (1024 * 1024))
                    .unwrap_or(0);
                eprintln!("  {} ({size}MB) — {}", 
                    path.file_name().unwrap_or_default().to_string_lossy().bold(),
                    path.display().to_string().dimmed());
                found = true;
            }
        }
    }
    if !found {
        eprintln!("  (none — run `brief models install` to download SmolLM2-135M)");
    }
    true
}

fn download_with_progress(url: &str, dest: &std::path::Path) -> Result<u64, String> {
    use std::io::Write;

    let agent = ureq::AgentBuilder::new()
        .timeout(std::time::Duration::from_secs(300))
        .build();

    let resp = agent.get(url)
        .call()
        .map_err(|e| e.to_string())?;

    let content_length = resp.header("content-length")
        .and_then(|v| v.parse::<u64>().ok());

    let mut reader = resp.into_reader();
    let tmp = dest.with_extension("gguf.tmp");
    let mut file = std::fs::File::create(&tmp).map_err(|e| e.to_string())?;

    let mut buf = vec![0u8; 64 * 1024];
    let mut total = 0u64;
    let mut last_report = 0u64;

    loop {
        let n = reader.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 { break; }
        file.write_all(&buf[..n]).map_err(|e| e.to_string())?;
        total += n as u64;
        // Print progress every 5MB.
        if total - last_report > 5 * 1024 * 1024 {
            last_report = total;
            let mb = total / (1024 * 1024);
            if let Some(cl) = content_length {
                let pct = total * 100 / cl;
                eprint!("\r  {mb}MB / {}MB ({pct}%)   ", cl / (1024 * 1024));
            } else {
                eprint!("\r  {mb}MB downloaded ...   ");
            }
            let _ = std::io::stderr().flush();
        }
    }
    eprintln!(); // newline after progress

    // Rename tmp → final.
    std::fs::rename(&tmp, dest).map_err(|e| e.to_string())?;

    Ok(total)
}

// Needed for read() call.
use std::io::Read;
