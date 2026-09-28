"""Opt-in UUID fixtures against the installed extension."""

import json
from pathlib import Path
import datafog_core as api


def verify():
    for line in (
        (Path(__file__).resolve().parents[3] / "fixtures/uuid.jsonl")
        .read_text()
        .splitlines()
    ):
        row = json.loads(line)
        findings = api.scan(row["text"], row["config"])
        uuids = [f for f in findings if f.entity_type == "UUID"]
        assert [
            dict(
                text=f.matched_text,
                start=f.codepoint_range.start,
                end=f.codepoint_range.end,
            )
            for f in uuids
        ] == row["matches"], row["id"]
        for f in uuids:
            assert (
                row["text"].encode()[f.byte_range.start : f.byte_range.end].decode()
                == f.matched_text
            )
            assert (
                row["text"][f.codepoint_range.start : f.codepoint_range.end]
                == f.matched_text
            )
            assert (
                f.detector_name == "datafog-core/uuid"
                and f.detector_version
                and f.confidence is None
            )
        data = {"a/b": row["text"], "items": [row["text"]]}
        located = api.scan_structured(data, row["config"]).findings
        for path in ["/a~1b", "/items/0"]:
            assert [f.finding for f in located if f.path == path] == findings
        for strategy in ["redact", "mask", "remove"]:
            config = {"default": {"strategy": strategy}, "entities": ["UUID"]}
            result = api.scan_and_transform(
                row["text"], {"scan": row["config"], "transform": config}
            )
            expected = row["text"]
            for match in reversed(row["matches"]):
                replacement = (
                    "[UUID]"
                    if strategy == "redact"
                    else "*" * 36
                    if strategy == "mask"
                    else ""
                )
                expected = (
                    expected[: match["start"]] + replacement + expected[match["end"] :]
                )
            assert result.text == expected
            assert api.transform(row["text"], findings, config) == result
            for t in result.transformations:
                assert (
                    result.text.encode()[
                        t.output_byte_range.start : t.output_byte_range.end
                    ].decode()
                    == t.replacement
                )
                assert not hasattr(t, "matched_text") and not hasattr(t, "finding")
            structured = api.scan_and_transform_structured(
                data, {"scan": row["config"], "transform": config}
            )
            assert structured.data == {"a/b": expected, "items": [expected]}
            assert (
                api.transform_structured(data, located, config).data == structured.data
            )
    value = "550e8400-e29b-41d4-a716-446655440000"
    findings = api.scan(value, {"detect_uuid": True})
    config = {"default": {"strategy": "redact"}, "entities": ["UUID"]}
    assert api.scan_and_transform(value, {"transform": config}).text == value
    for allow in [
        {"exact": {"UUID": [value]}},
        {"regex": {"UUID": [{"pattern": value}]}},
    ]:
        assert api.transform(value, findings, {**config, "allow": allow}).text == value
    assert (
        api.transform(
            value,
            findings,
            {
                **config,
                "default": {"strategy": "remove"},
                "overrides": {"UUID": {"strategy": "redact"}},
                "allow": {"regex": {"UUID": [{"pattern": "550e"}]}},
            },
        ).text
        == "[UUID]"
    )
    for invalid in [None, 1, "true", [], {}]:
        for call in [
            lambda: api.scan(value, {"detect_uuid": invalid}),
            lambda: api.scan_structured({"value": value}, {"detect_uuid": invalid}),
        ]:
            try:
                call()
            except api.DataFogConfigurationError as e:
                assert e.path == "/detect_uuid"
            else:
                raise AssertionError("UUID invalid config accepted")


async def verify_providers(manager, token_manager, context):
    text = "👋 550e8400-e29b-41d4-a716-446655440000"

    def config(strategy):
        return {
            "scan": {"detect_uuid": True},
            "transform": {"default": strategy, "entities": ["UUID"]},
        }

    result = await manager.scan_and_transform(
        text, config({"strategy": "pseudonymize", "key_ref": "uuid"})
    )
    assert len(result.transformations) == 1 and result.text != text
    tokens = await token_manager.scan_and_transform(
        text, config({"strategy": "tokenize", "token_ref": "uuid"}), context
    )
    assert len(tokens.transformations) == 1
    assert (await token_manager.restore(tokens.text, context)).text == text
