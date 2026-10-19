"""Reject mismatched package versions and release tags before publishing."""

import json
import os
import re
from pathlib import Path
import sys
import tomllib

root = Path(__file__).resolve().parent.parent
versions = {}
for relative in (
    "crates/core/Cargo.toml",
    "bindings/python/Cargo.toml",
    "bindings/node/Cargo.toml",
    "bindings/wasm/Cargo.toml",
    "bindings/python/pyproject.toml",
    "workers/gliner2/pyproject.toml",
):
    document = tomllib.loads((root / relative).read_text())
    versions[relative] = document.get("package", document.get("project"))["version"]
for binding in ("node", "wasm"):
    for filename in ("package.json", "package-lock.json"):
        relative = f"bindings/{binding}/{filename}"
        document = json.loads((root / relative).read_text())
        versions[relative] = document["version"]
        if filename == "package-lock.json":
            versions[f"{relative}:root"] = document["packages"][""]["version"]
for package in tomllib.loads((root / "Cargo.lock").read_text())["package"]:
    if package["name"] in {"datafog-core", "datafog-core-python", "datafog-node", "datafog-wasm"}:
        versions[f"Cargo.lock:{package['name']}"] = package["version"]

def logical_version(value):
    # Cargo/npm SemVer and Python PEP 440 spell this same beta differently.
    return re.sub(r"^(\d+\.\d+\.\d+)b(\d+)$", r"\1-beta.\2", value)

normalized = {name: logical_version(value) for name,value in versions.items()}
if len(set(normalized.values())) != 1:
    raise SystemExit(f"Release versions do not match: {versions}")
version = next(iter(normalized.values()))
if "-beta." in version:
    python_version = version.replace("-beta.", "b")
    for relative in ("bindings/python/pyproject.toml", "workers/gliner2/pyproject.toml"):
        if versions[relative] != python_version:
            raise SystemExit(f"Expected PEP 440 beta version {python_version}: {relative}")
    for binding in ("node", "wasm"):
        package = json.loads((root / f"bindings/{binding}/package.json").read_text())
        if package.get("publishConfig",{}).get("tag") != "beta":
            raise SystemExit(f"Beta publication must use npm beta dist-tag: {binding}")
if os.environ.get("GITHUB_REF_TYPE") == "tag":
    expected = f"{sys.argv[1]}-v{version}"
    if os.environ.get("GITHUB_REF_NAME") != expected:
        raise SystemExit(f"Expected release tag {expected}")
print(f"All package versions match {version}; release ref is valid.")
