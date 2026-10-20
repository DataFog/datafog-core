"""Check installed model loading, result conversion and errors; no accuracy grading."""
import json
import os
from pathlib import Path
import subprocess
import sys

from datafog_core import DataFogModelError, ModelCancellation, ModelSession, scan as rule_scan

config = json.loads(Path(sys.argv[1]).read_text(encoding="utf-8"))
assert not any(name in sys.modules for name in ("torch", "gliner2", "transformers", "onnxruntime"))
texts = [
    "John Smith lives at 123 Main Street, Boston, MA 02110. Email john@example.com.",
    "", "The weather is clear.",
    "👋 e\u0301 John Smith lives at 123 Main Street. John Smith lives at 123 Main Street.",
    "ordinary " * 505 + "John Smith lives at 123 Main Street.",
]
general = os.environ.get("MODEL_BACKEND") == "gliner2-general"
model_entity = "CURRENT_EMPLOYER" if general else "STREET_ADDRESS"
if general:
    texts = [text.replace("lives at 123 Main Street", "works for Cedar Labs") for text in texts]
manifest = json.loads((Path(config["bundle"]) / "bundle-manifest.json").read_text(encoding="utf-8"))
reference = None
if "gliner2" not in config:
    executable = Path(config["bundle"]) / ("datafog-pii.exe" if sys.platform == "win32" else "datafog-pii")
    native = subprocess.run([str(executable), config["bundle"]],
        input="".join(json.dumps({"text": text}) + "\n" for text in texts),
        text=True, encoding="utf-8", capture_output=True, check=True,
        timeout=config["limits"]["timeout_ms"] / 1000)
    reference = [json.loads(line)["findings"] for line in native.stdout.splitlines()]
    assert len(reference) == len(texts)
report = {"cases": []}
with ModelSession.open(config) as session:
    for case_index, text in enumerate(texts):
        result = session.scan_model(text)
        assert result.contract_version == 1 and result.model
        if "model" in manifest:
            assert result.model == manifest["model"]["id"]
        assert len(result.evidence) == len(result.findings)
        for index, (finding, evidence) in enumerate(zip(result.findings, result.evidence, strict=True)):
            assert text[finding.codepoint_range.start:finding.codepoint_range.end] == finding.matched_text
            assert text.encode()[finding.byte_range.start:finding.byte_range.end].decode() == finding.matched_text
            assert evidence["finding_index"] == index
            assert finding.entity_type and evidence["sources"]
            source = evidence["sources"][0]
            if "model" in manifest:
                assert manifest["model"]["revision"] in source["detector_version"]
            assert source["native_label"] and 0 <= source["raw_score"]["value"] <= 1
            if reference is not None:
                expected = reference[case_index][index]
                assert (finding.byte_range.start, finding.byte_range.end) == (expected["start"], expected["end"])
                assert source["native_label"] == expected["label"]
                assert abs(source["raw_score"]["value"] - expected["raw_score"]) <= 2 ** -52
                assert abs(finding.confidence - expected["confidence"]) <= 2 ** -24
            else:
                assert finding.confidence is None
        if reference is not None:
            assert len(result.findings) == len(reference[case_index])
        report["cases"].append({"text": text, "model": result.model, "findings": [
            {"entity": f.entity_type, "text": f.matched_text, "start": f.codepoint_range.start,
             "end": f.codepoint_range.end, "byte_start": f.byte_range.start, "byte_end": f.byte_range.end}
            for f in result.findings], "evidence": result.evidence})
    if not general:
        assert report["cases"][0]["findings"]  # Existing PII fixtures exercise actual spans.
    assert report["cases"][1]["findings"] == []
    cancellation = ModelCancellation()
    cancellation.cancel()
    try:
        session.scan_model(texts[0], cancellation=cancellation)
    except DataFogModelError as error:
        assert error.code == "model_cancelled"
    else:
        raise AssertionError("cancelled request returned findings")
    assert session.scan_model("").findings == []  # Cancellation before execution permits reuse.
    # Routing: requested rule types come from rule detectors; requested model
    # types equal the plain model scan's findings for those types.
    routed_text = texts[0] + " Card 4111 1111 1111 1111 123."
    def spans(findings, entity=None):
        return [(f.entity_type, f.byte_range.start, f.byte_range.end, f.matched_text)
                for f in findings if entity is None or f.entity_type == entity]
    rules_only = session.scan(routed_text, {"entities": ["CREDIT_CARD", "EMAIL"]})
    assert spans(rules_only.findings) == spans(rule_scan(routed_text, {"entities": ["CREDIT_CARD", "EMAIL"]}))
    assert {f.entity_type for f in rules_only.findings} == {"CREDIT_CARD", "EMAIL"}
    assert all(e["sources"][0]["raw_score"] is None for e in rules_only.evidence)
    mixed = session.scan(routed_text, {"entities": [model_entity, "EMAIL"]})
    assert spans(mixed.findings, model_entity) == spans(session.scan_model(routed_text).findings, model_entity)
    assert {f.entity_type for f in mixed.findings} <= {model_entity, "EMAIL"}
    assert [f.detector_name for f in mixed.findings if f.entity_type == "EMAIL"] == ["datafog-core/email"]
    assert len(mixed.evidence) == len(mixed.findings)
    # Omitting the configuration routes like `{}`: the rule scanner's findings
    # plus the model labels rules do not produce.
    default = session.scan(routed_text)
    assert spans(default.findings) == spans(session.scan(routed_text, {}).findings)
    rule_part = [f for f in default.findings if not f.detector_name.startswith("datafog-core/model/")]
    assert spans(rule_part) == spans(rule_scan(routed_text))
    report["default"] = [
        {"entity": f.entity_type, "text": f.matched_text, "byte_start": f.byte_range.start, "byte_end": f.byte_range.end}
        for f in default.findings]
    report["routed"] = {"text": routed_text, "entities": [model_entity, "EMAIL"], "findings": [
        {"entity": f.entity_type, "text": f.matched_text, "start": f.codepoint_range.start,
         "end": f.codepoint_range.end, "byte_start": f.byte_range.start, "byte_end": f.byte_range.end}
        for f in mixed.findings], "evidence": mixed.evidence}
session.close()
try:
    session.scan("")
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
output = Path(sys.argv[2])
output.parent.mkdir(parents=True, exist_ok=True)
output.write_text(json.dumps(report), encoding="utf-8")
print("Installed Python model loading, results, routing, offsets, errors and cleanup passed.")
