---
name: verify-datafog
description: Verify DataFog Core detection and transformation logic across all SDKs (Rust, Python, Node, WASM) using the dfv verification CLI. Use after detection changes, before releases, or when setting up the development environment.
---

# DataFog Verification

Verify detection semantics and cross-SDK parity using `cargo dfv`. The CLI drives installed packages (Python wheel, Node package, browser WASM bundle, Rust crate) the way a user does.

## Launch

Ensure the environment is ready before verification.

```bash
cargo dfv doctor --json
```

Doctor checks:
- rust_toolchain: Rust 1.88
- python: Python 3.10+
- maturin: maturin installed
- node: Node.js 24+
- wasm_bindgen_cli: wasm-bindgen 0.2.127
- wasm_target: wasm32-unknown-unknown rustup target
- playwright_chromium: Playwright Chromium browser
- python_binding_fresh: built wheel or .so, fresher than crates/core/src
- node_binding_fresh: built index.js or .node file
- wasm_binding_fresh: built dist/datafog_wasm_bg.wasm
- model_weights (optional): files in models/ match manifest.json sha256 hashes

Doctor hashes the files listed in the combined `models/manifest.json` using sha256. It does not verify the model revision. If the manifest is missing, doctor emits a warning but does not fail.

## Doctor

Fix failures before running check or parity.

```bash
cargo dfv doctor
```

If bindings are stale or missing:

```bash
cargo dfv build           # Build all bindings
cargo dfv build --sdk python   # Build only Python
cargo dfv build --sdk node     # Build only Node
cargo dfv build --sdk wasm     # Build only WASM
```

For model weights, provision them with the existing pinned scripts:

```bash
python scripts/provision-datafog-model.py --archive <path> --destination models/datafog
python scripts/provision-gliner2-model.py --source <path> --destination models/gliner2
```

## Drive

Run detection tests across all SDKs.

```bash
cargo dfv check           # Rule-based detectors
cargo dfv check --json    # Machine-readable output
cargo dfv check --dry-run # Show commands without executing
```

Check runs doctor first and stops early if required toolchains or bindings are missing.

For cross-SDK span and offset comparison:

```bash
cargo dfv parity          # Verify fixture offsets across coordinate systems
cargo dfv parity --dry-run
```

### When --models is required

Add `--models` to `cargo dfv check` and `cargo dfv eval` when the change touches:
- Files under `crates/core/src/models/`
- Model provisioning scripts (`scripts/provision-*.py`)
- Baseline files (`baselines/eval-models-*.json`)
- Model-related issues or features

The `--models` tier is not yet implemented. Rule-based verification covers the current stable release.

### Future commands

These commands are planned but not yet implemented:

```bash
cargo dfv check --models          # Include model-based detectors
cargo dfv eval                    # Run evaluation suite
cargo dfv eval --models           # Evaluation with model detectors
cargo dfv parity --live --sample  # Live SDK comparison with sampling
cargo dfv parity --live --full    # Full live SDK comparison
cargo dfv smoke                   # Smoke test installed packages
cargo dfv bench                   # Run benchmarks
```

## Evidence

Reports are written to `target/dfv/runs/<timestamp>/` and never deleted by the CLI.

Each run directory includes:
- `metadata.json`: git commit SHA, branch, tool versions
- `doctor.json`: environment check results
- `check.json`: test results per SDK
- `parity.json`: cross-SDK comparison results
- SDK-specific logs: `rust_tests.log`, `python_tests.log`, `node_tests.log`, `wasm_tests.log`

## Cleanup

Evidence directories accumulate. Remove old runs manually when disk space is a concern.

```bash
ls -lt target/dfv/runs/ | tail -n +10 | xargs rm -rf
```

Do not delete the most recent run directory before the PR is merged.

## Helpers

### Quick verification after a detection change

```bash
cargo dfv doctor --json && cargo dfv check --json
```

### Full verification before release

```bash
cargo dfv doctor
cargo dfv build
cargo dfv check
cargo dfv parity
```

### Debugging a failing SDK

Read the SDK-specific log in the run directory:

```bash
cat target/dfv/runs/<timestamp>/python_tests.log
```

### Verifying model changes

Model verification requires provisioned weights. After provisioning:

```bash
cargo dfv doctor --json  # Check model_weights status
# When --models is implemented:
# cargo dfv check --models
```
