#!/usr/bin/env python3
"""Validate Python notices/source pins and smoke the actual candidate wheel."""
import argparse
from email.parser import Parser
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import tarfile
import tempfile
import tomllib
import venv
import zipfile

ROOT = Path(__file__).resolve().parents[1]
LICENSES = {
    "LICENSE": ROOT / "LICENSE",
    "LICENSE-APACHE-2.0.txt": ROOT / "crates/core/src/cnn/assets/LICENSE-APACHE-2.0.txt",
    "LICENSE-CC0-names.txt": ROOT / "crates/core/src/cnn/assets/LICENSE-CC0-names.txt",
}


def check(path, version):
    """Check the bytes shipped to the registry, without extracting archives."""
    if path.suffix == ".whl":
        with zipfile.ZipFile(path) as archive:
            metadata_paths = [name for name in archive.namelist() if name.endswith(".dist-info/METADATA")]
            if len(metadata_paths) != 1:
                raise ValueError("wheel must have exactly one distribution metadata file")
            metadata = Parser().parsestr(archive.read(metadata_paths[0]).decode())
            if metadata["Name"] != "datafog-core" or metadata["Version"] != version:
                raise ValueError("wheel metadata differs from candidate version")
            notices = metadata.get_all("License-File", [])
            prefix = metadata_paths[0].removesuffix("METADATA") + "licenses/"
            for name, source in LICENSES.items():
                matches = [notice for notice in notices if Path(notice).name == name]
                if len(matches) != 1 or archive.read(prefix + matches[0]) != source.read_bytes():
                    raise ValueError("wheel licence is missing or differs: " + name)
            if "datafog_core/py.typed" not in archive.namelist():
                raise ValueError("wheel has no py.typed marker")
    elif path.name.endswith(".tar.gz"):
        with tarfile.open(path) as archive:
            roots = {name.split("/", 1)[0] for name in archive.getnames()}
            if len(roots) != 1:
                raise ValueError("sdist must have one root")
            prefix = next(iter(roots)) + "/"
            for name, source in {"LICENSE": ROOT / "LICENSE", "rust-toolchain.toml": ROOT / "rust-toolchain.toml"}.items():
                member = archive.extractfile(prefix + name)
                if member is None or member.read() != source.read_bytes():
                    raise ValueError("sdist source file is missing or differs: " + name)
            project = archive.extractfile(prefix + "pyproject.toml")
            if project is None or tomllib.loads(project.read().decode())["project"]["version"] != version:
                raise ValueError("sdist version differs from candidate")
    else:
        raise ValueError("unexpected Python distribution: " + path.name)
    return {"file": path.name, "sha256": hashlib.sha256(path.read_bytes()).hexdigest(), "version": version}


def smoke(directory, version):
    with tempfile.TemporaryDirectory(prefix="datafog-wheel-smoke-") as temporary:
        root = Path(temporary)
        venv.EnvBuilder(with_pip=True).create(root / "venv")
        python = root / "venv" / ("Scripts/python.exe" if sys.platform == "win32" else "bin/python")
        subprocess.run([str(python), "-m", "pip", "install", "--no-index", "--no-deps", "--only-binary=:all:",
                        "--find-links", str(directory.resolve()), "datafog-core==" + version], check=True, cwd=root)
        subprocess.run([str(python), "-c", '''
import importlib.metadata, pathlib, sys
import datafog_core as df
assert pathlib.Path(df.__file__).is_relative_to(pathlib.Path(sys.prefix))
assert importlib.metadata.version("datafog-core") == sys.argv[1] == df.__version__
text = "👋 café jane@example.com 123-45-6789"
findings = df.scan(text)
assert {item.entity_type for item in findings} >= {"EMAIL", "SSN"}
for item in findings:
    assert text[item.codepoint_range.start:item.codepoint_range.end] == item.matched_text
    assert text.encode()[item.byte_range.start:item.byte_range.end].decode() == item.matched_text
assert df.scan_and_transform(text, {"transform":{"default":{"strategy":"redact"}}}).text == "👋 café [EMAIL] [SSN]"
try:
    df.scan(text, {"entities":"EMAIL"})
except df.DataFogConfigurationError:
    pass
else:
    raise AssertionError("invalid configuration accepted")
''', version], check=True, cwd=root)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("directory", type=Path)
    parser.add_argument("--smoke", action="store_true", help="install and execute the compatible wheel in a fresh venv")
    args = parser.parse_args()
    paths = sorted(args.directory.glob("*.whl")) + sorted(args.directory.glob("*.tar.gz"))
    if not paths:
        raise ValueError("no Python distributions found")
    version = tomllib.loads((ROOT / "bindings/python/pyproject.toml").read_text())["project"]["version"]
    evidence = [check(path, version) for path in paths]
    if args.smoke:
        if not any(path.suffix == ".whl" for path in paths):
            raise ValueError("wheel smoke requires a candidate wheel")
        smoke(args.directory, version)
    print(json.dumps({"artifacts": evidence, "installed_smoke": args.smoke}, sort_keys=True))


if __name__ == "__main__":
    main()
