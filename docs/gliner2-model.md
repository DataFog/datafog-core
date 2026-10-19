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

After publication, install `datafog-core[gliner2]==0.4.5b1` instead of the local
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

All checkpoint-listed labels can be selected, with a threshold and optional
description. Unknown or duplicate labels fail. Labels use uppercase names;
`phone_number` becomes `PHONE`. `ADDRESS` and `STREET_ADDRESS` remain distinct.
Core validates spans before threshold filtering and preserves original Unicode
positions. Empty/whitespace input returns no findings. Model processing uses
local files only and does not download assets or fall back to a hosted service.
