"""Build a fresh ABI3 wheel, install it in isolation and require named native cases."""
import argparse
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import subprocess
import sys
import tempfile
from zipfile import ZipFile

PACKAGE = Path(__file__).resolve().parents[1]
ROOT = PACKAGE.parents[1]
REQUIRED = {
    "test_installed_native_extension_not_source_wrapper",
    "test_create_review_commit_lock_reopen_read_and_unicode_offsets",
    "test_only_literal_true_approves_and_rejection_preserves_pending_plan",
    "test_tenant_and_thread_boundaries_are_controlled",
    "test_foreign_plan_and_conflicting_writer_reject",
    "test_wrong_password_identity_and_full_head_reject",
    "test_create_never_reopens_or_overwrites_existing_paths",
    "test_heads_are_lossless_python_ints_and_strict_bytes",
    "test_streams_revoke_on_lock_and_saved_head_change",
    "test_owner_gc_revokes_surviving_children",
    "test_conversation_pending_then_observed_send_remains_explicit",
    "test_progressive_failure_has_no_failed_call_text",
    "test_unsupported_profile_and_malformed_transport_fail_before_creation",
    "test_reports_counts_and_policy_delegate_to_core",
    "test_read_only_review_recall_batches_and_policy_preview",
    "test_authenticated_recovery_uses_new_destination_and_preserves_source",
    "test_restart_reconciliation_returns_only_receipt",
    "test_malformed_feed_transport_terminates_response_and_utf16_is_exact",
}

def sha(data):
    return hashlib.sha256(data).hexdigest()

def inputs():
    files = subprocess.check_output(["git", "ls-files", "--cached", "--others", "--exclude-standard", "-z"], cwd=ROOT).decode().split("\0")
    prefixes = ("integrations/fogvault-python/", "integrations/fogvault-native/",
                "integrations/fogvault-migration/", "crates/core/", "fixtures/workspace/", ".cargo/")
    exact = {"Cargo.toml", "Cargo.lock", "rust-toolchain.toml"}
    return {f: sha((ROOT/f).read_bytes()) for f in sorted(set(files))
            if f and (f in exact or f.startswith(prefixes)) and (ROOT/f).is_file()}

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--target-dir", type=Path, default=ROOT/"target/fogvault-language")
    args = parser.parse_args()
    output = args.output.resolve()
    if output.exists():
        parser.error("Use a fresh report path; existing evidence is never overwritten")
    output.parent.mkdir(parents=True, exist_ok=True)
    work = Path(tempfile.mkdtemp(prefix="python-consumer-", dir=output.parent))
    wheels = work/"artifacts"; wheels.mkdir()
    environment = dict(os.environ, RUSTUP_TOOLCHAIN="1.94.1", CARGO_TARGET_DIR=str(args.target_dir.resolve()),
                       CARGO_PROFILE_DEV_DEBUG="0", CARGO_PROFILE_TEST_DEBUG="0", CARGO_INCREMENTAL="0")
    before = inputs()
    commands = []
    versions = {}
    candidate = None
    result = None
    installed = {}
    failure = None

    def run(name, command, timeout=1800):
        log = work/(name+".log")
        with log.open("w", encoding="utf-8") as stream:
            completed = subprocess.run(command, cwd=ROOT, env=environment, stdout=stream, stderr=subprocess.STDOUT, timeout=timeout)
        commands.append({"id":name,"command":[str(x) for x in command],"exit_code":completed.returncode,
                         "log":str(log),"log_sha256":sha(log.read_bytes())})
        if completed.returncode:
            raise ValueError(name+" failed; inspect the retained command log")
        return log.read_text(encoding="utf-8", errors="replace").strip()

    try:
        for name, command in [
            ("rust", ["rustc", "+1.94.1", "--version"]),
            ("cargo", ["cargo", "+1.94.1", "--version"]),
            ("maturin", [sys.executable, "-m", "maturin", "--version"]),
            ("python", [sys.executable, "--version"]),
        ]:
            versions[name] = run(name+"-version", command, 30)
        if versions["maturin"] != "maturin 1.14.1":
            raise ValueError("Install repository-declared maturin 1.14.1 before building")
        run("wheel-build", [sys.executable, "-m", "maturin", "build", "--locked", "--manifest-path", str(PACKAGE/"Cargo.toml"),
            "--out", str(wheels), "--interpreter", sys.executable])
        artifacts = list(wheels.glob("*.whl"))
        if len(artifacts) != 1:
            raise ValueError("Build must produce exactly one wheel")
        wheel = artifacts[0]
        candidate = {"path":str(wheel),"bytes":wheel.stat().st_size,"sha256":sha(wheel.read_bytes())}
        run("isolated-venv", [sys.executable, "-m", "venv", str(work/"venv")])
        python = work/"venv"/("Scripts/python.exe" if os.name == "nt" else "bin/python")
        run("candidate-install", [str(python), "-I", "-m", "pip", "install", "--no-index", "--no-deps", str(wheel)])
        module_path = Path(run("installed-module", [str(python), "-I", "-c", "import datafog_fogvault; print(datafog_fogvault.__file__)"]))
        package = module_path.parent
        if not package.is_relative_to(work/"venv"):
            raise ValueError("Consumer imported a package outside the isolated environment")
        with ZipFile(wheel) as archive:
            names = archive.namelist()
            if len(names) != len(set(n.casefold() for n in names)):
                raise ValueError("Wheel has duplicate/colliding members")
            for member in names:
                if member.startswith("/") or "\\" in member or any(p in {".",".."} for p in member.split("/")):
                    raise ValueError("Unsafe wheel member path")
                if not member.startswith("datafog_fogvault/") or member.endswith("/"):
                    continue
                path = package/member.removeprefix("datafog_fogvault/")
                if not path.is_file() or path.is_symlink() or sha(path.read_bytes()) != sha(archive.read(member)):
                    raise ValueError("Installed package differs from candidate wheel")
                installed[member] = sha(path.read_bytes())
        if not any(n.endswith((".so", ".pyd")) for n in installed) or "datafog_fogvault/__init__.pyi" not in installed:
            raise ValueError("Candidate omitted native code or public type declarations")
        child = work/"installed-cases.json"
        run("installed-contract", [str(python), "-I", str(PACKAGE/"tests/test_installed.py"), "--report", str(child)])
        result = json.loads(child.read_text(encoding="utf-8"))
        cases = result.get("cases", [])
        if (result.get("pass") is not True or result.get("executed") is not True
                or len(cases) != len(REQUIRED) or {c.get("id") for c in cases} != REQUIRED
                or any(c.get("executed") is not True or c.get("status") != "pass" for c in cases)):
            raise ValueError("Installed suite failed, skipped or omitted a required named case")
        if sha(wheel.read_bytes()) != candidate["sha256"] or inputs() != before:
            raise ValueError("Source or candidate changed during build/verification")
    except (OSError, ValueError, TypeError, KeyError, subprocess.SubprocessError) as error:
        failure = str(error)
    report = {"schema":"datafog-fogvault-python-proof/v1","pass":failure is None,"executed":True,
              "recorded_at_utc":datetime.now(timezone.utc).isoformat(),"platform":platform.platform(),
              "git_commit":subprocess.check_output(["git","rev-parse","HEAD"],cwd=ROOT,text=True).strip(),
              "git_status":subprocess.check_output(["git","status","--porcelain"],cwd=ROOT,text=True),
              "input_identity":before,"tools":versions,"candidate":candidate,"installed_assets":installed,
              "required_cases":sorted(REQUIRED),"result":result,"commands":commands,"failure":failure,
              "run_directory":str(work),"qualification":"Synthetic installed native candidate; no provider, physical-device, power-loss or registry-release approval"}
    output.write_text(json.dumps(report,indent=2)+"\n",encoding="utf-8")
    print(json.dumps({"pass":report["pass"],"report":str(output)}))
    return 0 if report["pass"] else 1

if __name__ == "__main__":
    sys.exit(main())
