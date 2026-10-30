"""Explicitly build pinned integration candidates inside Core's ignored target tree."""
import argparse
import hashlib
import json
import os
import shutil
from pathlib import Path
import subprocess
import sys
from fogvault_inputs import core_build_inputs

ROOT = Path(__file__).resolve().parents[1]
REV = "f4dc492549b70f0bc44f5c6bf82fe3022353030f"
BASE = ROOT / "target/fogvault-integration"


def run(args, cwd, log, env=None):
    with log.open("w", encoding="utf-8") as stream:
        result = subprocess.run(args, cwd=cwd, stdout=stream,
                                stderr=subprocess.STDOUT, env=env)
    if result.returncode:
        raise RuntimeError(f"Command failed ({result.returncode}); see {log}")


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def prepare():
    BASE.mkdir(parents=True, exist_ok=True)
    (BASE / "prepared.json").unlink(missing_ok=True)
    artifacts = BASE / "artifacts"
    artifacts.mkdir(exist_ok=True)
    source = BASE / "dependencies/fogvault"
    source.parent.mkdir(exist_ok=True)
    if not source.exists():
        run(["git", "clone", "--no-checkout", "--filter=blob:none",
             "https://github.com/DataFog/fogvault.git", str(source)], ROOT,
            BASE / "clone.log")
    has_working_files = any(path.name != ".git" for path in source.iterdir())
    if has_working_files and subprocess.check_output(["git", "status", "--porcelain"], cwd=source).strip():
        raise RuntimeError("Pinned dependency checkout is dirty; refusing to overwrite it")
    # Explicit preparation can upgrade an existing pinned checkout; its original
    # clone may not contain a later reviewed revision. Never fetch during check.
    run(["git", "fetch", "--no-tags", "origin", REV], source, BASE / "fetch.log")
    run(["git", "checkout", "--detach", REV], source, BASE / "checkout.log")
    if subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=source, text=True).strip() != REV:
        raise RuntimeError("Wrong FogVault revision")
    # This generated workspace stops Cargo discovery at the Core-owned build
    # boundary without changing any file in the pinned FogVault source tree.
    excluded = [str(path.parent.relative_to(source.parent))
                for path in (source / "crates").rglob("Cargo.toml")
                if path.parent != source / "crates/fogvault-wasm"]
    (source.parent / "Cargo.toml").write_text(
        '[workspace]\nmembers = ["fogvault/crates/fogvault-wasm"]\nresolver = "2"\n'
        + 'exclude = ' + json.dumps(sorted(excluded)) + '\n', encoding="utf-8")
    pinned_lock = source / "crates/fogvault-wasm/Cargo.lock"
    generated_lock = source.parent / "Cargo.lock"
    shutil.copy2(pinned_lock, generated_lock)
    run(["npm", "ci", "--legacy-peer-deps", "--ignore-scripts"],
        ROOT / "integrations/fogvault-legacy-browser", BASE / "legacy-npm-ci.log")
    core_inputs = core_build_inputs(ROOT)
    run(["cargo", "dfv", "build", "--sdk", "wasm"], ROOT, BASE / "core-build.log")
    run(["npm", "pack", "--ignore-scripts", "--pack-destination", str(artifacts)],
        ROOT / "bindings/wasm", BASE / "core-pack.log")
    env = dict(os.environ, RUSTUP_TOOLCHAIN="1.94.1",
               CARGO_TARGET_DIR=str(BASE / "fogvault-build"))
    run(["rustup", "target", "add", "--toolchain", "1.94.1", "wasm32-unknown-unknown"],
        ROOT, BASE / "vault-wasm-target.log")
    run(["npm", "ci", "--ignore-scripts"], source / "packages/browser",
        BASE / "vault-npm-ci.log", env)
    run(["npm", "run", "build"], source / "packages/browser", BASE / "vault-build.log", env)
    run(["npm", "pack", "--ignore-scripts", "--pack-destination", str(artifacts)],
        source / "packages/browser", BASE / "vault-pack.log", env)
    if subprocess.check_output(["git", "status", "--porcelain"], cwd=source).strip():
        raise RuntimeError("Dependency build changed reviewed source")
    if sha(generated_lock) != sha(pinned_lock):
        raise RuntimeError("Generated dependency lock differs from pinned source lock")
    if core_build_inputs(ROOT) != core_inputs:
        raise RuntimeError("Core build/package inputs changed during preparation")
    # Browser installation is an intentional preparation step, never part of check.
    run(["node", "node_modules/playwright/cli.js", "install", "chromium"],
        ROOT / "bindings/wasm", BASE / "playwright-install.log")
    core_version = json.loads((ROOT / "bindings/wasm/package.json").read_text())["version"]
    vault_version = json.loads((source / "packages/browser/package.json").read_text())["version"]
    paths = {"core_tarball": artifacts / f"datafog-wasm-{core_version}.tgz",
             "vault_tarball": artifacts / f"datafog-fogvault-browser-{vault_version}.tgz"}
    report = {"schema": "datafog-fogvault-preparation/v1", "pass": True,
              "fogvault_commit": REV,
              "fogvault_lock_sha256": sha(pinned_lock),
              "fogvault_tree": subprocess.check_output(["git", "rev-parse", "HEAD^{tree}"],
                                                        cwd=source, text=True).strip(),
              "core_commit": subprocess.check_output(["git", "rev-parse", "HEAD"],
                                                      cwd=ROOT, text=True).strip(),
              "core_build_inputs": core_inputs,
              "artifacts": {k: {"path": str(p), "sha256": sha(p), "bytes": p.stat().st_size}
                            for k, p in paths.items()}}
    (BASE / "prepared.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(report))


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--json", action="store_true")
    parser.parse_args()
    try:
        prepare()
    except (OSError, ValueError, RuntimeError, subprocess.SubprocessError) as error:
        print(str(error), file=sys.stderr)
        sys.exit(1)
