# DataFog Core repository rules

## Verification CLI

Use `cargo dfv` to verify changes across all SDKs (Rust, Python, Node, WASM):

```bash
cargo dfv doctor              # Check environment readiness
cargo dfv build               # Build all bindings
cargo dfv check               # Run all rule-based detector tests
cargo dfv check --models      # Run model tier checks (GLiNER2/DataFog)
cargo dfv eval                # Per-entity precision/recall/F1 for rules
cargo dfv eval --model <name> # Per-entity metrics for a specific model engine (e.g., datafog, gliner2)
cargo dfv bench               # Latency benchmark against baseline
cargo dfv smoke               # Install and test packages as a user would
cargo dfv parity              # Cross-SDK fixture offset verification
cargo dfv parity --live       # Run scan() through each SDK and diff outputs
```

Run `cargo dfv check` after any detection change. It runs doctor first and stops early if toolchains or bindings are missing.

**Thresholds:**
- Rule-based: any precision/recall drop fails.
- Model-based: depends on engine tier (see below).
- Benchmark: warn-only while baseline is provisional. Once a stable runner baseline exists: > 10% slower fails, 5-10% warns.
- Parity: zero tolerance unless a PR intentionally updates a fixture.
- Live parity: compares actual scan() output across Python and Node bindings. WASM parity is verified via smoke test in headless Chromium.

**Model engine tiers:**
- **first_party** (e.g. datafog): F1 drop > 1pt fails PR, <= 1pt warns. Owned by DataFog.
- **community** (e.g. gliner2): F1 regression is warn-only, never fails PRs. Requires named owner in CODEOWNERS.

Contract checks block for all tiers: pinned revision verified by doctor (sha256 verified from each bundle's own manifest against its files), engine loads behind its feature/config, spans have valid offsets matching across SDKs. Integration code (e.g. GLiNER2 entity-count gating in crates/core) is ours regardless of tier and regression tests for it block.

### Model weight verification

`cargo dfv doctor` checks model weights when `models/<engine>/bundle-manifest.json` files exist. Doctor hashes each file listed in each bundle manifest using SHA-256 and compares against the expected digest. It does not verify the model revision or download weights.

If no bundle manifests exist, doctor emits a warning but does not fail. Model weights are optional for rule-based verification.

Provisioning scripts produce per-engine bundles with their own manifests. Registry-driven provisioning will follow the unified model-engine interface (see [ROADMAP.md](ROADMAP.md)). The GLiNER2 baseline belongs with [issue #87](https://github.com/DataFog/datafog-core/issues/87).

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
