"""Qualify the installed reference-backup importer; checking never downloads/builds prerequisites."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import sys
from fogvault_inputs import core_build_inputs

ROOT = Path(__file__).resolve().parents[1]
REV = "6c6176a3a9df320447ba42b924b26de7ac4d693a"

def validate(path):
    data = json.loads(path.read_text())
    if (data.get("schema") != "datafog-fogvault-preparation/v1" or data.get("pass") is not True
            or data.get("fogvault_commit") != REV or data.get("core_build_inputs") != core_build_inputs(ROOT)):
        raise ValueError("Prepared Core/FogVault inputs do not match this checkout; run explicit integration preparation")
    for name in ("core_tarball", "vault_tarball"):
        item = data["artifacts"][name]
        archive = Path(item["path"])
        if hashlib.sha256(archive.read_bytes()).hexdigest() != item["sha256"]:
            raise ValueError("Prepared candidate bytes changed")
    return data


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--prepared", type=Path, default=ROOT/"target/fogvault-integration/prepared.json")
    parser.add_argument("--output", type=Path)
    parser.add_argument("--engine", choices=("chromium", "firefox", "msedge"), default="chromium")
    parser.add_argument("--validate-only", action="store_true")
    args = parser.parse_args()
    try:
        data = validate(args.prepared)
        if args.validate_only:
            print(json.dumps(data, sort_keys=True))
            return 0
        if args.output is None:
            parser.error("--output requires a fresh evidence path")
        subprocess.run([sys.executable, str(ROOT/"scripts/test-fogvault-legacy.py")], cwd=ROOT, check=True)
        return subprocess.run(["node", str(ROOT/"integrations/fogvault-legacy-browser/test/run-installed.mjs"),
            "--prepared", str(args.prepared.resolve()), "--core-tarball", data["artifacts"]["core_tarball"]["path"],
            "--vault-tarball", data["artifacts"]["vault_tarball"]["path"], "--engine", args.engine,
            "--output", str(args.output.resolve())], cwd=ROOT).returncode
    except (OSError, ValueError, KeyError, TypeError, subprocess.SubprocessError) as error:
        print(str(error), file=sys.stderr)
        return 1

if __name__ == "__main__":
    raise SystemExit(main())
