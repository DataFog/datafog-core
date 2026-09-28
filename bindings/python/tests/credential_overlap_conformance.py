"""Cross-detector overlap selection against the installed wheel."""

import json
from pathlib import Path
import datafog_core as api
from synthetic_fixtures import expand_fixture

RECORDS = [
    expand_fixture(json.loads(line))
    for line in (Path(__file__).resolve().parents[3] / "fixtures/credential-overlaps.jsonl")
    .read_text()
    .splitlines()
]


def project(text, findings):
    result = []
    for f in findings:
        if f.entity_type not in {"API_KEY", "BEARER_TOKEN", "CREDENTIAL_URI", "JWT"}:
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
        if "authorization_value" in row:
            value = row["authorization_value"]
            data = {"👋/headers": {"aUtHoRiZaTiOn": value}}
            located = api.scan_structured(data).findings
            assert all(item.path == "/👋~1headers/aUtHoRiZaTiOn" for item in located)
            expected = [dict(entity, start=7, end=len(value)) for entity in row["entities"]]
            assert project(value, [item.finding for item in located]) == expected
            for entity in row["entities"]:
                label = entity["label"]
                result = api.scan_and_transform_structured(data, {"transform": {"default": {"strategy": "redact"}, "entities": [label]}})
                assert result.data == {"👋/headers": {"aUtHoRiZaTiOn": f"Bearer [{label}]"}}
        if "overlap" in row:
            assert any(f.entity_type == row["overlap"] for f in findings)
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
