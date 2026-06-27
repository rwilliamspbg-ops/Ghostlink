# Changelog

All notable changes to Ghost-Link will be documented in this file.

## Unreleased

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