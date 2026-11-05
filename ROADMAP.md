# Roadmap

This document summarizes work in progress and planned changes to DataFog Core, for contributors and maintainers.

## In progress

### Verification CLI

`cargo dfv` (`crates/xtask`) is the single command that proves a change works across the Rust core and the Python, Node, and browser (WASM) SDKs.

Subcommands:

- `doctor` — check environment readiness
- `check` — run all rule-based detector tests
- `parity` — cross-SDK span/offset comparison, including live comparison
- `smoke` — install and test each published package
- `eval` — run evaluations with per-entity baselines
- `bench` — performance benchmarks (warn-only until a stable baseline exists)

The project verification skill and feature map live at `.cursor/skills/verify-datafog/`. See [AGENTS.md](AGENTS.md) for repository rules and verification workflow.

## Completed

- **GLiNER2 entity-count gating.** Fixed in [PR #112](https://github.com/DataFog/datafog-core/pull/112); [issue #87](https://github.com/DataFog/datafog-core/issues/87) is closed. Regression checks remain part of model verification.

## Planned

1. **Unified model-engine interface.** A single interface in `crates/core` for model engines. Existing engines (the DataFog model and GLiNER2) will move onto it. Engines remain a fixed list behind optional features, each pinned to a revision with SHA-256 hashes and with its own eval baseline. There is no runtime plugin system.

2. **Contributed multilingual engines.** Engines for Arabic, Persian, Urdu, and Turkish coverage, built on the unified interface.

## Under consideration

Additional language coverage and engine contributions are welcome. See [Proposing changes](#proposing-changes) below.

## Engine maintenance policy

**First-party engines.** The DataFog model is first-party. Accuracy regressions block merges.

**Community-tier engines.** Engines DataFog does not own, including GLiNER2 and contributed engines, are community tier:

- Accuracy is tracked nightly as a warning, not a merge blocker.
- Contract checks always block: pinned and verified weights, loads only behind its feature, valid offsets matching across every SDK, and no change to the core when disabled.
- Each community engine requires a named owner.
- An engine that stays broken without a fix may be marked unmaintained or removed.

## Proposing changes

Open an issue describing the change before submitting a large PR.

New engines require an issue covering:

- Model export format
- License
- Test data for the languages it covers
- Eval numbers from the proposer
