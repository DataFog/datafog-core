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
# Issue #87 reproduces on fastino/gliner2-base-v1 with a four-label described
# schema. Prefer that bundle; fall back to the privacy-filter engine bundle.
BUNDLE_CANDIDATES = (
    ROOT / "models" / "gliner2-base-v1",
    ROOT / "models" / "gliner2",
)

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


def selected_bundle() -> Path | None:
    for bundle in BUNDLE_CANDIDATES:
        if (bundle / "bundle-manifest.json").exists():
            return bundle
    return None


def require_bundle() -> Path | None:
    required = os.environ.get("DATAFOG_REQUIRE_GLINER2") == "1"
    bundle = selected_bundle()
    if bundle is not None:
        return bundle
    message = "GLiNER2 bundle missing (looked for models/gliner2-base-v1 and models/gliner2)"
    if required:
        raise SystemExit(f"FAIL: {message} (DATAFOG_REQUIRE_GLINER2=1)")
    print(f"SKIP: {message}")
    return None


def python_interpreter() -> Path:
    """Absolute interpreter path without resolving a venv `python` symlink.

    ModelSession launches `python -I`. Resolving `venv/bin/python` to
    `/usr/bin/python3` would drop the venv site-packages and fail the load.
    """
    if override := os.environ.get("DATAFOG_GLINER2_PYTHON"):
        path = Path(override)
    else:
        venv = ROOT / "target" / "gliner2-venv" / "bin" / "python"
        path = venv if venv.exists() else Path(sys.executable)
    return path if path.is_absolute() else (Path.cwd() / path)


def session_config(bundle: Path) -> dict:
    sha256 = hashlib.sha256((bundle / "bundle-manifest.json").read_bytes()).hexdigest()
    python = python_interpreter()
    return {
        "bundle": str(bundle),
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
            # Four-label described schema from issue #87. Descriptions change
            # count-head behavior; a single unlabeled street_address task does
            # not reproduce the skip on gliner2-base-v1.
            "labels": [
                {"label": "person", "threshold": 0.5, "description": "A person's full name"},
                {
                    "label": "street_address",
                    "threshold": 0.5,
                    "description": "A street address including house number and street name",
                },
                {
                    "label": "current_employer",
                    "threshold": 0.5,
                    "description": "The organization the caller currently works for",
                },
                {"label": "salary", "threshold": 0.5, "description": "A compensation amount"},
            ],
            "intra_threads": 1,
            "chunk_size": 384,
            "chunk_overlap": 64,
        },
    }


def addresses(result) -> list[str]:
    return [f.matched_text for f in result.findings if f.entity_type == "STREET_ADDRESS"]


def main() -> int:
    bundle = require_bundle()
    if bundle is None:
        return 0
    print(f"Using bundle {bundle}")
    config = session_config(bundle)
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
                failures.append(f"no-PII input {text!r} returned street_address {found}")
    if failures:
        print("FAIL: entity-count gating still drops valid spans:")
        for item in failures:
            print(f"  - {item}")
        return 1
    print("PASS: GLiNER2 entity-count gating regression")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
