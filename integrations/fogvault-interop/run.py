"""Run installed Node -> Python -> Node on one real native vault, without rebuilding."""
import argparse
from datetime import datetime, timezone
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import secrets
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[2]


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def identity():
    names = subprocess.check_output(["git", "ls-files", "--cached", "--others", "--exclude-standard", "-z"], cwd=ROOT).decode().split("\0")
    return {name: digest(ROOT / name) for name in sorted(set(names)) if name and (ROOT / name).is_file()}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--node-report", type=Path, required=True)
    parser.add_argument("--python-report", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    args.output = args.output.resolve()
    args.output.parent.mkdir(parents=True, exist_ok=True)
    if args.output.exists():
        parser.error("Output already exists; use a fresh evidence path")
    base = ROOT / "target/fogvault-language-interop"
    base.mkdir(parents=True, exist_ok=True)
    run_dir = Path(tempfile.mkdtemp(prefix="interop-", dir=base))
    before = identity()
    checks, failure = [], None
    try:
        node = json.loads(args.node_report.read_text(encoding="utf-8"))
        python = json.loads(args.python_report.read_text(encoding="utf-8"))
        spec = importlib.util.spec_from_file_location("language_gate", ROOT / "scripts/verify-fogvault-languages.py")
        gate = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(gate)
        # The standalone runner accepts only the exact qualified installed files,
        # not merely reports whose old top-level pass flag still reads true.
        gate.validate_producers(node, python)
        module = Path(node["artifacts"]["consumerPath"]) / "node_modules/@datafog/core-fogvault-node/index.mjs"
        executable = Path(python["run_directory"]) / "venv" / ("Scripts/python.exe" if os.name == "nt" else "bin/python")
        config = {"tenant": "synthetic-cross-language-tenant", "control": str(run_dir / "control"),
                  "vault": str(run_dir / "vault.sqlite"), "password": list(b"synthetic-only cross-language password"),
                  "source": "🦀 alice@example.test", "value": "alice@example.test",
                  "pythonSource": "🦀 bob@example.test", "pythonValue": "bob@example.test",
                  "maximumTime": 253402300799999,
                  "nodeResult": str(run_dir / "node.json"), "pythonResult": str(run_dir / "python.json"),
                  "finalResult": str(run_dir / "final.json")}
        for name in ["recordId", "namespace", "logicalId", "createOperation", "createRequest", "nodeOperation",
                     "nodeRequest", "pythonOperation", "pythonRequest", "sendOperation", "sendRequest",
                     "observeOperation", "observeRequest"]:
            value = secrets.token_bytes(16)
            while not any(value): value = secrets.token_bytes(16)
            config[name] = list(value)
        config_path = run_dir / "synthetic-config.json"
        config_path.write_text(json.dumps(config), encoding="utf-8")
        commands = [
            ("node_produces_confirmed_encrypted_state", ["node", str(ROOT / "integrations/fogvault-interop/node.mjs"), "produce", str(config_path), str(module)]),
            ("python_reopens_and_preserves_node_state", [str(executable), "-I", str(ROOT / "integrations/fogvault-interop/python.py"), str(config_path)]),
            ("node_reopens_python_state_and_scoped_issuance", ["node", str(ROOT / "integrations/fogvault-interop/node.mjs"), "verify", str(config_path), str(module)]),
        ]
        for name, argv in commands:
            log_path = run_dir / (name + ".log")
            with log_path.open("w", encoding="utf-8") as log:
                result = subprocess.run(argv, cwd=ROOT, stdout=log, stderr=subprocess.STDOUT)
            checks.append({"id": name, "pass": result.returncode == 0, "executed": True,
                           "command": argv, "log": str(log_path), "log_sha256": digest(log_path)})
            if result.returncode:
                raise ValueError(f"Interop command failed; see {log_path}")
        gate.validate_producers(node, python)
    except (OSError, ValueError, KeyError, TypeError) as error:
        failure = str(error)
    checks.append({"id": "source_stability", "pass": before == identity(), "executed": True})
    passed = failure is None and len(checks) == 4 and all(c["pass"] for c in checks)
    report = {"schema": "datafog-fogvault-native-interop/v1", "pass": passed, "executed": True,
              "recorded_at_utc": datetime.now(timezone.utc).isoformat(), "checks": checks,
              "input_identity": before, "failure": failure, "run_directory": str(run_dir),
              "node_proof_sha256": digest(args.node_report) if args.node_report.is_file() else None,
              "python_proof_sha256": digest(args.python_report) if args.python_report.is_file() else None,
              "qualification": "Synthetic actual installed packages, shared SQLite/control namespace; not provider or power-loss qualification"}
    with args.output.open("x", encoding="utf-8") as stream:
        stream.write(json.dumps(report, indent=2) + "\n")
    print(json.dumps({"pass": passed, "report": str(args.output)}))
    return 0 if passed else 1


if __name__ == "__main__":
    raise SystemExit(main())
