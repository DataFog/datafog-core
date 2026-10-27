#!/usr/bin/env python3
"""Validate that fixture entity spans match their text at declared offsets.

Runs automatically during `cargo dfv check` for all *.jsonl fixture files.
Exit codes:
  0 = all offsets valid
  1 = one or more offset mismatches found
"""
import json
import sys
from pathlib import Path


def validate_fixture(path: Path) -> list[str]:
    """Validate entity offsets in a fixture file, returning list of errors."""
    errors = []
    with open(path, encoding="utf-8") as f:
        for line_num, line in enumerate(f, 1):
            line = line.strip()
            if not line:
                continue
            try:
                record = json.loads(line)
            except json.JSONDecodeError as e:
                errors.append(f"{path}:{line_num}: JSON parse error: {e}")
                continue

            fixture_id = record.get("id", f"line-{line_num}")
            text = record.get("text", "")

            # Skip fixtures with synthetic credentials — their offsets refer to
            # post-expansion text, validated separately by the test harness.
            if "synthetic_credentials" in record:
                continue
            entities = record.get("entities", [])

            for i, entity in enumerate(entities):
                label = entity.get("label", "UNKNOWN")
                expected_text = entity.get("text", "")
                start = entity.get("start")
                end = entity.get("end")

                if start is None or end is None:
                    errors.append(
                        f"{path}:{line_num} [{fixture_id}] entity {i} ({label}): "
                        f"missing start or end offset"
                    )
                    continue

                if start < 0 or end < 0:
                    errors.append(
                        f"{path}:{line_num} [{fixture_id}] entity {i} ({label}): "
                        f"negative offset (start={start}, end={end})"
                    )
                    continue

                if start > end:
                    errors.append(
                        f"{path}:{line_num} [{fixture_id}] entity {i} ({label}): "
                        f"start ({start}) > end ({end})"
                    )
                    continue

                if end > len(text):
                    errors.append(
                        f"{path}:{line_num} [{fixture_id}] entity {i} ({label}): "
                        f"end ({end}) exceeds text length ({len(text)})"
                    )
                    continue

                actual_text = text[start:end]
                if actual_text != expected_text:
                    errors.append(
                        f"{path}:{line_num} [{fixture_id}] entity {i} ({label}): "
                        f"offset mismatch — expected {expected_text!r} at [{start}:{end}], "
                        f"got {actual_text!r}"
                    )

    return errors


def main() -> int:
    fixture_dir = Path(__file__).parent.parent / "fixtures"
    if not fixture_dir.exists():
        print(f"Fixture directory not found: {fixture_dir}", file=sys.stderr)
        return 1

    all_errors = []
    fixture_files = sorted(fixture_dir.glob("*.jsonl"))

    if not fixture_files:
        print("No fixture files found", file=sys.stderr)
        return 1

    for path in fixture_files:
        errors = validate_fixture(path)
        all_errors.extend(errors)

    if all_errors:
        print(f"Offset validation failed with {len(all_errors)} error(s):\n")
        for error in all_errors:
            print(f"  {error}")
        return 1

    print(f"Validated {len(fixture_files)} fixture file(s) — all offsets OK")
    return 0


if __name__ == "__main__":
    sys.exit(main())
