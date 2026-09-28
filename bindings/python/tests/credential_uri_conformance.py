"""Shared numeric identifier fixtures against the installed wheel, including provider round trips."""

import json
from pathlib import Path
import datafog_core as api

RECORDS = [
    json.loads(line)
    for line in (Path(__file__).resolve().parents[3] / "fixtures/credential-uri.jsonl")
    .read_text()
    .splitlines()
]


def project(text, findings):
    result = []
    for f in findings:
        if f.entity_type != "CREDENTIAL_URI":
            continue
        assert (
            text.encode()[f.byte_range.start : f.byte_range.end].decode()
            == f.matched_text
        )
        assert text[f.codepoint_range.start : f.codepoint_range.end] == f.matched_text
        assert f.confidence is None
        assert f.detector_name == "datafog-core/" + f.entity_type.lower().replace(
            "_", "-"
        )
        assert f.detector_version
        result.append(
            dict(
                label=f.entity_type,
                text=f.matched_text,
                start=f.codepoint_range.start,
                end=f.codepoint_range.end,
            )
        )
    return result


def verify():
    for row in RECORDS:
        findings = api.scan(row["text"], row["config"])
        if "overlaps" in row:
            assert all(any(f.entity_type == label for f in findings) for label in row["overlaps"])
            assert api.transform(row["text"], findings, {"default": {"strategy": "redact"}}).text == row["default_text"]

        assert project(row["text"], findings) == row["entities"], row["id"]
        data = {"a/b": row["text"], "array": [row["text"]], "z": row["text"]}
        located = api.scan_structured(data, row["config"]).findings
        for path in ["/a~1b", "/array/0", "/z"]:
            assert (
                project(row["text"], [f.finding for f in located if f.path == path])
                == row["entities"]
            ), row["id"]
        for case in row.get("transforms", []):
            result = api.scan_and_transform(
                row["text"], {"scan": row["config"], "transform": case["config"]}
            )
            assert result.text == case["text"], row["id"]
            assert api.transform(row["text"], findings, case["config"]) == result
            for t in result.transformations:
                assert (
                    result.text.encode()[
                        t.output_byte_range.start : t.output_byte_range.end
                    ].decode()
                    == t.replacement
                )
                assert (
                    result.text[
                        t.output_codepoint_range.start : t.output_codepoint_range.end
                    ]
                    == t.replacement
                )
                assert any(
                    f.byte_range == t.source_byte_range
                    and f.entity_type == t.entity_type
                    for f in findings
                )
                assert not hasattr(t, "matched_text") and not hasattr(t, "finding")
            structured = api.scan_and_transform_structured(
                data, {"scan": row["config"], "transform": case["config"]}
            )
            assert structured.data == {
                "a/b": case["text"],
                "array": [case["text"]],
                "z": case["text"],
            }
            explicit = api.transform_structured(data, located, case["config"])
            assert explicit.data == structured.data
            assert [(t.path, t.transformation) for t in explicit.transformations] == [
                (t.path, t.transformation) for t in structured.transformations
            ]


async def verify_providers(manager, token_manager, context):
    for row in RECORDS:
        if not row.get("sample"):
            continue
        entities = list(dict.fromkeys(e["label"] for e in row["entities"]))
        pseudonyms = await manager.scan_and_transform(
            row["text"],
            {
                "scan": row["config"],
                "transform": {
                    "default": {"strategy": "pseudonymize", "key_ref": "german"},
                    "entities": entities,
                },
            },
        )
        assert (
            len(pseudonyms.transformations) == len(row["entities"])
            and pseudonyms.text != row["text"]
        )
        tokens = await token_manager.scan_and_transform(
            row["text"],
            {
                "scan": row["config"],
                "transform": {
                    "default": {"strategy": "tokenize", "token_ref": "german"},
                    "entities": entities,
                },
            },
            context,
        )
        assert len(tokens.transformations) == len(row["entities"])
        assert (await token_manager.restore(tokens.text, context)).text == row["text"]
