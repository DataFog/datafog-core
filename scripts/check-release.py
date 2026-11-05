"""Reject mismatched package versions and release tags before publishing."""
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import tomllib

ROOT = Path(__file__).resolve().parents[1]
CARGO_MANIFESTS = (
    "crates/core/Cargo.toml", "bindings/python/Cargo.toml", "bindings/node/Cargo.toml", "bindings/wasm/Cargo.toml",
    "integrations/fogvault-migration/Cargo.toml", "integrations/fogvault-native/Cargo.toml",
    "integrations/fogvault-node/Cargo.toml", "integrations/fogvault-python/Cargo.toml",
)
PYTHON_MANIFESTS = ("bindings/python/pyproject.toml", "integrations/fogvault-python/pyproject.toml")
NPM_MANIFESTS = (
    "bindings/node/package.json", "bindings/wasm/package.json", "integrations/fogvault-browser/package.json",
    "integrations/fogvault-node/package.json", "integrations/fogvault-legacy-browser/package.json",
    "integrations/sqlite-browser/package.json",
)
LOCKFILES = ("Cargo.lock", *(f"integrations/fogvault-{name}/Cargo.lock" for name in ("migration", "native", "node", "python")))
NUMBER = r"(?:0|[1-9][0-9]*)"
STABLE = rf"{NUMBER}\.{NUMBER}\.{NUMBER}"


def logical_version(value):
    if not isinstance(value, str):
        raise ValueError("Package version must be a string")
    value = re.sub(rf"^({STABLE})b({NUMBER})$", r"\1-beta.\2", value)
    value = re.sub(rf"^({STABLE})rc({NUMBER})$", r"\1-rc.\2", value)
    if not re.fullmatch(rf"{STABLE}(?:-(?:beta|rc)\.{NUMBER})?", value):
        raise ValueError("Unsupported release version: " + value)
    return value


def python_version(version):
    return logical_version(version).replace("-beta.", "b").replace("-rc.", "rc")


def npm_tag(version):
    version = logical_version(version)
    return "next" if "-rc." in version else "beta" if "-beta." in version else "latest"


def check(root=ROOT, prefix=None, environ=None, check_contract=True):
    root = Path(root)
    environ = os.environ if environ is None else environ
    versions, cargo, npm = {}, {}, {}
    for relative in CARGO_MANIFESTS:
        document = tomllib.loads((root / relative).read_text())
        versions[relative] = document["package"]["version"]
        cargo[document["package"]["name"]] = document
    for relative in PYTHON_MANIFESTS:
        versions[relative] = tomllib.loads((root / relative).read_text())["project"]["version"]
    for relative in NPM_MANIFESTS:
        document = json.loads((root / relative).read_text())
        versions[relative] = document["version"]
        npm[document["name"]] = document
        lock = root / Path(relative).with_name("package-lock.json")
        if lock.exists():
            locked = json.loads(lock.read_text())
            versions[str(lock.relative_to(root))] = locked["version"]
            versions[str(lock.relative_to(root)) + ":root"] = locked["packages"][""]["version"]
            if locked["packages"][""].get("peerDependencies", {}) != document.get("peerDependencies", {}):
                raise ValueError("npm lockfile peer requirements differ: " + relative)
    for relative in LOCKFILES:
        packages = tomllib.loads((root / relative).read_text())["package"]
        active = {package["name"]: package["version"] for package in packages if package["name"] in cargo and "source" not in package}
        if relative == "Cargo.lock":
            expected = {tomllib.loads((root / path).read_text())["package"]["name"] for path in CARGO_MANIFESTS[:4]}
        else:
            pending = [tomllib.loads((root / Path(relative).with_name("Cargo.toml")).read_text())["package"]["name"]]
            expected = set()
            while pending:
                name = pending.pop()
                if name in expected:
                    continue
                expected.add(name)
                for section in ("dependencies", "dev-dependencies", "build-dependencies"):
                    for dependency, required in cargo[name].get(section, {}).items():
                        if isinstance(required, dict) and required.get("path") and required.get("package", dependency) in cargo:
                            pending.append(required.get("package", dependency))
        if set(active) != expected:
            raise ValueError("Core lockfile identities are incomplete: " + relative)
        versions.update({f"{relative}:{name}": value for name, value in active.items()})
    normalized = {name: logical_version(value) for name, value in versions.items()}
    if len(set(normalized.values())) != 1:
        raise ValueError(f"Release versions do not match: {versions}")
    version = next(iter(normalized.values()))
    for relative, value in versions.items():
        if relative not in PYTHON_MANIFESTS and value != version:
            raise ValueError("Cargo/npm require SemVer release spelling: " + relative)
    for relative in PYTHON_MANIFESTS:
        if versions[relative] != python_version(version):
            raise ValueError(f"Expected PEP 440 version {python_version(version)}: {relative}")
    for relative in NPM_MANIFESTS:
        document = json.loads((root / relative).read_text())
        if document["version"] != version:
            raise ValueError("npm requires SemVer release spelling: " + relative)
        if document.get("publishConfig", {}).get("tag") != npm_tag(version):
            raise ValueError(f"Publication must use npm {npm_tag(version)} dist-tag: {relative}")
        for name, required in document.get("peerDependencies", {}).items():
            if name in npm and "-" in version and version not in required.split(" || "):
                raise ValueError("Peer requirement excludes this prerelease: " + relative + ":" + name)
    for document in cargo.values():
        for section in ("dependencies", "dev-dependencies", "build-dependencies"):
            for name, required in document.get(section, {}).items():
                if isinstance(required, dict) and required.get("path") and required.get("package", name) in cargo:
                    if required.get("version", "").removeprefix("=") != version:
                        raise ValueError("Core path dependency version differs: " + name)
    if int(version.split(".")[0]) >= 1 and check_contract:
        subprocess.run([sys.executable, str(root / "scripts/check-core-contract.py")], check=True)
    if environ.get("GITHUB_REF_TYPE") == "tag":
        if not prefix:
            raise ValueError("Release tag validation requires a package prefix")
        expected = f"{prefix}-v{version}"
        if environ.get("GITHUB_REF_NAME") != expected:
            raise ValueError(f"Expected release tag {expected}")
    return version


if __name__ == "__main__":
    try:
        version = check(prefix=sys.argv[1] if len(sys.argv) > 1 else None)
    except (ValueError, KeyError) as error:
        raise SystemExit(str(error)) from error
    print(f"All package versions match {version}; release ref is valid.")
