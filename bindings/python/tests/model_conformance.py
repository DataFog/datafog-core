"""Explicit installed-wheel, real pinned-model check. Assets are never optional."""
from __future__ import annotations

import hashlib
import json
from pathlib import Path
import subprocess
import sys

from datafog_core import (
    DataFogModelError, ModelSession, ModelCancellation, transform, transform_structured, scan,
)

config = json.loads(Path(sys.argv[1]).read_text())
bundle = Path(config["bundle"])
assert hashlib.sha256((bundle / "bundle-manifest.json").read_bytes()).hexdigest() == config["manifest_sha256"]
texts = [
    "John Smith lives at 123 Main Street, Boston, MA 02110. Email john@example.com.",
    "The weather is clear.",
    "👋 Contact May Smith at 123 Main Street, Boston, MA 02110.",
    "e\u0301 👋 John Smith lives at 123 Main Street. John Smith lives at 123 Main Street.",
    "Ordinary words without private details. " * 160 + "John Smith lives at 123 Main Street.",
]
executable = bundle / ("datafog-pii.exe" if sys.platform == "win32" else "datafog-pii")
reference = subprocess.run(
    [str(executable), str(bundle)], input="".join(json.dumps({"text": t}) + "\n" for t in texts),
    text=True, encoding="utf-8", capture_output=True, check=True, timeout=config["limits"]["timeout_ms"] / 1000,
)
reference = [json.loads(line)["findings"] for line in reference.stdout.splitlines()]
with ModelSession.open(config) as session:
    cancellation = ModelCancellation()
    cancellation.cancel()
    try:
        session.scan("private@example.test", "combined", cancellation=cancellation)
    except DataFogModelError as error:
        assert error.code == "model_cancelled"
    else:
        raise AssertionError("cancelled request returned findings")
    assert session.inspect()["state"] == "ready"
    identity = session.inspect()["configuration_id"]
    session.warm_up()
    for text, native in zip(texts, reference, strict=True):
        result = session.scan(text, "model_only")
        assert len(result.findings) == len(native)
        for finding, expected, evidence in zip(result.findings, native, result.evidence, strict=True):
            assert finding.byte_range.start == expected["start"]
            assert finding.byte_range.end == expected["end"]
            assert evidence["sources"][0]["native_label"] == expected["label"]
            # JSON decoding can differ by one binary64 ulp; this is not a model tolerance.
            assert abs(evidence["sources"][0]["raw_score"]["value"] - expected["raw_score"]) <= 2 ** -52
            assert abs(finding.confidence - expected["confidence"]) <= 2 ** -24
            assert text.encode()[finding.byte_range.start:finding.byte_range.end].decode() == finding.matched_text
            assert text[finding.codepoint_range.start:finding.codepoint_range.end] == finding.matched_text
        assert result.diagnostics is None
        assert result.configuration_id == identity
    combined = session.scan(texts[0], "combined", diagnostics=True)
    assert combined.diagnostics is not None
    assert any(len(item["sources"]) > 1 for item in combined.evidence)
    assert all(f.confidence is None for f in scan(texts[0]))
    protected = transform(texts[0], combined.findings, {"default": {"strategy": "redact"}})
    assert "[STREET_ADDRESS]" in protected.text and "john@example.com" not in protected.text
    document = {"first_name": "John", "note": texts[2], "keep": 12, "array": [texts[1]]}
    structured = session.scan_structured(document, "combined")
    protected = transform_structured(document, structured.findings, {"default": {"strategy": "redact"}})
    assert protected.data["keep"] == 12 and protected.data["array"] == [texts[1]]
    assert "[STREET_ADDRESS]" in protected.data["note"]
    assert any(f.path == "/first_name" and f.finding.entity_type == "PERSON" for f in structured.findings)
    assert [item["finding_index"] for item in structured.evidence] == list(range(len(structured.findings)))
    session.health()
session.close()
assert session.inspect()["state"] == "closed"
try:
    session.scan("", "combined")
except DataFogModelError as error:
    assert error.code == "model_session_closed"
else:
    raise AssertionError("closed scanner accepted inference")
bad = dict(config, manifest_sha256="0" * 64)
try:
    ModelSession.open(bad)
except DataFogModelError as error:
    assert error.reason == "integrity_mismatch" and config["bundle"] not in str(error)
else:
    raise AssertionError("corrupt identity accepted")
print("Installed Python pinned-model parity, negatives, Unicode, long input, reuse, structured composition and cleanup passed.")
