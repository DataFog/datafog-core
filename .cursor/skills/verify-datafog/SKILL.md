---
name: verify-datafog
description: Verify DataFog Core detection and transformation logic across all SDKs (Rust, Python, Node, WASM) using the dfv verification CLI. Use after detection changes, before releases, or when setting up the development environment.
---

# DataFog Verification

Verify detection semantics and cross-SDK parity using `cargo dfv`. The CLI drives installed packages (Python wheel, Node package, browser WASM bundle, Rust crate) the way a user does.

For version pins and environment setup, see [CONTRIBUTING.md](../../../CONTRIBUTING.md#dev-environment-setup).

## Launch

Run doctor first. If it fails, fix issues before proceeding:

```bash
cargo dfv doctor --json
```

Doctor reads version pins from repository files and fails loudly on missing files. It never installs anything—on mismatch it prints the required vs found version and the exact command to fix it, then exits non-zero.

Checks:
- rust_toolchain: matches rust-toolchain.toml channel
- python: Python 3.10+
- maturin: maturin installed
- node: Node.js major >= .nvmrc
- wasm_bindgen_cli: exactly matches Cargo.lock wasm-bindgen version
- wasm_target: wasm32-unknown-unknown rustup target
- playwright_chromium: exactly matches bindings/wasm/package.json version
- python_binding_fresh: built wheel or .so, fresher than crates/core/src
- node_binding_fresh: built native.js or .node file (not the hand-written index.js wrapper)
- wasm_binding_fresh: built dist/datafog_wasm_bg.wasm
- model_weights (optional): files in models/ match manifest.json sha256 hashes

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
cargo dfv check --models  # Include model-based detector tier
```

Check runs doctor first and stops early if required toolchains or bindings are missing.

For cross-SDK span and offset comparison:

```bash
cargo dfv parity                   # Verify fixture offsets across coordinate systems
cargo dfv parity --dry-run         # Show commands without executing
cargo dfv parity --live --sample 100  # Run actual scans, sample 100 fixtures
cargo dfv parity --live --full     # Run all fixtures (nightly/workflow_dispatch)
cargo dfv parity --workspace       # Workspace contract matrix (Rust/Python/Node/WASM)
```

### When --models is required

Add `--models` to `cargo dfv check` or use `cargo dfv eval --models` when the change touches:
- Files under `crates/core/src/models/`
- Model provisioning scripts (`scripts/provision-*.py`)
- Baseline files (`baselines/eval-models-*.json`)
- Model-related issues or features

### Evaluation and benchmarks

```bash
cargo dfv eval                     # Per-entity precision/recall/F1 for rules
cargo dfv eval --models            # Include model detectors
cargo dfv eval --model datafog     # Evaluate specific engine
cargo dfv eval --update-baseline   # Write current metrics as new baseline
cargo dfv bench                    # Latency benchmark against baseline
cargo dfv bench --runs 10          # Custom run count
cargo dfv bench --update-baseline  # Write current median as new baseline
cargo dfv smoke                    # Install and test packages as user would
cargo dfv smoke --sdk python       # Test only Python
```

## Evidence

Reports are written to `target/dfv/runs/<timestamp>/` and never deleted by the CLI.

Each run directory includes:
- `metadata.json`: git commit SHA, branch, tool versions
- `doctor.json`: environment check results (when --json or failure)
- `check.json`: test results per SDK
- `eval.json`: precision/recall/F1 metrics
- `bench.json`: latency measurements
- `smoke.json`: smoke test results
- `parity.json` / `parity_live.json`: cross-SDK comparison results
- `parity_diffs.json`: diff details when mismatches found
- SDK-specific logs: `rust_tests.log`, `python_tests.log`, `node_tests.log`, `wasm_tests.log`

## Cleanup

Evidence directories accumulate. Remove old runs manually when disk space is a concern.

```bash
(cd target/dfv/runs && ls -t | tail -n +10 | xargs rm -rf)
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
cargo dfv doctor --json           # Check model_weights status
cargo dfv check --models          # Run model-based detector tier
cargo dfv eval --models --json    # Full evaluation with metrics
```

### Engine-specific evaluation

```bash
cargo dfv eval --model datafog    # DataFog engine only
cargo dfv eval --model gliner2    # GLiNER2 engine only
```
