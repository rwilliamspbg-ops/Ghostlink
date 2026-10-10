//! The durable memory store: schema, scoping rules, and ranking.
//!
//! Kept separate from the MCP tool surface in `main.rs` so the invariants can be
//! tested directly, without standing up a server or an embedding model.
//!
//! Scoping is the load-bearing rule here. Every read and write is filtered by
//! `workspace_id`, and the caller supplies that id from `ghost-link`, which
//! stamps it at dispatch time — the model never chooses its own scope. A memory
//! written in workspace A is invisible to a chat bound to workspace B.

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::Connection;

/// What a memory is for. Drives catalog grouping and ranking weight.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryKind {
    Preference,
    ProjectFact,
    Decision,
    Person,
    OpenLoop,
    Summary,
}

impl MemoryKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Preference => "preference",
            Self::ProjectFact => "project_fact",
            Self::Decision => "decision",
            Self::Person => "person",
            Self::OpenLoop => "open_loop",
            Self::Summary => "summary",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "preference" => Some(Self::Preference),
            "project_fact" => Some(Self::ProjectFact),
            "decision" => Some(Self::Decision),
            "person" => Some(Self::Person),
            "open_loop" => Some(Self::OpenLoop),
            "summary" => Some(Self::Summary),
            _ => None,
        }
    }
}

/// Where a memory came from. A compaction card the user accepted is not the
/// same provenance as something the model wrote on its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemorySource {
    /// Stated directly by the user.
    User,
    /// Produced by summarizing dropped conversation turns.
    Compaction,
    /// Written by a tool during a turn.
    Tool,
}

impl MemorySource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Compaction => "compaction",
            Self::Tool => "tool",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "user" => Some(Self::User),
            "compaction" => Some(Self::Compaction),
            "tool" => Some(Self::Tool),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Memory {
    pub id: String,
    pub workspace_id: String,
    pub kind: MemoryKind,
    pub title: String,
    pub body: String,
    pub source: MemorySource,
    pub created_at: i64,
    pub updated_at: i64,
    pub pinned: bool,
    pub shared: bool,
}

/// A catalog entry: titles and scopes only, never bodies.
///
/// The split exists so the system prompt can carry the shape of a workspace's
/// memory without carrying its contents — see the explicit-memory invariant.
#[derive(Debug, Clone, PartialEq)]
pub struct CatalogEntry {
    pub id: String,
    pub kind: MemoryKind,
    pub title: String,
    pub pinned: bool,
    pub shared: bool,
    pub updated_at: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SearchHit {
    pub memory: Memory,
    pub score: f64,
}

pub fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Opens (or creates) the memory database and applies the schema.
///
/// `workspace_id` in the path is not a security boundary — SQLite has no idea
/// what a workspace is. It exists so two workspaces pointed at the same data dir
/// get separate files rather than silently sharing rows. The real isolation is
/// the `WHERE workspace_id = ?` on every statement below.
pub fn open(path: &Path) -> Result<Connection, String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("creating memory dir {}: {e}", parent.display()))?;
    }
    let conn = Connection::open(path).map_err(|e| format!("opening memory db: {e}"))?;
    migrate(&conn)?;
    Ok(conn)
}

fn migrate(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(
        "PRAGMA journal_mode=WAL;
         PRAGMA foreign_keys=ON;
         CREATE TABLE IF NOT EXISTS memories (
             id           TEXT PRIMARY KEY,
             workspace_id TEXT NOT NULL,
             kind         TEXT NOT NULL,
             title        TEXT NOT NULL,
             body         TEXT NOT NULL,
             source       TEXT NOT NULL,
             created_at   INTEGER NOT NULL,
             updated_at   INTEGER NOT NULL,
             pinned       INTEGER NOT NULL DEFAULT 0,
             shared       INTEGER NOT NULL DEFAULT 0
         );
         CREATE INDEX IF NOT EXISTS idx_memories_ws_kind
             ON memories(workspace_id, kind);
         CREATE INDEX IF NOT EXISTS idx_memories_ws_updated
             ON memories(workspace_id, updated_at DESC);",
    )
    .map_err(|e| format!("migrating memory schema: {e}"))
}

fn row_to_memory(row: &rusqlite::Row<'_>) -> rusqlite::Result<Memory> {
    let kind_raw: String = row.get(2)?;
    let source_raw: String = row.get(5)?;
    // An unknown kind/source in an existing row degrades to the safest reading
    // rather than dropping the row — losing a memory because a future version
    // added a kind would be worse than mislabeling it.
    Ok(Memory {
        id: row.get(0)?,
        workspace_id: row.get(1)?,
        kind: MemoryKind::parse(&kind_raw).unwrap_or(MemoryKind::ProjectFact),
        title: row.get(3)?,
        body: row.get(4)?,
        source: MemorySource::parse(&source_raw).unwrap_or(MemorySource::Tool),
        created_at: row.get(6)?,
        updated_at: row.get(7)?,
        pinned: row.get(8)?,
        shared: row.get(9)?,
    })
}

const SELECT_COLS: &str =
    "id, workspace_id, kind, title, body, source, created_at, updated_at, pinned, shared";

/// Lists a workspace's memory titles. Bodies are never returned.
///
/// `workspace_shared` is the set of workspace ids whose `shared` (pinned)
/// collections this workspace may also read — a workspace may pin a collection
/// visible to others, but never the reverse by accident.
pub fn catalog(
    conn: &Connection,
    workspace_id: &str,
    workspace_shared: &[String],
) -> Result<Vec<CatalogEntry>, String> {
    let sql = format!(
        "SELECT {SELECT_COLS} FROM memories
         WHERE workspace_id = ? OR (shared = 1 AND workspace_id IN ({}))
         ORDER BY pinned DESC, updated_at DESC",
        placeholders(workspace_shared.len())
    );
    let mut stmt = conn
        .prepare(&sql)
        .map_err(|e| format!("preparing catalog: {e}"))?;

    let mut args: Vec<&dyn rusqlite::ToSql> = Vec::with_capacity(1 + workspace_shared.len());
    args.push(&workspace_id);
    for id in workspace_shared {
        args.push(id);
    }

    let rows = stmt
        .query_map(args.as_slice(), |row| {
            let m = row_to_memory(row)?;
            Ok(CatalogEntry {
                id: m.id,
                kind: m.kind,
                title: m.title,
                pinned: m.pinned,
                shared: m.shared,
                updated_at: m.updated_at,
            })
        })
        .map_err(|e| format!("reading catalog: {e}"))?;

    let mut out = Vec::new();
    for r in rows {
        out.push(r.map_err(|e| format!("reading catalog row: {e}"))?);
    }
    Ok(out)
}

fn placeholders(n: usize) -> String {
    (0..n)
        .map(|_| "?".to_string())
        .collect::<Vec<_>>()
        .join(",")
}

/// Scores a candidate against the query terms.
///
/// Deliberately not an embedding model: a single local user's memories are a
/// few hundred rows, and a keyword score is explainable, needs no model pull, and
/// can't leak the query to a network service. Weighted so an exact title hit
/// outranks a body mention, and a pinned memory outranks an equal-textual one.
pub fn score(memory: &Memory, query_terms: &[String]) -> f64 {
    if query_terms.is_empty() {
        return 0.0;
    }
    let title = memory.title.to_lowercase();
    let body = memory.body.to_lowercase();
    let mut total = 0.0;
    for term in query_terms {
        if title.contains(term.as_str()) {
            total += 3.0;
        }
        if body.contains(term.as_str()) {
            total += 1.0;
        }
    }
    if memory.pinned {
        total *= 1.5;
    }
    total
}

/// Ranks a workspace's memories against `query`, best first.
///
/// Only bodies for the returned hits are populated — the caller never sees a
/// body it did not ask for, which is what keeps the explicit-memory invariant
/// enforceable rather than aspirational.
pub fn search(
    conn: &Connection,
    workspace_id: &str,
    workspace_shared: &[String],
    query: &str,
    kinds: &[MemoryKind],
    limit: usize,
) -> Result<Vec<SearchHit>, String> {
    let terms: Vec<String> = tokenize(query);
    let candidates = if kinds.is_empty() {
        all_in_scope(conn, workspace_id, workspace_shared)?
    } else {
        let mut out = Vec::new();
        for kind in kinds {
            out.extend(in_scope_of_kind(
                conn,
                workspace_id,
                workspace_shared,
                *kind,
            )?);
        }
        out
    };

    let mut hits: Vec<SearchHit> = candidates
        .into_iter()
        .filter_map(|memory| {
            let score = score(&memory, &terms);
            // A zero score means "did not match" — returning every memory when
            // the query shares no terms with any of them would defeat the point
            // of an explicit search.
            (score > 0.0).then_some(SearchHit { memory, score })
        })
        .collect();

    // Total ordering: score desc, then most recently updated, then id.
    // OPTIMIZATION: Unstable sorting avoids scratch buffer allocations and stability tracking overhead.
    hits.sort_unstable_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(b.memory.updated_at.cmp(&a.memory.updated_at))
            .then(a.memory.id.cmp(&b.memory.id))
    });
    hits.truncate(limit);
    Ok(hits)
}

fn all_in_scope(
    conn: &Connection,
    workspace_id: &str,
    workspace_shared: &[String],
) -> Result<Vec<Memory>, String> {
    let sql = format!(
        "SELECT {SELECT_COLS} FROM memories
         WHERE workspace_id = ? OR (shared = 1 AND workspace_id IN ({}))",
        placeholders(workspace_shared.len())
    );
    query_scope(conn, &sql, &[], workspace_id, workspace_shared)
}

fn in_scope_of_kind(
    conn: &Connection,
    workspace_id: &str,
    workspace_shared: &[String],
    kind: MemoryKind,
) -> Result<Vec<Memory>, String> {
    // The kind is bound positionally *before* the shared-id list so the `?`
    // numbering in the SQL matches the order args are pushed.
    let sql = format!(
        "SELECT {SELECT_COLS} FROM memories
         WHERE kind = ?
           AND (workspace_id = ? OR (shared = 1 AND workspace_id IN ({})))",
        placeholders(workspace_shared.len())
    );
    query_scope(conn, &sql, &[kind.as_str()], workspace_id, workspace_shared)
}

/// Runs a scoped `SELECT`, binding `leading` args first, then the workspace id,
/// then any granted shared-workspace ids — the order the SQL's `?` markers are
/// written in.
fn query_scope(
    conn: &Connection,
    sql: &str,
    leading: &[&str],
    workspace_id: &str,
    workspace_shared: &[String],
) -> Result<Vec<Memory>, String> {
    let mut stmt = conn
        .prepare(sql)
        .map_err(|e| format!("preparing query: {e}"))?;
    let mut args: Vec<&dyn rusqlite::ToSql> =
        Vec::with_capacity(leading.len() + 1 + workspace_shared.len());
    for value in leading {
        args.push(value);
    }
    args.push(&workspace_id);
    for id in workspace_shared {
        args.push(id);
    }
    let rows = stmt
        .query_map(args.as_slice(), row_to_memory)
        .map_err(|e| format!("executing query: {e}"))?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r.map_err(|e| format!("reading row: {e}"))?);
    }
    Ok(out)
}

/// Lowercases and splits a query into terms, dropping short noise words.
pub fn tokenize(query: &str) -> Vec<String> {
    const STOPWORDS: [&str; 9] = ["the", "a", "an", "is", "are", "was", "of", "to", "and"];
    query
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| !t.is_empty())
        .map(|t| t.to_lowercase())
        .filter(|t| t.len() > 2 && !STOPWORDS.contains(&t.as_str()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mem(conn: &Connection, ws: &str, kind: MemoryKind, title: &str, body: &str) -> String {
        let id = format!("{ws}-{title}");
        let now = now_secs();
        conn.execute(
            "INSERT INTO memories (id, workspace_id, kind, title, body, source, created_at, updated_at, pinned, shared)
             VALUES (?1, ?2, ?3, ?4, ?5, 'user', ?6, ?6, 0, 0)",
            rusqlite::params![id, ws, kind.as_str(), title, body, now],
        )
        .unwrap();
        id
    }

    fn conn() -> (tempfile::TempDir, Connection) {
        let dir = tempfile::tempdir().unwrap();
        let conn = open(&dir.path().join("memories.db")).unwrap();
        (dir, conn)
    }

    #[test]
    fn catalog_returns_titles_without_bodies() {
        let (_d, conn) = conn();
        mem(
            &conn,
            "ws_a",
            MemoryKind::Decision,
            "use sqlite",
            "bodies stay out",
        );
        let cat = catalog(&conn, "ws_a", &[]).unwrap();
        assert_eq!(cat.len(), 1);
        assert_eq!(cat[0].title, "use sqlite");
        // CatalogEntry has no body field at all — the type enforces the split.
    }

    #[test]
    fn catalog_is_scoped_to_workspace() {
        let (_d, conn) = conn();
        mem(&conn, "ws_a", MemoryKind::ProjectFact, "a fact", "body a");
        mem(&conn, "ws_b", MemoryKind::ProjectFact, "b fact", "body b");
        let cat_a = catalog(&conn, "ws_a", &[]).unwrap();
        assert_eq!(cat_a.len(), 1);
        assert_eq!(cat_a[0].title, "a fact");
    }

    #[test]
    fn shared_collection_is_visible_across_workspaces() {
        let (_d, conn) = conn();
        conn.execute(
            "INSERT INTO memories (id, workspace_id, kind, title, body, source, created_at, updated_at, pinned, shared)
             VALUES ('s1', 'ws_shared', 'project_fact', 'shared note', 'body', 'user', 1, 1, 1, 1)",
            [],
        )
        .unwrap();
        // ws_a is granted access to ws_shared's pinned collection.
        let cat = catalog(&conn, "ws_a", &["ws_shared".to_string()]).unwrap();
        assert_eq!(cat.len(), 1);
        // Without the grant, nothing leaks.
        let ungranted = catalog(&conn, "ws_a", &[]).unwrap();
        assert!(ungranted.is_empty());
    }

    #[test]
    fn unshared_memory_never_leaks_across_workspaces() {
        let (_d, conn) = conn();
        mem(&conn, "ws_a", MemoryKind::ProjectFact, "secret", "body");
        let cat = catalog(&conn, "ws_b", &[]).unwrap();
        assert!(cat.is_empty());
        let hits = search(&conn, "ws_b", &[], "secret", &[], 10).unwrap();
        assert!(hits.is_empty());
    }

    #[test]
    fn search_ranks_title_hit_above_body_hit() {
        let (_d, conn) = conn();
        mem(
            &conn,
            "ws",
            MemoryKind::ProjectFact,
            "sqlite migration",
            "unrelated",
        );
        mem(
            &conn,
            "ws",
            MemoryKind::ProjectFact,
            "unrelated title",
            "mentions sqlite once",
        );
        let hits = search(&conn, "ws", &[], "sqlite", &[], 10).unwrap();
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].memory.title, "sqlite migration");
    }

    #[test]
    fn search_returns_nothing_when_nothing_matches() {
        let (_d, conn) = conn();
        mem(&conn, "ws", MemoryKind::ProjectFact, "alpha", "body");
        let hits = search(&conn, "ws", &[], "zzzz", &[], 10).unwrap();
        assert!(
            hits.is_empty(),
            "a non-matching query must not return the whole store"
        );
    }

    #[test]
    fn search_can_filter_by_kind() {
        let (_d, conn) = conn();
        mem(
            &conn,
            "ws",
            MemoryKind::Decision,
            "deploy friday",
            "ship it",
        );
        mem(
            &conn,
            "ws",
            MemoryKind::Preference,
            "deploy early",
            "prefer mornings",
        );
        let hits = search(&conn, "ws", &[], "deploy", &[MemoryKind::Decision], 10).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].memory.kind, MemoryKind::Decision);
    }

    #[test]
    fn pinned_memory_outranks_equal_textual_match() {
        let (_d, conn) = conn();
        mem(
            &conn,
            "ws",
            MemoryKind::ProjectFact,
            "note",
            "shared term here",
        );
        conn.execute(
            "INSERT INTO memories (id, workspace_id, kind, title, body, source, created_at, updated_at, pinned, shared)
             VALUES ('p1', 'ws', 'project_fact', 'pinned', 'shared term here', 'user', 1, 1, 1, 0)",
            [],
        )
        .unwrap();
        let hits = search(&conn, "ws", &[], "shared", &[], 10).unwrap();
        assert_eq!(hits[0].memory.id, "p1");
    }

    #[test]
    fn tokenizer_drops_stopwords_and_short_tokens() {
        let terms = tokenize("The a is of deployment to kubernetes");
        assert!(terms.contains(&"deployment".to_string()));
        assert!(terms.contains(&"kubernetes".to_string()));
        assert!(!terms.contains(&"the".to_string()));
        assert!(!terms.contains(&"is".to_string()));
    }

    #[test]
    fn unknown_kind_in_a_row_degrades_instead_of_dropping_it() {
        let (_d, conn) = conn();
        conn.execute(
            "INSERT INTO memories (id, workspace_id, kind, title, body, source, created_at, updated_at, pinned, shared)
             VALUES ('x', 'ws', 'kind_from_the_future', 't', 'b', 'tool', 1, 1, 0, 0)",
            [],
        )
        .unwrap();
        let cat = catalog(&conn, "ws", &[]).unwrap();
        assert_eq!(
            cat.len(),
            1,
            "an unrecognized kind must not make the row vanish"
        );
    }

    #[test]
    fn schema_is_idempotent_across_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("memories.db");
        {
            let conn = open(&path).unwrap();
            mem(&conn, "ws", MemoryKind::Preference, "keep", "body");
        }
        // Reopening runs migrate() again; it must not fail on existing tables.
        let conn = open(&path).unwrap();
        assert_eq!(catalog(&conn, "ws", &[]).unwrap().len(), 1);
    }
}
