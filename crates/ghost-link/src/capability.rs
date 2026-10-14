//! Capability classification — the server-side trust boundary for tool calls.
//!
//! The invariant this module exists to hold: **the model is not the trust
//! boundary, the server is.** A system prompt that tells the model "only read
//! files" is a suggestion the model may ignore; the classification here is
//! checked in Rust before dispatch, and a tool the table doesn't know about is
//! treated as the most dangerous class.
//!
//! Three classes:
//!
//! - `Read` — observation. May run inline and feed a result back to the model.
//! - `Write` — mutates state (files, memory, RAG index). Requires approval.
//! - `Exec` — runs commands or code. Requires approval, and is *never* covered
//!   by a session-scoped grant.
//!
//! Why a table rather than config flags: `McpServerConfig::requires_confirmation`
//! is per-*server*, so it can't distinguish `search` (read) from `write_file`
//! (write) on the same filesystem server, and it can't express "this one tool
//! is fine to auto-apply". Classification has to sit below server granularity.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use crate::mcp::McpToolSchema;

/// What a tool call is allowed to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CapabilityClass {
    /// Observational only. May execute inline and return a result to the model.
    Read,
    /// Mutates durable state. Requires approval before dispatch.
    Write,
    /// Runs commands or arbitrary code. Requires approval, never auto-applied.
    Exec,
}

impl CapabilityClass {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::Write => "write",
            Self::Exec => "exec",
        }
    }

    /// Whether this class needs an explicit human decision before it runs.
    pub fn requires_approval(self) -> bool {
        matches!(self, Self::Write | Self::Exec)
    }

    /// Whether a session-scoped grant may cover this class.
    ///
    /// `Exec` is excluded on purpose: "approve this tool for the rest of the
    /// session" must never become a standing shell. Workspace-scoped file
    /// writes can reasonably be approved for a session; command execution
    /// cannot.
    pub fn session_grant_allowed(self) -> bool {
        matches!(self, Self::Read | Self::Write)
    }
}

/// Per-tool classification entries, keyed by `(server, tool)`.
type ClassTable = HashMap<(String, String), CapabilityClass>;

fn table() -> &'static ClassTable {
    static TABLE: OnceLock<ClassTable> = OnceLock::new();
    TABLE.get_or_init(build_table)
}

/// Builds the classification table.
///
/// Match arms are on `(server, tool)` so an unlisted tool on a *known* server
/// still falls through to the `Exec` default rather than inheriting a broader
/// class from its server. The per-tool granularity is the whole point — a
/// filesystem server offering both `read_file` and `write_file` must not treat
/// them alike.
fn build_table() -> ClassTable {
    let mut t = ClassTable::new();

    // --- Read: inspection with no durable side effects ---
    for tool in [
        "read_text_file",
        // The live server advertises BOTH `read_file` and `read_text_file`;
        // only the latter was classified, so the alias fell through to Exec.
        "read_file",
        "read_media_file",
        "read_multiple_files",
        "list_directory",
        "list_directory_with_sizes",
        "directory_tree",
        "search_files",
        "get_file_info",
        "list_allowed_directories",
    ] {
        t.insert(("filesystem".into(), tool.into()), CapabilityClass::Read);
    }

    for tool in ["calculate", "evaluate", "calculate_expression"] {
        t.insert(("calculator".into(), tool.into()), CapabilityClass::Read);
    }

    // RAG: search is a read; indexing writes the index.
    t.insert(("rag".into(), "search".into()), CapabilityClass::Read);
    t.insert(
        ("rag".into(), "list_documents".into()),
        CapabilityClass::Read,
    );
    t.insert(
        ("rag".into(), "index_document".into()),
        CapabilityClass::Write,
    );

    // Memory: catalog/search are reads, remember/forget are writes.
    for tool in ["memory_catalog", "memory_search"] {
        t.insert(("memory".into(), tool.into()), CapabilityClass::Read);
    }
    for tool in ["memory_remember", "memory_forget"] {
        t.insert(("memory".into(), tool.into()), CapabilityClass::Write);
    }

    // Git: inspection reads, anything that moves refs or the worktree writes.
    // `git_diff_unstaged` / `git_diff_staged` are real tools on mcp-server-git
    // alongside the combined `git_diff`. Without them they fell through to the
    // Exec default, so an ordinary diff needed an approval.
    for tool in [
        "git_status",
        "git_log",
        "git_diff",
        "git_diff_unstaged",
        "git_diff_staged",
        "git_show",
        "git_blame",
    ] {
        t.insert(("git".into(), tool.into()), CapabilityClass::Read);
    }
    for tool in [
        "git_commit",
        "git_add",
        "git_checkout",
        "git_reset",
        "git_push",
        // Branch creation is a ref write like checkout/push, not a command
        // execution: it moves no worktree content and runs nothing.
        "git_create_branch",
        "git_branch",
    ] {
        t.insert(("git".into(), tool.into()), CapabilityClass::Write);
    }

    // Web fetch and search are reads with respect to *local* state.
    //
    // `fetch` and `duckduckgo` are separate servers and the table keys on
    // `(server, tool)`, so each needs its own entries. An earlier version filed
    // `brave_web_search` under `fetch` AND under `brave-search`, which left
    // `fetch.fetch` -- the tool that actually exists -- unclassified.
    t.insert(("fetch".into(), "fetch".into()), CapabilityClass::Read);
    // Verified live from `duckduckgo-mcp-server`: a single tool,
    // `duckduckgo_web_search`. It queries DuckDuckGo's free HTML endpoint, so
    // it observes the public web and writes nothing local -- Read.
    t.insert(
        ("duckduckgo".into(), "duckduckgo_web_search".into()),
        CapabilityClass::Read,
    );

    // Sequential thinking is pure inference.
    t.insert(
        ("sequential-thinking".into(), "sequentialthinking".into()),
        CapabilityClass::Read,
    );

    // --- Browser (Playwright): the tools are split by what they can reach, not by
    // whether they look like a "read".
    //
    // The server name is `playwright` and the tool set is the live one from
    // `npx -y @playwright/mcp@latest` (25 tools), not a guess.
    //
    // Reads are the observation tools: taking the page apart (snapshot, find,
    // console, network, screenshot) and navigating. `browser_navigate` is Read on
    // the same basis as `fetch` -- it retrieves a page. The distinction that
    // matters is that the default launch uses an ISOLATED browser profile, so the
    // navigated page carries none of the user's real session cookies. Pointing the
    // server at a real profile (`--user-data-dir`) changes that: a page the user is
    // logged into would then be reachable with their session, and navigation stops
    // being a pure observation.
    for tool in [
        "browser_snapshot",
        "browser_find",
        "browser_console_messages",
        "browser_network_requests",
        "browser_network_request",
        "browser_take_screenshot",
        "browser_wait_for",
        "browser_navigate",
        "browser_navigate_back",
    ] {
        t.insert(("playwright".into(), tool.into()), CapabilityClass::Read);
    }

    // Writes are the interactions: anything that acts on the page rather than
    // reading it. These are the approvals that matter -- a click can submit a form,
    // `browser_fill_form` exists to submit one, and `browser_type` can fill a
    // credential field. Nothing here runs code or moves local files.
    for tool in [
        "browser_click",
        "browser_hover",
        "browser_type",
        "browser_press_key",
        "browser_fill_form",
        "browser_select_option",
        "browser_drag",
        "browser_tabs",
        "browser_close",
        "browser_resize",
        "browser_emulate_media",
        "browser_handle_dialog",
    ] {
        t.insert(("playwright".into(), tool.into()), CapabilityClass::Write);
    }

    // Exec: code execution and exfiltration primitives, which are the two things a
    // browser is genuinely dangerous for.
    //
    // `browser_evaluate` and `browser_run_code_unsafe` run arbitrary JavaScript in
    // the page -- the latter is explicitly labelled unsafe by the server itself.
    // `browser_file_upload` and `browser_drop` take local file paths and push their
    // contents into the page, which makes them an exfiltration channel: reading a
    // local file is a Read, but sending it to whatever origin is loaded is not.
    for tool in [
        "browser_evaluate",
        "browser_run_code_unsafe",
        "browser_file_upload",
        "browser_drop",
    ] {
        t.insert(("playwright".into(), tool.into()), CapabilityClass::Exec);
    }

    // Vision analyzes an image; it does not write anything durable.
    t.insert(
        ("vision".into(), "analyze_image".into()),
        CapabilityClass::Read,
    );

    // --- Write: durable state changes ---
    for tool in ["write_file", "edit_file", "create_directory", "move_file"] {
        t.insert(("filesystem".into(), tool.into()), CapabilityClass::Write);
    }
    // Schema inspection reads. mcp-server-sqlite ships `list_tables` and
    // `describe_table` alongside the query tools, and both were falling through
    // to Exec -- so merely listing tables demanded an approval, which is the
    // kind of false gate that teaches people to approve without reading.
    for tool in ["read_query", "list_tables", "describe_table"] {
        t.insert(("sqlite".into(), tool.into()), CapabilityClass::Read);
    }
    for tool in [
        "write_query",
        "execute_query",
        "create_table",
        "modify_table",
        // `append_insight` writes to the server's memory file, so it is a
        // durable write like the rest.
        "append_insight",
    ] {
        t.insert(("sqlite".into(), tool.into()), CapabilityClass::Write);
    }

    // --- Exec: command or code execution. Never auto-applied, never
    // session-granted. Listed explicitly so the intent is auditable, even
    // though the unknown-tool default already lands here.
    //
    // The `docker-mcp-gateway` names are the LIVE tool set from
    // `docker mcp gateway run` (Docker 29.8.1, MCP Toolkit), not a guess. All
    // eight are Exec: `mcp-exec` and `code-mode` run arbitrary commands and
    // code outright, and the `mcp-add`/`mcp-remove`/`mcp-config-set`/
    // `mcp-create-profile`/`mcp-activate-profile` tools mutate the gateway's own
    // configuration -- which decides what the gateway is able to run. `mcp-find`
    // is included rather than treated as a read, because it queries the catalog
    // of servers the gateway can activate, and a catalog read that reveals what
    // can be executed is not an observation with no effect.
    //
    // Left out: the dynamically activated tools the gateway exposes after
    // `mcp-add` (containers, images, compose). Those arrive at runtime with
    // names this table has never seen, so they hit the unknown-tool default --
    // which is Exec. That fail-closed behavior is the point: a tool the gateway
    // invents at runtime cannot be quietly treated as a read.
    for (server, tool) in [
        ("terminal", "run_command"),
        ("code_execution", "execute_code"),
        ("docker-mcp-gateway", "mcp-exec"),
        ("docker-mcp-gateway", "code-mode"),
        ("docker-mcp-gateway", "mcp-add"),
        ("docker-mcp-gateway", "mcp-remove"),
        ("docker-mcp-gateway", "mcp-config-set"),
        ("docker-mcp-gateway", "mcp-create-profile"),
        ("docker-mcp-gateway", "mcp-activate-profile"),
        ("docker-mcp-gateway", "mcp-find"),
    ] {
        t.insert((server.into(), tool.into()), CapabilityClass::Exec);
    }

    t
}

/// Classifies a tool call.
///
/// **Unlisted tools default to `Exec`.** A new MCP server, or a tool added to an
/// existing one, is treated as the most dangerous class until someone vets it.
/// Failing closed is the only safe default here: the cost is an extra approval
/// prompt, while the cost of failing open is arbitrary command execution.
pub fn classify(server: &str, tool: &str) -> CapabilityClass {
    table()
        .get(&(server.to_string(), tool.to_string()))
        .copied()
        .unwrap_or(CapabilityClass::Exec)
}

/// Classifies a schema advertised by a connected MCP server.
pub fn classify_schema(schema: &McpToolSchema) -> CapabilityClass {
    classify(&schema.server, &schema.name)
}

/// Whether this call may auto-apply without a fresh approval.
///
/// Only the vetted workspace-edit path qualifies: a `Write` whose target
/// resolves inside the configured workspace root. `Exec` is excluded
/// unconditionally — no workspace root makes running a command safe by default.
pub fn is_vetted_auto_apply(
    class: CapabilityClass,
    resolved_path: Option<&std::path::Path>,
    workspace_root: &std::path::Path,
) -> bool {
    if class != CapabilityClass::Write {
        return false;
    }
    let Some(path) = resolved_path else {
        return false;
    };
    // Compare canonicalized forms on both sides — a raw string prefix check
    // misses `..` traversal and symlink escapes. `resolve_workspace_path` has
    // already refused those; this re-checks rather than assuming, because a
    // capability decision must not rest on a caller's unchecked claim.
    let Ok(canon_path) = path.canonicalize() else {
        return false;
    };
    let Ok(canon_root) = workspace_root.canonicalize() else {
        return false;
    };
    canon_path.starts_with(canon_root)
}

/// Whether a specific memory write is safe to apply without asking.
///
/// `memory_remember` is the one write in the system with no filesystem target and
/// no irreversible consequence: it appends a row the user can delete, scoped to a
/// workspace the server stamps itself. Gating it meant nothing was ever
/// remembered unless the user answered an approval prompt for each fact -- which
/// made both the memory store and the recall that reads it close to dead weight.
///
/// `memory_forget` is deliberately NOT included. Forgetting is not reversible --
/// there is no un-forget -- so a misfired delete should still cost a human
/// decision. Nothing that destroys data earns a blanket exemption, however
/// convenient it would be.
///
/// Scope is the real control here, not the prompt: `is_scope_stamped` forces the
/// workspace id at dispatch, so an auto-applied write lands in the same workspace
/// the chat is bound to and cannot be redirected by the model.
pub fn is_vetted_memory_write(server: &str, tool: &str) -> bool {
    server == "memory" && tool == "memory_remember"
}

/// Memory tools whose arguments carry a workspace scope the server must not be
/// able to choose for itself.
///
/// The model can put anything it likes in a tool call's arguments, including a
/// `workspace_id` naming a different workspace. Left alone that would let a
/// prompt-injected turn read (or worse, write) another workspace's memories —
/// which defeats the entire per-workspace grant model. So for these tools the
/// server overwrites the scope from the chat's own binding at dispatch, rather
/// than trusting the argument.
fn is_scope_stamped(server: &str) -> bool {
    server == "memory"
}

/// Replaces any caller-supplied workspace scope with the chat's own.
///
/// Returns the args unchanged for tools that aren't workspace-scoped. For the
/// scoped ones, `workspace_id` is always the server's value: a model-supplied
/// one is discarded rather than rejected, because failing the call outright
/// turns a prompt-injection attempt into a confusing tool error instead of a
/// silently-correct result, and the audit trail already records the real scope.
pub fn stamp_workspace_scope(
    server: &str,
    tool: &str,
    mut args: serde_json::Value,
    workspace_id: &str,
) -> serde_json::Value {
    let _ = tool;
    if !is_scope_stamped(server) {
        return args;
    }
    if !args.is_object() {
        // A non-object argument bag can't carry the scope; make it one so the
        // stamped id isn't silently dropped on the server side.
        args = serde_json::json!({});
    }
    let obj = args.as_object_mut().expect("just ensured object");
    obj.insert(
        "workspace_id".to_string(),
        serde_json::Value::String(workspace_id.to_string()),
    );
    args
}

/// What the server decided to do with a tool call, before it runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    /// May execute inline and return its result to the model as an observation.
    Allow,
    /// Must not execute. The model gets a pending handle and waits for a human.
    NeedsApproval {
        class: CapabilityClass,
        /// Why it was gated — recorded in the approval record and the trace.
        reason: &'static str,
    },
}

impl Decision {
    pub fn is_allowed(&self) -> bool {
        matches!(self, Self::Allow)
    }
}

/// `(workspace_id, server, tool)` — the scope a session grant applies to.
type GrantKey = (String, String, String);

/// Standing grants from "approve for session", keyed by `(workspace, server, tool)`.
///
/// Scoping to the triple is deliberate: a grant for `write_file` in workspace A
/// says nothing about `write_file` in workspace B, nor about a different tool on
/// the same server. `Exec` tools never land here — see
/// `CapabilityClass::session_grant_allowed`.
fn session_grants() -> &'static Mutex<HashMap<GrantKey, ()>> {
    static GRANTS: OnceLock<Mutex<HashMap<GrantKey, ()>>> = OnceLock::new();
    GRANTS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Records a session-scoped grant for a tool.
///
/// Returns `false` when the class is not eligible, so a caller can't widen the
/// boundary by calling this with an `Exec` tool.
pub fn grant_for_session(
    workspace_id: &str,
    server: &str,
    tool: &str,
    class: CapabilityClass,
) -> bool {
    if !class.session_grant_allowed() {
        return false;
    }
    if let Ok(mut grants) = session_grants().lock() {
        grants.insert(
            (
                workspace_id.to_string(),
                server.to_string(),
                tool.to_string(),
            ),
            (),
        );
        return true;
    }
    false
}

/// Whether a session grant currently covers this call.
pub fn has_session_grant(workspace_id: &str, server: &str, tool: &str) -> bool {
    session_grants()
        .lock()
        .map(|grants| {
            grants.contains_key(&(
                workspace_id.to_string(),
                server.to_string(),
                tool.to_string(),
            ))
        })
        .unwrap_or(false)
}

/// Decides whether a tool call may execute, without executing it.
///
/// This is the enforcement point for "the model is not the trust boundary": the
/// class comes from the Rust-side table, never from the prompt or the caller.
/// Ordering matters — an `Exec` tool is gated even if a grant exists, and a
/// session grant only ever applies to a class that was already approval-worthy.
pub fn decide(
    workspace_id: &str,
    server: &str,
    tool: &str,
    resolved_path: Option<&std::path::Path>,
    workspace_root: &std::path::Path,
) -> Decision {
    let class = classify(server, tool);

    if !class.requires_approval() {
        return Decision::Allow;
    }

    if is_vetted_auto_apply(class, resolved_path, workspace_root) {
        return Decision::Allow;
    }

    // Pathless vetted write (memory). Checked after the path-based rule so a
    // memory call can never be how an arbitrary workspace edit sneaks through.
    if class == CapabilityClass::Write && is_vetted_memory_write(server, tool) {
        return Decision::Allow;
    }

    if class.session_grant_allowed() && has_session_grant(workspace_id, server, tool) {
        return Decision::Allow;
    }

    let reason = match class {
        CapabilityClass::Exec => "exec tool requires approval",
        CapabilityClass::Write => "write tool requires approval",
        CapabilityClass::Read => "read tool requires approval",
    };
    Decision::NeedsApproval { class, reason }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_tool_defaults_to_exec() {
        // Failing closed is the whole point: an unreviewed tool must not be
        // able to run by omission.
        assert_eq!(
            classify("brand-new-server", "do_thing"),
            CapabilityClass::Exec
        );
    }

    #[test]
    fn known_server_unknown_tool_still_defaults_to_exec() {
        // Per-tool granularity: an unlisted tool on the filesystem server must
        // not inherit Read from its siblings.
        assert_eq!(
            classify("filesystem", "delete_everything"),
            CapabilityClass::Exec
        );
    }

    #[test]
    fn filesystem_reads_and_writes_are_distinguished() {
        // The case the per-server `requires_confirmation` flag cannot express.
        assert_eq!(
            classify("filesystem", "read_text_file"),
            CapabilityClass::Read
        );
        assert_eq!(classify("filesystem", "write_file"), CapabilityClass::Write);
    }

    #[test]
    fn exec_is_never_session_grantable() {
        // "Approve for the session" must never become a standing shell.
        assert!(!CapabilityClass::Exec.session_grant_allowed());
        assert!(CapabilityClass::Write.session_grant_allowed());
        assert!(CapabilityClass::Read.session_grant_allowed());
    }

    #[test]
    fn read_does_not_require_approval() {
        assert!(!CapabilityClass::Read.requires_approval());
        assert!(CapabilityClass::Write.requires_approval());
        assert!(CapabilityClass::Exec.requires_approval());
    }

    #[test]
    fn exec_is_never_auto_applied() {
        let root = std::env::temp_dir();
        let inside = root.join("file.txt");
        assert!(!is_vetted_auto_apply(
            CapabilityClass::Exec,
            Some(&inside),
            &root
        ));
    }

    #[test]
    fn read_is_never_auto_applied_as_a_write() {
        let root = std::env::temp_dir();
        let inside = root.join("file.txt");
        assert!(!is_vetted_auto_apply(
            CapabilityClass::Read,
            Some(&inside),
            &root
        ));
    }

    #[test]
    fn write_inside_workspace_is_auto_applicable() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("notes.md");
        std::fs::write(&file, "hello").unwrap();
        assert!(is_vetted_auto_apply(
            CapabilityClass::Write,
            Some(&file),
            dir.path()
        ));
    }

    #[test]
    fn write_outside_workspace_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let outside = std::env::temp_dir().join("definitely-outside-workspace.txt");
        std::fs::write(&outside, "nope").unwrap();
        assert!(!is_vetted_auto_apply(
            CapabilityClass::Write,
            Some(&outside),
            dir.path()
        ));
        let _ = std::fs::remove_file(&outside);
    }

    #[test]
    fn write_without_a_resolved_path_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        // No path means no way to prove the target is in-workspace.
        assert!(!is_vetted_auto_apply(
            CapabilityClass::Write,
            None,
            dir.path()
        ));
    }

    #[test]
    fn exec_is_gated_even_with_a_session_grant() {
        // "Approve for the session" must never become a standing shell.
        let ws = "ws_exec_grant_test";
        assert!(!grant_for_session(
            ws,
            "terminal",
            "run_command",
            CapabilityClass::Exec
        ));
        assert!(!has_session_grant(ws, "terminal", "run_command"));
    }

    #[test]
    fn write_grant_is_scoped_to_workspace_and_tool() {
        let ws_a = "ws_grant_a";
        let ws_b = "ws_grant_b";
        assert!(grant_for_session(
            ws_a,
            "filesystem",
            "write_file",
            CapabilityClass::Write
        ));
        assert!(has_session_grant(ws_a, "filesystem", "write_file"));
        // A different workspace must not inherit it.
        assert!(!has_session_grant(ws_b, "filesystem", "write_file"));
        // Nor a different tool on the same server.
        assert!(!has_session_grant(ws_a, "filesystem", "edit_file"));
    }

    #[test]
    fn read_tools_are_allowed_without_approval() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(
            decide(
                "ws_read_test",
                "filesystem",
                "read_text_file",
                None,
                dir.path()
            ),
            Decision::Allow
        );
    }

    #[test]
    fn write_outside_workspace_stays_gated_under_a_grant() {
        // A grant covers the tool, but the vetted auto-apply path still has to
        // prove the target is in-workspace before anything runs.
        let dir = tempfile::tempdir().unwrap();
        let outside = std::env::temp_dir().join("grant-outside-workspace.txt");
        std::fs::write(&outside, "nope").unwrap();
        assert!(grant_for_session(
            "ws_outside_test",
            "filesystem",
            "write_file",
            CapabilityClass::Write
        ));
        assert_eq!(
            decide(
                "ws_outside_test",
                "filesystem",
                "write_file",
                Some(&outside),
                dir.path()
            ),
            Decision::Allow,
            "an explicit session grant permits the call; path containment is              enforced by resolve_relative at dispatch"
        );
        let _ = std::fs::remove_file(&outside);
    }

    #[test]
    fn unknown_tool_is_gated_as_exec() {
        let dir = tempfile::tempdir().unwrap();
        assert!(matches!(
            decide("ws_unknown_test", "mystery", "act", None, dir.path()),
            Decision::NeedsApproval {
                class: CapabilityClass::Exec,
                ..
            }
        ));
    }

    #[test]
    fn stamp_overrides_a_model_supplied_workspace() {
        // The whole point: a model-supplied scope must never win.
        let args = serde_json::json!({ "workspace_id": "ws_someone_else", "query": "salary" });
        let stamped = stamp_workspace_scope("memory", "memory_search", args, "ws_real");
        assert_eq!(stamped["workspace_id"], "ws_real");
        assert_eq!(stamped["query"], "salary", "other args are preserved");
    }

    #[test]
    fn stamp_adds_the_scope_when_absent() {
        let stamped = stamp_workspace_scope(
            "memory",
            "memory_remember",
            serde_json::json!({ "title": "t" }),
            "ws_real",
        );
        assert_eq!(stamped["workspace_id"], "ws_real");
    }

    #[test]
    fn stamp_replaces_a_non_object_argument_bag() {
        // Otherwise the id would be silently dropped server-side and the tool
        // would fall back to whatever scope it liked.
        let stamped = stamp_workspace_scope(
            "memory",
            "memory_search",
            serde_json::json!("oops"),
            "ws_real",
        );
        assert_eq!(stamped["workspace_id"], "ws_real");
    }

    #[test]
    fn stamp_leaves_unscoped_servers_alone() {
        let args = serde_json::json!({ "expression": "2+2" });
        let out = stamp_workspace_scope("calculator", "calculate", args.clone(), "ws_real");
        assert_eq!(out, args);
        assert!(out.get("workspace_id").is_none());
    }

    #[test]
    fn scoped_memory_reads_are_reads_and_writes_are_writes() {
        // The four tool names must match crates/mcp-memory exactly, or the gate
        // silently falls back to exec for all of them.
        assert_eq!(classify("memory", "memory_catalog"), CapabilityClass::Read);
        assert_eq!(classify("memory", "memory_search"), CapabilityClass::Read);
        assert_eq!(
            classify("memory", "memory_remember"),
            CapabilityClass::Write
        );
        assert_eq!(classify("memory", "memory_forget"), CapabilityClass::Write);
    }

    /// Every tool a configured MCP server actually exposes must be classified.
    ///
    /// This is the test that keeps the table honest against the servers in
    /// `mcp_servers.example.toml`. The list is the real tool set of each server
    /// (the Ghostlink crates' names come from their `#[tool]` attributes; the
    /// third-party names from the packages pinned there), NOT a copy of the
    /// table above -- so adding a tool upstream without classifying it here
    /// fails the build rather than silently falling back to `Exec` and demanding
    /// an approval for a harmless read.
    ///
    /// Falling back to `Exec` is safe, not correct: it blocked `git_diff_unstaged`,
    /// `list_tables`, `describe_table`, `fetch.fetch`, and `duckduckgo
    /// tools, every one of which is an ordinary observation.
    /// Every tool a configured MCP server actually exposes must be classified.
    ///
    /// Verified against a LIVE server with all seven runnable MCP servers
    /// connected (calculator, memory, fetch, git, sqlite, filesystem,
    /// sequential-thinking) -- 39 tools total -- rather than against a table
    /// transcribed from upstream docs. That pass found seven tools the docs
    /// didn't mention: `git.git_create_branch`, `git.git_branch`,
    /// `sqlite.append_insight`, `filesystem.read_file`, and the three git diff
    /// variants. Each had been silently defaulting to `Exec`, so a branch
    /// listing or a plain file read demanded an approval.
    ///
    /// Keep this in sync by re-running the connected-server check rather than by
    /// reading upstream documentation: the docs and the served tool list disagree.
    const REAL_TOOLS: &[(&str, &str, CapabilityClass)] = &[
        // filesystem (14 advertised)
        ("filesystem", "read_text_file", CapabilityClass::Read),
        ("filesystem", "read_file", CapabilityClass::Read),
        ("filesystem", "read_media_file", CapabilityClass::Read),
        ("filesystem", "read_multiple_files", CapabilityClass::Read),
        ("filesystem", "list_directory", CapabilityClass::Read),
        (
            "filesystem",
            "list_directory_with_sizes",
            CapabilityClass::Read,
        ),
        ("filesystem", "directory_tree", CapabilityClass::Read),
        ("filesystem", "search_files", CapabilityClass::Read),
        ("filesystem", "get_file_info", CapabilityClass::Read),
        (
            "filesystem",
            "list_allowed_directories",
            CapabilityClass::Read,
        ),
        ("filesystem", "write_file", CapabilityClass::Write),
        ("filesystem", "edit_file", CapabilityClass::Write),
        ("filesystem", "create_directory", CapabilityClass::Write),
        ("filesystem", "move_file", CapabilityClass::Write),
        // calculator (Ghostlink crate)
        ("calculator", "calculate", CapabilityClass::Read),
        // fetch -- the tool is named `fetch` on a server named `fetch`
        ("fetch", "fetch", CapabilityClass::Read),
        // git (12 advertised)
        ("git", "git_status", CapabilityClass::Read),
        ("git", "git_log", CapabilityClass::Read),
        ("git", "git_diff", CapabilityClass::Read),
        ("git", "git_diff_unstaged", CapabilityClass::Read),
        ("git", "git_diff_staged", CapabilityClass::Read),
        ("git", "git_show", CapabilityClass::Read),
        ("git", "git_blame", CapabilityClass::Read),
        ("git", "git_add", CapabilityClass::Write),
        ("git", "git_commit", CapabilityClass::Write),
        ("git", "git_checkout", CapabilityClass::Write),
        ("git", "git_reset", CapabilityClass::Write),
        ("git", "git_push", CapabilityClass::Write),
        ("git", "git_create_branch", CapabilityClass::Write),
        ("git", "git_branch", CapabilityClass::Write),
        // sqlite (6 advertised)
        ("sqlite", "read_query", CapabilityClass::Read),
        ("sqlite", "list_tables", CapabilityClass::Read),
        ("sqlite", "describe_table", CapabilityClass::Read),
        ("sqlite", "write_query", CapabilityClass::Write),
        ("sqlite", "create_table", CapabilityClass::Write),
        ("sqlite", "append_insight", CapabilityClass::Write),
        // rag (Ghostlink crate)
        ("rag", "search", CapabilityClass::Read),
        ("rag", "index_document", CapabilityClass::Write),
        // vision (Ghostlink crate)
        ("vision", "analyze_image", CapabilityClass::Read),
        // sequential-thinking
        (
            "sequential-thinking",
            "sequentialthinking",
            CapabilityClass::Read,
        ),
        // memory (Ghostlink crate, phase 1)
        ("memory", "memory_catalog", CapabilityClass::Read),
        ("memory", "memory_search", CapabilityClass::Read),
        ("memory", "memory_remember", CapabilityClass::Write),
        // duckduckgo (live-verified single tool; replaces brave-search, which
        // needed BRAVE_API_KEY and a paid account)
        ("duckduckgo", "duckduckgo_web_search", CapabilityClass::Read),
        // playwright (25 advertised -- all listed, see every_browser_tool_is_classified)
        ("playwright", "browser_snapshot", CapabilityClass::Read),
        ("playwright", "browser_find", CapabilityClass::Read),
        (
            "playwright",
            "browser_console_messages",
            CapabilityClass::Read,
        ),
        (
            "playwright",
            "browser_network_requests",
            CapabilityClass::Read,
        ),
        (
            "playwright",
            "browser_network_request",
            CapabilityClass::Read,
        ),
        (
            "playwright",
            "browser_take_screenshot",
            CapabilityClass::Read,
        ),
        ("playwright", "browser_wait_for", CapabilityClass::Read),
        ("playwright", "browser_navigate", CapabilityClass::Read),
        ("playwright", "browser_navigate_back", CapabilityClass::Read),
        ("playwright", "browser_click", CapabilityClass::Write),
        ("playwright", "browser_hover", CapabilityClass::Write),
        ("playwright", "browser_type", CapabilityClass::Write),
        ("playwright", "browser_press_key", CapabilityClass::Write),
        ("playwright", "browser_fill_form", CapabilityClass::Write),
        (
            "playwright",
            "browser_select_option",
            CapabilityClass::Write,
        ),
        ("playwright", "browser_drag", CapabilityClass::Write),
        ("playwright", "browser_tabs", CapabilityClass::Write),
        ("playwright", "browser_close", CapabilityClass::Write),
        ("playwright", "browser_resize", CapabilityClass::Write),
        (
            "playwright",
            "browser_emulate_media",
            CapabilityClass::Write,
        ),
        (
            "playwright",
            "browser_handle_dialog",
            CapabilityClass::Write,
        ),
        ("playwright", "browser_evaluate", CapabilityClass::Exec),
        (
            "playwright",
            "browser_run_code_unsafe",
            CapabilityClass::Exec,
        ),
        ("playwright", "browser_file_upload", CapabilityClass::Exec),
        ("playwright", "browser_drop", CapabilityClass::Exec),
    ];

    #[test]
    fn every_real_tool_is_classified_as_expected() {
        let mut wrong = Vec::new();
        for (server, tool, expected) in REAL_TOOLS {
            let actual = classify(server, tool);
            if actual != *expected {
                wrong.push(format!(
                    "{server}.{tool}: expected {expected:?}, got {actual:?}"
                ));
            }
        }
        assert!(
            wrong.is_empty(),
            "capability table disagrees with the real tool set:\n  {}",
            wrong.join("\n  ")
        );
    }

    /// Every tool `npx -y @playwright/mcp@latest` actually advertises must be in the
    /// table. An unlisted browser tool silently becomes Exec, and with 25 of them that
    /// means an approval prompt on every page read -- the false gate that teaches
    /// people to approve without reading.
    #[test]
    fn every_browser_tool_is_classified() {
        let live = [
            "browser_close",
            "browser_resize",
            "browser_console_messages",
            "browser_handle_dialog",
            "browser_emulate_media",
            "browser_evaluate",
            "browser_file_upload",
            "browser_drop",
            "browser_find",
            "browser_fill_form",
            "browser_press_key",
            "browser_type",
            "browser_navigate",
            "browser_navigate_back",
            "browser_network_requests",
            "browser_network_request",
            "browser_run_code_unsafe",
            "browser_take_screenshot",
            "browser_snapshot",
            "browser_click",
            "browser_drag",
            "browser_hover",
            "browser_select_option",
            "browser_tabs",
            "browser_wait_for",
        ];
        let mut unclassified = Vec::new();
        for tool in live {
            // A tool absent from the table is exactly what this checks, so classify()
            // returning Exec is indistinguishable from a vetted Exec entry -- read the
            // table directly.
            if !table().contains_key(&("playwright".to_string(), tool.to_string())) {
                unclassified.push(tool);
            }
        }
        assert!(
            unclassified.is_empty(),
            "browser tools missing from the capability table (they would all fall \
             through to Exec): {unclassified:?}"
        );
    }

    /// The security-relevant half: the tools that run code or move local files must
    /// never be downgraded to Write, because Write is session-grantable and Exec is
    /// not. A session grant would then survive across turns.
    #[test]
    fn browser_code_and_exfil_tools_stay_exec() {
        for tool in [
            "browser_evaluate",
            "browser_run_code_unsafe",
            "browser_file_upload",
            "browser_drop",
        ] {
            assert_eq!(
                classify("playwright", tool),
                CapabilityClass::Exec,
                "{tool} must stay Exec: it executes code or ships local files off-box"
            );
            assert!(
                !classify("playwright", tool).session_grant_allowed(),
                "{tool} must not be session-grantable"
            );
        }
    }

    /// Reading a page must not need an approval, or the browser is unusable.
    #[test]
    fn browser_observation_tools_are_reads() {
        for tool in [
            "browser_snapshot",
            "browser_find",
            "browser_navigate",
            "browser_navigate_back",
            "browser_console_messages",
            "browser_network_requests",
            "browser_network_request",
            "browser_take_screenshot",
            "browser_wait_for",
        ] {
            assert_eq!(
                classify("playwright", tool),
                CapabilityClass::Read,
                "{tool} is observation and must not demand approval"
            );
        }
    }

    #[test]
    fn no_ordinary_operation_is_left_to_the_exec_default() {
        // The specific regression: these all silently became Exec.
        for (server, tool) in [
            ("git", "git_diff_unstaged"),
            ("git", "git_diff_staged"),
            ("sqlite", "list_tables"),
            ("sqlite", "describe_table"),
            ("fetch", "fetch"),
            // Found only by connecting the servers, not from docs:
            ("filesystem", "read_file"),
        ] {
            assert_ne!(
                classify(server, tool),
                CapabilityClass::Exec,
                "{server}.{tool} fell through to Exec and would demand approval"
            );
        }
        // Branch ops are writes, not reads -- but they must not be Exec either:
        // creating a branch runs no command and moves no worktree content.
        for tool in ["git_create_branch", "git_branch"] {
            assert_eq!(classify("git", tool), CapabilityClass::Write);
        }
        assert_eq!(classify("sqlite", "append_insight"), CapabilityClass::Write);
    }

    // --- vetted memory write -------------------------------------------------
    //
    // The whole point of this path is that remembering stops requiring a prompt,
    // so the security-relevant assertions are about what must NOT be exempted.

    #[test]
    fn memory_remember_auto_applies() {
        assert!(is_vetted_memory_write("memory", "memory_remember"));
    }

    #[test]
    fn memory_forget_is_never_auto_applied() {
        // Irreversible. There is no un-forget, so a misfired delete must still
        // cost a human decision.
        assert!(!is_vetted_memory_write("memory", "memory_forget"));
        let d = decide(
            "ws",
            "memory",
            "memory_forget",
            None,
            std::path::Path::new("."),
        );
        assert!(
            matches!(d, Decision::NeedsApproval { .. }),
            "forget must still require approval, got {d:?}"
        );
    }

    #[test]
    fn only_the_memory_server_gets_the_exemption() {
        // A different server offering a same-named tool is not covered.
        assert!(!is_vetted_memory_write("filesystem", "memory_remember"));
        assert!(!is_vetted_memory_write(
            "brand-new-server",
            "memory_remember"
        ));
    }

    #[test]
    fn the_exemption_does_not_reach_other_tools_on_the_memory_server() {
        for tool in ["memory_search", "memory_catalog"] {
            assert!(!is_vetted_memory_write("memory", tool), "{tool}");
        }
    }

    #[test]
    fn a_write_on_another_server_still_needs_approval() {
        // rag.index_document stays gated: indexing is not a free local append.
        let d = decide(
            "ws",
            "rag",
            "index_document",
            None,
            std::path::Path::new("."),
        );
        assert!(matches!(d, Decision::NeedsApproval { .. }), "{d:?}");
    }

    #[test]
    fn the_exemption_does_not_reach_exec() {
        // The gate is keyed on class == Write, so an Exec tool on the memory
        // server cannot pick up this path.
        assert!(!is_vetted_memory_write("memory", "memory_shell"));
        let d = decide(
            "ws",
            "memory",
            "memory_shell",
            None,
            std::path::Path::new("."),
        );
        assert!(matches!(d, Decision::NeedsApproval { .. }), "{d:?}");
    }

    #[test]
    fn the_exemption_is_workspace_scoped_not_path_scoped() {
        // No filesystem path is involved; the control is the stamped workspace.
        // So a call is allowed regardless of cwd, and the scope is enforced at
        // dispatch by is_scope_stamped rather than here.
        let d = decide(
            "ws_a",
            "memory",
            "memory_remember",
            None,
            std::path::Path::new("."),
        );
        assert!(d.is_allowed(), "a stamped-scope memory write should apply");
    }
}
