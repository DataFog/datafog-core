# GLiNER2 setup

GLiNER2 needs model weights and the official Python runtime that processes them.
Its private runner ships inside `datafog-core`; there is no separate worker
package. Python, Node and Rust use that same runner through a selected local
Python interpreter. Installing ordinary Core does not install GLiNER2 or PyTorch.

Use Python 3.12 for the GLiNER2 environment. From this checkout:

```sh
python3.12 -m venv /opt/datafog/gliner2-env
/opt/datafog/gliner2-env/bin/python -m pip install './bindings/python[gliner2]'
hf download fastino/gliner2-privacy-filter-PII-multi \
  config.json encoder_config/config.json tokenizer.json tokenizer_config.json model.safetensors README.md \
  --revision 1cb4166094dc58fa8d836429f060d6c95f62b495 --local-dir /tmp/gliner2-download
python scripts/provision-gliner2-model.py \
  --source /tmp/gliner2-download --destination /opt/datafog/gliner2
```

After publication, install `datafog-core[gliner2]==0.4.5b4` instead of the local
checkout. On Windows use the environment's `Scripts/python.exe`. On Linux, a
CPU-only installation can first install `torch==2.12.0` from the official
PyTorch CPU index. Retain model and dependency licenses in deployments.

The extra installs GLiNER2 2.1.0 and compatible dependencies. Intel macOS retains
its supported PyTorch 2.2.2/Transformers 4.57.6 stack; other platforms use
PyTorch 2.12.0/Transformers 5.8.0. Core's base Python range remains unchanged.

Use the provisioned bundle and manifest digest in the normal
[session configuration](model-sessions.md), adding:

```json
"gliner2": {
  "python": "/opt/datafog/gliner2-env/bin/python",
  "labels": [{"label": "street_address", "threshold": 0.5}],
  "intra_threads": 1,
  "chunk_size": 384,
  "chunk_overlap": 64
}
```

The shown window settings follow the runtime defaults. Thresholds are explicit
caller choices; scores are raw span-sigmoid values, not calibrated confidence.
The integration workflow uses `max_input_bytes=65536`,
`max_response_bytes=16777216`, `max_findings=65536`, `timeout_ms=120000` and
`cleanup_timeout_ms=5000`. These are test budgets, not required production limits.

Labels are caller-defined, with a threshold and optional description. Empty,
whitespace-padded, duplicate labels and labels that map to the same output type
fail configuration validation. Labels use uppercase names;
`phone_number` becomes `PHONE`. `ADDRESS` and `STREET_ADDRESS` remain distinct.
Core validates spans before threshold filtering and preserves original Unicode
positions. Empty/whitespace input returns no findings. Model processing uses
local files only and does not download assets or fall back to a hosted service.

## Select another compatible model

The provisioner can create the [model manifest](model-manifests.md) from
explicitly downloaded local assets; no handwritten file hashes are needed:

```sh
hf download fastino/gliner2-base-v1 \
  config.json encoder_config/config.json tokenizer.json tokenizer_config.json \
  added_tokens.json special_tokens_map.json spm.model model.safetensors README.md \
  --revision f9634218e53580c56edf0de97ca1a7d3f1c2354e --local-dir /tmp/general-gliner2-download
python scripts/provision-gliner2-model.py \
  --source /tmp/general-gliner2-download \
  --model-id fastino/gliner2-base-v1 \
  --revision f9634218e53580c56edf0de97ca1a7d3f1c2354e \
  --destination /opt/datafog/general-gliner2
```

Use the emitted bundle path and manifest digest in the existing session config.
Alternatively, use `--manifest /path/to/model.json` to verify against an existing
manifest instead of generating one. `--model-id` and `--revision` record the
operator's selection; they do not independently authenticate the model publisher.
The manifest identifies the model and exact files; session `gliner2.labels`
selects the task. For example, a label may be `current_employer` with description
`The organization the caller currently works for`. Its output type is
`CURRENT_EMPLOYER`. The existing `phone_number` to `PHONE` alias is preserved.

Compatibility means the existing GLiNER2 2.1.0 span loader, CPU runtime, local
single-file asset layout and extraction methods used by this adapter. A manifest
does not add GLiNER2.5 boundary support, another inference architecture or runtime
dependencies. Omit `--manifest`, `--model-id` and `--revision` to keep
provisioning the original pinned PII checkpoint with the existing setup commands.

## Entity-count gating

GLiNER2's span runtime uses a count head to predict how many entities to extract.
When this head predicts zero, the upstream runtime skips span decoding entirely,
which can cause valid high-confidence spans to be missed.

Core's GLiNER2 worker floors the count prediction at 1 so that span decoding
always runs. Core's threshold filtering handles any resulting false positives.
This ensures inputs like "My home address is 27 Willow Lane." return the expected
street address findings even when the count head would otherwise predict zero.
