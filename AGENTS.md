# Ghostlink State — July 30, 2026 (Session 7)

## Last Commit
`HEAD` — "feat(gui): Editor tab with copilot features (Explain/Fix/Refactor, multi-file refactor, ghost-text autocomplete, repo-aware RAG context)"

## What Was Done (Session 7 — July 30)

### Phase 1: Editor Tab Core (`ghostlink_gui_modern/src/components/EditorTab.tsx`)
- New Monaco-based Editor tab (registered as GUI tab 8) backed by three new
  backend routes in `crates/ghost-link/src/main.rs`:
  `GET /api/workspace/tree`, `GET`/`PUT /api/workspace/file` — confined to a
  canonicalized `GHOSTLINK_WORKSPACE_ROOT` with a path-traversal guard
  (verified against real `../` escape attempts on read, tree, and write).
- Monaco self-hosted locally (no CDN) via `vite-plugin-monaco-editor-esm` +
  `monacoSetup.ts` — consistent with the rest of the app's local-first
  stance.
- Fixed a real Monaco+flex layout bug found during verification: Monaco's
  own `automaticLayout` locked onto a bogus 5×5px size in this nested-flex
  pane and never self-corrected; fixed with an explicit `.layout()` call on
  mount plus a `ResizeObserver` on a container we control.

### Phase 2: Copilot Features
- **Explain / Fix / Refactor** — scoped to selection or whole file;
  Fix/Refactor propose changes via a side-by-side `DiffEditor` with explicit
  Accept/Reject (nothing writes until accepted).
- **Multi-file refactor** — tree checkboxes select files, one batched
  prompt (`### FILE: <path>` sections), sequential per-file diff review
  queue (Accept/Reject/Skip).
- **Ghost-text autocomplete** (opt-in toggle) — Monaco's native
  inline-completions provider, debounced against the existing chat
  endpoint. MVP: no FIM model support, no suffix awareness.
- **Repo-aware chat context**: `POST /api/workspace/index` feeds the
  workspace into the `rag` MCP server's `index_document` tool directly
  (not via an LLM tool-calling loop). Triggered once per page load from the
  Editor tab; `"skipped"` (not an error) when `rag`/Ollama isn't reachable.

### Phase 3: RAG / MCP Fixes
- **`rag` MCP server enabled by default** (`mcp_servers.example.toml`) —
  needs `ollama pull nomic-embed-text` to do anything.
- **Fixed unbounded duplicate growth in `mcp-rag`**: `index_document` only
  ever appended chunks, never removed a document's prior ones — every
  re-index (which happens every page load) grew `rag_index.json` forever.
  Now replaces by doc-id prefix before inserting. Verified live: indexed a
  directory (10 chunks), re-indexed, still exactly 10.
- **Hardened the "is rag usable" check**: `rag`'s own MCP handshake never
  touches Ollama, so it reports "connected" even with Ollama down. Added a
  direct `OllamaClient::health()` probe before the indexing loop so a
  native-only machine with no Ollama still gets the clean `"skipped"`
  response instead of a wall of per-file failures.

### Phase 4: Real Security Audit Log
- `/api/security/audit-log` was a hardcoded stub (always empty). Now
  records failed auth, JWT refresh, PQC enable, and tool-call approve/deny
  in-memory (capped at 500, most-recent-first). Required adding
  `ConnectInfo<SocketAddr>` + `State` extraction to `auth_middleware` (via
  `middleware::from_fn_with_state`) to log the client IP on auth failures.
  Verified live through the Security tab.

### Phase 5: Tests & Docs
- `EditorTab.test.tsx` — 10 tests (tree load/expand, open/save,
  Explain/Fix/Refactor, diff accept/reject, multi-file refactor,
  autocomplete toggle, error handling).
- `mcp-rag`: 2 new unit tests for the dedup-by-doc-id logic.
- `ghost-link`: 2 new unit tests for the audit-log cap/ordering logic.
- `README.md`, `CHANGELOG.md` (`[1.16.0]`), `docs/API_REFERENCE.md`,
  `docs/openapi.yaml` updated for the new endpoints and the no-longer-a-stub
  audit log.

## Build Verification
- `cargo fmt --all --check` — **OK**
- `cargo clippy --workspace --all-targets -- -D warnings` — **OK**
- `cargo test --workspace` — **all passed** (150 in `ghost-link`, 12 in
  `mcp-rag`, plus `ghostlink-core` integration suites — 0 failed)
- `npx tsc --noEmit` (ghostlink_gui_modern) — **OK**
- `npx vitest run` — **128/128 passed**
- Manual: verified the Editor tab, audit log, and workspace-index endpoint
  live against the built release binary through the actual GUI (not just
  curl).

## Known Issues / Follow-ups
1. Ghost-text autocomplete and multi-file refactor are explicitly MVP-scoped
   (see CHANGELOG) — not exercised against a live completion-capable model
   this session (no model was loaded in the test environment).
2. Audit log is in-memory only — resets on restart. A persistent
   append-only trail is a bigger follow-up, not done here.
3. Everything from Session 6's "Known Issues" list still applies (worker
   networking is local-only, no discovery-token enforcement, etc.) — not
   touched this session.

## To Restart
```powershell
cd C:\Users\rwill\Ghostlink
.\launch.bat
```

For Linux:
```bash
cd ~/Ghostlink
./launch.sh
```

**Ports:** GUI → API `:8003` only. Inference is `:8080` (llama-server) or `:11434` (ollama).
Pointing the GUI at `:8000`/`:8080` caused **405** on chat/models — fixed in unified launchers.

## Models on Disk
- `models/stories15M-q4_0.gguf` (~19 MB)
- `models/tinyllama-1.1b-chat-v1.0.Q2_K.gguf` (~483 MB)
