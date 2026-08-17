# Release Audit — v2.0.0 (GA)

Audit date: 2026-08-17. Scope: documentation quality/accuracy across the
whole repo, GitHub Pages, version bump to 2.0.0, crate/SDK publication
readiness, and a CHANGELOG update covering everything shipped since 1.17.0.
Format follows the Release Rubric in [CONTRIBUTING.md](../CONTRIBUTING.md#release-rubric).

## 1. Hard gates (must pass)

| Gate | Result |
| --- | --- |
| `cargo fmt --all --check` | **OK** |
| `cargo clippy --workspace --all-targets -- -D warnings` | **OK** — 0 warnings |
| `cargo test --workspace` | **OK** — 0 failures. `ghost-link` 223 passed (6 ignored, hardware/network-dependent) + 1 doc-test; `ghostlink-core` 188 + 7 + 28 + 19 across its lib and three integration suites; `mcp-rag` 13; `mcp-calculator`/`mcp-vision` 0 (both delegate to Ollama, no local inference to unit-test) |
| CI: ubuntu / windows / macos | **Not run by this audit** — these run in GitHub Actions on push; not reproduced locally. Recommend confirming green on the release branch/tag before publishing. |
| Performance baseline — no regression | **Not re-benchmarked** — no transport/ring-buffer/pipeline code changed in this pass (documentation and metadata only). Existing baselines in [BENCHMARKS.md](BENCHMARKS.md) stand. |

## 2. Scoring factors

- **Documentation completeness**: significantly improved this pass (see §3). `docs/SECURITY_MODEL.md` was materially out of date relative to shipped functionality (RBAC, RPC peer auth, durable audit trail, OpenTelemetry) and is now current. Two lower-priority items remain open — see §5.
- **Changelog updated**: yes — `CHANGELOG.md`'s `[Unreleased]` section, which only covered GUI/perf work, has been merged with newly-documented entries for RBAC, RPC peer authentication, the durable audit trail, Grafana/Prometheus, OpenTelemetry, and five previously-undocumented fixes, and re-headed as `[2.0.0] - 2026-08-17`.
- **Version bumped**: yes — see §4.
- **Operational caveats documented**: yes, in the CHANGELOG entries and `docs/SECURITY_MODEL.md`: RPC auth doesn't encrypt the stream, the audit trail has no rotation/retention policy yet, pre-upgrade JWTs stop validating post-upgrade, MCP standalone-server tools are now opt-in (behavior change).

## 3. Documentation changes made

**Fixed / rewritten for accuracy:**
- `docs/SECURITY_MODEL.md` — added RBAC, RPC peer-auth handshake, durable audit trail, OpenTelemetry, and Grafana/Prometheus to "Current Controls"; removed the stale claim that the audit-log endpoint is a stub.
- `SECURITY.md` — supported-version table bumped from the 1.16.x/1.15.x lineage to 2.0.x/1.17.x.
- `docs/TROUBLESHOOTING.md` — added RPC peer version/secret-mismatch guidance and a note on post-upgrade JWT invalidation.
- `CONTRIBUTING.md` — fixed a broken reference to a nonexistent `docs/INDEX.md`; replaced a full duplicate copy of the Code of Conduct with a link to `CODE_OF_CONDUCT.md`.
- `AGENTS.md` — was a dated (2026-07-30) development-session diary, not repo-wide guidance, despite living under a filename AI tools treat as special. Replaced with real, durable instructions (layout, required checks, conventions, security-sensitive areas). Original content preserved at `docs/archive/legacy-root-docs/SESSION_NOTES_2026-07-30.md`.
- `crates/ghostlink-gui/README.md` and its `Cargo.toml` — this is an earlier Tauri/Svelte prototype GUI, confirmed (not guessed) superseded: absent from the root Cargo workspace, not built by any launcher, not mentioned in the current root README, and untouched by recent CHANGELOG entries relative to `ghostlink_gui_modern`. Added an explicit deprecation notice to its README and `publish = false` to its `Cargo.toml` (it previously had no `publish` field, so `cargo publish` from that directory would have pushed a near-empty listing to crates.io).

**Archived:**
- `DEVELOPMENT_GUIDELINES.md` (the one tracked stray root file) → `docs/archive/legacy-root-docs/` — it described a test setup (`test_gui_framework.py`, `run_gui_tests.py`) that doesn't exist anywhere in this repo, and duplicated/conflicted with the accurate `TESTING.md`/`CONTRIBUTING.md`.
- `docs/archive/INDEX.md` updated to list both newly-archived files.

**Left alone, verified as non-issues:**
- Seven root-level files (`BENCHMARK_EXECUTIVE_SUMMARY.md`, `BENCHMARK_REPORT.md`, `FINAL_STATUS_REPORT.md`, `GHOSTLINK_COMPLETE_SUMMARY.md`, `GHOSTLINK_TESTING_SUMMARY.md`, `PHASE_1_FINAL_SUMMARY.md`, `PHASE_2_FINAL_SUMMARY.md`) look like AI-session artifacts with benchmark numbers that conflict with the real, hardware-attributed data in `BENCHMARKS.md`. Checked and confirmed: **all seven are `.gitignore`d** (`*_SUMMARY.md`, `*_REPORT.md` patterns) and were never part of the tracked repo — they exist only on this machine. Nothing to fix in the published repo; left in place rather than deleted, since they're local files outside this audit's scope of "what ships."
- `crates/mcp-calculator`, `crates/mcp-rag`, `crates/mcp-vision` READMEs/manifests — already clear, honest about `publish = false`, no issues found.
- `sdks/python`, `sdks/js` READMEs/manifests — already clear and honest about "not yet published." Added a missing `LICENSE` file to `sdks/python` (present in `sdks/js`, absent there) for consistency — everything else was already in order.

## 4. Version bump to 2.0.0

| Component | Was | Now |
| --- | --- | --- |
| `ghost-link` (crate) | 1.17.0 | **2.0.0** |
| `ghostlink-core` (crate) | 1.17.0 | **2.0.0** |
| `ghostlink_gui_modern` (npm) | 1.17.0 | **2.0.0** |
| `ghostlink-core` path-dependency pin in `ghost-link/Cargo.toml` | 1.17.0 | **2.0.0** |
| `README.md` version badge | 1.17.0 | **2.0.0** |
| `docs/index.html` (GitHub Pages hero badge) | v1.16.0 (already stale before this pass) | **v2.0.0** |
| `Cargo.lock` | — | regenerated, confirms both crates resolve at 2.0.0 |
| `sdks/python` (PyPI, unreleased) | 0.1.0 | unchanged — see §5 |
| `sdks/js` (npm, unreleased) | 0.1.0 | unchanged — see §5 |
| `crates/ghostlink-gui` (Tauri, deprecated) | 1.0.0 | unchanged — deprecated component, not part of the coordinated version |

Also fixed in passing: `crates/ghostlink-core/Cargo.toml` was missing an MSRV
(`rust-version`) declaration that `ghost-link` had — added `1.85.0` to match,
since the two are always released together. A stray two-space indent on
`ghost-link`'s `rust-version` line was also fixed.

## 5. Open decisions — not mine to make unilaterally

1. **SDK versioning/publishing.** `sdks/python` and `sdks/js` are both at `0.1.0` and have never been published to PyPI/npm — there's no existing CI workflow for either (only `publish-crates.yml`, which handles the two Rust crates). If they should ship alongside the 2.0.0 GA: decide a version number (a fresh `1.0.0` first-stable release is the more conventional semver move than jumping an unreleased package straight to `2.0.0`), and someone will need to set up publish credentials/workflows and run `npm publish` / `twine upload` — I did not do this, see §6.
2. **`docs/ENTERPRISE_PLAN.md` placement.** This is a go-to-market/monetization strategy document (pricing, licensing tension, sales trust risks) currently sitting in the public `docs/` folder alongside user-facing reference docs. Worth confirming that's intentional for a GA release, or moving it somewhere internal-only.
3. **Two parallel archive directories.** Root `_archived/` (80 tracked files, its own README) predates and is now itself stale relative to `docs/archive/` (82 tracked files, referenced by CI and `CONTRIBUTING.md` as the current convention). Not consolidated in this pass — low risk as-is, but worth a cleanup decision at some point so contributors aren't unsure which one to use.
4. **`docs/BENCHMARKS.md` (1,157 lines) and `docs/ROADMAP.md` (781 lines).** Both are accurate, rigorous, and internally consistent, but long enough that "what's true today" is hard to extract from "how we got here" for a prospective GA evaluator. Not restructured in this pass (would be a substantial editorial project, not a wording fix) — flagging as a post-GA polish candidate: a short current-state summary at the top of each, with the detailed narrative history kept as-is below or moved to `docs/archive/`.

## 6. What this audit did **not** do

Publishing is an external, hard-to-reverse action that needs your explicit
go-ahead and, in most cases, credentials I don't have. Specifically not
done:

- **No git tag pushed.** `publish-crates.yml` and `release-artifacts.yml`
  both trigger on `push: tags: 'v*'` — nothing was tagged or pushed.
- **No `cargo publish`, `npm publish`, or `twine upload` run.**
- **No GitHub Release created.**
- **All changes above are uncommitted in the working tree** — nothing has
  been committed or pushed to `main` either.

### To actually ship v2.0.0 once you're ready

1. Review the diff (`git status` currently shows the files listed in this
   audit) and commit it.
2. Push to `main` (or open a PR, per your usual flow) — `deploy-pages.yml`
   will redeploy GitHub Pages with the updated version badge automatically
   on merge to `main`.
3. Tag the release commit `v2.0.0` and push the tag — this triggers
   `publish-crates.yml` (needs the `CARGO_REGISTRY_TOKEN` repo secret) and
   `release-artifacts.yml` (builds signed binaries for all three OSes and
   creates the GitHub Release from the `CHANGELOG.md` section, needs
   `RELEASE_GPG_PRIVATE_KEY`/`RELEASE_GPG_PASSPHRASE` for signing — falls
   back to an unsigned bundle if absent).
4. Decide and act on the open items in §5 before or after — none of them
   block the crates.io/GitHub Release path above, which only concerns the
   two Rust crates.

I'd recommend doing this as a normal reviewed PR rather than pushing
straight to `main`, given the size of this change.

## 7. Final recommendation

**Conditional GO.** All hard gates pass locally (fmt/clippy/test) and the
documentation set is now internally consistent and accurate as of this
audit. Conditions before tagging:

- Confirm CI is green on ubuntu/windows/macos for the actual release commit
  (not reproduced by this local audit).
- A human decision on the three open items in §5 (or an explicit "defer,
  ship without them" call) — none are blocking defects, all are judgment
  calls.
