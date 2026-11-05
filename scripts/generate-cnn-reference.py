#!/usr/bin/env python3
"""Regenerate serialized-weight reference vectors with pinned upstream source.

Usage: python scripts/generate-cnn-reference.py /path/to/cnn-NA-1mb
Qualification generator only: requires numpy and CPU PyTorch, never shipped as runtime.
"""
import json
from pathlib import Path
import subprocess
import sys

import numpy as np
import torch

SOURCE_REVISION = "75f5a20e05380de5457502afefaca0383c4e8e91"
source = Path(sys.argv[1]).resolve()
if subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=source, text=True).strip() != SOURCE_REVISION:
    raise SystemExit("reference source revision mismatch")
sys.path.insert(0, str(source / "models"))
import r3_tagger as reference  # noqa: E402

reference.SPLIT_TOKENS = True
torch.set_num_threads(1)
root = Path(__file__).resolve().parents[1] / "crates/core/src/cnn"
raw = (root / "assets/bundle.bin").read_bytes()
layout = json.loads((root / "assets/bundle.json").read_text())["tensors"]
weights = {}
for key, tensor in layout.items():
    parts = [raw[start:start + size] for start, size in tensor["parts"]]
    shape = tensor["shape"]
    if tensor["kind"] == "fp16":
        values = np.frombuffer(parts[0], dtype="<f2").astype("float32").reshape(shape)
    else:
        if tensor["kind"] == "int4_row_sym":
            packed = np.frombuffer(parts[0], dtype="uint8")
            quantized = np.stack([packed & 15, packed >> 4], axis=1).reshape(shape).astype("float32") - 8
        else:
            quantized = np.frombuffer(parts[0], dtype="int8").astype("float32").reshape(shape)
        scales = np.frombuffer(parts[1], dtype="<f2").astype("float32")
        values = quantized * scales.reshape([-1] + [1] * (len(shape) - 1))
    weights[key] = torch.from_numpy(values.copy())
model = reference.Tagger()
model.load_state_dict(weights)
model.eval()
texts = [
    "BILL TO\nJohn Smith\n123 Main Street\nSpringfield IL 62704\nUSA",
    "The quarterly report contains no recipient mailing information.",
    "😀 Résumé\nSTATEMENT FOR\nMüller Hans\n42 Maple Road\nOttawa ON K1A 0B1",
    "BILL TO\nMaryJones 123Main Street\nToronto ON M5V 2T6",
    "",
    "BILL TO\nJohn Smith\n123 Main Street\ńIL 62704\n²TO: Jane Doe\n\x1c12 Main Street",
    "αΣ ǅ ² Ⅷ İ Á 1Abc abc1 fooBar\n\nBILL TO\nAda Smith\n17 Meadway\nLondon SW1A 1AA",
    " ".join(["noise"] * 215) + "\nBILL TO\nJohn Smith\n123 Main Street\nSpringfield IL 62704\n" + " ".join(["noise"] * 80),
]
fixtures = []
for text in texts:
    encoded = reference.encode_record({"text": text})
    tokens, words, chars, features, _ = encoded
    row = {"text": text, "tokens": [
        {"start": t.start, "end": t.end, "word": int(words[i]), "chars": chars[i].tolist(),
         "features": features[i].astype("float16").astype("float32").tolist()}
        for i, t in enumerate(tokens)]}
    row["spans"] = reference.predict_records(model, [{"id": "fixture", "text": text}], torch.device("cpu"))[0]["spans"]
    if tokens and len(tokens) <= 256:
        item = reference.page_items(encoded, False)[0]
        w, c, f, mask, _ = reference.make_batch([item], torch.device("cpu"))
        with torch.no_grad():
            row["logits"] = model(w, c, f, mask)[0].tolist()
    fixtures.append(row)
output = {"source_revision": SOURCE_REVISION, "serialized_scales": True,
          "torch": torch.__version__, "numpy": np.__version__, "fixtures": fixtures}
(root / "reference.json").write_text(json.dumps(output, ensure_ascii=False, separators=(",", ":")))
print([(len(row["tokens"]), row["spans"]) for row in fixtures])
