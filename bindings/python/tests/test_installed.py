"""Verify the installed Python wheel against the checked-in fixtures."""

from __future__ import annotations

import json
from pathlib import Path

from datafog_core import scan


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
    print("Installed datafog_core wheel matches fixtures and the Finding contract.")


if __name__ == "__main__":
    main()
