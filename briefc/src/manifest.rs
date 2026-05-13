/// brief.toml project manifest reader.
///
/// `brief.toml` lives at the root of a Brief project and defines:
/// - `[project]` — name, version, authors
/// - `[skills]`  — name → path overrides for skill resolution
/// - `[ci]`      — examples list checked in CI
///
/// Example:
/// ```toml
/// [project]
/// name    = "my-app"
/// version = "0.1.0"
///
/// [skills]
/// GraphQL = ".claude/skills/GraphQL"
///
/// [ci]
/// examples = ["hello.brief", "auth.brief"]
/// ```

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::Deserialize;

// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
pub struct BriefManifest {
    pub project:  ProjectSection,
    #[serde(default)]
    pub skills:   HashMap<String, String>,
    #[serde(default)]
    pub ci:       CiSection,

    /// Directory where this manifest was loaded from (set by `load_manifest`).
    #[serde(skip)]
    pub root_dir: PathBuf,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ProjectSection {
    pub name:    String,
    #[serde(default = "default_version")]
    pub version: String,
    #[serde(default)]
    pub authors: Vec<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct CiSection {
    #[serde(default)]
    pub examples: Vec<String>,
}

fn default_version() -> String { "0.1.0".into() }

// ─────────────────────────────────────────────────────────────────────────────

impl BriefManifest {
    /// Resolve a skill name to its `.briefskill` file path using manifest overrides.
    ///
    /// The `[skills]` table maps skill name → directory path (relative to `root_dir`).
    /// If found, returns `<root_dir>/<dir>/<name>.briefskill` if it exists.
    pub fn resolve_skill(&self, name: &str) -> Option<PathBuf> {
        let dir_str = self.skills.get(name)?;
        let skill_dir = self.root_dir.join(dir_str);
        let candidate = skill_dir.join(format!("{name}.briefskill"));
        if candidate.exists() { Some(candidate) } else { None }
    }
}

// ─────────────────────────────────────────────────────────────────────────────

/// Search for and parse a `brief.toml` manifest, starting at `start_dir` and
/// walking up toward the filesystem root. Returns `None` if no manifest found
/// or if the manifest cannot be parsed (errors are printed to stderr).
pub fn load_manifest(start_dir: &Path) -> Option<BriefManifest> {
    let mut dir = start_dir.to_path_buf();
    loop {
        let candidate = dir.join("brief.toml");
        if candidate.exists() {
            return parse_manifest(&candidate, &dir);
        }
        if !dir.pop() { break; }
    }
    None
}

fn parse_manifest(path: &Path, root_dir: &Path) -> Option<BriefManifest> {
    let text = std::fs::read_to_string(path).ok()?;
    match toml::from_str::<BriefManifest>(&text) {
        Ok(mut m) => {
            m.root_dir = root_dir.to_path_buf();
            Some(m)
        }
        Err(e) => {
            eprintln!("warn: could not parse {}: {e}", path.display());
            None
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_minimal_manifest() {
        let toml = r#"
[project]
name = "my-app"
"#;
        let m: BriefManifest = toml::from_str(toml).unwrap();
        assert_eq!(m.project.name, "my-app");
        assert_eq!(m.project.version, "0.1.0");
        assert!(m.skills.is_empty());
    }

    #[test]
    fn parses_skills_section() {
        let toml = r#"
[project]
name = "my-app"

[skills]
GraphQL = ".claude/skills/GraphQL"
Auth    = ".claude/skills/Auth"
"#;
        let m: BriefManifest = toml::from_str(toml).unwrap();
        assert_eq!(m.skills["GraphQL"], ".claude/skills/GraphQL");
        assert_eq!(m.skills["Auth"],    ".claude/skills/Auth");
    }

    #[test]
    fn parses_ci_section() {
        let toml = r#"
[project]
name = "my-app"

[ci]
examples = ["hello.brief", "auth.brief"]
"#;
        let m: BriefManifest = toml::from_str(toml).unwrap();
        assert_eq!(m.ci.examples, vec!["hello.brief", "auth.brief"]);
    }
}
