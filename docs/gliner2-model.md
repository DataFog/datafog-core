# Optional GLiNER2 model sessions

GLiNER2 executes in a separately installed local Python worker, including when
called from Rust or Node. Core owns offsets, normalized-score validation,
per-label thresholds, canonical mapping, composition and transformations.
Rule-only package installation never installs or imports this worker or PyTorch.

## Pinned setup

Use Python 3.12 for the optional worker environment. This does not narrow the
base binding's supported Python range. The initial checkpoint is
`fastino/gliner2-privacy-filter-PII-multi` at revision
`1cb4166094dc58fa8d836429f060d6c95f62b495`. It is a span checkpoint with maximum
span width eight word tokens; it is not GLiNER2.5. The model card declares
Apache-2.0 and is retained verbatim in the verified installation. Retain the
worker, GLiNER2, PyTorch and Transformers licenses in deployment artifacts.

```sh
hf download fastino/gliner2-privacy-filter-PII-multi \
  config.json encoder_config/config.json tokenizer.json tokenizer_config.json model.safetensors README.md \
  --revision 1cb4166094dc58fa8d836429f060d6c95f62b495 --local-dir /tmp/gliner2-download
python scripts/provision-gliner2-model.py \
  --source /tmp/gliner2-download --destination /opt/datafog/gliner2
python3.12 -m venv /opt/datafog/gliner2-worker
/opt/datafog/gliner2-worker/bin/python -m pip install ./workers/gliner2
```

On Windows use the environment's `Scripts/python.exe`. For Linux CPU-only
provisioning install `torch==2.12.0` from the official PyTorch CPU index before
the worker. The worker pins GLiNER2 2.1.0, Transformers 5.8.0 and PyTorch 2.12.0;
Intel macOS uses Transformers 4.57.6/PyTorch 2.2.2 because newer PyTorch releases
have no Intel macOS wheel. Its pinned NumPy and protobuf dependencies allow the
published checkpoint's tokenizer metadata to load through the official
compatibility path. This platform-specific stack needs its own actual-model
qualification; successful Apple Silicon execution does not establish it.

Add `gliner2` to a normal [session configuration](model-sessions.md), with the
provisioner's manifest digest:

```json
{
  "python": "/opt/datafog/gliner2-worker/bin/python",
  "labels": [{"label": "street_address", "threshold": 0.5}],
  "intra_threads": 1,
  "chunk_size": 384,
  "chunk_overlap": 64
}
```

These word-window settings follow the official runtime defaults. The example
threshold follows the model card; it is not a calibrated probability or a
promise of accuracy. Limits remain mandatory caller settings. Qualification
uses explicit synthetic-workload budgets recorded in the workflow and evidence;
they are not universal production defaults. Native unfiltered candidate output
is larger than final findings, so response/finding limits apply before filtering.

The same `ModelSession.open`, model-only/combined scans, decoded structured
leaves, cancellation and transformations work with this configuration. Assets
are verified before readiness; inference sets offline mode and performs no
provisioning. `inspect()` reports the selected labels, descriptions, thresholds,
CPU thread count, revisions, runtime and score semantics.

## Behavior and integration checks

All checkpoint-listed labels may be configured, each with an explicit threshold
and optional description. Unknown, empty or duplicate labels reject. Canonical
names use uppercase native labels; `phone_number` maps to `PHONE`. `address`
and `street_address` remain `ADDRESS` and `STREET_ADDRESS`. Core preserves these label distinctions; it does not evaluate checkpoint accuracy. Model scores use normalized span-sigmoid
values in evidence; `Finding.confidence` is absent. No calibration is fabricated.

The adapter enables the official strict extraction mode and disables the pinned
processor's punctuation-appending hook. This is necessary to keep spans inside
the exact original input, including a single unpunctuated structured leaf.
It does not rewrite tokenization, decoding or the official overlap-window
merger. Core validates every returned candidate before applying thresholds.
Repeated values use supplied code-point coordinates, never substring search.

The official overlapping word windows cover the complete input and retain
meaningful overlapping spans. Whitespace remains unchanged. Empty/whitespace
input has no model findings. A malformed result, oversize native response,
worker failure, cancellation or timeout fails atomically and retires active
work. There are no dataloader subprocesses, automatic retries or hosted fallback.

Run the explicit Rust model test with `DATAFOG_MODEL_CONFIG` pointing to the
GLiNER2 session JSON. Run `scripts/model-qualification.py CONFIG REPORT` against
an installed Core wheel to check loading, result shape, offsets, errors, reuse
and cleanup. Node integration compares its installed package with that
Python/Core report. The manual `Model integration` workflow runs these checks on
all five native platforms. There are no accuracy or performance release gates.
