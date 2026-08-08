# mcp-rag

Local stdio [MCP](https://modelcontextprotocol.io/) server exposing `index_document` and
`search` retrieval tools for [Ghostlink](https://github.com/rwilliamspbg-ops/Ghostlink)
chat's RAG (retrieval-augmented generation) tool — backed by a local Ollama embedding
model and an in-process cosine-similarity index, no external vector database.

**Not published to crates.io** (`publish = false`) — an internal component of the main
Ghostlink repository, spawned as a child process over stdio by the Ghostlink chat runtime.

## What it does

- **`index_document(path, ...)`** — chunks a document (splitting on blank lines, then
  breaking long paragraphs at word boundaries under a max-chars limit) and stores each
  chunk's embedding (via a local Ollama embedding model over HTTP) in a persisted local
  index.
- **`search(query, top_k)`** — embeds the query the same way, ranks indexed chunks by
  cosine similarity, and returns the top matches.

The index is a flat in-process store with a real persist/load round-trip to disk (no
external vector DB dependency) — appropriate for the scale of documents a single chat
session's RAG tool actually needs, not a general-purpose retrieval system.

## Running standalone (for testing)

```bash
cargo run -p mcp-rag
```

Requires a local Ollama instance running an embedding model. Speaks MCP over stdio — use
an MCP-aware client or Ghostlink's own chat tool-call loop
(`mcp_servers.toml` in the main repo registers it).

## Testing

```bash
cargo test -p mcp-rag
```

Covers chunking behavior (blank-line splitting, word-boundary breaking, whitespace-only
input) and the index persist/load round-trip without requiring a live Ollama instance.

## More

See the [main repository](https://github.com/rwilliamspbg-ops/Ghostlink) — this crate is
one of three local MCP tool servers alongside
[`mcp-calculator`](https://github.com/rwilliamspbg-ops/Ghostlink/tree/main/crates/mcp-calculator)
and
[`mcp-vision`](https://github.com/rwilliamspbg-ops/Ghostlink/tree/main/crates/mcp-vision).

## License

MIT — see [`LICENSE`](https://github.com/rwilliamspbg-ops/Ghostlink/blob/main/LICENSE) in
the main repository.
