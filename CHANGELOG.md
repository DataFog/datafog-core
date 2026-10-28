# Changelog

This changelog starts October 22, 2026. The changelog policy applies
to changes from this point forward; earlier changes are not backfilled.
Earlier release notes are in [docs/releases](docs/releases/).

## [Unreleased]

### Changed

- Toolchain versions are now pinned in repository root files (`rust-toolchain.toml`, `.nvmrc`, `Cargo.lock`, `bindings/wasm/package.json`) as the single source of truth; the Dockerfile, devcontainer, and CI read these files instead of hardcoding versions
- `cargo dfv doctor` reads version requirements from pin files and fails loudly when a pin file is missing; it no longer uses hardcoded version constants

### Added

- Protected follow-up text in Rust and WASM. Committed workspace pseudonyms stay
  unchanged; unknown or uncommitted markers and overlapping findings block the
  operation. The synthetic browser workflow requires local preview and approval.
- Atomic workspace batches in Rust and WASM. Inputs share pseudonyms and one saved
  snapshot; a rejected input blocks the whole batch. The synthetic browser harness
  supports explicit input removal followed by a fresh preview and approval.
- Workspace values of one type that differ only in letter case or spacing share
  one marker, so `JOHN SMITH` and `John  Smith` are one person. Restoration
  returns the first spelling seen. Whitespace-only findings are rejected.
  **Migration note:** development snapshots with case-variant duplicate entries
  will fail to decode; discard them and start fresh.
- Persistent workspace pseudonyms in Rust and WASM, with exact local restoration.
  Markers are short typed codes such as `[[PERSON_7KQ4ZM]]`, unique within the
  workspace. A synthetic browser harness verifies approval, encrypted mapping
  storage, and reuse after browser restart. The workspace API and snapshot format
  are provisional.
- `.devcontainer/devcontainer.json` for VS Code, Cursor, and GitHub Codespaces dev environments
- Dev environment setup documentation in CONTRIBUTING.md covering devcontainer and local install paths, verification steps, and troubleshooting
- `scripts/pin.sh` helper to read pinned tool versions from repository files
- Optional browser Small model sessions return canonical findings for the scan/transform flow, with Core-owned entity routing and score evidence.
- Experimental browser entity extraction using pinned GLiNER2.5 Small ONNX assets, configurable labels, and explicit Unicode ranges (#116)
- Agent-friendly verification CLI (`cargo dfv`) with `doctor`, `build`, `check`, `parity`, `eval`, `bench`, and `smoke` commands for cross-SDK testing (#108, #109)
- Per-entity precision/recall/F1 evaluation with tiered accuracy gating: first-party engines fail on F1 drops >1pt, community engines warn only (#109)
- Provisional baselines for DataFog and GLiNER2 models; regressions against provisional baselines warn but do not block PRs (#109, #112)
- `ROADMAP.md` documenting current and planned work, engine maintenance policy, and process for proposing new engines (#110)
- Cursor Cloud Agent environment with Dockerfile, install script, and verification skill for `cargo dfv` workflows (#111)
- Documentation for GLiNER2 model integration in `docs/gliner2-model.md` (#112)

- Native model sessions accept an explicit DataFog or GLiNER2 engine selector, with shared validation across Rust, Python and Node and compatibility for existing configurations.

### Fixed

- Development containers build with the pinned tool versions and mount the repository at the configured workspace path.
- GLiNER2 entity-count gating now floors predictions at 1, preventing valid spans from being skipped when the count classifier selects 0 (#112)
