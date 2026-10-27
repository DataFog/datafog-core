"""Issue #87 regression: GLiNER2 entity-count gating must not skip valid spans.

Fails (does not skip) when the GLiNER2 bundle is required and missing.
Used by `cargo dfv check --models` and the dfv-models workflow.
"""
from __future__ import annotations

import hashlib
import json
import os
import sys
from pathlib import Path

from datafog_core import ModelSession

ROOT = Path(__file__).resolve().parents[1]
BUNDLE = ROOT / "models" / "gliner2"
MANIFEST = BUNDLE / "bundle-manifest.json"

# Exact reproduction inputs from https://github.com/DataFog/datafog-core/issues/87
POSITIVE = [
    ("My home address is 27 Willow Lane.", ["27 Willow Lane"]),
    ("Please send mail to 912 Spruce Road.", ["912 Spruce Road"]),
    (
        "Send packages to 27 Willow Lane or 912 Spruce Road.",
        ["27 Willow Lane", "912 Spruce Road"],
    ),
]
NEGATIVE = [
    "The weather is nice today.",
    "Hello world.",
    "This is a test sentence with no PII.",
    "Mathematics is fascinating.",
]


def require_bundle() -> bool:
    required = os.environ.get("DATAFOG_REQUIRE_GLINER2") == "1"
    if MANIFEST.exists():
        return True
    message = f"GLiNER2 bundle missing: {MANIFEST}"
    if required:
        raise SystemExit(f"FAIL: {message} (DATAFOG_REQUIRE_GLINER2=1)")
    print(f"SKIP: {message}")
    return False


def session_config() -> dict:
    sha256 = hashlib.sha256(MANIFEST.read_bytes()).hexdigest()
    python = Path(sys.executable).resolve()
    return {
        "bundle": str(BUNDLE),
        "manifest_sha256": sha256,
        "limits": {
            "max_input_bytes": 65536,
            "max_response_bytes": 16777216,
            "max_findings": 65536,
            "timeout_ms": 120000,
            "cleanup_timeout_ms": 5000,
        },
        "gliner2": {
            "python": str(python),
            "labels": [{"label": "street_address", "threshold": 0.5}],
            "intra_threads": 1,
            "chunk_size": 384,
            "chunk_overlap": 64,
        },
    }


def addresses(result) -> list[str]:
    return [f.matched_text for f in result.findings if f.entity_type == "STREET_ADDRESS"]


def main() -> int:
    if not require_bundle():
        return 0
    config = session_config()
    failures = []
    with ModelSession.open(config) as session:
        for text, expected in POSITIVE:
            found = addresses(session.scan_model(text))
            missing = [item for item in expected if not any(item in span for span in found)]
            print(f"POSITIVE {text!r}")
            print(f"  expected={expected} found={found}")
            if missing:
                failures.append(f"{text!r} missing {missing}; found {found}")
        for text in ("", "   \n\t   "):
            found = addresses(session.scan_model(text))
            print(f"EMPTY {text!r} found={found}")
            if found:
                failures.append(f"empty/whitespace input returned {found}")
        for text in NEGATIVE:
            found = addresses(session.scan_model(text))
            print(f"NEGATIVE {text!r} found={found}")
            if found:
                print(f"  warning: street_address on a no-PII input: {found}", file=sys.stderr)
    if failures:
        print("FAIL: entity-count gating still drops valid spans:")
        for item in failures:
            print(f"  - {item}")
        return 1
    print("PASS: GLiNER2 entity-count gating regression")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
