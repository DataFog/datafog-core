"""Qualify freshly installed native Node/Python packages and their shared vault."""
import argparse
from datetime import datetime, timezone
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import tarfile
from zipfile import ZipFile

ROOT = Path(__file__).resolve().parents[1]
BASE = ROOT / "target/fogvault-language"
NODE_CASES = {
    "tests::native_binding_preserves_u64_and_unknown_request_error",
    "tests::native_binding_rejects_wrong_thread_before_owner_access",
    *{"installed_node_" + suffix for suffix in (
        "roundtrip_approval_lock_reopen_unicode", "tenant_foreign_and_hostile_transport_reject",
        "conflict_and_payload_admission_preserve_saved_state", "conversation_pending_and_scoped_restore",
        "streams_terminal_transport_and_lifetimes", "dropped_ack_reconciliation_requires_fresh_owner",
        "errors_are_structured_and_content_free", "authenticated_import_uses_fresh_destination",
        "abandoned_handles_release_native_capacity")}}
PYTHON_CASES = {
    "test_" + suffix for suffix in (
        "authenticated_recovery_uses_new_destination_and_preserves_source",
        "conversation_pending_then_observed_send_remains_explicit", "create_never_reopens_or_overwrites_existing_paths",
        "create_review_commit_lock_reopen_read_and_unicode_offsets", "foreign_plan_and_conflicting_writer_reject",
        "heads_are_lossless_python_ints_and_strict_bytes", "installed_native_extension_not_source_wrapper",
        "malformed_feed_transport_terminates_response_and_utf16_is_exact",
        "only_literal_true_approves_and_rejection_preserves_pending_plan", "owner_gc_revokes_surviving_children",
        "progressive_failure_has_no_failed_call_text", "read_only_review_recall_batches_and_policy_preview",
        "reports_counts_and_policy_delegate_to_core", "restart_reconciliation_returns_only_receipt",
        "streams_revoke_on_lock_and_saved_head_change", "tenant_and_thread_boundaries_are_controlled",
        "unsupported_profile_and_malformed_transport_fail_before_creation", "wrong_password_identity_and_full_head_reject")}
INTEROP_CASES = {"node_produces_confirmed_encrypted_state", "python_reopens_and_preserves_node_state",
                 "node_reopens_python_state_and_scoped_issuance", "source_stability"}


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def identity():
    names = subprocess.check_output(["git", "ls-files", "--cached", "--others", "--exclude-standard", "-z"], cwd=ROOT).decode().split("\0")
    prefixes = ("crates/core/", "integrations/fogvault-", "fixtures/", ".cargo/")
    exact = {"Cargo.toml", "Cargo.lock", "rust-toolchain.toml", ".nvmrc",
             "scripts/prepare-fogvault-languages.py", "scripts/verify-fogvault-languages.py",
             "scripts/test-fogvault-languages.py"}
    return {name: sha(ROOT / name) for name in sorted(set(names))
            if name and (name in exact or name.startswith(prefixes)) and (ROOT / name).is_file()}


def object_report(value, schema):
    if not isinstance(value, dict) or value.get("schema") != schema or value.get("pass") is not True or value.get("executed") is not True:
        raise ValueError("Missing successful executed report: " + schema)


def cases(value, required, flag="status"):
    if not isinstance(value, list) or len(value) != len(required) or any(not isinstance(c, dict) for c in value):
        raise ValueError("Malformed or missing named evidence")
    if {c.get("id") for c in value} != required or any(c.get("executed") is not True or c.get(flag) != ("pass" if flag == "status" else True) for c in value):
        raise ValueError("Missing, duplicate, skipped or failed named case")


def artifact(item):
    if not isinstance(item, dict) or not isinstance(item.get("path"), str) or sha(item["path"]) != item.get("sha256"):
        raise ValueError("Artifact identity mismatch")


def source_matches(recorded, prefixes, exact=()):
    current = {k: v for k, v in identity().items() if k in exact or k.startswith(prefixes)}
    if not isinstance(recorded, dict) or recorded != current:
        raise ValueError("Producer source inputs differ from current source bytes")


def validate_producers(node, python):
    object_report(node, "datafog-fogvault-node-installed/v1")
    cases(node.get("cases"), NODE_CASES)
    if node.get("installedTypesVerified") is not True:
        raise ValueError("Installed Node typecheck missing")
    source_matches(node.get("source", {}).get("inputs"),
                   ("integrations/fogvault-node/", "integrations/fogvault-native/",
                    "integrations/fogvault-migration/", "crates/core/", "fixtures/fogvault/"))
    for key in ("tarball", "sourceAddon", "installedAddon"):
        artifact(node.get("artifacts", {}).get(key))
    if node["artifacts"]["sourceAddon"]["sha256"] != node["artifacts"]["installedAddon"]["sha256"]:
        raise ValueError("Installed Node addon differs")
    installed_node = Path(node["artifacts"]["consumerPath"]) / "node_modules/@datafog/core-fogvault-node"
    for name in ("index.mjs", "index.d.ts", "native-loader.mjs", "LICENSE", "README.md", "package.json"):
        if sha(installed_node / name) != sha(ROOT / "integrations/fogvault-node" / name):
            raise ValueError("Installed Node runtime/types differ from current source")
    with tarfile.open(node["artifacts"]["tarball"]["path"], "r:gz") as archive:
        names = set()
        for member in archive.getmembers():
            if member.isdir():
                continue
            parts = member.name.split("/")
            if not member.isfile() or parts[0] != "package" or any(p in ("", ".", "..") for p in parts) or "\\" in member.name or member.name.casefold() in names:
                raise ValueError("Unsafe/colliding candidate tar member")
            names.add(member.name.casefold())
            content = archive.extractfile(member).read()
            if sha(installed_node.joinpath(*parts[1:])) != hashlib.sha256(content).hexdigest():
                raise ValueError("Installed Node package differs from actual tarball")
    object_report(python, "datafog-fogvault-python-proof/v1")
    source_matches(python.get("input_identity"),
                   ("integrations/fogvault-python/", "integrations/fogvault-native/",
                    "integrations/fogvault-migration/", "crates/core/", "fixtures/workspace/", ".cargo/"),
                   ("Cargo.toml", "Cargo.lock", "rust-toolchain.toml"))
    artifact(python.get("candidate"))
    result = python.get("result")
    if not isinstance(result, dict) or result.get("pass") is not True or result.get("executed") is not True:
        raise ValueError("Installed Python execution missing")
    cases(result.get("cases"), PYTHON_CASES)
    assets = python.get("installed_assets")
    if not isinstance(assets, dict) or not assets or not any(n.endswith((".so", ".pyd")) for n in assets):
        raise ValueError("Installed Python native assets missing")
    package = Path(python["run_directory"]) / "venv" / ("Lib/site-packages" if os.name == "nt" else "lib")
    for name, digest in assets.items():
        matches = list(package.glob("python*/site-packages/" + name)) if os.name != "nt" else [package / name]
        if len(matches) != 1 or sha(matches[0]) != digest:
            raise ValueError("Installed Python bytes changed")
    # Pip regenerates RECORD to include its own installer files; all original
    # code, types, binary and distribution metadata must match the wheel.
    with ZipFile(python["candidate"]["path"]) as archive:
        names = archive.namelist()
        if len(names) != len({n.casefold() for n in names}):
            raise ValueError("Colliding wheel members")
        for name in names:
            if name.endswith("/"):
                continue
            if name.startswith("/") or "\\" in name or any(p in ("", ".", "..") for p in name.split("/")):
                raise ValueError("Unsafe wheel member")
            if ".dist-info/" in name and name.endswith("/RECORD"):
                continue
            matches = list(package.glob("python*/site-packages/" + name)) if os.name != "nt" else [package / name]
            if len(matches) != 1 or sha(matches[0]) != hashlib.sha256(archive.read(name)).hexdigest():
                raise ValueError("Installed Python package differs from actual wheel")


def validate(node, python, interop, node_path, python_path):
    validate_producers(node, python)
    object_report(interop, "datafog-fogvault-native-interop/v1")
    cases(interop.get("checks"), INTEROP_CASES, "pass")
    if interop.get("node_proof_sha256") != sha(node_path) or interop.get("python_proof_sha256") != sha(python_path):
        raise ValueError("Interop report refers to different installed candidates")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output-dir", type=Path)
    parser.add_argument("--json", action="store_true", help="Emit machine-readable result")
    args = parser.parse_args()
    BASE.mkdir(parents=True, exist_ok=True)
    if args.output_dir:
        output = args.output_dir.resolve()
        output.mkdir(parents=True, exist_ok=False)
    else:
        output = Path(tempfile.mkdtemp(prefix="check-", dir=BASE))
    before = identity()
    commands, failure, versions = [], None, {}
    def run(name, argv):
        log = output / (name + ".log")
        with log.open("w", encoding="utf-8") as stream:
            environment = dict(os.environ, CARGO_TARGET_DIR=str(BASE),
                               CARGO_PROFILE_DEV_DEBUG="0", CARGO_PROFILE_TEST_DEBUG="0", CARGO_INCREMENTAL="0")
            result = subprocess.run(argv, cwd=ROOT, env=environment, stdout=stream, stderr=subprocess.STDOUT)
        commands.append({"id": name, "command": list(map(str, argv)), "exit_code": result.returncode,
                         "log": str(log), "sha256": sha(log)})
        if result.returncode:
            raise ValueError(name + " failed; see " + str(log))
    try:
        spec = importlib.util.spec_from_file_location("language_setup", ROOT / "scripts/prepare-fogvault-languages.py")
        setup = importlib.util.module_from_spec(spec); spec.loader.exec_module(setup)
        versions, node, _ = setup.tools()
        rust_pin, _, _ = setup.pins()
        run("evidence-negative-tests", [sys.executable, str(ROOT / "scripts/test-fogvault-languages.py")])
        for package in ("node", "python"):
            manifest = str(ROOT / "integrations" / ("fogvault-" + package) / "Cargo.toml")
            run(package + "-fmt", ["cargo", "+" + rust_pin, "fmt", "--manifest-path", manifest, "--check"])
            run(package + "-clippy", ["cargo", "+" + rust_pin, "clippy", "--locked", "--manifest-path", manifest, "--all-targets", "--all-features", "--", "-D", "warnings"])
        run("node", [node, str(ROOT / "integrations/fogvault-node/test/run-installed.mjs"), "--output", str(output / "node.json")])
        run("python", [sys.executable, str(ROOT / "integrations/fogvault-python/tests/run-installed.py"), "--output", str(output / "python.json")])
        run("interop", [sys.executable, str(ROOT / "integrations/fogvault-interop/run.py"), "--node-report", str(output / "node.json"), "--python-report", str(output / "python.json"), "--output", str(output / "interop.json")])
        validate(*(json.loads((output / name).read_text(encoding="utf-8")) for name in ("node.json", "python.json", "interop.json")), output / "node.json", output / "python.json")
        if identity() != before:
            raise ValueError("Current source bytes changed during qualification")
    except (OSError, ValueError, TypeError, KeyError, AttributeError, subprocess.SubprocessError) as error:
        failure = str(error)
    report = {"schema": "datafog-fogvault-languages/v1", "pass": failure is None, "executed": True,
              "recorded_at_utc": datetime.now(timezone.utc).isoformat(), "input_identity": before,
              "tools": versions, "commands": commands, "failure": failure,
              "reports": {name: {"path": str(output / (name + ".json")), "sha256": sha(output / (name + ".json"))}
                          for name in ("node", "python", "interop") if (output / (name + ".json")).is_file()}}
    (output / "report.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({"pass": report["pass"], "report": str(output / "report.json")}))
    return 0 if report["pass"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
