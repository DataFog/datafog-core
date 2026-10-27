# Changelog

This changelog starts October 22, 2026. The changelog policy applies
to changes from this point forward; earlier changes are not backfilled.
Earlier release notes are in [docs/releases](docs/releases/).

## [Unreleased]

### Added

- Agent-friendly verification CLI (`cargo dfv`) with `doctor`, `build`, `check`, `parity`, `eval`, `bench`, and `smoke` commands for cross-SDK testing (#108, #109)
- Per-entity precision/recall/F1 evaluation with tiered accuracy gating: first-party engines fail on F1 drops >1pt, community engines warn only (#109)
- Provisional baselines for DataFog and GLiNER2 models; regressions against provisional baselines warn but do not block PRs (#109, #112)
- `ROADMAP.md` documenting current and planned work, engine maintenance policy, and process for proposing new engines (#110)
- Cursor Cloud Agent environment with Dockerfile, install script, and verification skill for `cargo dfv` workflows (#111)
- Documentation for GLiNER2 model integration in `docs/gliner2-model.md` (#112)

### Fixed

- GLiNER2 entity-count gating now floors predictions at 1, preventing valid spans from being skipped when the count classifier selects 0 (#112)
