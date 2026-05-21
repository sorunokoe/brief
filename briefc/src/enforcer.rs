/// Runtime enforcement of `allow{}/deny{}` call-level permission patterns.
///
/// The enforcer is the production counterpart to the compile-time checker:
/// - Compile time: checker validates patterns reference real functions with correct arg names.
/// - Runtime: enforcer matches actual JSON call arguments against compiled patterns.
///
/// This module has no side-effects — it is a pure function from (policy, call) → decision.
use crate::ast::{ArgPattern, CallPattern};

// ─────────────────────────────────────────────────────────────────────────────
// Public API
// ─────────────────────────────────────────────────────────────────────────────

/// The outcome of checking a call against the task's allow/deny policy.
#[derive(Debug, PartialEq)]
pub enum CallDecision {
    Permitted,
    Blocked { reason: String },
}

impl CallDecision {
    pub fn is_permitted(&self) -> bool {
        matches!(self, CallDecision::Permitted)
    }
}

/// Check whether a tool call is permitted by the task's allow/deny policy.
///
/// Algorithm:
/// 1. Check deny patterns first. Any match → Blocked immediately.
/// 2. If allow is non-empty (whitelist mode): no match → Blocked.
/// 3. Otherwise → Permitted.
///
/// Backward compatibility: if both `allow` and `deny` are empty, the call is
/// always Permitted (existing `uses[]/forbids{}` behavior is unchanged).
pub fn check_call(
    skill: &str,
    func: &str,
    args: &serde_json::Value,
    allow: &[CallPattern],
    deny: &[CallPattern],
) -> CallDecision {
    // Step 1: deny always wins.
    for pat in deny {
        if pattern_matches(pat, skill, func, args) {
            return CallDecision::Blocked {
                reason: format!(
                    "matched deny: {}.{}{}",
                    pat.skill,
                    pat.func.as_deref().unwrap_or("*"),
                    format_pattern_args(&pat.args),
                ),
            };
        }
    }

    // Step 2: whitelist mode (allow is non-empty).
    if !allow.is_empty() {
        for pat in allow {
            if pattern_matches(pat, skill, func, args) {
                return CallDecision::Permitted;
            }
        }
        return CallDecision::Blocked {
            reason: format!("{}.{} is not in allow{{}} patterns", skill, func),
        };
    }

    // Step 3: no allow/deny constraints — backward-compatible pass-through.
    CallDecision::Permitted
}

// ─────────────────────────────────────────────────────────────────────────────
// Matching internals
// ─────────────────────────────────────────────────────────────────────────────

/// Returns true if `pat` matches the call `(skill, func, args)`.
fn pattern_matches(
    pat: &CallPattern,
    skill: &str,
    func: &str,
    args: &serde_json::Value,
) -> bool {
    // Skill must match exactly.
    if pat.skill != skill {
        return false;
    }

    // Function: None = wildcard (matches any func).
    if let Some(pat_func) = &pat.func {
        if pat_func != func {
            return false;
        }
    }

    // Args: empty pattern = any args accepted.
    if pat.args.is_empty() {
        return true;
    }

    // All declared arg constraints must be satisfied.
    for (key, arg_pat) in &pat.args {
        let actual = args.get(key.as_str());
        if !arg_pattern_matches(arg_pat, actual) {
            return false;
        }
    }

    true
}

/// Returns true if the actual argument value satisfies the `ArgPattern`.
fn arg_pattern_matches(pat: &ArgPattern, actual: Option<&serde_json::Value>) -> bool {
    match pat {
        ArgPattern::Any => true,

        ArgPattern::Exact(expected) => match actual {
            Some(v) => v == expected,
            None => false,
        },

        ArgPattern::Glob(glob_str) => match actual {
            Some(serde_json::Value::String(s)) => {
                let normalized_path = normalize_path(s);
                let normalized_glob = normalize_path(glob_str);
                glob::Pattern::new(&normalized_glob)
                    .map(|p| p.matches(&normalized_path))
                    .unwrap_or(false)
            }
            _ => false,
        },
    }
}

/// Normalize a path string before glob matching.
///
/// Rules (no filesystem access — string-only):
/// - Remove trailing slashes
/// - Collapse `/./ ` → `/`
/// - Resolve `segment/..` pairs (one level at a time, repeated until stable)
/// - Normalize `//` → `/`
fn normalize_path(path: &str) -> String {
    // Work with `/` as separator throughout (glob crate uses `/` on all platforms).
    let mut p = path.replace('\\', "/");

    // Collapse // sequences.
    while p.contains("//") {
        p = p.replace("//", "/");
    }

    // Remove trailing slash (unless it's the root `/`).
    if p.len() > 1 && p.ends_with('/') {
        p.pop();
    }

    // Resolve /./
    while p.contains("/./") {
        p = p.replace("/./", "/");
    }
    if p.ends_with("/.") {
        p.truncate(p.len() - 2);
    }

    // Resolve .. segments (clamped at root).
    p = resolve_dotdot(&p);

    p
}

/// Resolve all `..` segments using a stack-based approach.
/// Paths that try to ascend above root are clamped (e.g. `/../x` → `/x`).
/// This prevents `/../etc/passwd` from escaping a `/tmp/**` allow pattern.
fn resolve_dotdot(path: &str) -> String {
    let is_absolute = path.starts_with('/');
    let mut stack: Vec<&str> = Vec::new();

    for segment in path.split('/') {
        match segment {
            "" | "." => {}  // skip empty / self-reference
            ".." => {
                if stack.is_empty() {
                    // Clamp: cannot ascend above root (or current dir for relative paths).
                } else {
                    stack.pop();
                }
            }
            s => stack.push(s),
        }
    }

    if is_absolute {
        format!("/{}", stack.join("/"))
    } else {
        stack.join("/")
    }
}

/// Format arg patterns for display in blocked reason strings.
fn format_pattern_args(args: &[(String, ArgPattern)]) -> String {
    if args.is_empty() {
        return String::new();
    }
    let inner: Vec<String> = args
        .iter()
        .map(|(k, v)| match v {
            ArgPattern::Any => format!("{k}=*"),
            ArgPattern::Exact(val) => format!("{k}={val}"),
            ArgPattern::Glob(g) => format!("{k}=\"{g}\""),
        })
        .collect();
    format!("({})", inner.join(", "))
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{ArgPattern, CallPattern, Span};
    use serde_json::json;

    fn dummy_span() -> Span {
        Span { start: 0, end: 0 }
    }

    fn pat(skill: &str, func: Option<&str>, args: Vec<(&str, ArgPattern)>) -> CallPattern {
        CallPattern {
            skill: skill.to_string(),
            func: func.map(str::to_string),
            args: args.into_iter().map(|(k, v)| (k.to_string(), v)).collect(),
            span: dummy_span(),
        }
    }

    fn exact_str(s: &str) -> ArgPattern {
        ArgPattern::Exact(serde_json::Value::String(s.to_string()))
    }

    fn exact_num(n: i64) -> ArgPattern {
        ArgPattern::Exact(serde_json::Value::Number(n.into()))
    }

    fn glob(s: &str) -> ArgPattern {
        ArgPattern::Glob(s.to_string())
    }

    // ── allow whitelist ───────────────────────────────────────────────────────

    #[test]
    fn permit_when_no_policy() {
        let decision = check_call("GitHub", "get_pr", &json!({}), &[], &[]);
        assert_eq!(decision, CallDecision::Permitted);
    }

    #[test]
    fn permit_matching_allow_pattern() {
        let allow = vec![pat("GitHub", Some("get_pr"), vec![])];
        let decision = check_call("GitHub", "get_pr", &json!({}), &allow, &[]);
        assert_eq!(decision, CallDecision::Permitted);
    }

    #[test]
    fn block_non_listed_call_in_whitelist_mode() {
        let allow = vec![pat("GitHub", Some("get_pr"), vec![])];
        let decision = check_call("GitHub", "merge_pr", &json!({}), &allow, &[]);
        assert!(matches!(decision, CallDecision::Blocked { .. }));
    }

    #[test]
    fn permit_wildcard_allow_for_any_func() {
        let allow = vec![pat("GitHub", None, vec![])]; // GitHub.*
        let decision = check_call("GitHub", "anything", &json!({}), &allow, &[]);
        assert_eq!(decision, CallDecision::Permitted);
    }

    // ── deny blacklist ────────────────────────────────────────────────────────

    #[test]
    fn block_matching_deny_pattern() {
        let deny = vec![pat("GitHub", Some("merge_pr"), vec![])];
        let decision = check_call("GitHub", "merge_pr", &json!({}), &[], &deny);
        assert!(matches!(decision, CallDecision::Blocked { .. }));
    }

    #[test]
    fn permit_non_denied_call() {
        let deny = vec![pat("GitHub", Some("merge_pr"), vec![])];
        let decision = check_call("GitHub", "get_pr", &json!({}), &[], &deny);
        assert_eq!(decision, CallDecision::Permitted);
    }

    // ── deny overrides allow ──────────────────────────────────────────────────

    #[test]
    fn deny_overrides_allow() {
        let allow = vec![pat("GitHub", Some("get_pr"), vec![])];
        let deny = vec![pat("GitHub", None, vec![])]; // deny GitHub.* overrides all
        let decision = check_call("GitHub", "get_pr", &json!({}), &allow, &deny);
        assert!(
            matches!(decision, CallDecision::Blocked { .. }),
            "deny wildcard must override allow"
        );
    }

    // ── argument-level matching ───────────────────────────────────────────────

    #[test]
    fn permit_exact_arg_match() {
        let allow = vec![pat(
            "GitHub",
            Some("get_file"),
            vec![("owner", exact_str("acme")), ("repo", exact_str("brief"))],
        )];
        let args = json!({"owner": "acme", "repo": "brief", "path": "src/main.rs"});
        assert_eq!(
            check_call("GitHub", "get_file", &args, &allow, &[]),
            CallDecision::Permitted
        );
    }

    #[test]
    fn block_exact_arg_mismatch() {
        let allow = vec![pat(
            "GitHub",
            Some("get_file"),
            vec![("owner", exact_str("acme"))],
        )];
        let args = json!({"owner": "other"});
        assert!(matches!(
            check_call("GitHub", "get_file", &args, &allow, &[]),
            CallDecision::Blocked { .. }
        ));
    }

    #[test]
    fn permit_any_arg_wildcard() {
        let allow = vec![pat(
            "GitHub",
            Some("get_pr"),
            vec![("number", ArgPattern::Any)],
        )];
        let args = json!({"number": 42});
        assert_eq!(
            check_call("GitHub", "get_pr", &args, &allow, &[]),
            CallDecision::Permitted
        );
    }

    #[test]
    fn permit_glob_path_match() {
        let allow = vec![pat(
            "FileSystem",
            Some("write_file"),
            vec![("path", glob("/tmp/**"))],
        )];
        let args = json!({"path": "/tmp/review/output.md"});
        assert_eq!(
            check_call("FileSystem", "write_file", &args, &allow, &[]),
            CallDecision::Permitted
        );
    }

    #[test]
    fn block_glob_path_outside_allowed_prefix() {
        let allow = vec![pat(
            "FileSystem",
            Some("write_file"),
            vec![("path", glob("/tmp/**"))],
        )];
        let args = json!({"path": "/etc/passwd"});
        assert!(matches!(
            check_call("FileSystem", "write_file", &args, &allow, &[]),
            CallDecision::Blocked { .. }
        ));
    }

    // ── path normalization ────────────────────────────────────────────────────

    #[test]
    fn path_normalization_dotdot_blocked_by_deny() {
        // /tmp/../src/main.rs normalizes to /src/main.rs — matches deny /src/**
        let deny = vec![pat(
            "FileSystem",
            Some("write_file"),
            vec![("path", glob("/src/**"))],
        )];
        let args = json!({"path": "/tmp/../src/main.rs"});
        assert!(
            matches!(
                check_call("FileSystem", "write_file", &args, &[], &deny),
                CallDecision::Blocked { .. }
            ),
            "path traversal via .. should be normalized and caught by deny pattern"
        );
    }

    #[test]
    fn path_normalization_dotdot_in_allow() {
        // /tmp/./review.md normalizes to /tmp/review.md — matches allow /tmp/**
        let allow = vec![pat(
            "FileSystem",
            Some("read_file"),
            vec![("path", glob("/tmp/**"))],
        )];
        let args = json!({"path": "/tmp/./review.md"});
        assert_eq!(
            check_call("FileSystem", "read_file", &args, &allow, &[]),
            CallDecision::Permitted,
            "/tmp/./review.md should normalize and match /tmp/**"
        );
    }

    // ── JSON type matching ────────────────────────────────────────────────────

    #[test]
    fn exact_number_matches_json_number_not_string() {
        let allow = vec![pat("API", Some("call"), vec![("number", exact_num(1))])];
        let args_num = json!({"number": 1});
        let args_str = json!({"number": "1"});
        assert_eq!(
            check_call("API", "call", &args_num, &allow, &[]),
            CallDecision::Permitted,
            "JSON number 1 should match Exact(Number(1))"
        );
        assert!(
            matches!(
                check_call("API", "call", &args_str, &allow, &[]),
                CallDecision::Blocked { .. }
            ),
            "JSON string '1' should NOT match Exact(Number(1))"
        );
    }

    // ── blocked reason message ────────────────────────────────────────────────

    #[test]
    fn blocked_reason_names_deny_pattern() {
        let deny = vec![pat(
            "FileSystem",
            Some("write_file"),
            vec![("path", glob("./src/**"))],
        )];
        let args = json!({"path": "./src/main.rs"});
        let decision = check_call("FileSystem", "write_file", &args, &[], &deny);
        match decision {
            CallDecision::Blocked { reason } => {
                assert!(reason.contains("deny"), "reason should mention 'deny'");
                assert!(
                    reason.contains("FileSystem"),
                    "reason should name the skill"
                );
            }
            _ => panic!("expected Blocked"),
        }
    }

    // ── Path normalization / traversal security ──────────────────────────────

    #[test]
    fn path_traversal_above_root_clamped() {
        // `/../etc/passwd` must normalize to `/etc/passwd`, not stay as `/../etc/passwd`.
        assert_eq!(normalize_path("/../etc/passwd"), "/etc/passwd");
    }

    #[test]
    fn path_traversal_multiple_ascents_clamped() {
        // `/a/b/../../../c` — extra `..` beyond root is clamped.
        assert_eq!(normalize_path("/a/b/../../../c"), "/c");
    }

    #[test]
    fn path_dotdot_root_alone_clamped() {
        // `/..` should normalize to `/`.
        assert_eq!(normalize_path("/.."), "/");
    }

    #[test]
    fn path_traversal_cannot_bypass_tmp_allow() {
        // Allow only /tmp/**; path /tmp/../../etc/passwd must NOT match.
        let allow = vec![pat("FS", Some("read_file"), vec![("path", glob("/tmp/**"))])];
        let args = json!({"path": "/tmp/../../etc/passwd"});
        // Normalized to /etc/passwd — does NOT match /tmp/**
        let decision = check_call("FS", "read_file", &args, &allow, &[]);
        assert!(
            matches!(decision, CallDecision::Blocked { .. }),
            "traversal path must be blocked when allow is /tmp/**"
        );
    }

    #[test]
    fn path_normal_resolution_still_works() {
        assert_eq!(normalize_path("/a/b/../c"), "/a/c");
        assert_eq!(normalize_path("/a/./b"), "/a/b");
        assert_eq!(normalize_path("//a//b//"), "/a/b");
    }
}
