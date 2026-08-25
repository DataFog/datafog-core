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
        {"label": entity.label, "text": entity.text, "start": entity.start, "end": entity.end}
        for entity in scan(text)
    ]


def verify_fixture(name: str) -> None:
    path = ROOT / "fixtures" / name
    for line in path.read_text().splitlines():
        record = json.loads(line)
        actual = actual_entities(record["text"])
        expected = expected_entities(record)
        assert actual == expected, record["id"]


def main() -> None:
    verify_fixture("development.jsonl")
    verify_fixture("final.jsonl")
    print("Installed datafog_core wheel matches both fixtures.")


if __name__ == "__main__":
    main()
