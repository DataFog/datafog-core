#!/usr/bin/env python3
"""Verify actual Core/FogVault registry candidates locally; never publish."""
import argparse
from concurrent.futures import ThreadPoolExecutor
import hashlib
import json
import os
import re
from pathlib import Path
import shutil
import subprocess
import sys
import tarfile
import tempfile
import threading
import tomllib
import urllib.request

ROOT = Path(__file__).resolve().parents[1]
REGISTRY = "registry+https://github.com/rust-lang/crates.io-index"
CORE = {"datafog-core": "crates/core", "datafog-fogvault-migration": "integrations/fogvault-migration",
        "datafog-fogvault-native": "integrations/fogvault-native"}
FOG = {"fogvault": ("crates/fogvault", "0.1.0"), "fogvault-native": ("crates/fogvault-native", "0.1.0"),
       "fogvault-backup": ("crates/fogvault-backup", "0.1.0"),
       **{"fogvault-" + name: ("crates/fogvault-backup/vendor/" + name + "-" + version, version)
          for name, version in (("age", "0.11.5"), ("i18n-config", "0.4.8"), ("i18n-embed", "0.16.0"),
                                ("i18n-embed-impl", "0.8.4"), ("i18n-embed-fl", "0.10.1"))}}
REQUIRED_CONSUMER_TESTS = {
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
}


def admit_consumer_test_output(text):
    observed = re.findall(r"^test ([A-Za-z0-9_:]+) \.\.\. (ok|ignored|FAILED)$", text, re.MULTILINE)
    if len(observed) != 10 or {name for name, _ in observed} != REQUIRED_CONSUMER_TESTS or any(status != "ok" for _, status in observed):
        raise ValueError("registry consumer omitted, duplicated, ignored or failed a required native test")
    if "test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out;" not in text:
        raise ValueError("registry consumer lacks the complete ten-test success summary")
    return sorted(REQUIRED_CONSUMER_TESTS)


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def identity(path):
    return {"path": str(path), "bytes": path.stat().st_size, "sha256": digest(path)}


def index_path(name):
    if len(name) < 3:
        return str(len(name)) + "/" + name
    return "3/" + name[0] + "/" + name if len(name) == 3 else name[:2] + "/" + name[2:4] + "/" + name


def read_index_entry(cache, name, version, expected_sha, lock):
    """One index per name is shared by every requested version in this run."""
    index = cache / (name + ".index")
    with lock:
        if not index.exists():
            with urllib.request.urlopen("https://index.crates.io/" + index_path(name), timeout=60) as response:
                index.write_bytes(response.read())
        entry = next((json.loads(line) for line in index.read_bytes().splitlines() if json.loads(line)["vers"] == version), None)
    if entry is None or entry["cksum"] != expected_sha:
        raise ValueError("official index/checksum mismatch: " + name + "-" + version + ".crate")
    return entry


def lock_text(packages):
    lines = ["version = 4", ""]
    for package in packages:
        lines.append("[[package]]")
        for key, value in package.items():
            lines.append(key + " = " + json.dumps(value))
        lines.append("")
    return "\n".join(lines)


def execute(fog, output, allow_dirty):
    output.mkdir(parents=True, exist_ok=False)
    members = {name: (ROOT, relative, "1.0.0") for name, relative in CORE.items()}
    members.update({name: (fog, relative, version) for name, (relative, version) in FOG.items()})
    commands = []
    def run(name, argv, cwd=ROOT, env=None):
        result = subprocess.run(argv, cwd=cwd, env=env, capture_output=True, text=True)
        log = output / (name + ".log")
        log.write_text(result.stdout + result.stderr)
        commands.append({"name": name, "command": argv, "exit_code": result.returncode, "log": identity(log)})
        if result.returncode:
            raise ValueError(name + " failed: " + str(log))
        return result.stdout
    fog_commit = run("fog-commit", ["git", "rev-parse", "HEAD"], cwd=fog).strip()
    fog_dirty = bool(run("fog-status", ["git", "status", "--porcelain"], cwd=fog).strip())
    declared = tomllib.loads((ROOT / CORE["datafog-fogvault-native"] / "Cargo.toml").read_text())["dependencies"]["fogvault-native"]
    if not allow_dirty and (fog_dirty or fog_commit != declared.get("rev")):
        raise ValueError("Fog source must match the declared clean revision; dirty candidates require explicit --allow-dirty-fogvault")
    run("qualifier-test-parser", [sys.executable, "scripts/test-native-registry-qualification.py"])
    capture = json.loads(run("fog-reviewed-advisory-capture",
                             [sys.executable, "scripts/verify-dependency-audit.py", "--verify-capture"], cwd=fog))
    if capture.get("pass") is not True or capture.get("audits_reexecuted") is not False or capture.get("captured_audits_validated") != 14:
        raise ValueError("Fog reviewed-origin audit evidence is incomplete")
    capture_path = output / "fog-reviewed-advisory-capture.json"
    capture_path.write_text(json.dumps(capture, indent=2) + "\n")
    run("fog-upstream-provenance", [sys.executable, "crates/fogvault-backup/upstream/verify.py"], cwd=fog)
    source_files = {ROOT / "Cargo.lock", ROOT / "Cargo.toml", ROOT / "README.md", ROOT / "LICENSE", Path(__file__).resolve(),
                    ROOT / "scripts/test-native-registry-qualification.py",
                    ROOT / "fixtures/fogvault/record-binding-v1.json", fog / "experiments/vault-prototype/Cargo.lock"}
    for relative, expected in capture["source_identity"].items():
        source = fog / relative
        if not source.resolve().is_relative_to(fog) or digest(source) != expected:
            raise ValueError("Fog advisory receipt source identity differs")
        source_files.add(source)
    def snapshot_sources():
        paths = set(source_files)
        def visit(directory, source_root):
            if directory.is_symlink() or not directory.resolve().is_relative_to(source_root):
                raise ValueError("symlinked or escaping package directory")
            for source in directory.iterdir():
                if source.is_symlink():
                    raise ValueError("symlinked package input: " + str(source))
                if source.name in ("target", "node_modules", ".git"):
                    continue
                if source.is_dir(): visit(source, source_root)
                elif source.is_file(): paths.add(source)
                else: raise ValueError("nonregular package input")
        for source_root, relative, _ in members.values():
            visit(source_root / relative, source_root)
        for source in paths:
            if source.is_symlink() or not source.is_file() or not any(source.resolve().is_relative_to(base) for base in (ROOT, fog)):
                raise ValueError("nonregular or escaping source input")
        return {str(source): digest(source) for source in sorted(paths)}
    source_before = snapshot_sources()
    scratch = output / "workspace"
    paths = []
    for name, (source_root, relative, version) in members.items():
        target = scratch / ("core/" if name in CORE else "fog/") / relative
        # Copy reviewed sources, including package notices; never copy build trees.
        shutil.copytree(source_root / relative, target, dirs_exist_ok=True,
                        ignore=shutil.ignore_patterns("target", "node_modules", ".git"))
        manifest = target / "Cargo.toml"
        data = tomllib.loads(manifest.read_text())
        if (data["package"]["name"], data["package"]["version"]) != (name, version):
            raise ValueError("wrong candidate identity: " + name)
        text = manifest.read_text().replace("[workspace]\n", "")
        if name == "datafog-fogvault-native":
            if declared.get("version") not in ("0.1.0", "=0.1.0"):
                raise ValueError("native Fog dependency lacks its public registry version")
            dependency = {key: value for key, value in declared.items() if key not in ("git", "rev", "branch", "tag")}
            dependency["path"] = "../../../fog/crates/fogvault-native"
            overlay = "fogvault-native = { " + ", ".join(json.dumps(key) + " = " + json.dumps(value) for key, value in dependency.items()) + " }"
            lines = text.splitlines()
            lines = [overlay
                     if line.startswith("fogvault-native = ") else line for line in lines]
            text = "\n".join(lines) + "\n"
        manifest.write_text(text)
        paths.append(str(target.relative_to(scratch)))
    (scratch / "core").mkdir(exist_ok=True)
    shutil.copy2(ROOT / "README.md", scratch / "core/README.md")
    (scratch / "Cargo.toml").write_text('[workspace]\nresolver="2"\nmembers=' + json.dumps(paths) + "\n")
    # Approved identity union permits existing compatible versions to unify, but
    # never permits an unnoticed new registry release to enter the candidate.
    locks = [ROOT / "Cargo.lock", ROOT / "integrations/fogvault-native/Cargo.lock",
             fog / "crates/fogvault-native/Cargo.lock", fog / "experiments/vault-prototype/Cargo.lock"]
    approved, lock_inputs = {}, []
    for lock in locks:
        lock_inputs.append(identity(lock))
        for row in tomllib.loads(lock.read_text())["package"]:
            if row.get("source") == REGISTRY:
                key = row["name"], row["version"]
                if key in approved and approved[key]["checksum"] != row["checksum"]:
                    raise ValueError("conflicting approved registry checksum")
                approved[key] = row
    # Native's exact zeroize pin must win over Core's compatible older locked
    # version. Seed whole per-name groups from native graphs first; all possible
    # admitted identities remain constrained by the full approved union above.
    seed, seeded_names = [], set()
    for lock in (locks[1], locks[2], locks[0], locks[3]):
        groups = {}
        for row in tomllib.loads(lock.read_text())["package"]:
            if row.get("source") == REGISTRY:
                groups.setdefault(row["name"], []).append(row)
        for name, rows in groups.items():
            if name not in seeded_names:
                seed.extend(rows)
                seeded_names.add(name)
    (scratch / "Cargo.lock").write_text(lock_text(seed))
    run("lock-approved-union", ["cargo", "+1.94.1", "metadata", "--offline", "--format-version", "1", "--manifest-path", str(scratch / "Cargo.toml")])
    def check_graph(lock, candidate_hashes):
        graph = []
        for row in tomllib.loads(lock.read_text())["package"]:
            if row.get("source") != REGISTRY:
                if row.get("source"):
                    raise ValueError("archive graph retains non-registry dependency")
                continue
            key = row["name"], row["version"]
            expected = candidate_hashes.get(key) or approved.get(key, {}).get("checksum")
            if expected is None or row["checksum"] != expected:
                raise ValueError("unapproved registry identity/checksum: " + str(key))
            graph.append({"name": key[0], "version": key[1], "sha256": expected})
        return graph
    check_graph(scratch / "Cargo.lock", {})
    run("batch-package", ["cargo", "+1.94.1", "package", "--workspace", "--locked", "--registry", "crates-io", "--manifest-path", str(scratch / "Cargo.toml")])
    packages = scratch / "target/package"
    candidate_hashes, archives, graphs = {}, [], {}
    for name, (_, _, version) in members.items():
        archive = packages / (name + "-" + version + ".crate")
        candidate_hashes[name, version] = digest(archive)
        archives.append(identity(archive))
        with tarfile.open(archive) as bundle:
            prefix = name + "-" + version + "/"
            files = {item.name.removeprefix(prefix): bundle.extractfile(item).read() for item in bundle.getmembers() if item.isfile()}
        normalized = tomllib.loads(files["Cargo.toml"].decode())
        def registry_only(value):
            if isinstance(value, dict):
                if any(key in value for key in ("path", "git", "patch")):
                    raise ValueError("archive manifest retains local/Git/patch resolution")
                for child in value.values(): registry_only(child)
            elif isinstance(value, list):
                for child in value: registry_only(child)
        for key in ("dependencies", "dev-dependencies", "build-dependencies", "target", "patch"):
            registry_only(normalized.get(key, {}))
        if not any(Path(file).name.startswith("LICENSE") for file in files) or "README.md" not in files:
            raise ValueError("archive missing readme/license: " + name)
        source_root, relative, _ = members[name]
        for source in (source_root / relative / "src").rglob("*.rs"):
            if files.get(str(source.relative_to(source_root / relative))) != source.read_bytes():
                raise ValueError("archive Rust source differs from reviewed input: " + str(source))
    for name, (_, _, version) in members.items():
        graphs[name] = check_graph(packages / (name + "-" + version) / "Cargo.lock", candidate_hashes)
    registry = output / "registry"
    shutil.copytree(packages / "tmp-registry", registry)
    needed = {(row["name"], row["version"]): row for rows in graphs.values() for row in rows}
    # Consumer tests also retain the exact published beta.3 compatibility floor.
    for key, row in approved.items():
        needed.setdefault(key, {"name": key[0], "version": key[1], "sha256": row["checksum"]})
    cache = ROOT / "target/native-registry-inputs"
    cache.mkdir(parents=True, exist_ok=True)
    cargo_home = Path(os.environ.get("CARGO_HOME", str(Path.home() / ".cargo")))
    index_locks = {name: threading.Lock() for name, _ in needed}
    def seed(item):
        (name, version), row = item
        filename = name + "-" + version + ".crate"
        if (name, version) in candidate_hashes:
            return None
        locations = [cache / filename, *cargo_home.glob("registry/cache/*/" + filename)]
        data = next((p.read_bytes() for p in locations if p.is_file() and digest(p) == row["sha256"]), None)
        if data is None:
            with urllib.request.urlopen("https://static.crates.io/crates/" + name + "/" + filename, timeout=60) as response: data = response.read()
        if hashlib.sha256(data).hexdigest() != row["sha256"]: raise ValueError("official archive checksum mismatch")
        (cache / filename).write_bytes(data)
        (registry / filename).write_bytes(data)
        entry = read_index_entry(cache, name, version, row["sha256"], index_locks[name])
        return name, entry
    with ThreadPoolExecutor(max_workers=8) as pool: entries = list(pool.map(seed, sorted(needed.items())))
    grouped = {}
    for item in entries:
        if item: grouped.setdefault(item[0], []).append(item[1])
    for name, rows in grouped.items():
        index = registry / "index" / index_path(name)
        index.parent.mkdir(parents=True, exist_ok=True)
        combined = {row["vers"]: row for row in
                    ([json.loads(line) for line in index.read_text().splitlines()] if index.exists() else [])}
        for row in rows:
            if row["vers"] in combined and combined[row["vers"]]["cksum"] != row["cksum"]:
                raise ValueError("candidate/official registry version checksum conflict")
            combined[row["vers"]] = row
        index.write_text("\n".join(json.dumps(row) for row in combined.values()) + "\n")
    consumer = output / "consumer"
    (consumer / ".cargo").mkdir(parents=True)
    (consumer / "tests").mkdir()
    (consumer / "src").mkdir()
    (consumer / "src/lib.rs").write_text("// Synthetic registry-only consumer; behavior is exercised by integration tests.\n")
    (consumer / "fixtures/fogvault").mkdir(parents=True)
    shutil.copy2(ROOT / "fixtures/fogvault/record-binding-v1.json", consumer / "fixtures/fogvault/record-binding-v1.json")
    tests = (ROOT / "integrations/fogvault-native/tests/native.rs").read_text().replace("../../../fixtures/", "../fixtures/")
    (consumer / "tests/native.rs").write_text(tests)
    (consumer / ".cargo/config.toml").write_text('[source.crates-io]\nreplace-with="candidate"\n[source.candidate]\nlocal-registry=' + json.dumps(str(registry)) + "\n")
    (consumer / "Cargo.toml").write_text('[package]\nname="core-native-registry-consumer"\nversion="0.0.0"\nedition="2024"\n[workspace]\n[dependencies]\ndatafog-core="=1.0.0"\ndatafog-fogvault-native="=1.0.0"\ndatafog-fogvault-migration="=1.0.0"\npublished-beta3={package="datafog-core",version="=0.5.0-beta.3"}\nserde_json="1"\ngetrandom="=0.4.3"\ntempfile="3"\n[profile.test]\nopt-level=2\n')
    env = dict(os.environ, CARGO_HOME=str(output / "consumer-cargo-home"))
    tests_passed = admit_consumer_test_output(run("registry-consumer-tests", ["cargo", "+1.94.1", "test", "--offline", "--manifest-path", str(consumer / "Cargo.toml")], cwd=consumer, env=env))
    graphs["consumer"] = check_graph(consumer / "Cargo.lock", candidate_hashes)
    metadata = json.loads(run("registry-consumer-metadata", ["cargo", "+1.94.1", "metadata", "--locked", "--offline", "--format-version", "1"], cwd=consumer, env=env))
    observed = {row["name"]: row for row in metadata["packages"] if row["name"] in CORE}
    if set(observed) != set(CORE) or any(row["source"] != REGISTRY for row in observed.values()):
        raise ValueError("consumer bypassed candidate registry identities")
    if source_before != snapshot_sources():
        raise ValueError("reviewed source inputs changed during qualification")
    return {"schema": "datafog-native-registry-qualification/v1", "pass": True, "published": False,
            "fog_commit": fog_commit, "fog_dirty": fog_dirty, "explicit_dirty_candidate": allow_dirty,
            "source_identity": source_before, "approved_lock_inputs": lock_inputs,
            "commands": commands, "artifacts": archives, "actual_registry_graphs": graphs,
            "required_consumer_tests_passed": tests_passed,
            "fog_advisory_capture": {**identity(capture_path), "audits_reexecuted": False, "captured_audits_validated": 14},
            "beta3_regression": "actual registry-only consumer runs the existing native adapter suite including bidirectional beta.3 bundles",
            "limitations": ["Local candidate registry qualification; no registry publication, independent security review, or production control isolation approval.",
                            "Existing approved registry identity/checksum union permits compatible version unification; original source locks are preserved."]}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--fogvault-source", type=Path, default=ROOT / "target/fogvault-integration/dependencies/fogvault")
    parser.add_argument("--allow-dirty-fogvault", action="store_true", help="Explicit local candidate only; source hashes and dirty status remain in report")
    args = parser.parse_args()
    output = Path(tempfile.mkdtemp(prefix="native-registry-", dir=ROOT / "target")) / "run"
    try:
        report = execute(args.fogvault_source.resolve(), output, args.allow_dirty_fogvault)
    except Exception as error:
        report = {"schema": "datafog-native-registry-qualification/v1", "pass": False, "published": False,
                  "error": str(error), "run": str(output)}
    destination = output.parent / "report.json"
    destination.write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps({"pass": report["pass"], "report": str(destination), "error": report.get("error")}))
    raise SystemExit(0 if report["pass"] else 1)
