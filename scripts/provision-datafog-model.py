"""Explicit local archive provisioning for the pinned DataFog 0.2.0 runtime.

No download or inference occurs. The runtime archive must be obtained separately.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import platform
import shutil
import tarfile
import tempfile

ARCHIVES = {
    "linux-x86_64": "355f54837d3f4ff0aebc72700cb38591176b29e0f75c63c66e04d874df4eb713",
    "linux-aarch64": "c22fd91dd601e22ccb13d074a555e58a6926e7b57adb5234171261763fa5b203",
    "macos-arm64": "b2980aa1de1702957da3182a0aee3ac3fed2686cf146f52eb037256936b04e37",
    "macos-x86_64": "47ae14fe1a37704672c2e6ebcad5e428b4cef9b3c538f092fa9789975199a53a",
    "windows-x86_64": "cd4738fcff9f440fa80ce6a71be8c71719e2137d7fd35a8c099f42238740a8f3",
}


def sha256(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def host():
    system = {"Darwin": "macos", "Linux": "linux", "Windows": "windows"}[platform.system()]
    arch = {"arm64": "arm64", "aarch64": "arm64", "x86_64": "x86_64", "amd64": "x86_64"}[platform.machine().lower()]
    if system == "linux" and arch == "arm64":
        arch = "aarch64"
    return f"{system}-{arch}"


def relative_path(name):
    path = PurePosixPath(name)
    if not name or path.is_absolute() or ".." in path.parts or "\\" in name or ":" in name:
        raise ValueError("unsafe archive path")
    return Path(*path.parts)


def install(archive, destination):
    target = host()
    if sha256(archive) != ARCHIVES[target]:
        raise ValueError("archive integrity mismatch")
    if destination.exists():
        raise ValueError("destination already exists; provision an immutable fresh directory")
    destination.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(dir=destination.parent) as staging:
        root = Path(staging) / "bundle"
        root.mkdir()
        with tarfile.open(archive, "r:gz") as stream:
            for member in stream:
                name = root / relative_path(member.name)
                if member.isdir():
                    name.mkdir(parents=True, exist_ok=True)
                elif member.isfile():
                    name.parent.mkdir(parents=True, exist_ok=True)
                    with stream.extractfile(member) as source, name.open("xb") as output:
                        shutil.copyfileobj(source, output)
                else:
                    raise ValueError("links and special files are forbidden")
        manifest_path = root / "bundle-manifest.json"
        manifest = json.loads(manifest_path.read_text())
        if manifest["platform"] != target:
            raise ValueError("archive platform mismatch")
        for filename, expected in manifest["files"].items():
            file = root / relative_path(filename)
            if file.stat().st_size != expected["bytes"] or sha256(file) != expected["sha256"]:
                raise ValueError("bundle integrity mismatch")
        if os.name != "nt":
            (root / "datafog-pii").chmod(0o755)
        identity = sha256(manifest_path)
        root.rename(destination)
    return identity


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--archive", required=True, type=Path)
    parser.add_argument("--destination", required=True, type=Path)
    args = parser.parse_args()
    try:
        print(json.dumps({"manifest_sha256": install(args.archive, args.destination)}))
    except (OSError, ValueError, KeyError, tarfile.TarError) as error:
        raise SystemExit(f"Provisioning failed ({type(error).__name__}); verify archive, target and destination.") from None
