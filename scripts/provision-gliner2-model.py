"""Verify pinned GLiNER2 local files and install a complete bundle atomically.

Downloading is a separate explicit `hf download` operation. Never imports or
executes the model's repository code. Requires Python 3.11+ for file_digest.
"""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import tempfile

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--source", type=Path, required=True)
parser.add_argument("--destination", type=Path, required=True)
args = parser.parse_args()
root = Path(__file__).resolve().parent.parent
lock = json.loads((root / "crates/core/src/models/gliner2-lock.json").read_text())
destination = args.destination.absolute()
if destination.exists():
    raise SystemExit("destination must be a fresh directory")
destination.parent.mkdir(parents=True, exist_ok=True)
staging = Path(tempfile.mkdtemp(prefix=".gliner2-install-", dir=destination.parent))
try:
    for name, identity in lock["files"].items():
        source = args.source / name
        if source.is_symlink() or not source.is_file():
            raise SystemExit(f"missing or unsafe required asset: {name}")
        target = staging / name
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(source, target)
        with target.open("rb") as stream:
            actual = hashlib.file_digest(stream, "sha256").hexdigest()
        if actual != identity["sha256"] or target.stat().st_size != identity["bytes"]:
            raise SystemExit(f"asset integrity mismatch: {name}")
    manifest = json.dumps({"platform": "portable-cpu", "files": lock["files"]}, sort_keys=True).encode()
    (staging / "bundle-manifest.json").write_bytes(manifest)
    staging.rename(destination)
    print(json.dumps({"bundle": str(destination), "manifest_sha256": hashlib.sha256(manifest).hexdigest()}))
finally:
    if staging.exists():
        shutil.rmtree(staging)
