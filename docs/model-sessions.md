# Offline model sessions for 0.4.5-beta.1

Tracks [#56](https://github.com/DataFog/datafog-core/issues/56) and the first
[DataFog integration in #45](https://github.com/DataFog/datafog-core/issues/45),
using [ADR 003](adr/003-optional-model-inference.md).
The release target is **0.4.5-beta.1**. Both DataFog and the optional
[GLiNER2 worker](gliner2-model.md) have working integrations. Publication and
support claims still require the complete installed-package qualification;
unexecuted hosts are not counted as qualified.

The new Rust `models` feature is off by default. Python and Node native builds
include the small supervisor but do not install/download model weights, ONNX
libraries or PyTorch. Existing scans, findings, transformation APIs and
capabilities retain their behavior.

## Explicit provisioning

Provisioning requires Python 3.11 or newer for its standard-library hashing.
This does not change the binding's Python 3.10 minimum. Select the archive for
the host: `linux-x86_64`, `linux-aarch64`, `macos-arm64`,
`macos-x86_64` or `windows-x86_64`.

```sh
hf download DataFog/pii-en-65m \
  runtimes/v0.2.0/datafog-pii-0.1.0-runtime-0.2.0-macos-arm64.tar.gz \
  --revision 7cadd41e21f59b2a149e31378d073cf6ca057531 --local-dir /tmp/datafog-assets
python scripts/provision-datafog-model.py \
  --archive /tmp/datafog-assets/runtimes/v0.2.0/datafog-pii-0.1.0-runtime-0.2.0-macos-arm64.tar.gz \
  --destination /tmp/datafog-bundle
```

The provisioner verifies the published per-platform archive checksum, rejects
links/path traversal, verifies manifest files and installs atomically into a
fresh directory. Keep the original licenses and notices. The runtime requires
the host prerequisites described in its pinned release; notably macOS ARM64
14+, macOS Intel 15+, and the appropriate Windows MSVC runtime.

Create `session.json` with the emitted manifest digest and absolute bundle
path. Limits are mandatory caller configuration. The following are the explicit
budgets used by the integration fixtures, **not qualified performance
targets or library defaults**:

```json
{
  "bundle": "/tmp/datafog-bundle",
  "manifest_sha256": "<digest emitted by the verified provisioner>",
  "limits": {
    "max_input_bytes": 65536,
    "max_response_bytes": 1048576,
    "max_findings": 1024,
    "timeout_ms": 30000,
    "cleanup_timeout_ms": 1000
  }
}
```

Open verifies every manifest file, pinned model/tokenizer/config/calibration,
runtime provenance and platform, then starts the supplied local executable and
probes readiness. It never downloads or silently substitutes a model.

## Demo

```python
import json
from datafog_core import ModelSession, transform

text = "John Smith lives at 123 Main Street, Boston, MA 02110."
with ModelSession.open(json.load(open("session.json"))) as session:
    session.warm_up()
    result = session.scan(text, "combined")
    print(transform(text, result.findings, {"default": {"strategy": "redact"}}).text)
    # Reuse the same session for subsequent texts.
    assert session.scan("The weather is clear.", "model_only").findings == []
```

```javascript
import {readFileSync} from "node:fs";
import {ModelSession, transform} from "@datafog/node";
const session = await ModelSession.open(JSON.parse(readFileSync("session.json")));
try {
  const text = "John Smith lives at 123 Main Street, Boston, MA 02110.";
  const result = await session.scan(text, "combined");
  console.log(transform(text, result.findings, {default: {strategy: "redact"}}).text);
} finally {
  await session.close();
}
```

Node model calls run off the event loop; Python releases the GIL for loading,
inference and cleanup. Rust's equivalent usage example is
`cargo run -p datafog-core --example model_demo --features models --release -- session.json`.

Use `scan_structured` / `scanStructured` for decoded string leaves, then pass
the returned findings to existing structured transformations. Combined mode
retains schema-based PERSON discovery and header-aware rule detection.
Discovery exclusions do not suppress model inference; unrelated leaves never
share model context. Entity selection/allowlists remain transformation policy.

Results contain ordinary findings and compact, final-index source evidence.
The seven native labels map to FIRST_NAME, LAST_NAME, EMAIL, PHONE,
STREET_ADDRESS, CUSTOMER_ID and ACCOUNT_NUMBER. Scores are separate from
calibrated confidence. Source labels survive duplicate merging; request
diagnostics explicitly to retain all original detector findings.

## Lifecycle and boundaries

One session admits one request at a time, with no waiting queue. Concurrent
calls fail with `model_busy`; callers needing parallel inference construct
separate sessions and account for their separate memory. Rust callers can
share a `Cancellation` handle with another thread. Python and Node expose `ModelCancellation`: construct a handle, pass it as the
scan's `cancellation` option, and call `cancel()` from the controlling thread or
event loop. Handles are one-shot; a cancelled handle stays cancelled.
Core reserves admission before Node dispatches blocking work, so a second request
fails immediately with `model_busy`. Each admitted Node model operation uses a
separate standard-library thread rather than the shared libuv pool; unrelated
Node filesystem/crypto work cannot delay model inference or cleanup. The request
deadline starts before dispatch;
a request that expires or is cancelled before execution never enters inference
and leaves the session reusable. Cancellation after inference starts retires the
worker. `close()` signals cancellation before scheduling cleanup.

Timeout or malformed native output retires the worker and marks the session
failed; there is no automatic retry or successful rule-only fallback.
Open a new session to recover. `close` is idempotent and verifies child/task
cleanup; cleanup failures remain visible. `inspect` reports immutable identity,
labels, calibration, limits and state without local asset paths.

This adapter preserves the published operating threshold and the runtime's
512-token windows/stride 42. It adds no outer character windows and rejects
unsupported configuration rather than pretending to support threshold overrides,
other checkpoints, devices or model architectures. A demonstrable need for a
runtime extension must retain the pinned runtime as the equivalence oracle.
The current process adapter targets the pinned single-process DataFog runtime;
general descendant-process supervision for a future backend is not qualified
by these tests.

## Integration checks

Default workspace tests require neither assets nor network. Run real model
qualification explicitly; missing configuration/assets fail rather than skip:

```sh
DATAFOG_MODEL_CONFIG=/absolute/session.json cargo test -p datafog-core --features models --test model_datafog --release -- --ignored
python -I bindings/python/tests/model_conformance.py /absolute/session.json
node scripts/model-node-conformance.mjs /absolute/session.json /absolute/consumer/node_modules/@datafog/node/index.js
```

Build/install the wheel/tarball before binding tests. Reference comparisons use
binary32 epsilon for exposed confidence and binary64 epsilon for JSON-decoded
raw scores; those conversion bounds are distinct from model-quality claims.
Fixtures cover positives, negatives, repeated spans, emoji/combining marks,
long input, structured composition, ordinary transformations and reuse/cleanup.

The manual `Model integration` workflow checks both backends through packaged
Rust and installed Python/Node on all five native targets. It checks loading,
shared results/errors, coordinate conversion and worker cleanup. No accuracy,
performance or memory evaluation is required. Review the results before
publication; the normal publication jobs build their own packages.

## GLiNER2 backend

A session configuration can explicitly select the separately installed official
GLiNER2 Python runtime using the `gliner2` settings. See
[GLiNER2 provisioning and behavior](gliner2-model.md). It reuses the same Core
session lifecycle, canonical findings and transformations. Its raw scores are
uncalibrated and never populate `Finding.confidence`.


Actual-model integration runs only through an explicit workflow dispatch. Routine PR/push checks use asset-free
Core/binding tests. Qualification restores revision-keyed model assets from the
CI cache where available, then rechecks their pinned checksums before provisioning.
Fresh runners still transfer cached bytes; a cache miss fetches the same pinned
release from Hugging Face. Models are never upgraded by a CI check.
