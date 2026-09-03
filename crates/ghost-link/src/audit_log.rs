//! Durable, exportable audit trail.
//!
//! The in-memory capped `VecDeque` (`push_audit_entry`, owned by
//! `BackendState.audit_log` in `main.rs`) still serves the GUI's live
//! Security-tab feed exactly as before — fast, restart-reset, capped at
//! `AUDIT_LOG_CAP`. This module adds what it was missing: an append-only,
//! on-disk trail (`append_durable`/`read_all_durable`) that survives a
//! restart, bounded with file rotation and retention controls, plus a CEF
//! (Common Event Format) export path for SIEM ingestion, matching
//! `docs/ROADMAP.md`'s Enterprise Trust Track item #3.

use serde::{Deserialize, Serialize};
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};

/// One row for the GUI's Security tab audit log — field names/shape match
/// what `SecurityTab.tsx` already expects (it predates a real backend for
/// this and was rendering against a permanently-empty list).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditLogEntry {
    pub event: String,
    /// "SUCCESS"/"AUTHENTICATED" render green in the GUI; anything else
    /// (e.g. "FAILED", "DENIED") renders as a yellow warning badge.
    pub status: String,
    pub ip: String,
    /// RFC3339 — the GUI does `new Date(e.time)` on this directly.
    pub time: String,
    pub detail: Option<String>,
}

pub const AUDIT_LOG_CAP: usize = 500;

pub const DEFAULT_AUDIT_LOG_MAX_BYTES: u64 = 10 * 1024 * 1024; // 10 MB
pub const DEFAULT_AUDIT_LOG_MAX_LINES: usize = 0; // 0 = disabled
pub const DEFAULT_AUDIT_LOG_MAX_FILES: usize = 5; // retain 5 rotated files

/// Appends one entry and trims from the front once over `AUDIT_LOG_CAP` —
/// split out from `record_audit_event` (`main.rs`) so it's unit-testable
/// against a bare `VecDeque` without needing to construct a full
/// `BackendState`.
pub fn push_audit_entry(log: &mut std::collections::VecDeque<AuditLogEntry>, entry: AuditLogEntry) {
    log.push_back(entry);
    while log.len() > AUDIT_LOG_CAP {
        log.pop_front();
    }
}

fn audit_log_path() -> PathBuf {
    std::env::var("GHOSTLINK_AUDIT_LOG_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("audit_log.jsonl"))
}

fn audit_log_max_bytes() -> u64 {
    std::env::var("GHOSTLINK_AUDIT_LOG_MAX_BYTES")
        .ok()
        .and_then(|v| v.trim().parse::<u64>().ok())
        .unwrap_or(DEFAULT_AUDIT_LOG_MAX_BYTES)
}

fn audit_log_max_lines() -> usize {
    std::env::var("GHOSTLINK_AUDIT_LOG_MAX_LINES")
        .ok()
        .and_then(|v| v.trim().parse::<usize>().ok())
        .unwrap_or(DEFAULT_AUDIT_LOG_MAX_LINES)
}

fn audit_log_max_files() -> usize {
    std::env::var("GHOSTLINK_AUDIT_LOG_MAX_FILES")
        .ok()
        .and_then(|v| v.trim().parse::<usize>().ok())
        .unwrap_or(DEFAULT_AUDIT_LOG_MAX_FILES)
}

fn rotated_path(base_path: &Path, index: usize) -> PathBuf {
    let mut os_string = base_path.as_os_str().to_os_string();
    os_string.push(format!(".{index}"));
    PathBuf::from(os_string)
}

fn count_file_lines(path: &Path) -> usize {
    let Ok(file) = std::fs::File::open(path) else {
        return 0;
    };
    std::io::BufReader::new(file).lines().count()
}

fn should_rotate(path: &Path) -> bool {
    let Ok(meta) = std::fs::metadata(path) else {
        return false;
    };
    let file_bytes = meta.len();
    if file_bytes == 0 {
        return false;
    }

    let max_bytes = audit_log_max_bytes();
    if max_bytes > 0 && file_bytes >= max_bytes {
        return true;
    }

    let max_lines = audit_log_max_lines();
    if max_lines > 0 && count_file_lines(path) >= max_lines {
        return true;
    }

    false
}

fn rotate_audit_log(path: &Path) {
    let max_files = audit_log_max_files();
    if max_files == 0 {
        if path.exists() {
            let _ = std::fs::remove_file(path);
        }
        return;
    }

    // Shift existing rotated files from max_files - 1 down to 1
    for i in (1..max_files).rev() {
        let src = rotated_path(path, i);
        let dst = rotated_path(path, i + 1);
        if src.exists() {
            if dst.exists() {
                let _ = std::fs::remove_file(&dst);
            }
            let _ = std::fs::rename(&src, &dst);
        }
    }

    // Move active path to path.1
    if path.exists() {
        let dst = rotated_path(path, 1);
        if dst.exists() {
            let _ = std::fs::remove_file(&dst);
        }
        let _ = std::fs::rename(path, dst);
    }

    // Purge any excess rotated files beyond max_files
    let mut excess = max_files + 1;
    loop {
        let excess_path = rotated_path(path, excess);
        if excess_path.exists() {
            let _ = std::fs::remove_file(&excess_path);
            excess += 1;
        } else {
            break;
        }
    }
}

/// Appends one entry to the durable, restart-surviving audit trail —
/// one JSON object per line (JSONL). Applies rotation and retention policies
/// (`GHOSTLINK_AUDIT_LOG_MAX_BYTES`, `GHOSTLINK_AUDIT_LOG_MAX_LINES`,
/// `GHOSTLINK_AUDIT_LOG_MAX_FILES`). Best-effort: warns rather than failing
/// the caller's request on a write error.
pub fn append_durable(entry: &AuditLogEntry) {
    let line = match serde_json::to_string(entry) {
        Ok(l) => l,
        Err(err) => {
            tracing::warn!("audit_log: failed to serialize entry for the durable trail: {err}");
            return;
        }
    };
    let path = audit_log_path();

    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() && !parent.exists() {
            let _ = std::fs::create_dir_all(parent);
        }
    }

    if should_rotate(&path) {
        rotate_audit_log(&path);
    }

    let result = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .and_then(|mut file| writeln!(file, "{line}"));
    if let Err(err) = result {
        tracing::warn!(
            "audit_log: failed to append to the durable trail at {}: {err}",
            path.display()
        );
    }
}

fn read_entries_from_file(path: &Path) -> Vec<AuditLogEntry> {
    let file = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(_) => return Vec::new(),
    };
    std::io::BufReader::new(file)
        .lines()
        .filter_map(|line| match line {
            Ok(l) if l.trim().is_empty() => None,
            Ok(l) => match serde_json::from_str::<AuditLogEntry>(&l) {
                Ok(entry) => Some(entry),
                Err(err) => {
                    tracing::warn!(
                        "audit_log: skipping unparseable line in durable trail at {}: {err}",
                        path.display()
                    );
                    None
                }
            },
            Err(err) => {
                tracing::warn!(
                    "audit_log: skipping unreadable line in durable trail at {}: {err}",
                    path.display()
                );
                None
            }
        })
        .collect()
}

/// Reads the full durable audit trail across active and retained rotated files.
/// Entries are returned in chronological order (oldest rotated files first, then active file).
/// Unparseable or unreadable lines are skipped with a warning.
pub fn read_all_durable() -> Vec<AuditLogEntry> {
    let path = audit_log_path();
    let mut files_to_read = Vec::new();

    let max_files = audit_log_max_files();
    let max_check = max_files.max(100);
    let mut existing_indices = Vec::new();
    for k in 1..=max_check {
        let p = rotated_path(&path, k);
        if p.exists() {
            existing_indices.push(k);
        }
    }
    existing_indices.sort_unstable_by(|a, b| b.cmp(a));

    for k in existing_indices {
        files_to_read.push(rotated_path(&path, k));
    }
    files_to_read.push(path);

    let mut all_entries = Vec::new();
    for file_path in files_to_read {
        all_entries.extend(read_entries_from_file(&file_path));
    }
    all_entries
}

/// CEF severity (0-10 scale): a successful event is informational,
/// anything else (FAILED/DENIED/...) is worth an analyst's attention.
fn cef_severity(status: &str) -> u8 {
    if status.eq_ignore_ascii_case("success") {
        1
    } else {
        7
    }
}

/// Escapes a CEF header field (Signature ID / Name) per the CEF spec:
/// backslash and pipe are the only characters requiring escaping there.
fn cef_escape_header(value: &str) -> String {
    value.replace('\\', "\\\\").replace('|', "\\|")
}

/// Escapes a CEF extension value (the part after `key=`): backslash and
/// equals sign must be escaped per the CEF spec, and a newline is replaced
/// (not escaped) since CEF is one event per line — an embedded newline
/// would otherwise split one audit entry into two lines for a naive
/// line-based ingester. This is not cosmetic: several real `detail`
/// strings already in this codebase contain a raw `=` (e.g.
/// `"name='{}' id={}"` on the key-revocation event), which would otherwise
/// corrupt the extension field boundary for any real CEF parser.
fn cef_escape_extension(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('=', "\\=")
        .replace(['\n', '\r'], " ")
}

/// Formats one entry as a single CEF (Common Event Format) line — the
/// SIEM-standard export format `docs/ROADMAP.md` names. `event` doubles as
/// both the CEF Signature ID and Name fields: this codebase's event names
/// (e.g. `"security.key.created"`, `"authz"`) are already stable,
/// descriptive identifiers, so a separate signature-id scheme would be
/// redundant.
pub fn to_cef_line(entry: &AuditLogEntry) -> String {
    let signature = cef_escape_header(&entry.event);
    let severity = cef_severity(&entry.status);
    let mut extension = format!(
        "rt={} src={} outcome={}",
        cef_escape_extension(&entry.time),
        cef_escape_extension(&entry.ip),
        cef_escape_extension(&entry.status),
    );
    if let Some(detail) = &entry.detail {
        extension.push_str(&format!(" msg={}", cef_escape_extension(detail)));
    }
    format!(
        "CEF:0|Ghostlink|ghost-link|{}|{signature}|{signature}|{severity}|{extension}",
        env!("CARGO_PKG_VERSION")
    )
}

/// Formats a full list of entries as newline-separated CEF lines, ready to
/// hand a SIEM ingester or write straight to a response body.
pub fn export_cef(entries: &[AuditLogEntry]) -> String {
    entries
        .iter()
        .map(to_cef_line)
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, OnceLock};

    // Real filesystem + real env var mutation — serialized like every other
    // env-var-mutating test in this codebase (see auth.rs's own env_lock),
    // since GHOSTLINK_AUDIT_LOG_PATH is process-global and `cargo test`
    // runs in parallel by default.
    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    fn sample(event: &str, status: &str, detail: Option<&str>) -> AuditLogEntry {
        AuditLogEntry {
            event: event.to_string(),
            status: status.to_string(),
            ip: "127.0.0.1".to_string(),
            time: "2026-08-15T00:00:00+00:00".to_string(),
            detail: detail.map(|d| d.to_string()),
        }
    }

    #[test]
    fn push_audit_entry_appends_in_order() {
        let mut log = std::collections::VecDeque::new();
        push_audit_entry(&mut log, sample("first", "SUCCESS", None));
        push_audit_entry(&mut log, sample("second", "SUCCESS", None));
        assert_eq!(log.len(), 2);
        assert_eq!(log[0].event, "first");
        assert_eq!(log[1].event, "second");
    }

    #[test]
    fn push_audit_entry_trims_oldest_once_over_cap() {
        let mut log = std::collections::VecDeque::new();
        for i in 0..(AUDIT_LOG_CAP + 10) {
            push_audit_entry(&mut log, sample(&format!("event-{i}"), "SUCCESS", None));
        }
        assert_eq!(log.len(), AUDIT_LOG_CAP, "must never grow past the cap");
        assert_eq!(log.front().unwrap().event, "event-10");
        assert_eq!(
            log.back().unwrap().event,
            format!("event-{}", AUDIT_LOG_CAP + 9)
        );
    }

    #[test]
    fn append_then_read_all_durable_round_trips() {
        let _guard = env_lock().lock().expect("env lock poisoned");
        let path =
            std::env::temp_dir().join(format!("ghostlink-test-audit-{}.jsonl", std::process::id()));
        let _ = std::fs::remove_file(&path);
        std::env::set_var("GHOSTLINK_AUDIT_LOG_PATH", &path);

        append_durable(&sample("auth", "FAILED", Some("GET /api/models")));
        append_durable(&sample(
            "security.key.created",
            "SUCCESS",
            Some("name='ci' id=key_abc"),
        ));

        let read_back = read_all_durable();
        assert_eq!(read_back.len(), 2);
        assert_eq!(read_back[0].event, "auth");
        assert_eq!(read_back[1].event, "security.key.created");

        std::env::remove_var("GHOSTLINK_AUDIT_LOG_PATH");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn read_all_durable_skips_a_malformed_line_without_failing_the_rest() {
        let _guard = env_lock().lock().expect("env lock poisoned");
        let path = std::env::temp_dir().join(format!(
            "ghostlink-test-audit-malformed-{}.jsonl",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        std::env::set_var("GHOSTLINK_AUDIT_LOG_PATH", &path);

        append_durable(&sample("first", "SUCCESS", None));
        {
            let mut file = std::fs::OpenOptions::new()
                .append(true)
                .open(&path)
                .unwrap();
            writeln!(file, "{{ this is not valid json").unwrap();
        }
        append_durable(&sample("third", "SUCCESS", None));

        let read_back = read_all_durable();
        assert_eq!(
            read_back.len(),
            2,
            "the malformed middle line must be skipped, not fail the whole read"
        );
        assert_eq!(read_back[0].event, "first");
        assert_eq!(read_back[1].event, "third");

        std::env::remove_var("GHOSTLINK_AUDIT_LOG_PATH");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn read_all_durable_returns_empty_when_no_file_exists_yet() {
        let _guard = env_lock().lock().expect("env lock poisoned");
        let path = std::env::temp_dir().join(format!(
            "ghostlink-test-audit-missing-{}.jsonl",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        std::env::set_var("GHOSTLINK_AUDIT_LOG_PATH", &path);

        assert!(read_all_durable().is_empty());

        std::env::remove_var("GHOSTLINK_AUDIT_LOG_PATH");
    }

    #[test]
    fn append_durable_rotates_and_retains_by_max_lines() {
        let _guard = env_lock().lock().expect("env lock poisoned");
        let temp_dir = std::env::temp_dir();
        let path = temp_dir.join(format!(
            "ghostlink-test-audit-rotate-lines-{}.jsonl",
            std::process::id()
        ));

        // Clean up any test artifacts
        let _ = std::fs::remove_file(&path);
        for i in 1..=10 {
            let _ = std::fs::remove_file(rotated_path(&path, i));
        }

        std::env::set_var("GHOSTLINK_AUDIT_LOG_PATH", &path);
        std::env::set_var("GHOSTLINK_AUDIT_LOG_MAX_LINES", "2");
        std::env::set_var("GHOSTLINK_AUDIT_LOG_MAX_FILES", "2");

        // Write 6 entries.
        // Entry 1, 2 -> path (file has 2 lines)
        // Entry 3 -> triggers rotation! path -> path.1, entry 3 written to path (1 line)
        // Entry 4 -> path (2 lines)
        // Entry 5 -> triggers rotation! path.1 -> path.2, path -> path.1, entry 5 written to path (1 line)
        // Entry 6 -> path (2 lines)
        for i in 1..=6 {
            append_durable(&sample(&format!("event-{i}"), "SUCCESS", None));
        }

        assert!(path.exists(), "active file must exist");
        assert!(rotated_path(&path, 1).exists(), "path.1 must exist");
        assert!(rotated_path(&path, 2).exists(), "path.2 must exist");
        assert!(
            !rotated_path(&path, 3).exists(),
            "path.3 must NOT exist because max_files is 2"
        );

        let all_entries = read_all_durable();
        // Since entries 1, 2 rotated to path.2 then got purged when entry 5 rotated path.1 -> path.2 and old path.2 was dropped!
        // Wait, let's trace:
        // Entry 1, 2 written -> path has [e1, e2]
        // Entry 3 written -> path had 2 lines >= max_lines(2). Rotates: path -> path.1. path has [e3].
        // Entry 4 written -> path has [e3, e4] (2 lines).
        // Entry 5 written -> path had 2 lines >= max_lines(2). Rotates: path.1 -> path.2, path -> path.1. path has [e5].
        // Entry 6 written -> path has [e5, e6] (2 lines).
        // Rotated files retained:
        // path.2 has [e1, e2]
        // path.1 has [e3, e4]
        // path has [e5, e6]
        // Total entries retained = 6!
        assert_eq!(all_entries.len(), 6);
        assert_eq!(all_entries[0].event, "event-1");
        assert_eq!(all_entries[5].event, "event-6");

        // Now append entry 7:
        // path has 2 lines >= max_lines(2). Rotates:
        // path.2 (which has [e1, e2]) is purged because max_files=2.
        // path.1 ([e3, e4]) -> path.2
        // path ([e5, e6]) -> path.1
        // path gets [e7].
        append_durable(&sample("event-7", "SUCCESS", None));
        let entries_after_7 = read_all_durable();
        assert_eq!(
            entries_after_7.len(),
            5,
            "e1 and e2 must be purged beyond retention limit"
        );
        assert_eq!(entries_after_7[0].event, "event-3");
        assert_eq!(entries_after_7[4].event, "event-7");

        std::env::remove_var("GHOSTLINK_AUDIT_LOG_PATH");
        std::env::remove_var("GHOSTLINK_AUDIT_LOG_MAX_LINES");
        std::env::remove_var("GHOSTLINK_AUDIT_LOG_MAX_FILES");
        let _ = std::fs::remove_file(&path);
        for i in 1..=10 {
            let _ = std::fs::remove_file(rotated_path(&path, i));
        }
    }

    #[test]
    fn append_durable_rotates_by_max_bytes() {
        let _guard = env_lock().lock().expect("env lock poisoned");
        let temp_dir = std::env::temp_dir();
        let path = temp_dir.join(format!(
            "ghostlink-test-audit-rotate-bytes-{}.jsonl",
            std::process::id()
        ));

        let _ = std::fs::remove_file(&path);
        for i in 1..=10 {
            let _ = std::fs::remove_file(rotated_path(&path, i));
        }

        std::env::set_var("GHOSTLINK_AUDIT_LOG_PATH", &path);
        // Each JSON line for sample is ~100 bytes. Set max_bytes = 150.
        std::env::set_var("GHOSTLINK_AUDIT_LOG_MAX_BYTES", "150");
        std::env::set_var("GHOSTLINK_AUDIT_LOG_MAX_FILES", "2");

        append_durable(&sample("byte-event-1", "SUCCESS", None));
        append_durable(&sample("byte-event-2", "SUCCESS", None)); // file size ~200 > 150
        append_durable(&sample("byte-event-3", "SUCCESS", None)); // triggers rotation!

        assert!(path.exists());
        assert!(rotated_path(&path, 1).exists());

        let entries = read_all_durable();
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].event, "byte-event-1");
        assert_eq!(entries[2].event, "byte-event-3");

        std::env::remove_var("GHOSTLINK_AUDIT_LOG_PATH");
        std::env::remove_var("GHOSTLINK_AUDIT_LOG_MAX_BYTES");
        std::env::remove_var("GHOSTLINK_AUDIT_LOG_MAX_FILES");
        let _ = std::fs::remove_file(&path);
        for i in 1..=10 {
            let _ = std::fs::remove_file(rotated_path(&path, i));
        }
    }

    #[test]
    fn append_durable_purges_excess_rotated_files() {
        let _guard = env_lock().lock().expect("env lock poisoned");
        let temp_dir = std::env::temp_dir();
        let path = temp_dir.join(format!(
            "ghostlink-test-audit-purge-excess-{}.jsonl",
            std::process::id()
        ));

        let _ = std::fs::remove_file(&path);
        for i in 1..=10 {
            let _ = std::fs::remove_file(rotated_path(&path, i));
        }

        // Manually create files .1, .2, .3, .4
        std::fs::write(&path, "active\n").unwrap();
        std::fs::write(rotated_path(&path, 1), "rot1\n").unwrap();
        std::fs::write(rotated_path(&path, 2), "rot2\n").unwrap();
        std::fs::write(rotated_path(&path, 3), "rot3\n").unwrap();
        std::fs::write(rotated_path(&path, 4), "rot4\n").unwrap();

        std::env::set_var("GHOSTLINK_AUDIT_LOG_PATH", &path);
        std::env::set_var("GHOSTLINK_AUDIT_LOG_MAX_LINES", "1");
        std::env::set_var("GHOSTLINK_AUDIT_LOG_MAX_FILES", "2");

        // Appending will trigger rotation because active file has 1 line >= max_lines 1.
        append_durable(&sample("purge-trigger", "SUCCESS", None));

        // max_files is 2. Rotated files .1 and .2 should exist. .3 and .4 should be purged!
        assert!(rotated_path(&path, 1).exists());
        assert!(rotated_path(&path, 2).exists());
        assert!(!rotated_path(&path, 3).exists(), ".3 must be purged");
        assert!(!rotated_path(&path, 4).exists(), ".4 must be purged");

        std::env::remove_var("GHOSTLINK_AUDIT_LOG_PATH");
        std::env::remove_var("GHOSTLINK_AUDIT_LOG_MAX_LINES");
        std::env::remove_var("GHOSTLINK_AUDIT_LOG_MAX_FILES");
        let _ = std::fs::remove_file(&path);
        for i in 1..=10 {
            let _ = std::fs::remove_file(rotated_path(&path, i));
        }
    }

    #[test]
    fn cef_severity_maps_success_low_and_everything_else_high() {
        assert_eq!(cef_severity("SUCCESS"), 1);
        assert_eq!(cef_severity("success"), 1);
        assert_eq!(cef_severity("FAILED"), 7);
        assert_eq!(cef_severity("DENIED"), 7);
    }

    #[test]
    fn to_cef_line_escapes_every_equals_sign_in_the_detail_extension_value() {
        // Real-world case: several existing audit `detail` strings already
        // contain a raw '=' (e.g. "name='dashboard' id=key_9a0c17cfa3ee" has
        // two: after "name" and after "id"). Per the CEF spec, only '\' and
        // '=' require escaping inside an extension value ('|' does NOT --
        // unlike the pipe-delimited header fields, extension is
        // space-separated key=value pairs, so a literal '|' in a value is
        // unambiguous). An unescaped '=' would corrupt the field boundary
        // for any real SIEM parser -- every occurrence must be escaped, not
        // just one.
        let entry = sample(
            "security.key.revoked",
            "SUCCESS",
            Some("name='dashboard' id=key_9a0c17cfa3ee | extra"),
        );
        let line = to_cef_line(&entry);
        assert!(line.starts_with("CEF:0|Ghostlink|ghost-link|"));
        assert!(
            line.contains("msg=name\\='dashboard' id\\=key_9a0c17cfa3ee | extra"),
            "every raw '=' inside the detail must be escaped (the literal '|' is left alone, \
             valid inside an extension value): {line}"
        );
    }

    #[test]
    fn cef_escape_header_escapes_pipe_and_backslash() {
        assert_eq!(cef_escape_header("a|b"), "a\\|b");
        assert_eq!(cef_escape_header(r"a\b"), r"a\\b");
    }

    #[test]
    fn to_cef_line_omits_msg_when_detail_is_none() {
        let entry = sample("auth", "SUCCESS", None);
        let line = to_cef_line(&entry);
        assert!(!line.contains("msg="));
    }

    #[test]
    fn export_cef_joins_multiple_entries_with_newlines() {
        let entries = vec![sample("a", "SUCCESS", None), sample("b", "FAILED", None)];
        let out = export_cef(&entries);
        assert_eq!(out.lines().count(), 2);
    }
}
