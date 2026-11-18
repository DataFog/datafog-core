"""Finite diagnostic matrix; reports every child outcome without hiding failures."""

import base64
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tarfile
import urllib.request
import zipfile

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "shutdown-diagnosis"
OUT.mkdir(exist_ok=True)
NODE = shutil.which("node")
NPM = [NODE, str(Path(NODE).parent / "node_modules/npm/bin/npm-cli.js")]
RUN_ID = 44297537516
ARTIFACT_ID = 14426252339
HEAD = "8513a0f1b3216abc75173dcd94a7c137e24cbeca"
RESULTS = []


def api(endpoint):
    return json.loads(subprocess.check_output(["gh", "api", endpoint]))


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def run(label, command, cwd):
    try:
        result = subprocess.run(command, cwd=cwd, capture_output=True, text=True, encoding="utf-8", errors="replace", timeout=300)
    except subprocess.TimeoutExpired as error:
        result = subprocess.CompletedProcess(command, 124, (error.stdout or b"").decode("utf-8", errors="replace"), (error.stderr or b"").decode("utf-8", errors="replace") + "\n[DEBUG-df-node-shutdown] timeout")
    (OUT / (label + ".stdout")).write_text(result.stdout, encoding="utf-8")
    (OUT / (label + ".stderr")).write_text(result.stderr, encoding="utf-8")
    record = {"label": label, "returncode": result.returncode, "command": command}
    RESULTS.append(record)
    (OUT / "results.json").write_text(json.dumps(RESULTS, indent=2), encoding="utf-8")
    print(json.dumps(record), flush=True)
    return result


run_metadata = api(f"repos/DataFog/datafog-core/actions/runs/{RUN_ID}")
artifact = api(f"repos/DataFog/datafog-core/actions/artifacts/{ARTIFACT_ID}")
assert run_metadata["head_sha"] == HEAD
assert artifact["workflow_run"]["head_sha"] == HEAD
assert artifact["workflow_run"]["id"] == RUN_ID
archive = OUT / "candidate.zip"
with archive.open("wb") as stream:
    subprocess.run(["gh", "api", f"repos/DataFog/datafog-core/actions/artifacts/{ARTIFACT_ID}/zip"], stdout=stream, check=True)
assert "sha256:" + digest(archive) == artifact["digest"]
(OUT / "candidate-metadata.json").write_text(json.dumps({"run": run_metadata, "artifact": artifact}, indent=2), encoding="utf-8")
candidate = OUT / "candidate"
candidate.mkdir(exist_ok=True)
with zipfile.ZipFile(archive) as zipped:
    for name in zipped.namelist():
        assert not Path(name).is_absolute() and ".." not in Path(name).parts
    zipped.extractall(candidate)


def package(directory, name, version):
    matches = []
    for path in directory.rglob("*.tgz"):
        with tarfile.open(path) as packed:
            value = json.load(packed.extractfile("package/package.json"))
        if value["name"] == name and value["version"] == version:
            matches.append(path.resolve())
    assert len(matches) == 1, (name, version, matches)
    return matches[0]


baseline = OUT / "baseline"
baseline.mkdir(exist_ok=True)
for name in ("@datafog/node", "@datafog/node-win32-x64-msvc"):
    with urllib.request.urlopen("https://registry.npmjs.org/" + name + "/1.7.0") as response:
        metadata = json.load(response)
    path = baseline / (name.replace("@", "").replace("/", "-") + "-1.7.0.tgz")
    with urllib.request.urlopen(metadata["dist"]["tarball"]) as response:
        path.write_bytes(response.read())
    algorithm, expected = metadata["dist"]["integrity"].split("-", 1)
    assert base64.b64encode(hashlib.new(algorithm, path.read_bytes()).digest()).decode() == expected
    (baseline / (path.name + ".metadata.json")).write_text(json.dumps(metadata, indent=2), encoding="utf-8")

probe = r'''
import assert from "node:assert/strict";
import {Worker} from "node:worker_threads";
import {writeSync} from "node:fs";
const mode = process.argv[2];
if (process.env.DIAGNOSE_ASYNC_HOOKS === "1") {
  const {createHook} = await import("node:async_hooks");
  const live = new Map();
  createHook({init(id,type){if(type.includes("CustomGC")||type.includes("deferred"))live.set(id,type)},destroy(id){live.delete(id)}}).enable();
  for(const event of ["beforeExit","exit"]) process.on(event,()=>writeSync(2,`[DEBUG-df-node-shutdown] ${event} ${JSON.stringify([...live])}\n`));
}
const api = mode.startsWith("node-") ? null : await import("@datafog/node");
function sync() {
  assert(api.scan("jane@example.com").some(x=>x.entityType==="EMAIL"));
  const ws=api.Workspace.create(new Uint8Array(16),new Uint8Array(16));
  ws.close();
}
if (mode === "sync") sync();
if (mode === "rejected-model-open") {
  const config={bundle:"missing-engine-bundle",manifest_sha256:"a".repeat(64),limits:{max_input_bytes:1000,max_response_bytes:10000,max_findings:10,timeout_ms:1000,cleanup_timeout_ms:1000}};
  for (const [engine,path,reason] of [["private-engine","/engine","invalid_value"],[true,"/engine","invalid_type"],[null,"/engine","invalid_type"],["gliner2","/gliner2","missing_field"]]) {
    await assert.rejects(api.ModelSession.open({...config,engine}),e=>e.code==="invalid_configuration"&&e.path===path&&e.reason===reason);
  }
}
if (mode.startsWith("worker-") || mode === "node-worker-terminate") {
  const loadAddon=mode.startsWith("worker-");
  const terminate=mode.endsWith("terminate");
  const source=`const {parentPort}=require('node:worker_threads'); (async()=>{ ${loadAddon?"const api=await import('@datafog/node'); api.scan('jane@example.com'); const ws=api.Workspace.create(new Uint8Array(16),new Uint8Array(16)); ws.close();":""} ${terminate?"setInterval(()=>{},1000);":""} parentPort.postMessage('ready'); })().catch(error=>{throw error});`;
  const worker=new Worker(source,{eval:true});
  const exit=new Promise((resolve,reject)=>{worker.once('error',reject);worker.once('exit',resolve)});
  await new Promise((resolve,reject)=>{worker.once('message',resolve);worker.once('error',reject)});
  if(terminate) await worker.terminate();
  else assert.equal(await exit,0);
}
console.log(`[DEBUG-df-node-shutdown] completed ${mode}`);
'''

versions = subprocess.check_output([NODE, "-p", "JSON.stringify(process.versions)"], text=True)
(OUT / "node-versions.json").write_text(versions, encoding="utf-8")
for version, directory, source in [("1.8.0", candidate, ROOT), ("1.7.0", baseline, ROOT / "baseline-source")]:
    root_tarball = package(directory, "@datafog/node", version)
    platform_tarball = package(directory, "@datafog/node-win32-x64-msvc", version)
    (OUT / (version + "-inputs.json")).write_text(json.dumps({str(p): digest(p) for p in [root_tarball, platform_tarball]}, indent=2), encoding="utf-8")
    consumer = OUT / ("consumer-" + version)
    consumer.mkdir(exist_ok=True)
    (consumer / "package.json").write_text('{"private":true,"type":"module"}', encoding="utf-8")
    installed = run(version + "-install", [*NPM, "install", "--ignore-scripts", "--omit=optional", str(root_tarball), str(platform_tarball)], consumer)
    assert installed.returncode == 0
    (consumer / "probe.mjs").write_text(probe, encoding="utf-8")
    for mode in filter(None, os.environ.get("DIAGNOSE_MODES", "import-only,sync,rejected-model-open,worker-natural,worker-terminate").split(",")):
        for index in range(50):
            run(f"{version}-{mode}-{index:03}", [NODE, "probe.mjs", mode], consumer)
    for index in range(0 if os.environ.get("DIAGNOSE_SKIP_FULL") == "1" else 3):
        run(f"{version}-full-suite-{index:03}", [NODE, str(source / "scripts/test-node-package.mjs"), "--prebuilt", "--tarball", str(root_tarball), "--platform-tarball", str(platform_tarball)], source)
    repetitions = int(os.environ.get("DIAGNOSE_FULL_CONSUMER_REPEATS", "0"))
    if repetitions:
        original = source / "scripts/test-node-package.mjs"
        text = original.read_text(encoding="utf-8")
        cleanup = "  rmSync(temporaryDirectory, { recursive: true, force: true });"
        assert text.count(cleanup) == 1
        marker = OUT / (version + "-retained-consumer-path.txt")
        retained = source / "scripts/diagnostic-test-node-package.mjs"
        retained.write_text(text.replace(cleanup, "  writeFileSync(" + json.dumps(str(marker.resolve())) + ", temporaryDirectory);"), encoding="utf-8")
        run(f"{version}-retain-full-consumer", [NODE, str(retained), "--prebuilt", "--tarball", str(root_tarball), "--platform-tarball", str(platform_tarball)], source)
        full_consumer = Path(marker.read_text(encoding="utf-8"))
        full_test = full_consumer / "test-installed.mjs"
        shutil.copyfile(full_test, OUT / (version + "-full-consumer.mjs"))
        for index in range(repetitions):
            run(f"{version}-full-consumer-{index:03}", [NODE, "test-installed.mjs", str(source / "fixtures")], full_consumer)
        capture = os.environ.get("DIAGNOSE_PROCDUMP")
        if capture:
            for index in range(int(os.environ.get("DIAGNOSE_CAPTURE_REPEATS", "0"))):
                dump_directory = OUT / (f"{version}-capture-{index:03}")
                dump_directory.mkdir(exist_ok=True)
                result = run(f"{version}-captured-full-consumer-{index:03}", [capture, "-accepteula", "-e", "-mm", "-x", str(dump_directory), NODE, "test-installed.mjs", str(source / "fixtures")], full_consumer)
                report = {"label": f"{version}-captured-full-consumer-{index:03}", "debugger_returncode": result.returncode, "dumps": [str(p) for p in dump_directory.glob("*.dmp")]}
                (dump_directory / "capture.json").write_text(json.dumps(report, indent=2), encoding="utf-8")
                print(json.dumps(report), flush=True)

counts = {}
for result in RESULTS:
    key = result["label"].rsplit("-", 1)[0]
    if "-captured-full-consumer-" in result["label"]:
        value = counts.setdefault(key, {"capture_attempts": 0, "debugger_returncodes": {}})
        value["capture_attempts"] += 1
        codes = value["debugger_returncodes"]
        code = str(result["returncode"])
        codes[code] = codes.get(code, 0) + 1
        continue
    value = counts.setdefault(key, {"passed": 0, "failed": 0})
    value["passed" if result["returncode"] == 0 else "failed"] += 1
(OUT / "counts.json").write_text(json.dumps(counts, indent=2), encoding="utf-8")
print(json.dumps(counts, indent=2))
