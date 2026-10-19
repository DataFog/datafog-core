"""Installed-package checks for loading, result conversion, errors and cleanup.

Uses real local models to check integration. It does not grade model accuracy
or run performance benchmarks.
"""
import argparse
import json
from pathlib import Path
import platform
import sys

from datafog_core import DataFogModelError, ModelCancellation, ModelSession, transform

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("config", type=Path)
parser.add_argument("report", type=Path)
args = parser.parse_args()
config = json.loads(args.config.read_text(encoding="utf-8"))
assert not any(name in sys.modules for name in ("torch", "gliner2", "transformers", "onnxruntime"))
texts = [
    "John Smith lives at 123 Main Street, Boston, MA 02110. Email john@example.com.",
    "The weather is clear.",
    "",
    "👋 e\u0301 John Smith lives at 123 Main Street. John Smith lives at 123 Main Street.",
    "ordinary " * 505 + "John Smith lives at 123 Main Street.",
]
report = {"host": platform.platform(), "python": platform.python_version(), "cases": []}
with ModelSession.open(config) as session:
    report["session"] = session.inspect()
    identity = report["session"]["configuration_id"]
    assert report["session"]["state"] == "ready"
    session.warm_up()
    for value in texts:
        result = session.scan(value, "model_only")
        assert result.contract_version == 1
        assert result.configuration_id == identity
        assert result.mode == "model_only" and result.diagnostics is None
        assert len(result.evidence) == len(result.findings)
        for index, (finding, evidence) in enumerate(zip(result.findings, result.evidence, strict=True)):
            assert value[finding.codepoint_range.start:finding.codepoint_range.end] == finding.matched_text
            assert value.encode()[finding.byte_range.start:finding.byte_range.end].decode() == finding.matched_text
            assert evidence["finding_index"] == index
            assert finding.entity_type and evidence["sources"]
            for source in evidence["sources"]:
                assert source["native_label"]
                assert 0 <= source["raw_score"]["value"] <= 1
            if "gliner2" in config:
                assert finding.confidence is None
        report["cases"].append({"text": value, "findings": [
            {"entity": f.entity_type, "text": f.matched_text, "start": f.codepoint_range.start,
             "end": f.codepoint_range.end, "byte_start": f.byte_range.start, "byte_end": f.byte_range.end}
            for f in result.findings], "evidence": result.evidence})
    # Nonempty output proves that returned spans passed through the adapter.
    # There is no hand-labelled truth set or required detection rate.
    assert report["cases"][0]["findings"]
    assert report["cases"][2]["findings"] == []
    text = texts[0]
    combined = session.scan(text, "combined")
    assert combined.contract_version == 1 and combined.mode == "combined"
    assert combined.configuration_id == identity
    protected = transform(text, combined.findings, {"default": {"strategy": "redact"}})
    assert protected.text != text
    structured = session.scan_structured({"note": text, "keep": 12}, "model_only")
    assert structured.contract_version == 1 and structured.configuration_id == identity
    assert len(structured.evidence) == len(structured.findings)
    assert all(f.path == "/note" for f in structured.findings)
    cancellation = ModelCancellation()
    cancellation.cancel()
    try:
        session.scan(text, "model_only", cancellation=cancellation)
    except DataFogModelError as error:
        assert error.code == "model_cancelled"
    else:
        raise AssertionError("cancelled request returned findings")
    session.health()
assert session.inspect()["state"] == "closed"
session.close()
try:
    session.scan("", "model_only")
except DataFogModelError as error:
    assert error.code == "model_session_closed"
else:
    raise AssertionError("closed session accepted inference")
try:
    ModelSession.open(dict(config, manifest_sha256="0" * 64))
except DataFogModelError as error:
    assert error.code == "model_load_failed" and error.reason == "integrity_mismatch"
    assert config["bundle"] not in str(error)
else:
    raise AssertionError("invalid model identity accepted")
report["state_after_close"] = session.inspect()["state"]
args.report.parent.mkdir(parents=True, exist_ok=True)
args.report.write_text(json.dumps(report, indent=2), encoding="utf-8")
print("Installed Python model loading, result shape, offsets, errors, reuse and cleanup passed.")
