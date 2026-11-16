# Ghostlink 2.7.0 — Release Plan

Generated from a review of everything merged to `main` after the `v2.6.0` tag
(61 commits / 38 files; PRs #593–#619). This plan follows the
[Tag & Release Publication Flow](../../CONTRIBUTING.md#tag--release-publication-flow)
and the [Release Rubric](../../CONTRIBUTING.md#release-rubric) in `CONTRIBUTING.md`.

## 1. Target

| Field | Value |
|---|---|
| Version | **2.7.0** (minor) |
| Base | `main` @ `a0ee6c5` |
| Previous tag | `v2.6.0` @ `d4326a5` (release merge of #592) |
| Release branch | `release/2.7.0` |
| Tag | `v2.7.0` (created automatically by `.github/workflows/tag-release-on-main.yml`) |
| CHANGELOG section | `## [2.7.0] - <release date>` |

### Why minor, not patch or major

- **Minor**: new user-visible functionality landed after `v2.6.0` — the Palette
  GUI accessibility/UX batch (ReviewPane listbox semantics and excerpt copy,
  editor assistant dismiss/copy, message-edit keyboard shortcuts, task-event copy,
  ProjectsTab empty states, EditorTab/WorkersTab a11y).
- **Not major**: no public API break. `ratatui` 0.28 → 0.30 and `rmcp` 2 → 3.9 are
  internal dependency bumps; `ratatui` appears only in the private `fn`s of
  `ghostlink-core`'s public `dashboard` module, and `rmcp`/`rusqlite` are not
  re-exported. `ghostlink-core`'s public signatures are unchanged.
- **Not patch**: patch releases are for fixes only; this set adds features.

> LoRA/adapter passthrough (#591) and richer HF model cards (#590) are **already in
> `v2.6.0`** — the authoritative remote tag `v2.6.0` points at the release merge
> `d4326a5`, which contains both. Do not re-list them as new in 2.7.0.

## 2. Scope (what ships in 2.7.0)

**GUI / Palette** — ReviewPane `role="listbox"` + roving `tabIndex` and
failure-excerpt copy; editor assistant dismiss/copy; chat message-edit keyboard
shortcuts (`Esc`, `Ctrl/Cmd+Enter`); task-event copy; ProjectsTab empty-state CTAs;
EditorTab and WorkersTab accessibility (`aria-busy`, contextual labels, drawer
`aria-expanded`/`aria-controls`).

**Performance** — `ClusterState::node_count` atomic cache; zero-copy
`nodes_snapshot` in `/v1/completions`; Quickselect p95/p99 latencies; mcp-memory
zero-alloc enum parsing + partial sort; `PlacementPlan::new` capacity reclaim;
MetricsTab `React.memo`.

**Security / dependencies** — `ratatui` 0.30 (RUSTSEC-2026-0253 / `lru`);
`rmcp` 3.9 + `rusqlite` 0.41 / `libsqlite3-sys` 0.39.1; `quinn-udp` 0.5.16;
removal of unused `ghostlink-core` deps; Rust 1.100 clippy cleanup; Dependabot
batch (fastapi, pytest-asyncio, mypy, certifi, pytest, pyqt6, flake8,
markdownlint-cli2-action).

The full entry lives in `CHANGELOG.md` under `## [Unreleased]` and moves to
`## [2.7.0]` at release time.

## 3. Hard gates (must pass before merge)

Run locally, then confirm on CI:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

CI matrix that must be green: `ubuntu-latest`, `windows-latest`, `macos-latest`,
plus the repo's `lint`, `security` (cargo-deny, govulncheck), `tests`,
`benchmarks`, `distributed-e2e`, and `production-gate` workflows.

Performance baseline (required — transport/ring-buffer/pipeline code changed):

```bash
cargo bench --package ghostlink-core
```

Report `in-process` and `TCP loopback` throughput/latency at 1024 tok and
`ring_buffer: SPSC batch cross-thread` ops/sec, with before/after numbers, in the
release PR body. The `cluster/node_count_10` criterion bench is new in this cycle.

GUI (changed):

```bash
cd ghostlink_gui_modern && npx tsc --noEmit && npx eslint . && npx vitest run
```

## 4. Version bump checklist (lockstep)

Release PRs must set **all** of these to `2.7.0`:

| File | Line |
|---|---|
| `crates/ghostlink-core/Cargo.toml` | `version = "2.7.0"` |
| `crates/ghost-link/Cargo.toml` | `version = "2.7.0"` **and** `ghostlink-core = { ... version = "2.7.0" }` |
| `ghostlink_gui_modern/package.json` | `"version": "2.7.0"` |
| `sdks/js/package.json` | `"version": "2.7.0"` |
| `sdks/python/pyproject.toml` | `version = "2.7.0"` |
| `Cargo.lock` | `ghost-link` and `ghostlink-core` package versions (regenerate with `cargo check`) |
| `CHANGELOG.md` | add `## [2.7.0] - YYYY-MM-DD` above `## [Unreleased]` |

`tag-release-on-main.yml` fails the push if any manifest disagrees or if the
CHANGELOG header for the version is missing — that check is the guard, do not
hand-edit around it.

## 5. Release flow

1. `git checkout main && git pull --ff-only origin main`
2. `git checkout -b release/2.7.0`
3. Apply the version bump (Section 4) and move the `## [Unreleased]` content to
   `## [2.7.0] - <date>`.
4. Run the Section 3 gates; record results in the PR body.
5. Push and open `release/2.7.0` → `main` with the Release Rubric checklist.
6. Merge. The push to `main` triggers `tag-release-on-main.yml`, which validates
   lockstep and creates/pushes `v2.7.0` **at the `main` tip** (the merge commit).
7. Dispatch the publish workflows against the tag — the tag alone does not start
   them (the automation creates it with `GITHUB_TOKEN`, and GitHub does not run
   further workflows from `GITHUB_TOKEN` pushes):
   ```bash
   gh workflow run publish-crates.yml    --repo rwilliamspbg-ops/Ghostlink --ref v2.7.0
   gh workflow run publish-sdks.yml      --repo rwilliamspbg-ops/Ghostlink --ref v2.7.0
   gh workflow run release-artifacts.yml --repo rwilliamspbg-ops/Ghostlink --ref v2.7.0
   ```
   These publish to crates.io, npm, PyPI, and the GitHub Release (+ SHA256SUMS +
   SBOM).

### Tag-integrity checks

The automated workflow is the reliable path — for `v2.6.0` it created the tag at
`d4326a5` (tagger `github-actions[bot]`), i.e. on the release merge, so nothing
merged just before the release was left out.

Two gotchas worth guarding against:

- **Stale local tags.** A local tag can point at a different commit than the
  remote (a stray local `v2.6.0` at the release-branch tip `be18bef` never existed
  on the remote, whose `v2.6.0` is `d4326a5`). Always compare against the remote
  before drawing release conclusions:
  ```bash
  git ls-remote origin refs/tags/v2.7.0
  git fetch origin tag v2.7.0 --force
  ```
- **Historical drift.** `v2.2.1` was tagged and released from `aaa0e63`, whose
  `crates/ghost-link/Cargo.toml` says `2.2.0` and whose `CHANGELOG.md` has no
  `## [2.2.1]` section (the release body fell back to "No matching CHANGELOG.md
  section found"). `v2.2.0` is the release-prep merge, and `v2.5.0` points at a
  later CI-fix commit rather than the release commit. So for 2.7.0 verify all
  three agree:
  ```bash
  git fetch origin --tags
  git show v2.7.0:crates/ghost-link/Cargo.toml | grep '^version'
  git show v2.7.0:CHANGELOG.md | grep -F '## [2.7.0]'
  git merge-base --is-ancestor <last-feature-commit> v2.7.0 && echo ok
  ```

If the manual tag fallback in `CONTRIBUTING.md` is used, tag **`main`'s merge
commit** (checkout `main` and pull first), never the release-branch tip.

### Release metadata caveats

- The GitHub release's `target_commitish` is `main` (the branch), not the tag
  commit — cosmetic, but it means the release page does not pin a SHA.
- Only the `ghost-link` binary plus `SHA256SUMS-*` and the SBOM are attached to the
  GitHub release; the `control-plane` binary and the GUI `dist/` are published as
  Actions artifacts only.

## 6. Review follow-ups (post-`v2.6.0` audit)

Nothing below blocks the 2.7.0 release; all are small. Recommended before or
shortly after the release.

**Medium**

- **ReviewPane listbox has two tab stops** (`ghostlink_gui_modern/src/components/ReviewPane.tsx:265,279`):
  the container has `tabIndex={0}` while the selected option also has
  `tabIndex={0}`. A roving-tabindex listbox must expose exactly one. Set the
  container to `tabIndex={-1}` or drive it with `aria-activedescendant`.
- **LoRA auto-load ignores `lora_path`** (`crates/ghost-link/src/main.rs:11173`):
  the `ensure_model_loaded` auto-load call site passes `None`, so a model loaded
  on the first chat request after startup silently skips the configured adapter
  while an explicit GUI load applies it. Thread the setting into that closure.
- **`RUSTSEC-2026-0386` is ignored and still unpatched** (`deny.toml:74`):
  `tokio 1.52.3` is pinned while the patched ranges are `>=1.51.6,<1.52.0` and
  `>=1.53.3`. Either bump `tokio` to `>=1.53.3` and drop the ignore, or record a
  reason/expiry.

**Low**

- `#615` — the three TTFT `StatCard`s pass inline `history={[]}`, so `React.memo`
  always misses for them; hoist a module-level empty array.
- `#599` / `#608` / `#616` — `navigator.clipboard.writeText` is awaited with no
  `try/catch` and the 2s "copied" timeout is never cleared on unmount; existing
  code (`ChatTab.tsx:746`, `SecurityTab.tsx:40`) already wraps it.
- `#608` — dismiss removes the focused element without moving focus; `replyCopied`
  is not reset when a new reply arrives.
- `#591` — adapter paths are forwarded verbatim (never resolved against CWD /
  project root / models fallback, unlike `model_path`), a comment claims a
  `GHOSTLINK_LORA_PATH` env override that no code reads, and `lora_path` cannot be
  set through `POST /api/settings` or the GUI.
- `#607` / `#596` — accessible names omit the visible button text (WCAG 2.5.3
  Label in Name).
- `#590` — the downloads/likes stats lost their sr-only context text.
- `deny.toml` — two ignores are now dead: `RUSTSEC-2024-0436` (`paste`) and
  `RUSTSEC-2026-0441` (`pin-utils`); neither crate is in the graph any more.
- `#595` — the PR-reported `~1227 ns` snapshot cost contradicts the committed
  Criterion measurements (`cluster/nodes_snapshot_10` = 18–20 ns / 30.8–31.4 ns in
  `docs/BENCHMARKS.md`). Re-measure warm A/B before quoting a number.

## 7. Risks and rollback

| Risk | Mitigation |
|---|---|
| `ratatui` 0.30 behaviour change in the terminal dashboard | Dashboard is a secondary TUI path; `render_ascii` fallback exists. Smoke-test `cargo run -p ghostlink-core -- dashboard`, or note as a caveat. |
| `rmcp` 2 → 3.9 API change in MCP servers | Run `scripts/mcp_server_probe.py` against every server in `mcp_servers.toml`; all must handshake and list tools. |
| `rusqlite` 0.41 bundled SQLite change in `mcp-memory` | `cargo test -p mcp-memory` plus a real memory write/read. |
| Performance claims not reproduced on CI hardware | Benchmarks are documented as noisy; treat PR-reported deltas as indicative and re-run `cargo bench --package ghostlink-core`. |
| Publish workflows skip on a non-tag ref | `publish-crates`/`publish-sdks`/`release-artifacts` now fail loudly if dispatched on a branch; keep the tag ref. |
| Rollback | Revert the release merge commit on `main` and delete the tag (`git push origin :refs/tags/v2.7.0`); already-published crates/npm/PyPI versions cannot be unpublished, only yanked. |

## 8. Definition of done

- [ ] All Section 3 gates green on CI (ubuntu/windows/macos).
- [ ] All seven files in Section 4 at `2.7.0`; `tag-release-on-main.yml` validation passes.
- [ ] `CHANGELOG.md` has `## [2.7.0] - <date>`; `## [Unreleased]` empty again.
- [ ] `v2.7.0` points at `main`'s merge commit and contains all post-`v2.6.0` work.
- [ ] GitHub Release `v2.7.0` published with all six platform assets, `SHA256SUMS-*`, and SBOM.
- [ ] `ghostlink-core` / `ghost-link` on crates.io and both SDKs on npm/PyPI at `2.7.0`.
