# ADR 003: Optional model inference through reusable Core sessions

- **Status:** Accepted; implementation and release qualification tracked in #56, #45 and #55
- **Date:** 2026-10-19
- **Target release:** 0.4.5-beta.1
- **Tracks:** [#56](https://github.com/DataFog/datafog-core/issues/56)
- **Backends:** [#45: DataFog](https://github.com/DataFog/datafog-core/issues/45),
  [#55: GLiNER2](https://github.com/DataFog/datafog-core/issues/55)
- **Extends:** [ADR 001](001-privacy-core-contract.md) and
  [ADR 002](002-structured-person-discovery.md)

## Context

The initial proposal was checked against Core main at
`cab95117bfce9ddfdeec89ea736ba42107c71d21`. The package manifests are at
0.4.4; main's changes since the 0.4.4 release concern documentation.
The user has requested one published beta containing #56, #45 and #55 to
support a demo, followed by further maintainer testing. The target is
0.4.5-beta.1. Beta status does not relax compatibility, platform support,
atomic failure or qualification requirements. This ADR does not assert that
these features are implemented or ready.

The current implementation gives this decision several constraints:

| Existing component | Consequence for model integration |
| --- | --- |
| `crates/core/src/lib.rs`: `scan_with_config` returns `Vec<Finding>` without an error channel | Model loading and inference need a separate fallible API; default scanning cannot acquire model failures. |
| `ScanConfig`: locale, UUID activation, source format and built-in entity selection | The existing entity parser rejects model labels. Model schema configuration cannot be passed through that parser. |
| `Finding`: public Rust fields, Python constructor and binding conversions | Adding required fields would break callers constructing findings. Preserve the existing shape and use a new result envelope for detailed evidence. |
| `transform`, `PrivacyManager`, and structured provider coordination | Model findings can already enter the transformation pipeline. Do not create a second transformation engine or provider lifecycle. |
| `crates/core/src/offsets.rs`: `TextIndex` | Reuse Core's coordinate conversion and finding validation rather than language-specific offset adapters. |
| `crates/core/src/structured.rs`: private traversal, discovery and header context | Add a shared leaf execution seam in Core; calling text scanning alone would lose structured Authorization-header behavior. |
| `crates/core/src/capabilities.rs`: contract version 1, stable through 0.4.x | Keep the global rule inventory's meaning; model availability belongs to instance inspection. |
| `scripts/check-release.py` and four publish workflows | Existing Rust, Python, Node and WASM versions are aligned, but publishing uses separate language tags/jobs. A GitHub release alone does not publish them. |

The reviewed DataFog runtime already loads once and accepts successive JSONL
requests. It owns tokenization, windows, BIO decoding, fragment resolution and
calibration. The reviewed MCP adapter adds process management, span conversion
and a restricted label mapping; it also uses character-level outer windows.
Neither that restricted mapping nor those outer windows become Core defaults.

GLiNER2 has an official Python local-inference runtime with span output.
Its span and boundary architectures are distinct. A common session contract
does not establish that its checkpoint runs in DataFog's ONNX runtime.

## Decision

### 1. Core owns orchestration; workers own qualified model execution

Introduce a feature-gated `datafog_core::models` module in `crates/core`,
with a `models` Cargo feature disabled by default. It owns configuration
validation, process supervision, span conversion, label mapping, findings,
composition, provenance and structured execution. Bindings translate types
and coordinate calls; they do not implement independent detector adapters.

Use supervised local child processes for both initial backends:

| Backend | Proposed execution component | Detection responsibilities |
| --- | --- | --- |
| DataFog pii-en-65m | Core-owned Rust adapter around the pinned native executable initially | Preserve tokenizer, BIO decoding, windows, fragments and calibration; convert native output into the shared result contract. |
| GLiNER2 | An optional project-owned Python worker distribution around the pinned official local runtime | Load the qualified span checkpoint, invoke official extraction with spans/scores and expose its documented window behavior. |

The adapter and GLiNER2 worker are proposed additions. Worker/adapter source
belongs to this project, outside language-binding implementation code.
The GLiNER2 shim serializes native runtime output; canonical label mapping,
validation and policy stay in Rust. There is no runtime plugin discovery,
arbitrary Python callback backend, hosted-inference fallback or required API.

Process isolation is chosen because a blocked native/PyTorch call cannot be
reliably cancelled by dropping a Rust future or Python thread. A supervisor can
retire the worker, kill its process tree and reap it. This costs IPC, process
startup and per-session model memory. It is an execution boundary, not a
security sandbox or a universal guarantee against host-level OOM.

An in-process backend is deferred until it can meet the same cancellation,
resource and cleanup contract with qualification evidence. Do not embed
PyTorch in the Python extension or add ONNX Runtime to the base Core library.

### 2. Optional packaging and provisioning are explicit

The Rust supervisor uses existing serde/serde_json, sha2 and standard process,
thread and synchronization facilities where sufficient. Any process-tree or
platform resource-control dependency must justify a specific portability gap.
ONNX/tokenizer dependencies remain in the optional DataFog runtime; official
GLiNER2/PyTorch dependencies belong to the optional GLiNER2 worker distribution.

Native Python wheels enable the lightweight Core supervisor at build time,
while base installation and import install/load neither worker runtime nor
model weights. Provision the existing pinned DataFog native archive rather than
requiring a replacement executable. Distribute the GLiNER2 shim as a separate
`datafog-core-gliner2` Python package, installed through the opt-in
`datafog-core[gliner2]` extra and pinned to the matching Core release.
The worker package pins its qualified official GLiNER2/PyTorch dependencies.
The extra installs code/dependencies, never downloads weights or starts inference.
Rust callers configure the provisioned DataFog executable directly; Core does
not search PATH or invoke a shell to resolve a worker. The GLiNER2 backend
selects a verified Python environment/module with the same explicit ownership.
The same Rust supervisor is enabled in Node native packages; asynchronous Node
calls snapshot inputs and execute away from the event loop. No Python-only
or Node-only detector adapters are introduced.

A separately invoked provisioner resolves immutable revisions, verifies
archive paths and all manifest assets, and atomically installs into a
deterministic caller-selected cache. A local pre-provisioned bundle is sufficient
for inference and offline containers. Provisioning is not a scanner side effect.
Credentials belong only to provisioning, never to model identity or diagnostics.

The trusted bundle manifest identifies backend, architecture, model and runtime
versions/revisions, file hashes and sizes, labels/mappings, tokenizer/window
configuration, calibration if applicable, license/attribution and target ABI.
The caller supplies the expected manifest digest independently of downloaded
contents. Hash agreement with an untrusted manifest alone is not authenticity.
Validate the worker executable/package and runtime libraries as well as weights.
Do not mutate an installed bundle used by a session; upgrades install a new
content identity. Reject incompatible manifests, partial bundles, path escapes
and unsupported targets before ready status.

Workers load only verified local assets, disable remote-code loading and
model-library download fallback, and do not use hosted clients. Test inference
with network access denied. Offline library settings are not described as an
OS network sandbox. A relocated equivalent bundle retains its content identity;
local paths and credentials are excluded from inspection and identity hashes.

### 3. A new fallible session preserves stateless APIs

The proposed API vocabulary is:

```text
ModelSession.open(ModelSessionConfig) -> Result<ModelSession, ModelError>
session.inspect()                    -> ModelSessionDescriptor
session.warm_up(CallOptions)          -> Result<(), ModelError>
session.health(CallOptions)           -> Result<Health, ModelError>
session.scan(text, mode, CallOptions) -> Result<ModelScanResult, ModelError>
session.scan_structured(data, mode, CallOptions)
                                     -> Result<ModelStructuredScanResult, ModelError>
session.scan_and_transform(text, mode, transform_config, CallOptions)
                                     -> Result<TransformResult, ModelError>
session.close()                       -> Result<(), ModelError>
```

These signatures specify responsibilities; binding spellings follow existing
snake_case/camelCase conventions. Rust exposes a synchronous blocking API.
Python exposes the installed native session with a context manager and releases
the GIL while loading, waiting, running and closing, after copying/validating
Python-owned inputs. Cooperative cancellation uses a request cancellation handle;
dropping a waiting caller must not detach unbounded background work.
An asynchronous binding must use the same supervisor and cancellation ownership.

`ModelSessionConfig` has a discriminated backend selector, local bundle and
expected digest, worker/runtime identity, CPU device, immutable model schema,
qualified resource profile, rule scan configuration and structured policy.
Reject unknown fields, unsupported device/configuration, null-for-omission,
duplicate/empty labels and nonfinite/out-of-range thresholds.

The initial modes are `model_only` and `combined`, supplied explicitly per
request and recorded in the result. Existing stateless APIs are the rule-only
path. One session selects one backend/model; multi-model ensembles are deferred.
Model/labels/descriptions/thresholds, rule settings and structured policy are
frozen per session. Reconfiguration constructs a new session.

Keep `ScanConfig.entities` as a built-in rule filter: locale and UUID activation
still apply independently. Model labels and per-label thresholds use their own
schema. They neither activate built-in detectors nor broaden locale/model
coverage. Transformation entity selection stays separate and extensible.

The existing `scan`, `scan_with_config`, `transform`, `scan_and_transform`
and structured equivalents retain their signatures, defaults and behavior.
Supplied-findings transformation never starts or consults a model session.
Provider-backed callers pass returned ordinary findings to existing
`PrivacyManager.transform` / structured transform. Session convenience methods
delegate to the same transformation kernel, with provider requirements intact.
Do not invoke providers until the complete requested scan has succeeded.

### 4. Lifecycle, protocol and resource ownership

A session progresses through `loading -> ready -> failed/closed`;
`failed -> closed` is allowed and `closed` is terminal. Construction returns
only after validation and a backend readiness check; loading is observable through
the construction operation, not a partially usable scanner. Warm-up is explicit
and uses synthetic input. Health reports verified worker/session status, not
just that a PID exists. Invalid caller configuration/input does not poison a
healthy session.

Each session serializes execution through one worker. This follows the reviewed
DataFog runtime's mutable detector and serialized request protocol; concurrency
uses explicitly constructed sessions with separately measured memory.
No hidden worker pool or global model cache is introduced.

Keep one request in flight per worker and correlate requests in the Rust
supervisor. Backend adapters translate their native transport into the shared
Core result contract below. Do not require every executable to implement a new
universal handshake, health, close or request-ID protocol.

Initially use DataFog's existing JSONL transport. Verify executable/bundle
identity locally, establish readiness with a bounded synthetic inference,
and sanitize native errors without forwarding their messages. Disable debug
output, bound response decoding and discard native redacted-text output.
The GLiNER2 shim uses a small versioned JSONL schema with explicit spans and
scores. Newlines in input remain JSON-escaped; stdout is machine-readable only.
Unexpected, malformed or excess responses retire the worker; never reuse a
stream after a timeout.

Before replacing the pinned DataFog executable, #45 must demonstrate a specific
gap that its Rust adapter cannot meet, such as threshold override support or a
required resource control. Record that gap and the smallest worker change.
If replacement is necessary, give it a new artifact/runtime identity, retain the
original release bytes as the compatibility oracle and rerun equivalence and
all-platform qualification. Do not fork/rewrite inference merely to standardize
transport. Metadata for legacy transports is derived from the verified bundle
and qualified adapter, not invented as a worker handshake response.

Use finite admission queues with byte/count accounting and explicit limits for
input bytes, output bytes/findings, queued bytes/requests, CPU threads, startup,
execution and cleanup. A qualified resource profile supplies actual values
derived from backend/host measurements; callers may choose documented validated
overrides. This draft invents no numerical latency, memory or throughput limits.
Profile values and override constraints must be fixed before release.
Reject a requested hard resource limit if the platform cannot enforce it;
measured RSS or thread settings are not hard memory/CPU isolation.

One monotonic request deadline covers admission, queue waiting, IPC, every
window/structured leaf and scan-result validation. It is not reset per leaf.
Transformation/provider timeouts retain their existing owners; a model scan
deadline does not promise cancellation/rollback of external provider effects.
Startup, warm-up, health and close also have bounded operation deadlines.

Queued timeout/cancellation removes that request without killing another caller's
active work. Active timeout/cancellation, malformed worker output, worker death
or resource failure retires the worker, rejects queued work without replay and
sets the session failed. Terminate the process tree, close pipes and reap within
the profile's cleanup budget. Report inability to verify cleanup as a sanitized
cleanup failure; do not report the session healthy.
There is no automatic request retry or worker restart in the first release.
Explicitly opening a new session is the recovery path.

`close` is idempotent, rejects new submissions, cancels pending work and
verifies cleanup; it surfaces failures. Ownership includes process handles,
reader/writer tasks and outstanding request buffers. Destruction is a
non-panicking safety net, not a substitute for explicit close; supervisor
cleanup failures remain inspectable without retaining payloads.
Workers keep weights/tokenizers, not prior request text/results; release
request buffers on all exits. Allocator reclamation is not secure zeroization
of arbitrary framework memory.

### 5. Ordinary findings plus inspectable evidence

Preserve the public `Finding` struct and binding constructor. Before backend
implementation, #56 defines these minimum Core-owned types and shared fixtures:

```text
NativeSpan {
  native_label,
  start, end, coordinate_unit: utf8_byte | unicode_codepoint,
  raw_score?: { value, scale_id },
  calibrated_confidence?,
  matched_text?
}
BackendResult { spans: [NativeSpan], coverage: complete }
ModelScanResult {
  contract_version, configuration_id, mode,
  findings: [Finding],
  evidence: [{ finding_index, sources: [SourceEvidence] }]
}
SourceEvidence {
  detector_name, detector_version,
  native_label?, raw_score?, calibrated_confidence?
}
```

The verified backend descriptor defines label mappings, each score scale and
its permitted range, calibration identity and chunking/coverage guarantees.
Native start/end offsets are zero-based and half-open into the exact text sent
to the adapter; adapters rebase chunk-local ranges before returning them.
Coverage means that all input windows were processed successfully, not that all
PII was detected. A qualified adapter establishes coverage from its runtime
behavior; it must not assert completeness for a truncating execution path.

Evidence indexes address the final findings array, cover every finding and
contain every source contributing to an exact-duplicate group. They are
rebuilt after ordering/deduplication, never left pointing at pre-merge indexes.
Structured results add discovery mappings and `{path, finding}` entries;
evidence indexes address that ordered entry array and cannot cross leaf paths.

Default results retain source labels/scores through compact evidence, without
copying every original finding into a second detector-results collection.
An explicit diagnostics option returns original per-detector findings when
needed for evaluation, including duplicate losers. It is off by default and
counts toward the same output limits. Both backends use these types; transport
schemas are adapter details, not a public plugin framework.

Every model finding uses a stable backend/native-label detector name and a
detector version containing the adapter version and immutable session identity.
Its descriptor resolves that identity to model revision, manifest/assets,
runtime/worker versions, native-to-canonical mappings, descriptions, thresholds,
calibration, chunking, device and resource settings. Native-label-specific
detector names keep many-to-one mappings inspectable after a finding is passed
alone to existing transforms; transformation records already retain detector
name/version. Persist the descriptor with results when durable full provenance
is needed. Do not encode paths, input text or credentials in these fields.

Compute the configuration identity with SHA-256 over a versioned canonical
serialization of verified content digests and effective settings, including
Core/adapter/protocol versions. Specify field ordering, numeric encoding and
omission rules once in Rust. Bindings return Core's identity and test parity;
they do not implement separate canonical serializers. Local asset paths,
timestamps, PIDs, request IDs and payloads are excluded. Per-request mode is recorded
separately; identical configuration identity does not promise bit-identical
floating-point execution across hosts.

Validate native output atomically before publishing findings: known labels,
integral in-bounds non-empty ranges, valid UTF-8 boundaries after conversion,
consistent exact matched text and scores within their descriptor-declared
scale. Reject NaN/infinity and finite values outside that scale before applying
thresholds or publishing evidence. Normalized scores and calibrated confidence
must lie in inclusive 0..=1; logits may use an explicitly declared unbounded
finite scale and must not be passed off as normalized confidence. Reuse Core
finding validation for calibrated confidence; evidence-only raw scores require
their own scale validation. Include negative fixtures for -0.1 and 1.4 on
normalized scales, including GLiNER2 with `Finding.confidence` absent.
Use `TextIndex`
for Python code-point-to-byte conversion and derived public ranges. Never
locate repeated values with string search, normalize text or trim span edges.
One malformed record fails the entire request, including earlier valid leaves.
Expose no partial findings or transformed output as complete detection.

For DataFog compatibility mode, populate `Finding.confidence` using the
qualified published calibration; preserve raw score separately in evidence.
For uncalibrated GLiNER2 output, retain the model score and its semantics in
evidence and leave `Finding.confidence` absent. Per-label filtering uses the
declared native score scale. Do not label either raw score a calibrated
probability, fabricate confidence for composed spans or give rules a score.

### 6. Composition reuses existing selection and overlap behavior

`model_only` returns the selected model's qualified output.
`combined` runs the unchanged enabled rule pipeline and that model, validates
both, then creates a deterministically ordered union. Keep meaningful overlaps
until transformation. Collapse exact duplicates with the existing ADR 001
preference; preserve contributing source evidence for each retained finding.
The explicit diagnostics option retains detector-specific originals for
evaluation. Default composition does not duplicate matched text for diagnostics.

Reuse Core's duplicate/overlap implementation rather than reimplementing it in
the supervisor or bindings. Ordering uses source start, decreasing end, entity
type and stable provenance ties; structured paths retain existing traversal
order. Never rank absent rule confidence as zero or treat raw GLiNER2 scores
as calibrated confidence. No new entity priorities or model-wins rule.

Transforms receive ordinary findings and retain ADR 001 processing order:
validate everything, select entities, apply allowlists, resolve duplicates and
overlaps, choose strategy, then mutate. Existing provider contracts and limits
on external-side-effect atomicity remain unchanged.

### 7. Structured execution preserves discovery and leaf boundaries

Reuse/factor the existing Core leaf traversal and schema discovery, keeping JSON
conversion constraints, lexicographic object/numeric array traversal, header
context and shape preservation. Do not duplicate traversal in Python or flatten
serialized JSON into a model prompt.

In `combined` mode, keep all existing rule/schema behavior and add model
inference per decoded string leaf. `model_only` returns model findings only;
discovery mappings can still be reported as metadata, without adding
schema-generated PERSON findings. Neither mode joins names or context across
fields, array elements, records or requests.

Existing `exclude` continues to suppress automatic PERSON discovery only;
explicit PERSON mappings and the current conflict rules are preserved.
Model inference scans every decoded string leaf independently. The beta adds
no new model-specific path-exclusion language: discovery exclusions do not
suppress inference, and transformation allowlists affect transformation only.
Document this distinction in structured demo examples.

No supported leaf may be silently skipped because its inference fails or is
too long. Request limits cover the whole document as well as individual leaves.
Return string-local byte/code-point ranges, plus UTF-16 for any advertised JS
binding. Unsupported model/structured combinations reject before inference.
Ordinary text scanning of JSON remains text; CSV parsing is application-owned.

### 8. Windowing and backend semantics remain separately qualified

The common interface requires complete input coverage, explicit token/input
limits and exact rebasing. It does not impose one tokenizer, chunk size or
cross-window span merger on both backends.

| Backend | Required initial compatibility |
| --- | --- |
| DataFog | Pinned pii-en-65m 0.1.0/runtime 0.2.0 assets from #45, seven labels/fifteen BIO logits, maximum length 512, token stride 42, fragment handling and published isotonic threshold/calibration. |
| GLiNER2 | A pinned span-architecture privacy-filter checkpoint and official runtime, checkpoint-supported labels/descriptions, validated per-label thresholds, documented tokenizer/window/overlap settings. |

The DataFog mapping preserves FIRST_NAME, LAST_NAME, EMAIL, PHONE,
STREET_ADDRESS, CUSTOMER_ID and ACCOUNT_NUMBER. Preserve native first/last
distinctions; do not join into whole-name PERSON or relabel customer/account
identifiers as banking/sensitivity assertions. The adapter mapping is versioned
separately from any legacy bundle mapping metadata.

For GLiNER2, initially qualify `street_address -> STREET_ADDRESS`; keep
`address` distinct and evaluate/document its broader meaning before advertising
it. Do not infer GLiNER2.5 boundary support or arbitrary-category accuracy.
Pin the actual runtime/checkpoint revision and hashes during #55, before
implementation tests. Current upstream documentation is an integration
reference, not a tested release lock.

Preserve DataFog's explicit full-window entity-boundary rejection and other
reference behavior; sanitize it as a model failure rather than inventing a
complete span. Do not add MCP's outer character windows or generic Transformers
aggregation. For GLiNER2, qualify official long-input behavior or a documented
project-owned runtime adapter and prove coverage/rebasing; silent truncation
is never success.

### 9. Errors and capability contracts are additive

Introduce `ModelError` for new session operations, preserving the structured
`code`, `reason`, `path` and optional `finding_index` envelope. Existing
`PrivacyErrorCode` and existing functions retain their error behavior; this
avoids adding variants to that public enum merely to support a new API.
New convenience methods wrap existing configuration/finding/provider errors
without changing their fields or exception meaning.

Proposed new session error codes are `model_unavailable`, `model_load_failed`,
`model_inference_failed`, `model_timeout`, `model_cancelled`, `model_busy`,
`model_resource_exhausted`, `model_session_failed`, `model_session_closed`
and `model_cleanup_failed`. Stable reasons distinguish missing assets/runtime,
integrity/version/protocol failures, unsupported platform/device, invalid
output and size limits. Configuration errors continue to identify the invalid
option. Final reason/path tables belong to the contract implementation tests.

Use configuration paths such as `/backend`, `/bundle`, `/labels/0`,
`/resources`, and `/data`; runtime faults use the operation location, not a
filesystem path. Finding indexes refer to the documented result/output list,
never whichever subset survives selection. Python uses ValueError-derived
configuration/finding errors and a RuntimeError-derived model exception;
future JS bindings preserve the canonical fields. Do not forward raw stderr,
Python tracebacks, native exception strings, source snippets or sensitive paths.
Drain/discard worker stderr without persisting payloads or blocking the worker.
There is no default logging of text, telemetry export or request persistence.

Keep `capabilities()` contract version 1 as the built-in executable detector
inventory. Add a separately versioned instance descriptor through `inspect()`
for session state and enabled model support. Compiled supervisor availability
does not mean a worker, model bundle or qualified device is installed.
Descriptions of inference location must say local worker, not inference inside
the Core binary.

### 10. Release 0.4.5-beta.1 delivers the demo with full native support

Track #56, #45 and #55 under the 0.4.5-beta.1 milestone. The purpose is a
published demo-capable beta for further maintainer testing. Build the minimum
shared contract alongside #45, then integrate #55 through it. #56 can close
with its first real backend proof; publication waits for all three issues and
the full matrix below. Beta status is not a platform deferral or permission to
break existing APIs.

Both backends must execute through Rust, installed Python and installed Node
on every existing native target:

| Platform | Rust target / native package target |
| --- | --- |
| Linux x86_64 GNU | `x86_64-unknown-linux-gnu` |
| Linux ARM64 GNU | `aarch64-unknown-linux-gnu` |
| macOS Apple Silicon | `aarch64-apple-darwin` |
| macOS Intel | `x86_64-apple-darwin` |
| Windows x86_64 MSVC | `x86_64-pc-windows-msvc` |

Rust uses the optional models feature. Python and Node use the same Core
supervisor. GLiNER2 remains a local Python worker even when called from Rust
or Node; its explicitly installed environment is a documented prerequisite.
No row/backend/binding may be silently omitted, marked optional or counted as
passing through skipped real-model tests. Pin the supported host/runtime
prerequisites and exercise the installed packages on their actual hosts;
cross-compilation alone does not qualify model execution.

Preserve existing Rust/Python/Node public APIs, Python constructors/typing,
Node event-loop behavior, coordinate semantics and supported base-package
language/toolchain ranges. Resolve runtime-version compatibility across that
existing supported range during qualification; do not narrow base-package
support to accommodate a model dependency. Optional workers remain separately
installed and their requirements explicit. A dependency/platform blocker must
be resolved before publishing this beta rather than deferring that platform.

Browser WASM retains its existing rule/structured APIs, defaults and package
support. Local-process model execution remains explicitly unsupported, as in
#45/#55; no new browser model port or hosted fallback is implied. If an exposed
model entry point is invoked there, return a structured unsupported-platform
error. Never accept and ignore model configuration in an existing scan API.

Publish the aligned existing Core crate/Python/Node/WASM packages, GLiNER2
companion and any demonstrably necessary worker additions from the verified
source. Keep the pinned DataFog release and upstream model/runtime identities
unchanged when using their original artifacts.

Use these equivalent prerelease versions and channels:

| Surface | Version / publication setting |
| --- | --- |
| Cargo packages and npm packages | `0.4.5-beta.1` |
| Python distribution metadata, including the companion | `0.4.5b1` (PEP 440) |
| GitHub release | `v0.4.5-beta.1`, marked prerelease |
| npm publication | Explicit `beta` dist-tag; do not move `latest` |

See the [Python prerelease specification](https://packaging.python.org/en/latest/specifications/version-specifiers/#pre-releases)
and [npm dist-tag guidance](https://docs.npmjs.com/adding-dist-tags-to-packages/).

Update `scripts/check-release.py` to compare the same logical beta release
across these spellings and validate each ecosystem's expected tag. The current
language publish refs become `rust-v0.4.5-beta.1`, `python-v0.4.5-beta.1`,
`node-v0.4.5-beta.1` and `wasm-v0.4.5-beta.1`, all resolving to the qualified
source. A GitHub release alone does not trigger those jobs. Adapt companion
packaging and release checks accordingly; do not bump package versions in this
ADR-only change.

Use existing build/publish jobs with the smallest required qualification and
artifact handoff changes. Retain and test the actual wheel/npm/worker bytes
that will be published; qualify crate source through a packaged-crate install.
Record checksums and per-package publication results. Do not build a general
release orchestrator. After a partial registry failure, resume only missing
qualified publications and never replace already published artifacts.

The beta still requires compatibility, actual-model positive/negative checks,
Unicode/structured conformance and bounded failure/cleanup evidence. Maintainer
follow-up testing informs later betas or the stable release; publishing stable
0.4.5 is not part of this beta target. Demo consumer wiring is separate from
these Core issues, while reproducible Core demo commands belong in their docs.

## Alternatives considered

- **Keep consumer adapters.** Leaves loading, mapping, timeout and Unicode
  behavior duplicated, which is the gap #56 addresses.
- **One in-process ONNX backend for both models.** Does not establish GLiNER2
  compatibility and leaves hard cancellation unresolved.
- **Python owns orchestration.** Makes Rust consumers depend on binding logic
  and violates the project's ownership boundary.
- **Add model fields to Finding and the global capability inventory.** Risks
  existing Rust constructors and changes a stable inventory's meaning.
  New envelopes/descriptors provide the needed evidence without that break.
- **General plugin framework or automatic worker pool.** Neither initial
  backend requires those abstractions; explicit sessions satisfy reuse.

## Evidence required before implementation is called supported

1. Shared conformance covers exact/repeated values, Unicode and combining
   characters, emoji, all advertised coordinate units, empty input, overlaps,
   label/threshold validation, identity/provenance and complete long inputs.
   Validate malformed later findings/leaves before returning anything.
2. Default imports/scans/structured discovery and supplied-findings transforms
   pass installed-package regressions without assets, model dependencies or
   network. Session transforms reuse existing provider restrictions.
3. Fault tests prove bounded queueing, startup/deadlines/cancellation, protocol
   failures, corruption, missing runtimes/devices, exhaustion, worker death and
   verified process/task cleanup on each supported OS. Include queued versus
   active cancellation and cross-request leakage checks.
4. #45 executes the pinned real model through Rust and installed Python/Node
   on all five required native targets, matching the reference runtime including
   calibration/window boundaries with zero actual-model skips. Record ABI/host
   prerequisites and declared numerical tolerances; old qualification does not
   automatically transfer to the new integration.
5. #55 executes the pinned real GLiNER2 checkpoint through Rust and installed
   Python/Node on all five required native targets, proves street-address
   extraction/redaction on positive and legitimate negative examples, and reports
   misses/false positives with denominators on separate development and held-out
   fixtures. Extra labels require evidence; required platforms cannot be deferred.
6. Report model-only and combined-system quality separately. Record DataFog
   cold load, warm throughput, p50/p95, peak memory and package/import impact;
   #55 additionally requires at least 100 warm runs per representative payload
   size. Include versions/hashes, hardware/OS, thread/concurrency settings,
   labels/thresholds, payload lengths and first-inference timing. Use those
   measurements to fix the qualified resource profiles, not invented targets.
7. Check final packaged Rust and installed Python/Node execution, unsupported
   WASM behavior, offline provisioning/container examples, inspection and
   cleanup. Pin and retain the tested artifacts and licenses.
8. Before Rust changes merge, run `cargo fmt --all --check`,
   `cargo clippy --workspace --all-targets --all-features -- -D warnings`
   and `cargo test --workspace --all-features`, plus installed-binding and
   actual-model suites. Default workspace tests must not require network or
   model weights; explicit qualification jobs supply pinned local assets and
   fail if required model execution is skipped.

## Consequences and remaining release inputs

The public path is one reusable Core session contract with separately qualified
backends. Process supervision and installation become explicit responsibilities,
and each additional session can duplicate model memory. Complete model failure
remains visible rather than becoming successful partial protection.

Before release, #45/#55 must supply the exact worker packages/artifacts,
GLiNER2 revision/runtime lock, evidence for the fixed platform matrix,
resource-profile values, numerical tolerances, score/threshold evidence and
installation commands.
These are required implementation/qualification outputs, not permission to
claim support from this draft. API spelling and protocol schemas must be
frozen in shared fixtures before bindings are released. Shared configuration
identity is computed only in Core, and normalized-score rejection tests cover
both canonical confidence and evidence-only scores.

The release includes these three Core issues; downstream MCP adoption, model
training/evolution, generic plugins, browser inference and authorization of
disclosures remain outside this decision. Acceptance of this decision does not establish completed implementation or
qualification; the backend issues retain those release gates.

## Reviewed references

- Core main at [cab9511](https://github.com/DataFog/datafog-core/tree/cab95117bfce9ddfdeec89ea736ba42107c71d21):
  [core operations/configuration](https://github.com/DataFog/datafog-core/blob/cab95117bfce9ddfdeec89ea736ba42107c71d21/crates/core/src/lib.rs),
  [offsets](https://github.com/DataFog/datafog-core/blob/cab95117bfce9ddfdeec89ea736ba42107c71d21/crates/core/src/offsets.rs),
  [structured execution](https://github.com/DataFog/datafog-core/blob/cab95117bfce9ddfdeec89ea736ba42107c71d21/crates/core/src/structured.rs),
  [capabilities](https://github.com/DataFog/datafog-core/blob/cab95117bfce9ddfdeec89ea736ba42107c71d21/crates/core/src/capabilities.rs),
  and [release version checks](https://github.com/DataFog/datafog-core/blob/cab95117bfce9ddfdeec89ea736ba42107c71d21/scripts/check-release.py).
- [DataFog release lock](https://github.com/DataFog/DataFog-models/blob/31a4015e20fd220d46eb579d53151701175bf981/models/pii-en-65m/release.json)
  and [reference runtime](https://github.com/DataFog/DataFog-models/blob/31a4015e20fd220d46eb579d53151701175bf981/runtime/src/main.rs).
- [Reviewed MCP adapter](https://github.com/DataFog/datafog-mcp/blob/f31d6d99970218b2c8c80f3b5dafb56ddb857f3d/src/datafog_mcp/model_runtime.py).
- [Official GLiNER2 runtime documentation](https://github.com/fastino-ai/GLiNER2)
  and [candidate checkpoint card](https://huggingface.co/fastino/gliner2-privacy-filter-PII-multi),
  consulted 2026-10-19; actual compatibility remains a pinned #55 release gate.
