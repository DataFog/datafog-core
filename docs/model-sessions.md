# Local model sessions

DataFog Core supports DataFog and GLiNER2 on Linux x86_64 GNU, Linux ARM64 GNU,
macOS ARM64, macOS Intel and Windows x86_64 MSVC. Existing rule-based APIs are
unchanged. No model downloads or model-runtime imports happen on ordinary import.
Rust callers enable the optional `models` feature. WASM remains rule-based.

## DataFog setup

Download the archive for the host (`linux-x86_64`, `linux-aarch64`, `macos-arm64`,
`macos-x86_64` or `windows-x86_64`) and verify/install it with the provisioner.
The following selects macOS ARM64; replace that platform for other hosts.
Provisioning scripts require Python 3.11 or newer; the binding supports 3.10+.

```sh
hf download DataFog/pii-en-65m \
  runtimes/v0.2.0/datafog-pii-0.1.0-runtime-0.2.0-macos-arm64.tar.gz \
  --revision 7cadd41e21f59b2a149e31378d073cf6ca057531 --local-dir /tmp/datafog-assets
python scripts/provision-datafog-model.py \
  --archive /tmp/datafog-assets/runtimes/v0.2.0/datafog-pii-0.1.0-runtime-0.2.0-macos-arm64.tar.gz \
  --destination /tmp/datafog-bundle
```

Keep the supplied licenses/notices and host runtime prerequisites: macOS ARM64
14+, macOS Intel 15+, or the appropriate Windows MSVC runtime. Opening verifies
files and starts the runtime. Downloads remain a separate setup step.

Create `session.json` with the absolute bundle path and emitted manifest digest.
Limits are required caller settings. These are integration-test example budgets,
not performance targets or library defaults:

```json
{
  "bundle": "/tmp/datafog-bundle",
  "manifest_sha256": "<digest emitted by the provisioner>",
  "limits": {
    "max_input_bytes": 65536,
    "max_response_bytes": 1048576,
    "max_findings": 1024,
    "timeout_ms": 30000,
    "cleanup_timeout_ms": 1000
  }
}
```

See [GLiNER2 setup](gliner2-model.md) to use that model instead.

## Python and Node

```python
import json
from datafog_core import ModelSession

with open("session.json") as file:
    config = json.load(file)
with ModelSession.open(config) as session:
    result = session.scan("Contact John Smith at john@example.com.")
    print(result.model, result.findings, result.evidence)
```

```javascript
import {readFileSync} from "node:fs";
import {ModelSession} from "@datafog/node";
const session = await ModelSession.open(JSON.parse(readFileSync("session.json")));
try {
  const result = await session.scan("Contact John Smith at john@example.com.");
  console.log(result.model, result.findings, result.evidence);
} finally {
  await session.close();
}
```

Reuse the session for later texts. Python releases the GIL; Node runs model work
outside the event loop. Use `ModelCancellation` and the scan's `cancellation`
option to cancel from another thread/call. `close` is safe to repeat and stops
active work. A request that fails during inference retires its worker; open a
new session to continue. Concurrent scans on one session return `model_busy`.

Both models return `contract_version`/`contractVersion`, `model`, `findings` and
`evidence`. Findings retain the existing entity, text and offset fields. Python
exposes byte/code-point ranges; Node also exposes UTF-16 ranges. Evidence keeps
raw score values and their scale separate from calibrated confidence. DataFog
retains its published calibration; GLiNER2 leaves finding confidence absent.

Configuration errors use Python's existing `DataFogConfigurationError`; model
failures use `DataFogModelError`. Node uses `DataFogError`. All carry `code`,
`reason`, `path` and an optional finding index. Native messages, text and local
paths are not included. Missing files/dependencies, invalid output, timeouts,
cancellation and cleanup failures never return partial findings or rule fallback.

## Tests and publication

Routine CI does not download models. Manually run the `Model integration`
workflow for both models on all five native platforms before releasing.
It checks Rust and installed Python/Node packages, including failures and cleanup.
Publishing is blocked unless the latest run for the exact release commit passes
all ten backend/platform jobs. Release packages build from source.

Review the diff and passing tests before publishing. Target Cargo/npm
`0.4.5-beta.1` and Python `0.4.5b1`; keep npm on the `beta` tag and GitHub releases
marked as prereleases. Python publishes only the existing `datafog-core` project;
its trusted publisher uses repository `DataFog/datafog-core`, workflow
`publish-python.yml`, environment `pypi`. Upload retries skip existing files.
