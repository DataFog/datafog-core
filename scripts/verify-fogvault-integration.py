"""Verify optional maintained Core/FogVault consumers; setup is a separate command."""
import argparse
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import tempfile
import tarfile
import time
from fogvault_inputs import core_build_inputs

ROOT = Path(__file__).resolve().parents[1]
BASE = ROOT / "target/fogvault-integration"
VAULT_REV = "6c6176a3a9df320447ba42b924b26de7ac4d693a"
BROWSER_CASES = {
    "create_approve_save_activate_lock_reopen_restore",
    "literal_approval_and_foreign_plan_rejection_before_save",
    "complete_head_cas_conflict_closes_stale_owner",
    "authenticated_record_and_namespace_substitution_rejected",
    "saved_core_commit_id_mismatch_rejected",
    "lost_save_reply_reconciles_without_handoff_or_old_approval",
    "pending_conversation_survives_reopen_and_requires_observed_send",
    "late_confirmed_save_after_lock_never_activates",
    "authenticated_bundle_recovery_preserves_aliases_and_refuses_occupied_destination",
    "whole_bundle_growth_samples_and_payload_admission",
    "occupied_creation_rejects_exact_duplicate_request_without_overwrite",
    "async_open_expectations_and_creation_request_ids_are_snapshotted",
    "authenticated_import_rejects_unrelated_occupied_vault_without_mutation",
}
LEGACY_CASES = {
    "legacy_fixed_kdf_profiles_match_reference_vectors",
    "legacy_v1_v2_password_recovery_actual_kdf_reopen",
    "legacy_unused_slot_tampering_and_profile_versions_reject",
    "legacy_wrong_credentials_namespace_and_authenticated_head_reject",
    "legacy_literal_approval_opaque_session_and_occupied_destination",
    "legacy_worker_cancel_and_closed_session_never_publish",
    "legacy_archive_admission_and_source_preservation",
    "legacy_confirmed_save_cancel_cleanup_failure_preserves_request",
}
LEGACY_COMBINATIONS = {"v1_password", "v1_recovery", "v2_password", "v2_recovery"}
LEGACY_PACKAGES = {"core": "@datafog/wasm", "vault": "@datafog/fogvault-browser",
                   "adapter": "@datafog/core-fogvault-browser", "importer": "@datafog/core-fogvault-legacy-browser"}
RUST_CASES = {
    "migration": {
        "v1_source_stays_exact_while_recovery_upgrades_through_core",
        "v2_policy_and_aliases_keep_their_saved_meaning",
        "v3_recovery_preserves_history_and_pending_or_confirmed_conversation_authority",
        "namespace_revision_and_commit_id_are_all_required",
        "malformed_and_unsupported_sources_never_become_candidates",
        "exhausted_or_reused_recovery_authority_fails_without_changing_source",
    },
    "native": {
        "lost_ack_after_actual_publication_requires_reconciliation_and_fresh_owner",
        "shared_binding_vectors_are_exact_and_u64_lossless",
        "create_approve_save_lock_reopen_restore_and_lifetime",
        "foreign_changes_reject_before_storage_and_conflicting_writer_closes_owner",
        "authenticated_record_metadata_cannot_rebind_workspace_identity_or_head",
        "published_beta3_and_candidate_bundles_interoperate_through_registered_vault",
        "conversation_pending_and_observed_send_survive_saved_reopen",
        "recovery_candidate_publishes_new_head_without_overwriting_destination",
        "create_rejects_identical_request_replay_into_existing_workspace",
        "native_facade_delegates_preserve_core_reports_history_and_stream_lifetimes",
        "native_payload_admission_failure_preserves_saved_state_without_handoff",
    },
}


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def source_identity():
    names = subprocess.check_output(
        ["git", "ls-files", "--cached", "--others", "--exclude-standard", "-z"], cwd=ROOT
    ).decode().split("\0")
    return {name: digest(ROOT / name) for name in sorted(set(names))
            if name and (ROOT / name).is_file()}


def validate_browser(report, expected_artifacts=None):
    if not isinstance(report, dict):
        raise ValueError("Browser report must be an object")
    if (report.get("schema") != "datafog-fogvault-browser-proof/v1"
            or report.get("pass") is not True or report.get("executed") is not True
            or report.get("typecheck_executed") is not True or report.get("failure")):
        raise ValueError("Installed browser candidate did not pass all required checks")
    result = report.get("result", {})
    if not isinstance(result, dict):
        raise ValueError("Browser result must be an object")
    cases = result.get("cases", [])
    if not isinstance(cases, list) or any(not isinstance(case, dict) for case in cases):
        raise ValueError("Browser cases must be an object list")
    if (result.get("pass") is not True or result.get("executed") is not True
            or result.get("failure") or not result.get("userAgent")
            or set(report.get("required_cases", [])) != BROWSER_CASES
            or len(cases) != len(BROWSER_CASES)
            or {c.get("id") for c in cases} != BROWSER_CASES
            or any(c.get("status") != "pass" or c.get("executed") is not True for c in cases)):
        raise ValueError("Installed browser candidate omitted, skipped or failed a required case")
    if not report.get("installed_assets"):
        raise ValueError("Installed browser candidate omitted installed artifact identities")
    if expected_artifacts is not None:
        candidates = report.get("candidates")
        if not isinstance(candidates, dict) or set(candidates) != {"core", "vault", "adapter"}:
            raise ValueError("Browser proof omitted candidate package identities")
        for name, candidate in candidates.items():
            path = Path(candidate["path"])
            if digest(path) != candidate["sha256"] or path.stat().st_size != candidate["bytes"]:
                raise ValueError("Browser candidate artifact changed after execution")
            if name in {"core", "vault"}:
                expected = expected_artifacts[name + "_tarball"]
                if candidate["sha256"] != expected["sha256"] or candidate["bytes"] != expected["bytes"]:
                    raise ValueError("Browser proof used a different prepared package")


def validate_legacy(report, expected_artifacts=None, expected_engine=None):
    if (not isinstance(report, dict) or report.get("schema") != "datafog-fogvault-legacy-browser-proof/v1"
            or report.get("pass") is not True or report.get("executed") is not True
            or report.get("typecheck_executed") is not True or report.get("failure")):
        raise ValueError("Legacy installed report did not pass")
    if report.get("engine") not in {"chromium", "firefox", "msedge"} or (expected_engine is not None and report["engine"] != expected_engine):
        raise ValueError("Legacy browser engine does not match selected qualification scope")
    result = report.get("result")
    if (not isinstance(result, dict) or result.get("pass") is not True or result.get("executed") is not True
            or result.get("failure") or not result.get("userAgent")):
        raise ValueError("Legacy browser execution missing")
    cases = result.get("cases")
    if (not isinstance(cases, list) or len(cases) != len(LEGACY_CASES)
            or any(not isinstance(c, dict) for c in cases)
            or {c.get("id") for c in cases} != LEGACY_CASES
            or set(report.get("required_cases", [])) != LEGACY_CASES
            or any(c.get("status") != "pass" or c.get("executed") is not True for c in cases)):
        raise ValueError("Legacy required named cases missing, duplicated, failed or skipped")
    combinations = next(c for c in cases if c["id"] == "legacy_v1_v2_password_recovery_actual_kdf_reopen").get("combinations")
    if (not isinstance(combinations, list) or len(combinations) != len(LEGACY_COMBINATIONS)
            or any(not isinstance(c, dict) for c in combinations)
            or {c.get("id") for c in combinations} != LEGACY_COMBINATIONS
            or set(report.get("required_combinations", [])) != LEGACY_COMBINATIONS
            or any(c.get("status") != "pass" or c.get("executed") is not True for c in combinations)):
        raise ValueError("Legacy credential/envelope matrix is incomplete")
    installed = report.get("installed_assets")
    candidates = report.get("candidates")
    if (not isinstance(installed, dict) or set(installed) != set(LEGACY_PACKAGES.values())
            or not isinstance(candidates, dict) or set(candidates) != set(LEGACY_PACKAGES)
            or any(not isinstance(v, dict) or not v for v in installed.values())):
        raise ValueError("Legacy proof must identify all four actual installed candidates")
    if expected_artifacts is None:
        return
    preparation = report.get("preparation")
    if (not isinstance(preparation, dict) or preparation.get("core_build_inputs") != core_build_inputs(ROOT)
            or preparation.get("fogvault_commit") != VAULT_REV):
        raise ValueError("Legacy proof used stale Core/FogVault source inputs")
    inputs = report.get("source_identity")
    required_inputs = {"integrations/fogvault-legacy-browser/" + f for f in
                       ("index.mjs", "backup-read.mjs", "vault-read.mjs", "kdf-worker-source.mjs", "index.d.ts", "package-lock.json")}
    if not isinstance(inputs, dict) or not required_inputs <= inputs.keys():
        raise ValueError("Legacy source identities missing")
    for name, recorded in inputs.items():
        source = (ROOT / name).resolve()
        if not source.is_relative_to(ROOT) or digest(source) != recorded:
            raise ValueError("Legacy checked source bytes changed")
    consumer = Path(report["run_directory"])
    for key, package in LEGACY_PACKAGES.items():
        candidate = candidates[key]
        archive_path = Path(candidate["path"])
        if digest(archive_path) != candidate["sha256"] or archive_path.stat().st_size != candidate["bytes"]:
            raise ValueError("Legacy candidate archive changed")
        if key in ("core", "vault"):
            expected = expected_artifacts[key + "_tarball"]
            if candidate["sha256"] != expected["sha256"] or candidate["bytes"] != expected["bytes"]:
                raise ValueError("Legacy candidate differs from prepared Core/FogVault")
        seen, member_names = set(), set()
        with tarfile.open(archive_path, "r:gz") as archive:
            for member in archive.getmembers():
                if member.isdir():
                    continue
                parts = member.name.split("/")
                if (not member.isfile() or parts[0] != "package" or len(parts) < 2
                        or any(p in ("", ".", "..") for p in parts) or "\\" in member.name
                        or member.name.casefold() in seen):
                    raise ValueError("Unsafe/colliding legacy candidate archive member")
                seen.add(member.name.casefold())
                relative = "/".join(parts[1:]); member_names.add(relative)
                actual = consumer / "node_modules" / package / relative
                content = archive.extractfile(member).read()
                expected = hashlib.sha256(content).hexdigest()
                if (actual.is_symlink() or not actual.is_file() or digest(actual) != expected
                        or installed[package].get(relative) != expected):
                    raise ValueError("Legacy installed bytes differ from actual candidate archive")
        if set(installed[package]) != member_names:
            raise ValueError("Legacy installed member identities omitted or invented")
        if key in ("adapter", "importer"):
            directory = ROOT / "integrations" / ("fogvault-browser" if key == "adapter" else "fogvault-legacy-browser")
            for relative in member_names - {"kdf-worker.mjs"}:
                if digest(directory / relative) != installed[package][relative]:
                    raise ValueError("Legacy packed adapter/importer differs from current source")


def validate_rust(log, suite):
    summaries = re.findall(r"test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;", log)
    if (not summaries or sum(int(p) for p, _, _ in summaries) == 0
            or any(int(f) or int(i) for _, f, i in summaries)):
        raise ValueError("Rust suite did not execute passing, unskipped tests")
    executed = {name.rsplit("::", 1)[-1]
                for name in re.findall(r"^test (\S+) \.\.\. ok$", log, re.MULTILINE)}
    if not RUST_CASES[suite] <= executed:
        raise ValueError(f"Rust {suite} suite omitted required cases: {RUST_CASES[suite] - executed}")


def validate_prepared(prepared, root=ROOT):
    if not isinstance(prepared, dict) or prepared.get("pass") is not True or prepared.get("fogvault_commit") != VAULT_REV:
        raise ValueError("Wrong pinned FogVault preparation")
    if prepared.get("core_build_inputs") != core_build_inputs(root):
        raise ValueError("Prepared Core package does not match current build/package sources")
    artifacts = prepared.get("artifacts")
    if not isinstance(artifacts, dict) or set(artifacts) != {"core_tarball", "vault_tarball"}:
        raise ValueError("Preparation omitted required artifact identities")
    for artifact in artifacts.values():
        path = Path(artifact["path"])
        if digest(path) != artifact["sha256"] or path.stat().st_size != artifact["bytes"]:
            raise ValueError("Prepared candidate changed; prepare it again")
    return artifacts


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--native-only", action="store_true")
    parser.add_argument("--browser-only", action="store_true")
    parser.add_argument("--engine", choices=["chromium", "firefox", "msedge"], default="chromium")
    parser.add_argument("--json", action="store_true")
    args = parser.parse_args()
    if args.native_only and args.browser_only:
        parser.error("Select at most one platform-only scope")
    BASE.mkdir(parents=True, exist_ok=True)
    run_dir = Path(tempfile.mkdtemp(prefix="check-", dir=BASE))
    before = source_identity()
    checks = []

    def command(name, argv, validate=None):
        started = time.monotonic()
        log_path = run_dir / (name + ".log")
        error = None
        try:
            with log_path.open("w", encoding="utf-8") as log:
                child = subprocess.run(argv, cwd=ROOT, stdout=log, stderr=subprocess.STDOUT)
            if child.returncode:
                raise ValueError(f"Command exit {child.returncode}")
            if validate:
                validate(log_path.read_text(encoding="utf-8", errors="replace"))
        except (OSError, ValueError, KeyError, TypeError) as failure:
            error = str(failure)
        checks.append({"id": name, "pass": error is None, "executed": True,
                       "command": argv, "error": error, "log": str(log_path),
                       "log_sha256": digest(log_path) if log_path.exists() else None,
                       "duration_seconds": time.monotonic() - started})

    command("verifier-self-tests", [sys.executable, "scripts/test-fogvault-verifier.py"])

    if not args.browser_only:
        for name, manifest, toolchain in [
            ("migration", "integrations/fogvault-migration/Cargo.toml", "+1.88.0"),
            ("native", "integrations/fogvault-native/Cargo.toml", "+1.94.1"),
        ]:
            command(name + "-format", ["cargo", toolchain, "fmt", "--manifest-path", manifest, "--check"])
            command(name + "-clippy", ["cargo", toolchain, "clippy", "--locked", "--manifest-path", manifest,
                                      "--all-targets", "--all-features", "--", "-D", "warnings"])
            command(name, ["cargo", toolchain, "test", "--locked", "--manifest-path", manifest,
                           "--all-features"], lambda text, suite=name: validate_rust(text, suite))
    if not args.native_only:
        command("browser-unit", ["npm", "test", "--prefix", "integrations/fogvault-browser"],
                lambda text: require_node_tests(text))
        try:
            prepared = json.loads((BASE / "prepared.json").read_text(encoding="utf-8"))
            artifacts = validate_prepared(prepared)
            browser_output = run_dir / "browser.json"
            command("installed-browser", ["node", "integrations/fogvault-browser/test/run-installed.mjs",
                    "--core-tarball", artifacts["core_tarball"]["path"],
                    "--vault-tarball", artifacts["vault_tarball"]["path"],
                    "--engine", args.engine, "--output", str(browser_output)],
                    lambda _: validate_browser(json.loads(browser_output.read_text(encoding="utf-8")), artifacts))
            legacy_output = run_dir / "legacy.json"
            command("installed-legacy-reference-import", [sys.executable, "scripts/verify-fogvault-legacy.py",
                    "--prepared", str(BASE / "prepared.json"), "--engine", args.engine,
                    "--output", str(legacy_output)],
                    lambda _: validate_legacy(json.loads(legacy_output.read_text(encoding="utf-8")), artifacts, args.engine))
        except (OSError, ValueError, KeyError, TypeError) as failure:
            checks.append({"id": "browser-preparation", "pass": False, "executed": False,
                           "error": str(failure) + "; run cargo dfv build --fogvault"})
    after = source_identity()
    checks.append({"id": "source-stability", "pass": before == after, "executed": True})
    report = {"schema": "datafog-fogvault-integration/v1", "pass": all(c["pass"] for c in checks),
              "executed": True, "scope": "native" if args.native_only else "browser" if args.browser_only else "all",
              "recorded_at_utc": datetime.now(timezone.utc).isoformat(),
              "git_commit": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip(),
              "git_status": subprocess.check_output(["git", "status", "--porcelain"], cwd=ROOT, text=True),
              "input_identity": before, "checks": checks, "run_directory": str(run_dir)}
    path = run_dir / "report.json"
    path.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({"pass": report["pass"], "report": str(path)}))
    return 0 if report["pass"] else 1


def require_node_tests(text):
    match = re.search(r"# tests (\d+)", text)
    if (not match or int(match.group(1)) == 0 or not re.search(r"# fail 0\b", text)
            or re.search(r"# (?:skipped|cancelled) [1-9]", text)):
        raise ValueError("Browser unit suite omitted, skipped or failed tests")


if __name__ == "__main__":
    sys.exit(main())
