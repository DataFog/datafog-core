#!/usr/bin/env python3
"""Regenerate frozen feature metadata from the pinned reference source.

Usage: python scripts/generate-cnn-features.py /path/to/cnn-NA-1mb
Requires Python Unicode 16.0.0. Does not require training data or model runtime.
"""
import json
from pathlib import Path
import struct
import subprocess
import sys
import unicodedata

source = Path(sys.argv[1]).resolve()
revision = "75f5a20e05380de5457502afefaca0383c4e8e91"
if subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=source, text=True).strip() != revision:
    raise SystemExit("reference source revision mismatch")
if unicodedata.unidata_version != "16.0.0":
    raise SystemExit("Unicode 16.0.0 required")
if (source / "raw/libpostal/street_types.txt").exists():
    raise SystemExit("reference must not contain unpinned optional libpostal vocabulary")
sys.path.insert(0, str(source / "baselines"))
import rules  # noqa: E402

root = Path(__file__).resolve().parents[1] / "crates/core/src/cnn/assets"
names = ["RE_US_PC", "RE_UK_PC", "RE_CA_PC", "RE_NUM_START", "RE_POBOX", "RE_RR",
         "COMPANY_WORDS", "LABEL_LINE", "POS_CUES", "NEG_CUES", "STUB_CUES"]
obj = {key: getattr(rules, key).pattern for key in names}
for key in ["STREET_TYPES", "STATES", "PROVS"]:
    obj[key] = sorted(getattr(rules, key))
(root / "rules.json").write_text(json.dumps(obj, separators=(",", ":")))
rows = []
start = 0
previous = None
for point in range(0x110000):
    c = chr(point)
    category = unicodedata.category(c)
    flags = (int(category[0] in "LMN") | (int(c.isdigit()) << 1) | (int(category[0] == "P") << 2)
             | (int(c.islower()) << 3) | (int(c.isupper()) << 4) | (int(c.istitle()) << 5)
             | (int(c.isalnum() or c == "_") << 6) | (int(c.isspace()) << 7))
    if flags != previous:
        if previous is not None:
            rows.append((start, point - 1, previous))
        start = point
        previous = flags
rows.append((start, 0x10ffff, previous))
(root / "unicode.bin").write_bytes(b"".join(struct.pack("<IIB", *row) for row in rows))
