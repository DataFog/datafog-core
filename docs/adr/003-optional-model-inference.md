# ADR 003: Optional local model inference

Status: Accepted; implementation tracked in #56, #45 and #55.

## Scope

DataFog Core loads the DataFog and GLiNER2 models locally and exposes the same
findings and errors through Rust, Python and Node. Existing rule-based APIs keep
their 0.4.4 behavior. The Rust `models` feature is optional; WASM stays rule-based.

Supported native platforms are Linux x86_64 GNU, Linux ARM64 GNU, macOS ARM64,
macOS Intel and Windows x86_64 MSVC. Platform-specific runtime dependencies must
remain supported, including the older PyTorch stack required on Intel macOS.

## Implementation

- Use DataFog's published native runtime, retaining its tokenizer, labels,
  calibrated confidence and window handling.
- Use the official GLiNER2 Python runtime. Ship its private runner inside the
  existing `datafog-core` Python package. Install its dependencies with the
  optional `gliner2` extra; do not create a separate worker distribution.
- Rust and Node callers select a Python interpreter with that package and extra
  installed. Ordinary package imports do not load GLiNER2 or download weights.
- Explicit setup downloads pinned assets and verifies their contents. Opening a
  session checks files, platform and runtime readiness before returning success.
- Core owns label mapping, output validation and offset conversion. Bindings
  expose those results without duplicating detection logic.

## Public behavior

A session supports `open`, text `scan`, cancellation and `close`. Reuse loaded
models between calls. Results contain a format version, model name, findings
and corresponding score evidence. Raw model scores stay separate from calibrated
confidence. Preserve byte and code-point positions; Node also exposes UTF-16
positions. Validate every returned span before applying configured thresholds.

Invalid settings, missing assets/dependencies, incompatible runtimes, failed
inference, malformed output, timeouts and cleanup failures return consistent,
value-free errors. Never return partial findings or silently fall back to rules.
Caller-selected input/output limits and deadlines bound operations. Reject
concurrent requests with a busy error. Cancellation and close stop active work
and clean up its process. Python releases the GIL and Node keeps its event loop
free while work runs.

`scan` takes the rule scanner's configuration and routes each requested entity
type to one engine: rule detectors for every type they support, the model only
for its remaining labels. As with the rule scanner, an omitted configuration
equals `{}`, so a session returns the rule scanner's findings plus the model's
other labels. A request that needs no model label does not use or reserve the
worker. Findings for one entity type never come from both engines, so results
need no cross-engine merging or confidence reconciliation. `scan_model` returns
raw model output for evaluation and parity checks.

Model scans of JSON documents, diagnostic copies, configuration fingerprints,
inspection reports, and separate warm-up/health methods are outside this
change. Existing rule-based JSON and transformation APIs remain available.

## Verification and release

Routine CI checks Core and the installed bindings without downloading models.
The manually dispatched model integration workflow checks both models through
Rust and installed Python/Node packages on all five native platforms. Test
loading, returned spans and scores, Unicode offsets, reuse, errors, cancellation
and process cleanup. Do not grade model accuracy or impose speed/memory targets.

Publication must require the latest model integration run for the exact release
commit to pass every backend/platform job. Missing, failed or unfinished tests
block publication. Build release packages from source; no cross-run package
handoff or saved-file publication system is required.

Target `0.4.5-beta.1` for Cargo/npm and `0.4.5b1` for Python. Use npm's `beta`
tag and mark the GitHub release as a prerelease. Review the final diff and test
results before publishing.
