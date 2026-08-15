//! Durable, exportable audit trail.
//!
//! The in-memory capped `VecDeque` (`push_audit_entry`, owned by
//! `BackendState.audit_log` in `main.rs`) still serves the GUI's live
//! Security-tab feed exactly as before — fast, restart-reset, capped at
//! `AUDIT_LOG_CAP`. This module adds what it was missing: an append-only,
//! on-disk trail (`append_durable`/`read_all_durable`) that survives a
//! restart and isn't capped, plus a CEF (Common Event Format) export path
//! for SIEM ingestion, matching `docs/ROADMAP.md`'s Enterprise Trust Track
//! item #3.

use serde::{Deserialize, Serialize};
use std::io::{BufRead, Write};
use std::path::PathBuf;

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

/// Appends one entry to the durable, append-only, restart-surviving audit
/// trail — one JSON object per line (JSONL: trivial to append to, trivial
/// to `tail`/`grep`, no schema-migration machinery needed). Best-effort:
/// warns rather than failing the caller's request on a write error, the
/// same tradeoff every other persistence function in this codebase makes
/// (`save_settings`, `save_api_keys`, ...).
pub fn append_durable(entry: &AuditLogEntry) {
    let line = match serde_json::to_string(entry) {
        Ok(l) => l,
        Err(err) => {
            tracing::warn!("audit_log: failed to serialize entry for the durable trail: {err}");
            return;
        }
    };
    let path = audit_log_path();
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

/// Reads the full durable audit trail (not just the capped in-memory live
/// view). A line that fails to parse (a truncated write, a manual edit, a
/// future format change) is skipped with a warning rather than failing the
/// whole read — one bad line shouldn't hide the rest of a real security
/// trail. No file yet is not an error: an empty trail, not a failure.
pub fn read_all_durable() -> Vec<AuditLogEntry> {
    let path = audit_log_path();
    let file = match std::fs::File::open(&path) {
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
                    tracing::warn!("audit_log: skipping unparseable line in durable trail: {err}");
                    None
                }
            },
            Err(err) => {
                tracing::warn!("audit_log: skipping unreadable line in durable trail: {err}");
                None
            }
        })
        .collect()
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
