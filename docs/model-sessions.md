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
    result = session.scan("Contact John Smith at john@example.com.")  # rules + model
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

## Choosing rules or the model per entity type

`scan` routes each entity type to one engine. It takes the rule scanner's
configuration (`entities`, `locale`, `detect_uuid`, `format`), where `entities`
may also name the session's model labels. As with the rule scanner, omitting the
configuration equals `{}`.

```python
session.scan(text)                                               # rules + model-only labels
session.scan(text, {"entities": ["CREDIT_CARD", "EMAIL"]})       # rules only, no model
session.scan(text, {"entities": ["STREET_ADDRESS", "EMAIL"]})    # model + rules
session.scan_model(text)                                         # raw model output
```

```javascript
await session.scan(text);
await session.scan(text, {config: {entities: ["CREDIT_CARD", "EMAIL"]}});
await session.scanModel(text);
```

Rule detectors handle every requested type they support, including types the
model also labels, such as `EMAIL` and `PHONE`. Without `entities`, every
enabled rule detector runs, so a session returns the same rule findings as the
rule scanner, plus the model's labels that rules do not produce. The model runs
only when a requested type is such a label, and only those types are kept from
its output. A request that needs no model label runs without the model worker:
it costs microseconds, does not wait for or reserve the worker, and never
returns `model_busy`. It still fails once the session is closed or has failed.
A GLiNER2 session configured only with labels that rules cover, such as `email`,
therefore never runs its model through `scan`; use `scan_model`.

`scan_model` (`scanModel` in Node) returns every model label, including the
model's own `EMAIL` and `PHONE`, with no rule detectors. Use it to evaluate the
model or when its coverage of a shared type is wanted.

Unknown entity types fail with `invalid_configuration` at their `/entities`
index, and an empty `entities` list fails as it does for the rule scanner.
Selecting a German type or `UUID` does not activate it; set `locale` or
`detect_uuid` too. Rule findings carry evidence with the rule detector's name,
no raw score and the rule's confidence. Node rejects unknown keys in the scan
options object with a `TypeError`.

## Tests and publication

Routine CI does not download models. Manually run the `Model integration`
workflow for both models on all five native platforms before releasing.
It checks Rust and installed Python/Node packages, including failures and cleanup.
Publishing requires the latest run for the release commit to pass both models'
jobs on Linux x86_64, Linux ARM64, macOS ARM64 and Windows. The macOS Intel jobs
still run but are advisory: a failure is reported on the job without failing
the run, and publishing does not wait for them. Later commits may reuse a
passing ancestor only if every intervening commit has one parent and modifies
only `README.md`. Other file changes, merges, failed or cancelled runs, and
required jobs that have not passed still block publication.
Release packages build from source.

Review the diff and passing tests before publishing. Target Cargo/npm
`0.4.5-beta.3` and Python `0.4.5b3`; keep npm on the `beta` tag and GitHub releases
marked as prereleases. Python publishes only the existing `datafog-core` project;
its trusted publisher uses repository `DataFog/datafog-core`, workflow
`publish-python.yml`, environment `pypi`. Upload retries skip existing files.
