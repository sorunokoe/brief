/// `.brief.lock` — verification proof file.
///
/// Written by `brief verify`, read by `brief check` to gate execution.
/// Committed to version control alongside the `.brief` source file.
///
/// # Format (TOML)
/// ```toml
/// [meta]
/// brief_hash  = "sha256:<64-hex-chars>"  # SHA-256 of the .brief source
/// verified_at = "2026-05-30T10:00:00Z"   # RFC-3339 UTC timestamp
///
/// [verified]
/// "@url:https://api.example.com/health"  = { status = "ok" }
/// "figmaURL:https://figma.com/file/abc"  = { status = "ok", message = "2 components" }
/// ```

use std::collections::HashMap;
use std::fmt;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

// ─────────────────────────────────────────────────────────────────────────────
// Public types
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VerifyStatus {
    Ok,
    Fail,
}

impl fmt::Display for VerifyStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            VerifyStatus::Ok   => write!(f, "ok"),
            VerifyStatus::Fail => write!(f, "fail"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationResult {
    pub status:  VerifyStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LockMeta {
    /// `sha256:<hex>` of the `.brief` source file at verify time.
    pub brief_hash:  String,
    /// RFC-3339 UTC string, e.g. `"2026-05-30T10:00:00Z"`.
    pub verified_at: String,
}

/// Parsed `.brief.lock` file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LockFile {
    pub meta:     LockMeta,
    /// Maps `"<annotation>:<value>"` → `VerificationResult`.
    #[serde(default)]
    pub verified: HashMap<String, VerificationResult>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Path helpers
// ─────────────────────────────────────────────────────────────────────────────

/// Returns the `.brief.lock` path for a given `.brief` file path.
/// `foo/bar.brief` → `foo/bar.brief.lock`
pub fn lock_path(brief_path: &Path) -> PathBuf {
    let mut p = brief_path.to_path_buf();
    let old_name = p.file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_string();
    p.set_file_name(format!("{old_name}.lock"));
    p
}

// ─────────────────────────────────────────────────────────────────────────────
// SHA-256 source hash
// ─────────────────────────────────────────────────────────────────────────────

/// Compute the canonical `"sha256:<hex>"` hash of a file's contents.
pub fn sha256_file_hash(source: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(source);
    format!("sha256:{}", hex::encode(h.finalize()))
}

// ─────────────────────────────────────────────────────────────────────────────
// Timestamp
// ─────────────────────────────────────────────────────────────────────────────

/// Return the current UTC time as an RFC-3339 string (second precision).
pub fn now_rfc3339() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_secs();
    // Format: YYYY-MM-DDTHH:MM:SSZ
    let s  = secs % 60;
    let m  = (secs / 60) % 60;
    let h  = (secs / 3600) % 24;
    let days = secs / 86400; // days since epoch

    // Gregorian calendar from days-since-epoch using the Proleptic calendar algorithm.
    let (year, month, day) = days_to_ymd(days);
    format!("{year:04}-{month:02}-{day:02}T{h:02}:{m:02}:{s:02}Z")
}

/// Parse an RFC-3339 timestamp (as written by `now_rfc3339`) into seconds since epoch.
/// Returns `None` if the string is malformed.
pub fn rfc3339_to_unix(s: &str) -> Option<u64> {
    // Expected: "YYYY-MM-DDTHH:MM:SSZ"
    if s.len() < 20 { return None; }
    let year:  u64 = s[0..4].parse().ok()?;
    let month: u64 = s[5..7].parse().ok()?;
    let day:   u64 = s[8..10].parse().ok()?;
    let hour:  u64 = s[11..13].parse().ok()?;
    let min:   u64 = s[14..16].parse().ok()?;
    let sec:   u64 = s[17..19].parse().ok()?;

    let days = ymd_to_days(year, month, day)?;
    Some(days * 86400 + hour * 3600 + min * 60 + sec)
}

// ─────────────────────────────────────────────────────────────────────────────
// Read / Write
// ─────────────────────────────────────────────────────────────────────────────

/// Parse a `.brief.lock` TOML file.
/// Returns `None` and prints nothing on failure — callers decide how to report.
pub fn read_lock(lock_path: &Path) -> Option<LockFile> {
    let content = std::fs::read_to_string(lock_path).ok()?;
    toml::from_str(&content).ok()
}

/// Write a `LockFile` to the given path in TOML format.
pub fn write_lock(lock_path: &Path, lock: &LockFile) -> std::io::Result<()> {
    let content = toml::to_string_pretty(lock)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;
    std::fs::write(lock_path, content)
}

// ─────────────────────────────────────────────────────────────────────────────
// Staleness check
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, PartialEq)]
pub enum LockState {
    /// Lock is present, source hash matches, and is within `max_age_hours`.
    Fresh,
    /// Lock exists but source SHA-256 doesn't match the current brief file.
    SourceChanged,
    /// Lock is older than `max_age_hours`.
    Stale,
}

/// Check the freshness of a lock file against the current brief source.
///
/// Takes a parsed `&LockFile` — by the time this is called, the file exists and was
/// parsed successfully. As a result, this function can only return `Fresh`, `Stale`,
/// or `SourceChanged`. There is no `Missing` variant; call sites handle the
/// "file not found" case before calling this function.
///
/// - `brief_source`: contents of the `.brief` file
/// - `max_age_hours`: maximum allowed age; `0` means "never expire"
pub fn check_lock(lock: &LockFile, brief_source: &[u8], max_age_hours: u64) -> LockState {
    let expected = sha256_file_hash(brief_source);
    if lock.meta.brief_hash != expected {
        return LockState::SourceChanged;
    }

    if max_age_hours == 0 {
        return LockState::Fresh; // max_age_hours=0 means "never expire"
    }

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_secs();

    if let Some(lock_ts) = rfc3339_to_unix(&lock.meta.verified_at) {
        let age_secs = now.saturating_sub(lock_ts);
        if age_secs > max_age_hours * 3600 {
            return LockState::Stale;
        }
    }
    // If we can't parse the timestamp, treat as stale.
    else {
        return LockState::Stale;
    }

    LockState::Fresh
}

// ─────────────────────────────────────────────────────────────────────────────
// Calendar helpers (no external deps)
// ─────────────────────────────────────────────────────────────────────────────

fn is_leap(y: u64) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

fn days_in_month(y: u64, m: u64) -> u64 {
    match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11               => 30,
        2 => if is_leap(y) { 29 } else { 28 },
        _ => 0,
    }
}

/// Days since Unix epoch (1970-01-01) → (year, month, day).
fn days_to_ymd(mut days: u64) -> (u64, u64, u64) {
    let mut y = 1970u64;
    loop {
        let days_in_year = if is_leap(y) { 366 } else { 365 };
        if days < days_in_year { break; }
        days -= days_in_year;
        y   += 1;
    }
    let mut m = 1u64;
    loop {
        let dim = days_in_month(y, m);
        if days < dim { break; }
        days -= dim;
        m    += 1;
    }
    (y, m, days + 1)
}

/// (year, month, day) → days since Unix epoch.  Returns `None` for pre-1970 dates.
fn ymd_to_days(y: u64, month: u64, day: u64) -> Option<u64> {
    if y < 1970 { return None; }
    let mut d = 0u64;
    for yr in 1970..y {
        d += if is_leap(yr) { 366 } else { 365 };
    }
    for mo in 1..month {
        d += days_in_month(y, mo);
    }
    d += day.checked_sub(1)?;
    Some(d)
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn sha256_hash_is_deterministic_and_prefixed() {
        let h = sha256_file_hash(b"hello brief");
        assert!(h.starts_with("sha256:"), "{h}");
        assert_eq!(h.len(), 7 + 64);
        assert_eq!(h, sha256_file_hash(b"hello brief"));
    }

    #[test]
    fn sha256_hash_changes_on_content_change() {
        let h1 = sha256_file_hash(b"version 1");
        let h2 = sha256_file_hash(b"version 2");
        assert_ne!(h1, h2);
    }

    #[test]
    fn lock_path_appends_lock_suffix() {
        let p = lock_path(Path::new("/project/task.brief"));
        assert_eq!(p, PathBuf::from("/project/task.brief.lock"));
    }

    #[test]
    fn lock_path_for_nested_file() {
        let p = lock_path(Path::new("foo/bar/deploy.brief"));
        assert_eq!(p, PathBuf::from("foo/bar/deploy.brief.lock"));
    }

    #[test]
    fn write_and_read_roundtrip() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.brief.lock");
        let lock = LockFile {
            meta: LockMeta {
                brief_hash:  "sha256:abc".to_string(),
                verified_at: "2026-05-30T10:00:00Z".to_string(),
            },
            verified: {
                let mut m = HashMap::new();
                m.insert("@url:https://api.example.com".to_string(), VerificationResult {
                    status:  VerifyStatus::Ok,
                    message: Some("200 OK".to_string()),
                });
                m
            },
        };
        write_lock(&path, &lock).expect("write failed");
        let loaded = read_lock(&path).expect("read failed");
        assert_eq!(loaded.meta.brief_hash, "sha256:abc");
        assert!(loaded.verified.contains_key("@url:https://api.example.com"));
        let v = &loaded.verified["@url:https://api.example.com"];
        assert_eq!(v.status, VerifyStatus::Ok);
        assert_eq!(v.message, Some("200 OK".to_string()));
    }

    #[test]
    fn read_lock_returns_none_for_missing_file() {
        assert!(read_lock(Path::new("/nonexistent/path.lock")).is_none());
    }

    #[test]
    fn check_lock_fresh_when_hash_matches_and_recent() {
        let source = b"task T {}";
        let hash = sha256_file_hash(source);
        let lock = LockFile {
            meta: LockMeta {
                brief_hash:  hash,
                verified_at: now_rfc3339(), // just now
            },
            verified: HashMap::new(),
        };
        assert_eq!(check_lock(&lock, source, 24), LockState::Fresh);
    }

    #[test]
    fn check_lock_source_changed_when_hash_differs() {
        let source = b"task T {}";
        let lock = LockFile {
            meta: LockMeta {
                brief_hash:  "sha256:aaaa".to_string(), // wrong hash
                verified_at: now_rfc3339(),
            },
            verified: HashMap::new(),
        };
        assert_eq!(check_lock(&lock, source, 24), LockState::SourceChanged);
    }

    #[test]
    fn check_lock_stale_when_old_timestamp() {
        let source = b"task T {}";
        let hash = sha256_file_hash(source);
        let lock = LockFile {
            meta: LockMeta {
                brief_hash:  hash,
                verified_at: "2020-01-01T00:00:00Z".to_string(), // ancient
            },
            verified: HashMap::new(),
        };
        assert_eq!(check_lock(&lock, source, 24), LockState::Stale);
    }

    #[test]
    fn check_lock_zero_max_age_never_expires() {
        let source = b"task T {}";
        let hash = sha256_file_hash(source);
        let lock = LockFile {
            meta: LockMeta {
                brief_hash:  hash,
                verified_at: "2020-01-01T00:00:00Z".to_string(), // ancient
            },
            verified: HashMap::new(),
        };
        // max_age_hours=0 means never expire — should still be Fresh if hash matches
        assert_eq!(check_lock(&lock, source, 0), LockState::Fresh);
    }

    #[test]
    fn rfc3339_roundtrip() {
        let ts = "2026-05-30T10:30:45Z";
        let unix = rfc3339_to_unix(ts).expect("parse failed");
        // Just verify it parsed to a plausible value (after 2024-01-01)
        assert!(unix > 1700000000, "unix timestamp too small: {unix}");
    }

    #[test]
    fn calendar_roundtrip() {
        // 2026-05-30 → days → back to 2026-05-30
        let d = ymd_to_days(2026, 5, 30).unwrap();
        let (y, m, day) = days_to_ymd(d);
        assert_eq!((y, m, day), (2026, 5, 30));
    }

    #[test]
    fn calendar_epoch() {
        // 1970-01-01 = 0 days
        assert_eq!(ymd_to_days(1970, 1, 1), Some(0));
        assert_eq!(days_to_ymd(0), (1970, 1, 1));
    }
}
