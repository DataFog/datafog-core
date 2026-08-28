"""Verify the installed Python wheel against the checked-in fixtures."""

from __future__ import annotations

import json
from pathlib import Path

from datafog_core import Finding, TextRange, scan, scan_and_transform, transform


ROOT = Path(__file__).resolve().parents[3]


def expected_entities(record: dict[str, object]) -> list[dict[str, object]]:
    return record["entities"]  # type: ignore[return-value]


def actual_entities(text: str) -> list[dict[str, object]]:
    return [
        {
            "label": finding.entity_type,
            "text": finding.matched_text,
            "start": finding.codepoint_range.start,
            "end": finding.codepoint_range.end,
        }
        for finding in scan(text)
    ]


def verify_contract(text: str) -> None:
    encoded = text.encode("utf-8")
    for finding in scan(text):
        assert (
            encoded[finding.byte_range.start : finding.byte_range.end].decode("utf-8")
            == finding.matched_text
        )
        assert (
            text[finding.codepoint_range.start : finding.codepoint_range.end]
            == finding.matched_text
        )
        assert finding.confidence is None
        assert finding.detector_name.startswith("datafog-core/")
        assert finding.detector_version


def verify_fixture(name: str) -> None:
    path = ROOT / "fixtures" / name
    for line in path.read_text().splitlines():
        record = json.loads(line)
        actual = actual_entities(record["text"])
        expected = expected_entities(record)
        assert actual == expected, record["id"]
        verify_contract(record["text"])


def main() -> None:
    verify_fixture("development.jsonl")
    verify_fixture("final.jsonl")
    emoji_finding = scan("👋 jane@example.com")[0]
    assert (emoji_finding.byte_range.start, emoji_finding.byte_range.end) == (5, 21)
    assert (emoji_finding.codepoint_range.start, emoji_finding.codepoint_range.end) == (2, 18)

    text = "👋 jane@example.com and jane@example.com"
    explicit = transform(text, scan(text), {"strategy": "redact"})
    convenience = scan_and_transform(text, {"strategy": "redact"})
    assert explicit == convenience
    assert explicit.text == "👋 [EMAIL] and [EMAIL]"
    assert len(explicit.transformations) == 2
    first = explicit.transformations[0]
    assert first.replacement == "[EMAIL]"
    assert (first.output_byte_range.start, first.output_byte_range.end) == (5, 12)
    assert (first.output_codepoint_range.start, first.output_codepoint_range.end) == (2, 9)

    masked = scan_and_transform(
        "Email jane@example.com",
        {"strategy": "mask"},
    )
    assert masked.text == "Email ****************"

    partially_masked = scan_and_transform(
        "Email jane@example.com",
        {
            "strategy": "mask",
            "character": "•",
            "reveal": {"direction": "last", "count": 4},
        },
    )
    assert partially_masked.text == "Email ••••••••••••.com"
    assert partially_masked.transformations[0].strategy == "mask"
    assert partially_masked.transformations[0].replacement == "••••••••••••.com"
    assert (
        partially_masked.transformations[0].output_byte_range.start,
        partially_masked.transformations[0].output_byte_range.end,
    ) == (6, 46)

    unchanged = scan_and_transform(
        "Email jane@example.com",
        {
            "strategy": "mask",
            "reveal": {"direction": "first", "count": 99},
        },
    )
    assert unchanged.text == "Email jane@example.com"

    removed = scan_and_transform(
        "Email jane@example.com today",
        {"strategy": "remove"},
    )
    assert removed.text == "Email  today"
    assert removed.transformations[0].strategy == "remove"
    assert removed.transformations[0].replacement == ""
    assert (
        removed.transformations[0].output_codepoint_range.start,
        removed.transformations[0].output_codepoint_range.end,
    ) == (6, 6)

    invalid_configs = [
        {"strategy": "mask", "character": ""},
        {"strategy": "mask", "character": "**"},
        {"strategy": "mask", "character": " "},
        {"strategy": "mask", "unexpected": True},
        {"strategy": "remove", "character": "*"},
        {
            "strategy": "mask",
            "reveal": {"direction": "last", "count": -1},
        },
        {
            "strategy": "mask",
            "reveal": {"direction": "last", "count": True},
        },
        {
            "strategy": "mask",
            "reveal": {"direction": "middle", "count": 4},
        },
    ]
    for invalid_config in invalid_configs:
        try:
            scan_and_transform("Email jane@example.com", invalid_config)
        except ValueError:
            pass
        else:
            raise AssertionError(f"invalid configuration was accepted: {invalid_config}")

    invalid = Finding(
        "EMAIL",
        "jane@example.com",
        TextRange(5, 21),
        TextRange(2, 18),
        "test-detector",
        confidence=2.0,
    )
    try:
        transform(text, [invalid], {"strategy": "redact"})
    except ValueError as error:
        assert "InvalidConfidence" in str(error)
    else:
        raise AssertionError("invalid caller-supplied finding was accepted")

    print("Installed datafog_core wheel matches fixtures and transform contracts.")


if __name__ == "__main__":
    main()
