# Changelog

All notable changes to Ghost-Link will be documented in this file.

## Unreleased

- Ghostlink Studio GUI v1.0.0 release prep:
  - Added worker discovery with configurable node hints and fast/full probe modes.
  - Added multi-select batch connect for reachable workers with per-worker result summaries.
  - Added per-worker quick TCP connectivity checks with latency reporting.
  - Persisted cluster/discovery preferences in both local UI prefs and profile export/import bundles.
  - Added launch script preflight check modes and unsigned release bundle mode for easier packaging workflows.
  - Added signed release verification assets:
    - Public key: `artifacts/release/v1.0.0/GHOSTLINK_RELEASE_PUBLIC_KEY.asc`
    - Signing key fingerprint: `53B7 2478 A086 201F 2D0E  2CC6 8286 9E53 C58B 384E`
    - Verify command: `gpg --verify artifacts/release/v1.0.0/SHA256SUMS.asc artifacts/release/v1.0.0/SHA256SUMS`

- Enabled xdp-mode autotune by default when AF_XDP probe succeeds, with explicit opt-out via `GHOSTLINK_XDP_AUTOTUNE=0`.
- Added boolean env parsing + precedence tests for xdp/tcp autotune flags to lock in deterministic configuration behavior.
- Extended flow benchmark harness usage guidance for privileged AF_XDP validation and documented true `effective_transport_mode=xdp` results.
- Validated root-backed AF_XDP A/B profile in workspace:
  - autotune default on: `596,995.66 tok/s`, `p95=0.41 ms`, `effective_transport_mode=xdp`
  - autotune disabled: `331,150.29 tok/s`, `p95=0.99 ms`, `effective_transport_mode=xdp`
- Confirmed current PR lane status for this branch at 19/19 passing checks across CI, security, docs, benchmarks, and production gates.
- Fixed GUI/doctor Python interpreter resolution so generic `python3` config defaults no longer override the repository virtualenv fallback; updated sample config guidance accordingly.
- Expanded default TCP autotune candidate sweeps to include the active inflight setting and nearby queue depths, improving stressed TCP canary stability on validated local runs.
- Preallocated load-balance chunk vectors in the autotuned distribution path, removing the previously observed Criterion regression signal for `autotune/load_balance_80_layers_autotuned`.
- Added GitHub Actions CI for formatting, linting, and workspace tests.
- Added a Criterion benchmark workflow with uploaded benchmark artifacts.
- Updated the README with CI and benchmark badges plus the latest Criterion results.
- Added a shared node snapshot cache and cached total VRAM fast path in `ClusterState`.
- Switched hot readers to the shared snapshot API to reduce read-path overhead.
- Added `scripts/verify_hf_models.py` to validate Hugging Face model listing and file downloads.
- Refreshed project documentation to reflect current health probe behavior and verification workflow.
- Updated validation totals and usage examples across README and docs.
- Added dedicated `docs.yml`, `lint.yml`, and `tests.yml` workflows for split status visibility.
- Added scheduled `hf-model-verify.yml` workflow to validate model download availability nightly.
- Updated README badges to dynamic workflow badges for docs/lint/tests/HF verification.