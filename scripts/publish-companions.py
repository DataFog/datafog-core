"""Publish only exact-head qualified companion packages; default is nonpublication."""
import argparse
import hashlib
import importlib.util
import io
import json
import os
from pathlib import Path, PurePosixPath
import shutil
import subprocess
import tarfile
import tempfile
import time
import tomllib
import urllib.error
import urllib.request
import zipfile

ROOT = Path(__file__).resolve().parents[1]
REPO = "DataFog/datafog-core"
GROUPS = ("access", "fog-rust", "fog-browser", "core-rust-integrations",
          "core-browser-integrations", "core-node-integration", "core-python-integration")


def module(name):
    spec = importlib.util.spec_from_file_location(name, ROOT / "scripts" / (name + ".py"))
    result = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(result)
    return result


def run(argv, cwd=ROOT, capture=False, env=None):
    return subprocess.check_output(argv, cwd=cwd, env=env, text=True) if capture else subprocess.run(argv, cwd=cwd, env=env, check=True)


def digest(data):
    return hashlib.sha256(data).hexdigest()


def read(url):
    request = urllib.request.Request(url, headers={"User-Agent": "DataFog-qualified-release/1.0"})
    try:
        with urllib.request.urlopen(request, timeout=60) as response:
            return response.read()
    except urllib.error.HTTPError as error:
        if error.code == 404:
            return None
        raise


def json_read(url):
    data = read(url)
    return None if data is None else json.loads(data)


def safe_name(name):
    path = PurePosixPath(name)
    if path.is_absolute() or "\\" in name or any(part in ("", ".", "..") for part in name.rstrip("/").split("/")):
        raise ValueError("Unsafe archive member")
    return path


def unpack_zip(data, destination):
    with zipfile.ZipFile(io.BytesIO(data)) as archive:
        seen = set()
        for entry in archive.infolist():
            safe_name(entry.filename)
            if entry.filename.casefold() in seen or (entry.external_attr >> 16) & 0o170000 == 0o120000:
                raise ValueError("Duplicate or symlinked artifact entry")
            seen.add(entry.filename.casefold())
        archive.extractall(destination)


def download_artifact(run_id, name, destination):
    pages = module("check-fogvault-release").api(REPO, f"actions/runs/{run_id}/artifacts?per_page=100")
    matches = [item for page in pages for item in page["artifacts"] if item["name"] == name]
    if len(matches) != 1 or matches[0].get("expired") or matches[0].get("workflow_run", {}).get("head_sha") != os.environ["GITHUB_SHA"]:
        raise ValueError("Missing, duplicate, expired or stale qualified artifact: " + name)
    artifact = matches[0]
    data = subprocess.check_output(["gh", "api", f"repos/{REPO}/actions/artifacts/{artifact['id']}/zip"])
    if artifact.get("digest") != "sha256:" + digest(data):
        raise ValueError("GitHub artifact archive digest mismatch")
    destination.mkdir(parents=True, exist_ok=False)
    unpack_zip(data, destination)
    return destination


def report(directory, schema):
    matches = []
    for path in directory.rglob("*.json"):
        try:
            value = json.loads(path.read_text())
        except (UnicodeDecodeError, json.JSONDecodeError):
            continue
        if isinstance(value, dict) and value.get("schema") == schema:
            matches.append(value)
    if len(matches) != 1 or matches[0].get("pass") is not True:
        raise ValueError("Missing, duplicate or failed qualification receipt: " + schema)
    return matches[0]


def qualified_file(directory, receipt, suffix):
    matches = [path for path in directory.rglob("*" + suffix)
               if path.is_file() and digest(path.read_bytes()) == receipt.get("sha256")]
    if not matches:
        raise ValueError("Qualified archive bytes missing or changed")
    if "bytes" in receipt and any(path.stat().st_size != receipt["bytes"] for path in matches):
        raise ValueError("Qualified archive length differs")
    return matches[0]


def tar_files(path):
    result = {}
    with tarfile.open(path, "r:gz") as archive:
        for entry in archive:
            safe_name(entry.name)
            if entry.isdir():
                continue
            if not entry.isfile() or entry.name.casefold() in {key.casefold() for key in result}:
                raise ValueError("Unsafe or duplicate package member")
            result[entry.name] = archive.extractfile(entry).read()
    return result


def npm_package(path):
    files = tar_files(path)
    package = json.loads(files["package/package.json"])
    return package["name"], package["version"]


def publish_npm(path, expected_name, version, execute):
    if npm_package(path) != (expected_name, version):
        raise ValueError("Wrong qualified npm package identity")
    url = "https://registry.npmjs.org/" + expected_name.replace("/", "%2f") + "/" + version
    existing = json_read(url)
    if existing:
        remote = read(existing["dist"]["tarball"])
        if remote != path.read_bytes():
            raise ValueError("Existing npm version has different bytes: " + expected_name)
    else:
        env = None
        if expected_name == "@datafog/fogvault-browser":
            # A Core workflow cannot attest to being the FogVault source repo.
            # First publication uses the authorized owner token, not Core OIDC.
            if execute and not os.environ.get("NODE_AUTH_TOKEN"):
                raise ValueError("FogVault npm bootstrap requires owner token access")
            env = {key: value for key, value in os.environ.items()
                   if key not in ("ACTIONS_ID_TOKEN_REQUEST_TOKEN", "ACTIONS_ID_TOKEN_REQUEST_URL")}
            env["NPM_CONFIG_PROVENANCE"] = "false"
        run(["npm", "publish", str(path.resolve()), "--ignore-scripts", "--access", "public", "--tag", "latest"] + ([] if execute else ["--dry-run"]), env=env)
        if execute:
            # npm can acknowledge an upload before registry processing finishes.
            # Poll for up to ten minutes without submitting the archive again.
            for _ in range(120):
                existing = json_read(url)
                if existing:
                    break
                time.sleep(5)
            if not existing or read(existing["dist"]["tarball"]) != path.read_bytes():
                raise ValueError("Published npm archive is missing or differs")
    return {"name": expected_name, "version": version, "sha256": digest(path.read_bytes()), "published": bool(existing)}


def verify_python(output):
    receipt = json.loads((output / "receipt.json").read_text())
    if receipt["revision"] != os.environ["GITHUB_SHA"] or receipt["group"] != "core-python-integration":
        raise ValueError("Python publication receipt identity differs")
    expected = {entry["name"]: entry["sha256"] for entry in receipt["packages"]}
    for _ in range(120):
        public = json_read("https://pypi.org/pypi/datafog-fogvault/1.0.0/json")
        observed = {} if not public else {entry["filename"]: entry["digests"]["sha256"] for entry in public["urls"]}
        if any(name not in expected or sha256 != expected[name] for name, sha256 in observed.items()):
            raise ValueError("Published Python file set or hashes differ")
        if observed == expected:
            return
        # Only missing expected files can be delayed by registry replication.
        time.sleep(5)
    raise ValueError("Published Python file set is still incomplete after registry processing")


def stage_crate_sources(qualified, destination, prefix):
    """Keep qualified sources and locks; Cargo regenerates its reserved receipts."""
    for filename, data in tar_files(qualified).items():
        if not filename.startswith(prefix):
            raise ValueError("Wrong qualified archive root")
        relative_name = filename.removeprefix(prefix)
        if relative_name in (".cargo_vcs_info.json", "Cargo.toml.orig"):
            continue
        target = destination / relative_name
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(data)


def compare_crate(qualified, actual, source, revision, approved, candidates):
    expected, observed = tar_files(qualified), tar_files(actual)
    prefix = actual.name.removesuffix(".crate") + "/"
    if any(not name.startswith(prefix) for name in observed) or any(not name.startswith(prefix) for name in expected):
        raise ValueError("Wrong crate archive root")
    generated = {prefix + name for name in ("Cargo.lock", "Cargo.toml.orig", ".cargo_vcs_info.json")}
    if {key: value for key, value in expected.items() if key not in generated} != {key: value for key, value in observed.items() if key not in generated}:
        raise ValueError("Published crate source, notices or normalized manifest differs from qualification")
    if observed.get(prefix + "Cargo.toml.orig") != (source / "Cargo.toml").read_bytes():
        raise ValueError("Cargo original manifest differs from exact reviewed source")
    vcs = observed.get(prefix + ".cargo_vcs_info.json")
    if vcs:
        identity = json.loads(vcs)["git"]
        if identity.get("sha1") != revision or identity.get("dirty", False):
            raise ValueError("Cargo source identity differs")
    lock = observed.get(prefix + "Cargo.lock")
    if lock is None:
        raise ValueError("Published crate omits dependency lock evidence")
    with tempfile.NamedTemporaryFile(suffix=".lock") as handle:
        handle.write(lock)
        handle.flush()
        module("qualify-native-registry").admit_registry_graph(Path(handle.name), approved, candidates)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--group", choices=GROUPS, required=True)
    parser.add_argument("--execute", action="store_true")
    parser.add_argument("--verify-python", action="store_true")
    args = parser.parse_args()
    if os.environ.get("GITHUB_REPOSITORY", "").casefold() != REPO.casefold() or os.environ.get("GITHUB_REF") != "refs/heads/main":
        raise ValueError("Companion publishing requires the owning repository main branch")
    revision = os.environ["GITHUB_SHA"]
    if run(["git", "rev-parse", "HEAD"], capture=True).strip() != revision or run(["git", "status", "--porcelain", "--untracked-files=no"], capture=True).strip():
        raise ValueError("Publication checkout must be clean and exact")
    if json.loads(run(["gh", "api", f"repos/{REPO}/commits/main"], capture=True))["sha"] != revision:
        raise ValueError("Publication requires current main, not a superseded candidate")
    if args.group == "access":
        # This read-only identity probe can run while qualification is pending.
        print(json.dumps({"npm_token_configured": bool(os.environ.get("NODE_AUTH_TOKEN"))}))
        run(["npm", "whoami"])
        return
    gates = module("check-fogvault-release").check(REPO, revision, "1.0.0")
    module("check-model-integration").check(REPO, revision)
    output = ROOT / "target/companion-publication"
    if args.verify_python:
        if args.group != "core-python-integration":
            raise ValueError("Python verification requires its publication group")
        verify_python(output)
        return
    output.mkdir(parents=True, exist_ok=False)
    (output / "qualification.json").write_text(json.dumps(gates, indent=2) + "\n")
    runs = {Path(proof["workflow"]).name: proof["run_id"] for proof in gates["proofs"]}
    results = []
    if args.group in ("fog-browser", "core-browser-integrations"):
        source = download_artifact(runs["fogvault-integration.yml"], "core-fogvault-joint-chromium", output / "joint")
        proof = report(source, "datafog-fogvault-legacy-browser-proof/v1")
        if proof.get("executed") is not True or proof.get("preparation", {}).get("fogvault_commit") != tomllib.loads((ROOT / "integrations/fogvault-native/Cargo.toml").read_text())["dependencies"]["fogvault-native"]["rev"]:
            raise ValueError("Browser source qualification differs")
        packages = [("vault", "@datafog/fogvault-browser", "0.1.0")] if args.group == "fog-browser" else [("adapter", "@datafog/core-fogvault-browser", "1.0.0"), ("importer", "@datafog/core-fogvault-legacy-browser", "1.0.0")]
        for key, name, version in packages:
            results.append(publish_npm(qualified_file(source, proof["candidates"][key], ".tgz"), name, version, args.execute))
            (output / "partial-receipt.json").write_text(json.dumps(results, indent=2) + "\n")
    elif args.group == "core-node-integration":
        source = download_artifact(runs["fogvault-native-languages.yml"], "core-fogvault-node-unified", output / "node")
        proof = report(source, "datafog-fogvault-node-assembly/v1")
        if proof.get("revision") != revision or set(proof.get("targets", [])) != {"linux-x64-gnu", "darwin-arm64", "win32-x64-msvc"}:
            raise ValueError("Unified native package lacks exact qualified platforms")
        results.append(publish_npm(qualified_file(source, proof["tarball"], ".tgz"), "@datafog/core-fogvault-node", "1.0.0", args.execute))
    elif args.group == "core-python-integration":
        dist = output / "dist"
        dist.mkdir()
        public = json_read("https://pypi.org/pypi/datafog-fogvault/1.0.0/json")
        existing = {} if not public else {entry["filename"]: entry["digests"]["sha256"] for entry in public["urls"]}
        for platform in ("ubuntu-24.04", "macos-15", "windows-2025"):
            source = download_artifact(runs["fogvault-native-languages.yml"], "core-fogvault-native-languages-" + platform, output / platform)
            proof = report(source, "datafog-fogvault-python-proof/v1")
            if proof.get("executed") is not True or proof.get("git_commit") != revision or proof.get("git_status"):
                raise ValueError("Python package lacks clean exact source qualification")
            wheel = qualified_file(source, proof["candidate"], ".whl")
            name = Path(proof["candidate"]["path"].replace("\\", "/")).name
            if not name.startswith("datafog_fogvault-1.0.0-") or (dist / name).exists():
                raise ValueError("Wrong or duplicate Python distribution")
            if name in existing:
                if existing[name] != digest(wheel.read_bytes()):
                    raise ValueError("Existing Python wheel differs from qualified bytes")
            else:
                shutil.copyfile(wheel, dist / name)
            results.append({"name": name, "sha256": digest(wheel.read_bytes())})
        if set(existing) - {row["name"] for row in results}:
            raise ValueError("Unexpected existing Python release files")
        if os.environ.get("GITHUB_OUTPUT"):
            with open(os.environ["GITHUB_OUTPUT"], "a") as handle:
                handle.write("python_upload=" + str(bool(list(dist.glob("*.whl")))).lower() + "\n")
        # Upload is a separate OIDC action, using only this verified directory.
    else:
        publish_rust(args, output, runs, revision, results)
    (output / "receipt.json").write_text(json.dumps({"revision": revision, "group": args.group, "execute": args.execute, "packages": results}, indent=2) + "\n")


def publish_rust(args, output, runs, revision, results):
    source = download_artifact(runs["fogvault-integration.yml"], "core-fogvault-joint-chromium", output / "joint")
    proof = report(source, "datafog-native-registry-qualification/v1")
    declared = tomllib.loads((ROOT / "integrations/fogvault-native/Cargo.toml").read_text())["dependencies"]["fogvault-native"]
    fog = output / "fog-source"
    run(["git", "clone", "--no-checkout", "https://github.com/DataFog/fogvault.git", str(fog)])
    run(["git", "checkout", "--detach", declared["rev"]], cwd=fog)
    if proof.get("fog_commit") != declared["rev"] or proof.get("fog_dirty") or proof.get("explicit_dirty_candidate"):
        raise ValueError("Fog source differs from qualified clean pin")
    # Use the qualified registry manifests and locks, rather than resolving
    # differently from the original path/Git source manifests during upload.
    temporary = tempfile.TemporaryDirectory(prefix="fogvault-reviewed-publication-")
    qualifier = module("qualify-native-registry")
    identities = {name: (fog, relative, version) for name, (relative, version) in qualifier.FOG.items()}
    identities.update({name: (ROOT, relative, "1.0.0") for name, relative in qualifier.CORE.items()})
    approved = {}
    for lock in (ROOT / "Cargo.lock", ROOT / "integrations/fogvault-native/Cargo.lock", fog / "crates/fogvault-native/Cargo.lock", fog / "experiments/vault-prototype/Cargo.lock"):
        for item in tomllib.loads(lock.read_text())["package"]:
            if item.get("source") == qualifier.REGISTRY:
                key = item["name"], item["version"]
                if key in approved and approved[key]["checksum"] != item["checksum"]:
                    raise ValueError("Conflicting approved dependency checksum")
                approved[key] = item
    names = ["fogvault-i18n-config", "fogvault-i18n-embed-impl", "fogvault-i18n-embed", "fogvault-i18n-embed-fl", "fogvault-age", "fogvault", "fogvault-backup", "fogvault-native"] if args.group == "fog-rust" else ["datafog-fogvault-migration", "datafog-fogvault-native"]
    candidates = {}
    public = {}
    for dependency, (_, _, dep_version) in identities.items():
        observed = json_read(f"https://crates.io/api/v1/crates/{dependency}/{dep_version}")
        if observed:
            public[dependency] = observed
            candidates[dependency, dep_version] = observed["version"]["checksum"]
    for name in names:
        root, relative, version = identities[name]
        package = Path(temporary.name) / name
        package.mkdir()
        url = f"https://crates.io/api/v1/crates/{name}/{version}"
        entries = [row for row in proof["artifacts"] if Path(row["path"]).name == f"{name}-{version}.crate"]
        if len(entries) != 1:
            raise ValueError("Qualified crate receipt missing or ambiguous")
        qualified = qualified_file(source, entries[0], ".crate")
        prefix = f"{name}-{version}/"
        stage_crate_sources(qualified, package, prefix)
        lock = tomllib.loads((package / "Cargo.lock").read_text())
        for row in lock["package"]:
            if row.get("source") == qualifier.REGISTRY and (row["name"], row["version"]) in candidates:
                row["checksum"] = candidates[row["name"], row["version"]]
        (package / "Cargo.lock").write_text(qualifier.lock_text(lock["package"]))
        existing = public.get(name)
        actual = output / f"{name}-{version}.crate"
        if existing:
            data = read(f"https://static.crates.io/crates/{name}/{name}-{version}.crate")
            if digest(data) != existing["version"]["checksum"]:
                raise ValueError("Registry crate checksum differs")
            actual.write_bytes(data)
        else:
            env = dict(os.environ, CARGO_TARGET_DIR=str(output / (name + "-build")))
            command = ["cargo", "+1.94.1", "publish", "--locked", "--manifest-path", str(package / "Cargo.toml")]
            run(command + ["--dry-run"], env=env)
            shutil.copyfile(Path(env["CARGO_TARGET_DIR"]) / "package/tmp-crate" / actual.name, actual)
        compare_crate(qualified, actual, package, declared["rev"] if root == fog else revision, approved, candidates)
        if args.execute and not existing:
            run(command, env=env)
            for _ in range(12):
                existing = json_read(url)
                if existing:
                    break
                time.sleep(5)
            if not existing or existing["version"]["checksum"] != digest(actual.read_bytes()):
                raise ValueError("Published crate differs from admitted archive")
            candidates[name, version] = existing["version"]["checksum"]
        original = tar_files(qualified)
        published = tar_files(actual)
        results.append({"name": name, "version": version, "sha256": digest(actual.read_bytes()), "published": bool(existing),
                        "qualified_archive_sha256": digest(qualified.read_bytes()),
                        "qualified_original_manifest_sha256": digest(original[prefix + "Cargo.toml.orig"]),
                        "published_original_manifest_sha256": digest(published[prefix + "Cargo.toml.orig"]),
                        "fog_revision": declared["rev"]})
        (output / "partial-receipt.json").write_text(json.dumps(results, indent=2) + "\n")


if __name__ == "__main__":
    main()
