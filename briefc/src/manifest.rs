/// brief.toml project manifest reader.
///
/// `brief.toml` lives at the root of a Brief project and defines:
/// - `[project]`   — name, version, authors
/// - `[skills.*]`  — per-skill configuration (path override or MCP endpoint)
/// - `[verifiers.*]` — annotation → verifier routing for `brief verify`
/// - `[ci]`        — examples list checked in CI
/// - `[verify]`    — lock freshness settings
///
/// Example:
/// ```toml
/// [project]
/// name    = "my-app"
/// version = "0.1.0"
///
/// [skills.GraphQL]
/// path = ".claude/skills/GraphQL"      # local path override
///
/// [skills.DesignSystem]
/// mcp_command = ["npx", "@design-system/mcp-server"]
///
/// [verifiers."@url"]
/// skill = "builtin:url"
///
/// [verifiers.figmaURL]
/// mcp_command = ["npx", "@figma/brief-verifier"]
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
    pub project:   ProjectSection,
    /// Per-skill configuration. Keys are skill names.
    /// Supports both simple path overrides (`"path/to/dir"`) and full MCP configs.
    #[serde(default)]
    pub skills:    HashMap<String, SkillEntry>,
    /// Verifier routing: annotation name → verifier config.
    #[serde(default)]
    pub verifiers: HashMap<String, VerifierConfig>,
    #[serde(default)]
    pub ci:        CiSection,
    #[serde(default)]
    pub verify:    VerifySection,

    /// Directory where this manifest was loaded from (set by `load_manifest`).
    #[serde(skip)]
    pub root_dir: PathBuf,
}

// ─────────────────────────────────────────────────────────────────────────────

/// A skill entry in `[skills.*]` — either a legacy path string or a full config table.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum SkillEntry {
    /// Legacy form: `GraphQL = ".claude/skills/GraphQL"` (path override only).
    Path(String),
    /// Full form: `[skills.GraphQL]` table with optional path + MCP config.
    Config(SkillConfig),
}

impl SkillEntry {
    pub fn as_config(&self) -> SkillConfig {
        match self {
            SkillEntry::Path(p) => SkillConfig { path: Some(p.clone()), ..Default::default() },
            SkillEntry::Config(c) => c.clone(),
        }
    }
}

/// Per-skill configuration in `[skills.<Name>]`.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct SkillConfig {
    /// Local path override for `.briefskill` resolution (directory containing `<Name>.briefskill`).
    pub path: Option<String>,
    /// Spawn command for the skill's MCP server, e.g. `["npx", "@design-system/mcp-server"]`.
    pub mcp_command: Option<Vec<String>>,
    /// URL of a running MCP server for this skill, e.g. `"http://localhost:4000/mcp"`.
    pub mcp_url: Option<String>,
}

impl SkillConfig {
    /// Returns true if this config can launch/connect to an MCP server.
    pub fn has_mcp(&self) -> bool {
        self.mcp_command.is_some() || self.mcp_url.is_some()
    }
}

/// Verifier routing entry in `[verifiers.<annotation>]`.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct VerifierConfig {
    /// Built-in verifier name, e.g. `"builtin:url"`.
    pub skill: Option<String>,
    /// Spawn command for the verifier's MCP server.
    pub mcp_command: Option<Vec<String>>,
    /// URL of a running verifier MCP server.
    pub mcp_url: Option<String>,
}

/// `[verify]` section in `brief.toml`.
#[derive(Debug, Clone, Deserialize)]
pub struct VerifySection {
    /// Maximum age (hours) of `.brief.lock` before `brief check` emits E303.
    /// 0 means never expire (useful in offline / air-gapped setups).
    /// Default: 24.
    #[serde(default = "default_lock_age_hours")]
    pub max_lock_age_hours: u64,

    /// If true, `brief check` requires a valid `.brief.lock` whenever dynamic
    /// annotations are present.  Default: true.
    #[serde(default = "default_require_lock")]
    pub require_lock: bool,
}

impl Default for VerifySection {
    fn default() -> Self {
        Self { max_lock_age_hours: default_lock_age_hours(), require_lock: default_require_lock() }
    }
}

fn default_lock_age_hours() -> u64 { 24 }
fn default_require_lock() -> bool { true }

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
    /// Checks `[skills.<name>].path` (or legacy `[skills] name = "..."` form).
    /// Returns `<root_dir>/<dir>/<name>.briefskill` if it exists.
    pub fn resolve_skill(&self, name: &str) -> Option<PathBuf> {
        let entry = self.skills.get(name)?;
        let config = entry.as_config();
        let dir_str = config.path?;
        let skill_dir = self.root_dir.join(&dir_str);
        let candidate = skill_dir.join(format!("{name}.briefskill"));
        if candidate.exists() { Some(candidate) } else { None }
    }

    /// Return the SkillConfig for a named skill, if present and MCP-capable.
    pub fn skill_config(&self, name: &str) -> Option<SkillConfig> {
        let c = self.skills.get(name)?.as_config();
        if c.has_mcp() { Some(c) } else { None }
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
    fn parses_skills_legacy_string_form() {
        let toml = r#"
[project]
name = "my-app"

[skills]
GraphQL = ".claude/skills/GraphQL"
Auth    = ".claude/skills/Auth"
"#;
        let m: BriefManifest = toml::from_str(toml).unwrap();
        let gql = m.skills["GraphQL"].as_config();
        assert_eq!(gql.path.as_deref(), Some(".claude/skills/GraphQL"));
    }

    #[test]
    fn parses_skills_config_table_form() {
        let toml = r#"
[project]
name = "my-app"

[skills.DesignSystem]
mcp_command = ["npx", "@design-system/mcp-server"]

[skills.GraphQL]
path = ".claude/skills/GraphQL"
"#;
        let m: BriefManifest = toml::from_str(toml).unwrap();
        let ds = m.skills["DesignSystem"].as_config();
        assert_eq!(
            ds.mcp_command.as_ref().map(|v| v.iter().map(String::as_str).collect::<Vec<_>>()),
            Some(vec!["npx", "@design-system/mcp-server"])
        );
        let gql = m.skills["GraphQL"].as_config();
        assert_eq!(gql.path.as_deref(), Some(".claude/skills/GraphQL"));
    }

    #[test]
    fn parses_verifiers_section() {
        let toml = r#"
[project]
name = "my-app"

[verifiers."@url"]
skill = "builtin:url"

[verifiers.figmaURL]
mcp_command = ["npx", "@figma/brief-verifier"]

[verifiers."@k8s-namespace"]
mcp_url = "http://cluster-gateway:8080/mcp"
"#;
        let m: BriefManifest = toml::from_str(toml).unwrap();
        assert_eq!(m.verifiers["@url"].skill.as_deref(), Some("builtin:url"));
        assert_eq!(
            m.verifiers["figmaURL"].mcp_command.as_ref().map(|v| v.iter().map(String::as_str).collect::<Vec<_>>()),
            Some(vec!["npx", "@figma/brief-verifier"])
        );
        assert_eq!(m.verifiers["@k8s-namespace"].mcp_url.as_deref(), Some("http://cluster-gateway:8080/mcp"));
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
