"""Fixed uninstrumented before/after comparison of exact installed Node artifacts."""
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tarfile
import zipfile

ROOT = Path(__file__).resolve().parent.parent
SOURCE = ROOT / "candidate-source"
HEAD = "60a009832271f4240f44895a30cbce6eb3585c2d"
BASELINE_HEAD = "8513a0f1b3216abc75173dcd94a7c137e24cbeca"
OUT = ROOT / "output-comparison"
OUT.mkdir(exist_ok=True)
NODE = shutil.which("node")
NPM = [NODE, str(Path(NODE).parent / "node_modules/npm/bin/npm-cli.js")]
ENV = {key: value for key, value in os.environ.items() if not any(marker in key.upper() for marker in ("TOKEN", "SECRET", "PASSWORD", "CREDENTIAL", "API_KEY"))}
records = []
summary = {"schema": "datafog-node-output-before-after/v1", "pass": False,
           "baseline_source": BASELINE_HEAD, "candidate_source": HEAD, "commands": records}


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def save():
    (OUT / "comparison.json").write_text(json.dumps(summary, indent=2) + "\n", encoding="utf-8")


def run(label, command, cwd, timeout=600):
    result = subprocess.run(command, cwd=cwd, env=ENV, capture_output=True, text=True, encoding="utf-8", errors="replace", timeout=timeout)
    (OUT / (label + ".stdout")).write_text(result.stdout, encoding="utf-8")
    (OUT / (label + ".stderr")).write_text(result.stderr, encoding="utf-8")
    row = {"label": label, "command": [str(x) for x in command], "cwd": str(cwd), "returncode": result.returncode}
    records.append(row); save(); print(json.dumps(row), flush=True)
    return result


def api(endpoint):
    return json.loads(subprocess.check_output(["gh", "api", endpoint]))


def package(directory, name):
    values = []
    for path in directory.rglob("*.tgz"):
        with tarfile.open(path) as archive:
            value = json.load(archive.extractfile("package/package.json"))
        if value["name"] == name and value["version"] == "1.8.0": values.append(path.resolve())
    assert len(values) == 1, (name, values)
    return values[0]


def javascript_hashes(archive):
    with tarfile.open(archive) as packed:
        return {name: hashlib.sha256(packed.extractfile("package/" + name).read().decode("utf-8").replace("\r\n", "\n").encode("utf-8")).hexdigest()
                for name in ["index.js", "index.d.ts", "native.js"]}


try:
    assert subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=SOURCE, text=True, encoding="utf-8").strip() == HEAD
    artifact = api("repos/DataFog/datafog-core/actions/artifacts/14426252339")
    assert artifact["workflow_run"]["head_sha"] == BASELINE_HEAD and artifact["workflow_run"]["id"] == 44297537516
    archive = OUT / "baseline.zip"
    with archive.open("wb") as stream:
        subprocess.run(["gh", "api", "repos/DataFog/datafog-core/actions/artifacts/14426252339/zip"], stdout=stream, check=True)
    assert "sha256:" + sha(archive) == artifact["digest"]
    (OUT / "baseline-artifact.json").write_text(json.dumps(artifact, indent=2), encoding="utf-8")
    baseline = OUT / "baseline"
    with zipfile.ZipFile(archive) as packed:
        for name in packed.namelist(): assert not Path(name).is_absolute() and ".." not in Path(name).parts
        packed.extractall(baseline)
    old = package(baseline, "@datafog/node")
    old_platform = package(baseline, "@datafog/node-win32-x64-msvc")
    fixed = OUT / "fixed"; fixed.mkdir()
    result = run("pack-fixed", [*NPM, "pack", "--ignore-scripts", "--json", "--pack-destination", str(fixed)], SOURCE / "bindings/node")
    assert result.returncode == 0
    new = package(fixed, "@datafog/node")
    old_js, new_js = javascript_hashes(old), javascript_hashes(new)
    assert old_js == new_js, (old_js, new_js)
    summary["javascript_hashes_normalized_newlines"] = old_js
    summary["archives"] = {str(path): sha(path) for path in [old, old_platform, new]}
    summary["node_versions"] = json.loads(subprocess.check_output([NODE, "-p", "JSON.stringify(process.versions)"], text=True, encoding="utf-8"))
    results = {}
    full_hash = None
    shared_fixture_hashes = None
    for label, source, archive, platform_archive in [("baseline", SOURCE, old, old_platform), ("fixed", SOURCE, new, None)]:
        consumer = OUT / (label + "-consumer"); consumer.mkdir()
        (consumer / "package.json").write_text('{"private":true,"type":"module"}', encoding="utf-8")
        result = run(label + "-install", [*NPM, "install", "--ignore-scripts", "--omit=optional", str(archive), *([str(platform_archive)] if platform_archive else [])], consumer)
        assert result.returncode == 0
        marker = OUT / (label + "-full-consumer-path.txt")
        original = source / "scripts/test-node-package.mjs"
        code = original.read_text(encoding="utf-8")
        cleanup = "  rmSync(temporaryDirectory, { recursive: true, force: true });"
        assert code.count(cleanup) == 1
        retained = source / "scripts/diagnostic-retain-node-package.mjs"
        retained.write_text(code.replace(cleanup, "  writeFileSync(" + json.dumps(str(marker.resolve())) + ", temporaryDirectory);"), encoding="utf-8")
        installed = run(label + "-full-installed", [NODE, str(retained), "--prebuilt", "--tarball", str(archive), *(["--platform-tarball", str(platform_archive)] if platform_archive else [])], source)
        full_consumer = Path(marker.read_text(encoding="utf-8"))
        full_test = full_consumer / "test-installed.mjs"
        fixture_hashes = {path.name: hashlib.sha256(path.read_text(encoding="utf-8").replace("\r\n", "\n").encode("utf-8")).hexdigest() for path in sorted(full_consumer.glob("*.mjs"))}
        if shared_fixture_hashes is None: shared_fixture_hashes = fixture_hashes
        assert fixture_hashes == shared_fixture_hashes, "Before/after conformance helpers differ"
        summary["shared_conformance_helpers"] = {"source": HEAD, "sha256_normalized_newlines": fixture_hashes}
        current = hashlib.sha256(full_test.read_text(encoding="utf-8").replace("\r\n", "\n").encode("utf-8")).hexdigest()
        if full_hash is None: full_hash = current
        assert current == full_hash, "Before/after fixture child source differs"
        full_results = []
        for index in range(100):
            result = run(f"{label}-full-{index:03}", [NODE, str(full_test), str(source / "fixtures")], full_consumer, timeout=60)
            full_results.append({"index": index, "exit_code": result.returncode,
                                 "completed": "Installed @datafog/node package matches fixtures and transform contracts." in result.stdout,
                                 "uv_assertion": "UV_HANDLE_CLOSING" in result.stderr})
        results[label] = {"full": full_results,
                          "full_installed_exit_code": installed.returncode,
                          "full_consumer_sha256_normalized_newlines": current}
        summary["results"] = results; save()
    bad_baseline = any(row.get("uv_assertion") for row in results["baseline"]["full"])
    fixed_pass = results["fixed"]["full_installed_exit_code"] == 0 and all(row["exit_code"] == 0 and row["completed"] for row in results["fixed"]["full"])
    summary["bad_baseline_observed"] = bad_baseline
    summary["pass"] = bad_baseline and fixed_pass
    assert summary["pass"], "Before/after proof failed; inspect recorded outcomes"
finally:
    save()
    print(json.dumps({"pass": summary["pass"], "report": str(OUT / "comparison.json")}), flush=True)
