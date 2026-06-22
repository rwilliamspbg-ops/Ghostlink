# Changelog

All notable changes to Ghost-Link will be documented in this file.

## Unreleased

- Added GitHub Actions CI for formatting, linting, and workspace tests.
- Added a Criterion benchmark workflow with uploaded benchmark artifacts.
- Updated the README with CI and benchmark badges plus the latest Criterion results.
- Added a shared node snapshot cache and cached total VRAM fast path in `ClusterState`.
- Switched hot readers to the shared snapshot API to reduce read-path overhead.