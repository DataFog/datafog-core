"""Run the opt-in Core 0.4.1 gate with the real installed Python 4.9 adapter."""

import argparse
import importlib.metadata as metadata
import json
from pathlib import Path
import re

import datafog
from datafog import v5
import datafog_core

from synthetic_fixtures import expand_fixture


FIXTURES = Path(__file__).resolve().parents[3] / "fixtures/credential-overlaps.jsonl"
CREDENTIAL_LABELS = {"API_KEY", "BEARER_TOKEN", "JWT", "CREDENTIAL_URI"}


def verify_integration():
    rows = [expand_fixture(json.loads(line)) for line in FIXTURES.read_text().splitlines()]
    capabilities = datafog_core.capabilities()
    assert capabilities["contract_version"] == 1
    assert len(capabilities["supported_entities"]) == 23
    assert len(capabilities["default_entities"]) == 14
    assert {"API_KEY", "BEARER_TOKEN", "CREDENTIAL_URI"} <= set(
        capabilities["default_entities"]
    )
    assert v5.scan is datafog_core.scan
    results = []
    for row in rows:
        text = row["text"]
        native = v5.scan(text)
        for finding in native:
            assert (
                text[finding.codepoint_range.start : finding.codepoint_range.end]
                == finding.matched_text
            )
            assert (
                text.encode()[finding.byte_range.start : finding.byte_range.end].decode()
                == finding.matched_text
            )
        projected = [
            {
                "label": finding.entity_type,
                "text": finding.matched_text,
                "start": finding.codepoint_range.start,
                "end": finding.codepoint_range.end,
            }
            for finding in native
            if finding.entity_type in CREDENTIAL_LABELS
        ]
        assert projected == row["entities"], row["id"]
        for case in row["transforms"]:
            assert (
                v5.scan_and_transform(text, {"transform": case["config"]}).text
                == case["text"]
            ), row["id"]

        if row["id"].startswith("uri-"):
            expected_label = "CREDENTIAL_URI"
        elif row["id"].startswith("jwt"):
            expected_label = "BEARER_TOKEN"
        else:
            expected_label = "API_KEY"
        winner = next(f for f in native if f.entity_type == expected_label)
        legacy = datafog.scan(text, backend="rust")
        assert [(f.type, f.text) for f in legacy.entities] == [
            (expected_label, winner.matched_text)
        ], row["id"]
        expected = (
            text[: winner.codepoint_range.start]
            + f"[{expected_label}_1]"
            + text[winner.codepoint_range.end :]
        )
        assert datafog.redact(text, backend="rust").redacted_text == expected
        selections = {}
        for label in sorted({f["label"] for f in row["entities"]}):
            selected = datafog.scan(
                text, entity_types=[" " + label.lower() + " "], backend="rust"
            )
            assert [f.type for f in selected.entities] == (
                [label] if label == expected_label else []
            )
            transformed = datafog.redact(
                text, entity_types=[label], backend="rust"
            ).redacted_text
            assert transformed == (expected if label == expected_label else text)
            selections[label] = {
                "legacy_labels": [f.type for f in selected.entities],
                "legacy_changed": transformed != text,
                "native_selected_transform": "passed fixture",
            }
        assert (
            datafog.redact(
                text, backend="rust", allowlist=[winner.matched_text]
            ).redacted_text
            == text
        )
        assert (
            datafog.redact(
                text, backend="rust", allowlist_patterns=[re.escape(winner.matched_text)]
            ).redacted_text
            == text
        )
        if row["id"].startswith("uri-"):
            inner = next(
                f["text"] for f in row["entities"] if f["label"] != expected_label
            )
            assert (
                datafog.redact(text, backend="rust", allowlist=[inner]).redacted_text
                == expected
            )
        if "authorization_value" in row:
            value = row["authorization_value"]
            for key in ["Authorization", "aUtHoRiZaTiOn"]:
                located = v5.scan_structured({key: value})
                assert "BEARER_TOKEN" in {
                    f.finding.entity_type for f in located.findings
                }
                transformed = v5.scan_and_transform_structured(
                    {key: value},
                    {
                        "transform": {
                            "default": {"strategy": "redact"},
                            "entities": ["BEARER_TOKEN"],
                        }
                    },
                )
                assert transformed.data == {key: "Bearer [BEARER_TOKEN]"}
            for record in [
                {"Authorization": [value]},
                {"prefixAuthorization": value},
                {"kind": "Authorization", "value": value},
            ]:
                assert "BEARER_TOKEN" not in {
                    f.finding.entity_type for f in v5.scan_structured(record).findings
                }
        results.append(
            {
                "case": row["id"],
                "native_labels": [f.entity_type for f in native],
                "legacy_winner": expected_label,
                "selections": selections,
            }
        )

    for locale in ["de", "de-DE", "de_DE", " DE-dE\t", "en-US", "fr"]:
        datafog.scan("safe", backend="rust", locales=[locale])
    for locale in ["unsupported", ""]:
        try:
            datafog.scan("safe", backend="rust", locales=[locale])
        except ValueError:
            pass
        else:
            raise AssertionError("unknown locale accepted")
    try:
        v5.scan("safe", {"locale": "unsupported"})
    except datafog_core.DataFogConfigurationError as error:
        assert error.path == "/locale"
    else:
        raise AssertionError("native unknown locale accepted")
    default_result = datafog.scan(rows[0]["text"])
    assert default_result == datafog.scan(rows[0]["text"], backend="python")
    assert not any(
        f.type in {"API_KEY", "BEARER_TOKEN", "CREDENTIAL_URI"}
        for f in default_result.entities
    )

    direct_url = metadata.distribution("datafog-core").read_text("direct_url.json")
    return {
        "core_version": metadata.version("datafog-core"),
        "python_version": metadata.version("datafog"),
        "core_path": datafog_core.__file__,
        "python_path": datafog.__file__,
        "core_direct_url": json.loads(direct_url) if direct_url is not None else None,
        "capabilities": {
            "contract_version": capabilities["contract_version"],
            "supported_count": len(capabilities["supported_entities"]),
            "default_count": len(capabilities["default_entities"]),
        },
        "fixture_count": len(rows),
        "native_transformation_count": sum(len(row["transforms"]) for row in rows),
        "cases": results,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, help="Write detailed JSON audit results.")
    arguments = parser.parse_args()
    result = verify_integration()
    if arguments.output is not None:
        arguments.output.write_text(json.dumps(result, indent=2) + "\n")
    print(
        f"PASS: {result['fixture_count']} overlap fixtures, "
        f"{result['native_transformation_count']} native/v5 transformations; "
        "legacy selections, allowlists, Unicode offsets, structured Authorization, "
        "locales, and unchanged Python default."
    )


if __name__ == "__main__":
    main()
