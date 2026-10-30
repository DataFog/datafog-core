"""Explicit setup and read-only diagnosis for optional native language consumers."""
import argparse
import json
from pathlib import Path
import shutil
import re
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
PACKAGE = ROOT / "integrations/fogvault-node"


def pins():
    channels = []
    for package in ("fogvault-node", "fogvault-python"):
        text = (ROOT / "integrations" / package / "rust-toolchain.toml").read_text(encoding="utf-8")
        match = re.search(r'^channel\s*=\s*"([^"]+)"\s*$', text, re.M)
        if not match:
            raise ValueError("Missing native language Rust toolchain pin")
        channels.append(match[1])
    if channels[0] != channels[1]:
        raise ValueError("Native language Rust toolchain pins disagree")
    project = (ROOT / "integrations/fogvault-python/pyproject.toml").read_text(encoding="utf-8")
    maturin = re.search(r'"maturin==([^"]+)"', project)
    if not maturin:
        raise ValueError("Missing exact maturin build-system pin")
    engines = json.loads((PACKAGE / "package.json").read_text(encoding="utf-8"))["engines"]["node"]
    if not re.fullmatch(r">=\d+", engines):
        raise ValueError("Unsupported Node engine pin; require explicit integer minimum")
    return channels[0], maturin[1], int(engines[2:])


def capture(command):
    result = subprocess.run(command, cwd=ROOT, capture_output=True, text=True)
    if result.returncode:
        raise ValueError("Tool unavailable: " + " ".join(map(str, command)))
    return result.stdout.strip()


def tools(require_dependencies=True):
    rust_pin, maturin_pin, node_minimum = pins()
    node = shutil.which("node")
    if not node:
        raise ValueError("Install Node >=24 and npm, then rerun explicit setup")
    version = capture([node, "--version"])
    if int(version.removeprefix("v").split(".")[0]) < node_minimum:
        raise ValueError(f"Node >={node_minimum} required")
    # Run npm's JS entry point with Node; never rely on Windows .cmd execution.
    probe = """const fs=require('fs'),p=require('path');const d=p.dirname(fs.realpathSync(process.execPath));const paths=[process.env.npm_execpath,p.join(d,'node_modules/npm/bin/npm-cli.js'),p.resolve(d,'../lib/node_modules/npm/bin/npm-cli.js')];console.log(paths.find(x=>x&&fs.existsSync(x))||'');"""
    npm = capture([node, "-e", probe])
    if not npm:
        raise ValueError("npm CLI not found beside Node; install supported Node/npm")
    if sys.version_info < (3, 10):
        raise ValueError("Python >=3.10 required")
    found = {"node": version, "npm": capture([node, npm, "--version"]),
             "rust": capture(["rustc", "+" + rust_pin, "--version"]),
             "cargo": capture(["cargo", "+" + rust_pin, "--version"]),
             "rustfmt": capture(["cargo", "+" + rust_pin, "fmt", "--version"]),
             "clippy": capture(["cargo", "+" + rust_pin, "clippy", "--version"]),
             "maturin": capture([sys.executable, "-m", "maturin", "--version"]), "python": sys.version}
    if found["maturin"] != "maturin " + maturin_pin:
        raise ValueError(f"Install maturin=={maturin_pin} explicitly")
    if require_dependencies:
        compiler = PACKAGE / "node_modules/typescript/bin/tsc"
        pin = json.loads((PACKAGE / "package.json").read_text(encoding="utf-8"))["devDependencies"]["typescript"]
        if not compiler.is_file() or capture([node, str(compiler), "--version"]) != "Version " + pin:
            raise ValueError("Missing/mismatched TypeScript; run prepare-fogvault-languages.py")
    return found, node, npm


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--diagnose", action="store_true", help="Read-only; never install dependencies")
    parser.add_argument("--json", action="store_true", help="Emit machine-readable result")
    args = parser.parse_args()
    try:
        _, node, npm = tools(require_dependencies=args.diagnose)
        if not args.diagnose:
            subprocess.run([node, npm, "ci", "--ignore-scripts", "--no-audit", "--no-fund"], cwd=PACKAGE, check=True)
        found, _, _ = tools()
        print(json.dumps({"pass": True, "setup_executed": not args.diagnose, "tools": found}))
        return 0
    except (OSError, ValueError, subprocess.SubprocessError) as error:
        print(str(error), file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
