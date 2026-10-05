"""Email source-context fixtures against the installed extension."""

import json
from pathlib import Path

import datafog_core as api


def verify():
    fixture = Path(__file__).resolve().parents[3] / "fixtures/email-context.jsonl"
    for line in fixture.read_text().splitlines():
        row = json.loads(line)
        text = row["text"]
        findings = api.scan(text, row["config"])
        emails = [f for f in findings if f.entity_type == "EMAIL"]
        assert [
            dict(text=f.matched_text, start=f.codepoint_range.start, end=f.codepoint_range.end)
            for f in emails
        ] == row["matches"], row["id"]
        for finding in emails:
            assert text.encode()[finding.byte_range.start:finding.byte_range.end].decode() == finding.matched_text
            assert text[finding.codepoint_range.start:finding.codepoint_range.end] == finding.matched_text
        data = {"source": text}
        located = api.scan_structured(data, row["config"]).findings
        assert [f.finding for f in located] == findings
        for strategy in ["redact", "mask", "remove"]:
            policy = {"default": {"strategy": strategy}, "entities": ["EMAIL"]}
            envelope = {"scan": row["config"], "transform": policy}
            result = api.scan_and_transform(text, envelope)
            assert result.text == row["outputs"][strategy], (row["id"], strategy)
            assert api.transform(text, findings, policy) == result
            for record, finding in zip(result.transformations, emails):
                assert record.source_byte_range == finding.byte_range
                assert record.source_codepoint_range == finding.codepoint_range
                assert result.text.encode()[record.output_byte_range.start:record.output_byte_range.end].decode() == record.replacement
            assert api.scan_and_transform_structured(data, envelope).data == {"source": result.text}
        if row["config"]["format"] == "text":
            assert api.scan(text) == findings
    for invalid in [None, 1, True, {}, "yaml", "ENV"]:
        for call in [
            lambda: api.scan("", {"format": invalid}),
            lambda: api.scan_structured({"source": ""}, {"format": invalid}),
        ]:
            try:
                call()
            except api.DataFogConfigurationError as error:
                assert error.path == "/format"
            else:
                raise AssertionError("Invalid source format accepted")
