# DataFog Core repository rules

## Verification CLI

Use `cargo dfv` to verify changes across all SDKs (Rust, Python, Node, WASM):

```bash
cargo dfv doctor              # Check environment readiness
cargo dfv build               # Build all bindings
cargo dfv check               # Run all rule-based detector tests
cargo dfv check --models      # Run model tier checks (GLiNER2/DataFog)
cargo dfv eval                # Per-entity precision/recall/F1 for rules
cargo dfv eval --models       # Per-entity metrics for model detectors
cargo dfv bench               # Latency benchmark against baseline
cargo dfv smoke               # Install and test packages as a user would
cargo dfv parity              # Cross-SDK fixture offset verification
cargo dfv parity --live       # Run scan() through each SDK and diff outputs
```

Run `cargo dfv check` after any detection change. It runs doctor first and stops early if toolchains or bindings are missing.

**Thresholds:**
- Rule-based: any precision/recall drop fails.
- Model-based: F1 drop > 1 point fails, smaller drop warns.
- Benchmark: warn-only while baseline is provisional. Once a stable runner baseline exists: > 10% slower fails, 5-10% warns.
- Parity: zero tolerance unless a PR intentionally updates a fixture.
- Live parity: compares actual scan() output across Python and Node bindings. WASM parity is verified via smoke test in headless Chromium.

Reports are written to `target/dfv/runs/<timestamp>/` and never deleted by the CLI.

**Scripts wrapped by dfv:**
- `check --models`: scripts/provision-gliner2-model.py, scripts/provision-datafog-model.py, scripts/test-model-manifest.py, scripts/check-model-integration.py, scripts/model-qualification.py, scripts/model-fault-conformance.py
- `smoke`: bindings/python/tests/test_installed.py, scripts/test-node-package.mjs, scripts/test-wasm-package.mjs

## Correctness

- Detection changes require positive and negative tests that prove the changed behavior.
- Do not silently discard fallible results.
- Avoid panic paths for user-controlled input.
- Preserve documented entity offset semantics across the Rust core and all bindings.

## Structure

- Add files only for distinct logical components.
- Do not introduce abstractions without a demonstrated need in the current codebase.
- Keep bindings thin: PII detection semantics belong in `crates/core`.
- Do not duplicate detector logic in Python, Node, or WASM bindings.
- Do not perform unrelated refactoring while implementing a scoped change.

## Code

- Comments document public behavior or explain non-obvious reasoning; do not narrate syntax.
- Prefer descriptive names over abbreviations.
- New third-party dependencies require a concrete need that cannot be met reasonably with the standard library or existing dependencies.

## Changes

- Keep each pull request focused on one behavior or maintenance goal.
- Run `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, and `cargo test --workspace --all-features` before merging Rust changes.
