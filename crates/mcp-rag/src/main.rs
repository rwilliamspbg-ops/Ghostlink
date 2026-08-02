//! Minimal stdio MCP server exposing `index_document`/`search` retrieval tools
//! backed by a local Ollama embedding model (nomic-embed-text/...) and a
//! brute-force in-process cosine-similarity index persisted to a JSON file —
//! the real backend for Ghostlink chat's RAG addition. Stays local-model-first
//! like `mcp-vision`: no cloud embedding API, no external vector database (a
//! single local user's corpus is small enough that brute-force cosine search
//! over an in-memory `Vec` is simpler and sufficient — sqlite-vec/qdrant/faiss
//! would be solving a scale problem this tool doesn't have).

use rmcp::{
    handler::server::wrapper::Parameters, schemars, tool, tool_router, transport::stdio, ServiceExt,
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tokio::sync::Mutex;

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct IndexDocumentRequest {
    #[schemars(description = "The document text to index")]
    text: String,
    #[schemars(description = "Optional identifier for this document (defaults to a timestamp)")]
    id: Option<String>,
    #[schemars(
        description = "Optional source label (file path, URL, title, ...) shown in search results"
    )]
    source: Option<String>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct SearchRequest {
    #[schemars(description = "The search query")]
    query: String,
    #[schemars(description = "Number of results to return (default 5)")]
    top_k: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct IndexEntry {
    id: String,
    source: Option<String>,
    text: String,
    embedding: Vec<f32>,
    #[serde(default)]
    norm: f32,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct RagIndex {
    entries: Vec<IndexEntry>,
}

impl RagIndex {
    fn load(path: &std::path::Path) -> Self {
        let mut index: Self = std::fs::read_to_string(path)
            .ok()
            .and_then(|data| serde_json::from_str(&data).ok())
            .unwrap_or_default();
        for entry in &mut index.entries {
            if entry.norm == 0.0 && !entry.embedding.is_empty() {
                entry.norm = euclidean_norm(&entry.embedding);
            }
        }
        index
    }

    fn save(&self, path: &std::path::Path) -> std::io::Result<()> {
        let data = serde_json::to_string_pretty(self).unwrap_or_else(|_| "{}".to_string());
        std::fs::write(path, data)
    }
}

/// Removes any existing entries for `doc_id` (chunk ids are `"{doc_id}#N"`)
/// and appends `new_entries` in their place. Re-indexing the same document
/// (e.g. a file re-saved, or a workspace re-index revisiting an unchanged
/// path) must *replace* its prior chunks, not pile up duplicates alongside
/// them — otherwise every re-index grows the index and search() starts
/// returning multiple stale copies of the same source.
fn replace_document_entries(
    entries: &mut Vec<IndexEntry>,
    doc_id: &str,
    new_entries: Vec<IndexEntry>,
) {
    let prefix = format!("{doc_id}#");
    entries.retain(|e| !e.id.starts_with(&prefix));
    entries.extend(new_entries);
}

/// Splits `text` into paragraph-sized chunks (blank-line separated), further
/// breaking any paragraph longer than `max_chars` at word boundaries so a
/// single giant paragraph doesn't become one unwieldy embedding. Empty/
/// whitespace-only chunks are dropped.
fn chunk_text(text: &str, max_chars: usize) -> Vec<String> {
    let mut chunks = Vec::new();
    for paragraph in text.split("\n\n") {
        let paragraph = paragraph.trim();
        if paragraph.is_empty() {
            continue;
        }
        if paragraph.len() <= max_chars {
            chunks.push(paragraph.to_string());
            continue;
        }
        let mut current = String::new();
        for word in paragraph.split_whitespace() {
            if !current.is_empty() && current.len() + 1 + word.len() > max_chars {
                chunks.push(std::mem::take(&mut current));
            }
            if !current.is_empty() {
                current.push(' ');
            }
            current.push_str(word);
        }
        if !current.is_empty() {
            chunks.push(current);
        }
    }
    chunks
}

/// Helper to calculate Euclidean norm using loop-unrolled chunk processing via `chunks_exact(8)`.
/// This eliminates intermediate bounds checks and allows the compiler to leverage SIMD vectorization,
/// and breaks loop carry dependency chains by accumulating into multiple independent registers.
fn euclidean_norm(v: &[f32]) -> f32 {
    if v.is_empty() {
        return 0.0;
    }

    let mut sum0 = 0.0;
    let mut sum1 = 0.0;
    let mut sum2 = 0.0;
    let mut sum3 = 0.0;
    let mut sum4 = 0.0;
    let mut sum5 = 0.0;
    let mut sum6 = 0.0;
    let mut sum7 = 0.0;

    let chunks = v.chunks_exact(8);
    let remainder = chunks.remainder();

    for c in chunks {
        sum0 += c[0] * c[0];
        sum1 += c[1] * c[1];
        sum2 += c[2] * c[2];
        sum3 += c[3] * c[3];
        sum4 += c[4] * c[4];
        sum5 += c[5] * c[5];
        sum6 += c[6] * c[6];
        sum7 += c[7] * c[7];
    }

    let mut sum = sum0 + sum1 + sum2 + sum3 + sum4 + sum5 + sum6 + sum7;

    for &x in remainder {
        sum += x * x;
    }

    sum.sqrt()
}

/// Helper to calculate cosine similarity using precomputed norms for both embeddings.
/// This reduces the computation to a single-pass dot product loop with absolutely no sqrt or sum of squares.
fn cosine_similarity_precomputed(a: &[f32], b: &[f32], norm_a: f32, norm_b: f32) -> f32 {
    if a.len() != b.len() || a.is_empty() || norm_a == 0.0 || norm_b == 0.0 {
        return 0.0;
    }

    // Process elements in chunks of 8 to encourage SIMD auto-vectorization,
    // eliminate bounds checking overhead, and break dependencies by accumulating
    // into multiple independent registers.
    let mut dot0 = 0.0;
    let mut dot1 = 0.0;
    let mut dot2 = 0.0;
    let mut dot3 = 0.0;
    let mut dot4 = 0.0;
    let mut dot5 = 0.0;
    let mut dot6 = 0.0;
    let mut dot7 = 0.0;

    let chunks_a = a.chunks_exact(8);
    let chunks_b = b.chunks_exact(8);
    let remainder_a = chunks_a.remainder();
    let remainder_b = chunks_b.remainder();

    for (ca, cb) in chunks_a.zip(chunks_b) {
        dot0 += ca[0] * cb[0];
        dot1 += ca[1] * cb[1];
        dot2 += ca[2] * cb[2];
        dot3 += ca[3] * cb[3];
        dot4 += ca[4] * cb[4];
        dot5 += ca[5] * cb[5];
        dot6 += ca[6] * cb[6];
        dot7 += ca[7] * cb[7];
    }

    let mut dot = dot0 + dot1 + dot2 + dot3 + dot4 + dot5 + dot6 + dot7;

    for (&x, &y) in remainder_a.iter().zip(remainder_b) {
        dot += x * y;
    }

    dot / (norm_a * norm_b)
}

#[allow(dead_code)]
fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    let norm_a = euclidean_norm(a);
    let norm_b = euclidean_norm(b);
    cosine_similarity_precomputed(a, b, norm_a, norm_b)
}

/// Ranks `entries` against `query_embedding`, highest similarity first,
/// keeping the top `top_k`. Split out from the `search` tool method so it's
/// testable without a real Ollama embedding call.
fn rank<'a>(
    entries: &'a [IndexEntry],
    query_embedding: &[f32],
    top_k: usize,
) -> Vec<(f32, &'a IndexEntry)> {
    let norm_b = euclidean_norm(query_embedding);
    if norm_b == 0.0 {
        return Vec::new();
    }
    let mut scored: Vec<(f32, &IndexEntry)> = entries
        .iter()
        .map(|e| {
            (
                cosine_similarity_precomputed(&e.embedding, query_embedding, e.norm, norm_b),
                e,
            )
        })
        .collect();
    scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    scored.truncate(top_k);
    scored
}

#[derive(Debug, Clone)]
struct Rag {
    ollama_url: String,
    embed_model: String,
    client: reqwest::Client,
    index_path: PathBuf,
    index: std::sync::Arc<Mutex<RagIndex>>,
}

impl Rag {
    fn new() -> Self {
        let index_path = std::env::var("GHOSTLINK_RAG_INDEX_PATH")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("rag_index.json"));
        let index = RagIndex::load(&index_path);
        Self {
            ollama_url: std::env::var("OLLAMA_URL")
                .unwrap_or_else(|_| "http://127.0.0.1:11434".to_string()),
            embed_model: std::env::var("OLLAMA_EMBED_MODEL")
                .unwrap_or_else(|_| "nomic-embed-text".to_string()),
            client: reqwest::Client::new(),
            index_path,
            index: std::sync::Arc::new(Mutex::new(index)),
        }
    }

    async fn embed(&self, text: &str) -> Result<Vec<f32>, String> {
        let url = format!("{}/api/embeddings", self.ollama_url.trim_end_matches('/'));
        let payload = serde_json::json!({ "model": self.embed_model, "prompt": text });
        let resp = self
            .client
            .post(&url)
            .json(&payload)
            .send()
            .await
            .map_err(|err| {
                format!(
                    "failed to reach Ollama at {url}: {err} (is `ollama serve` running, and has `{}` been pulled?)",
                    self.embed_model
                )
            })?;
        let body: serde_json::Value = resp
            .json()
            .await
            .map_err(|err| format!("invalid JSON from Ollama: {err}"))?;
        let embedding = body
            .get("embedding")
            .and_then(|v| v.as_array())
            .ok_or_else(|| "error: no 'embedding' field in Ollama's reply".to_string())?
            .iter()
            .filter_map(|v| v.as_f64().map(|f| f as f32))
            .collect();
        Ok(embedding)
    }
}

const MAX_CHUNK_CHARS: usize = 1200;

#[tool_router(server_handler)]
impl Rag {
    #[tool(
        description = "Chunk and embed a document into the local retrieval index so it can later be found by search()"
    )]
    async fn index_document(
        &self,
        Parameters(IndexDocumentRequest { text, id, source }): Parameters<IndexDocumentRequest>,
    ) -> String {
        let chunks = chunk_text(&text, MAX_CHUNK_CHARS);
        if chunks.is_empty() {
            return "error: no non-empty text to index".to_string();
        }
        let doc_id = id.unwrap_or_else(|| {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| format!("doc-{}", d.as_millis()))
                .unwrap_or_else(|_| "doc".to_string())
        });

        // Embed every chunk into a scratch Vec first — only replace the
        // document's prior entries once all of them succeed, so a
        // mid-document embed failure (Ollama dropping mid-loop) leaves the
        // previous good index untouched instead of half-deleted.
        let mut new_entries = Vec::with_capacity(chunks.len());
        for (i, chunk) in chunks.iter().enumerate() {
            let embedding = match self.embed(chunk).await {
                Ok(e) => e,
                Err(err) => return err,
            };
            let norm = euclidean_norm(&embedding);
            new_entries.push(IndexEntry {
                id: format!("{doc_id}#{i}"),
                source: source.clone(),
                text: chunk.clone(),
                embedding,
                norm,
            });
        }

        let indexed = new_entries.len();
        let mut index = self.index.lock().await;
        replace_document_entries(&mut index.entries, &doc_id, new_entries);
        if let Err(err) = index.save(&self.index_path) {
            return format!(
                "indexed {indexed} chunk(s) from '{doc_id}' but failed to persist index: {err}"
            );
        }
        format!(
            "indexed {indexed} chunk(s) from '{doc_id}' ({} total chunks now in index)",
            index.entries.len()
        )
    }

    #[tool(description = "Search the local retrieval index for text relevant to a query")]
    async fn search(
        &self,
        Parameters(SearchRequest { query, top_k }): Parameters<SearchRequest>,
    ) -> String {
        let query_embedding = match self.embed(&query).await {
            Ok(e) => e,
            Err(err) => return err,
        };
        let index = self.index.lock().await;
        if index.entries.is_empty() {
            return "error: the retrieval index is empty — call index_document first".to_string();
        }
        let results = rank(&index.entries, &query_embedding, top_k.unwrap_or(5));
        let results_json: Vec<serde_json::Value> = results
            .iter()
            .map(|(score, entry)| {
                serde_json::json!({
                    "score": score,
                    "source": entry.source,
                    "text": entry.text,
                })
            })
            .collect();
        serde_json::to_string(&results_json)
            .unwrap_or_else(|_| "error: failed to serialize results".to_string())
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::INFO.into()),
        )
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .init();

    let service = Rag::new().serve(stdio()).await.inspect_err(|err| {
        tracing::error!("mcp-rag serving error: {err:?}");
    })?;

    service.waiting().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunk_text_splits_on_blank_lines_and_drops_empties() {
        let text = "First paragraph.\n\n\nSecond paragraph.\n\n   \n\nThird.";
        let chunks = chunk_text(text, 1200);
        assert_eq!(
            chunks,
            vec!["First paragraph.", "Second paragraph.", "Third."]
        );
    }

    #[test]
    fn chunk_text_breaks_long_paragraphs_at_word_boundaries_under_max_chars() {
        let long_word_run = "word ".repeat(500); // ~2500 chars, one "paragraph"
        let chunks = chunk_text(&long_word_run, 100);
        assert!(
            chunks.len() > 1,
            "expected multiple chunks, got {}",
            chunks.len()
        );
        for chunk in &chunks {
            assert!(
                chunk.len() <= 100,
                "chunk exceeded max_chars: {} chars",
                chunk.len()
            );
            // Never split mid-word — every chunk is whole "word " repeats.
            assert!(chunk.split_whitespace().all(|w| w == "word"));
        }
    }

    #[test]
    fn chunk_text_ignores_all_whitespace_input() {
        assert!(chunk_text("   \n\n  \n\n ", 1200).is_empty());
    }

    #[test]
    fn cosine_similarity_identical_vectors_is_one() {
        let v = vec![1.0, 2.0, 3.0];
        assert!((cosine_similarity(&v, &v) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn cosine_similarity_orthogonal_vectors_is_zero() {
        assert!((cosine_similarity(&[1.0, 0.0], &[0.0, 1.0])).abs() < 1e-6);
    }

    #[test]
    fn cosine_similarity_opposite_vectors_is_negative_one() {
        let v = vec![1.0, 2.0, 3.0];
        let neg: Vec<f32> = v.iter().map(|x| -x).collect();
        assert!((cosine_similarity(&v, &neg) + 1.0).abs() < 1e-6);
    }

    #[test]
    fn cosine_similarity_mismatched_lengths_or_zero_vector_is_zero() {
        assert_eq!(cosine_similarity(&[1.0, 2.0], &[1.0]), 0.0);
        assert_eq!(cosine_similarity(&[0.0, 0.0], &[1.0, 1.0]), 0.0);
    }

    fn entry(id: &str, embedding: Vec<f32>) -> IndexEntry {
        let norm = euclidean_norm(&embedding);
        IndexEntry {
            id: id.to_string(),
            source: Some("test".to_string()),
            text: format!("text for {id}"),
            embedding,
            norm,
        }
    }

    #[test]
    fn rank_orders_by_similarity_descending_and_respects_top_k() {
        let entries = vec![
            entry("far", vec![0.0, 1.0]),
            entry("exact", vec![1.0, 0.0]),
            entry("close", vec![0.9, 0.1]),
        ];
        let ranked = rank(&entries, &[1.0, 0.0], 2);
        assert_eq!(ranked.len(), 2);
        assert_eq!(ranked[0].1.id, "exact");
        assert_eq!(ranked[1].1.id, "close");
        assert!(ranked[0].0 >= ranked[1].0);
    }

    #[test]
    fn replace_document_entries_replaces_only_the_matching_doc_id() {
        let mut entries = vec![
            entry("readme.md#0", vec![0.1, 0.2]),
            entry("readme.md#1", vec![0.3, 0.4]),
            entry("other.md#0", vec![0.5, 0.6]),
        ];
        let fresh = vec![entry("readme.md#0", vec![0.9, 0.9])];
        replace_document_entries(&mut entries, "readme.md", fresh);

        assert_eq!(
            entries.len(),
            2,
            "old readme.md chunks dropped, other.md untouched"
        );
        assert!(entries.iter().any(|e| e.id == "other.md#0"));
        let readme_entries: Vec<_> = entries
            .iter()
            .filter(|e| e.id.starts_with("readme.md#"))
            .collect();
        assert_eq!(
            readme_entries.len(),
            1,
            "re-indexing must not leave stale duplicate chunks"
        );
        assert_eq!(readme_entries[0].embedding, vec![0.9, 0.9]);
    }

    #[test]
    fn replace_document_entries_does_not_match_on_prefix_alone() {
        // "readme.md-old" must not be treated as a chunk of "readme.md" just
        // because it shares a string prefix — only the literal "#"-delimited
        // doc_id boundary counts.
        let mut entries = vec![entry("readme.md-old#0", vec![0.1, 0.2])];
        replace_document_entries(
            &mut entries,
            "readme.md",
            vec![entry("readme.md#0", vec![0.5, 0.5])],
        );
        assert_eq!(entries.len(), 2);
    }

    #[test]
    fn index_persists_and_loads_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("rag_index.json");

        let mut index = RagIndex::default();
        index.entries.push(entry("a", vec![0.1, 0.2, 0.3]));
        index.entries.push(entry("b", vec![0.4, 0.5, 0.6]));
        index.save(&path).unwrap();

        let loaded = RagIndex::load(&path);
        assert_eq!(loaded.entries.len(), 2);
        assert_eq!(loaded.entries[0].id, "a");
        assert_eq!(loaded.entries[1].embedding, vec![0.4, 0.5, 0.6]);
    }

    #[test]
    fn index_load_missing_or_corrupt_file_returns_empty_index() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("does-not-exist.json");
        assert!(RagIndex::load(&missing).entries.is_empty());

        let corrupt = dir.path().join("corrupt.json");
        std::fs::write(&corrupt, "not valid json").unwrap();
        assert!(RagIndex::load(&corrupt).entries.is_empty());
    }
}
