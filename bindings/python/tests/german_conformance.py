"""Shared German fixtures against the installed wheel, including provider round trips."""

import json
from pathlib import Path
import datafog_core as api

RECORDS = [
    json.loads(line)
    for line in (Path(__file__).resolve().parents[3] / "fixtures/german.jsonl")
    .read_text()
    .splitlines()
]


def project(text, findings):
    result = []
    for f in findings:
        if not f.entity_type.startswith("DE_"):
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
    split = {
        "Steuer-ID": "12345678901",
        "a": "Steuer-ID",
        "b": "12345678901",
        "c": ["Passport", "C12345678"],
        "d": "DE4450010517",
        "e": "5407324931",
    }
    assert not any(
        f.finding.entity_type.startswith("DE_")
        for f in api.scan_structured(split, {"locale": "de"}).findings
    )
    for config in [
        {"locale": ""},
        {"locale": " \t"},
        {"locale": None},
        {"locale": 1},
        {"locales": ["de"]},
    ]:
        for call in [
            lambda: api.scan("", config),
            lambda: api.scan_structured({}, config),
        ]:
            try:
                call()
            except api.DataFogConfigurationError:
                pass
            else:
                raise AssertionError("malformed config accepted")
    assert (
        api.scan_and_transform(
            "DE44500105175407324931",
            {"transform": {"default": {"strategy": "redact"}, "entities": ["DE_IBAN"]}},
        ).text
        == "DE44500105175407324931"
    )
    for text, label, generic, expected in [
        ("DE44\t4111111111111111\t11", "DE_IBAN", "CREDIT_CARD", "[DE_IBAN]"),
        ("DE 123456789", "DE_VAT_ID", "SSN", "[DE_VAT_ID]"),
        ("Steuer-ID 12345678901", "DE_TAX_ID", "PHONE", "Steuer-ID [DE_TAX_ID]"),
        ("DE-10115", "DE_POSTAL_CODE", "ZIP_CODE", "[DE_POSTAL_CODE]"),
    ]:
        findings = api.scan(text, {"locale": "de"})
        assert all(
            any(f.entity_type == entity for f in findings)
            for entity in [label, generic]
        )
        assert (
            api.scan_and_transform(
                text,
                {
                    "scan": {"locale": "de"},
                    "transform": {"default": {"strategy": "redact"}},
                },
            ).text
            == expected
        )


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
