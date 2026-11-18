"""Observe original no-op outcomes without changing save scheduling or error mapping."""
import json
import os
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parent.parent
SOURCE = ROOT / "candidate-source"
OUT = ROOT / "noop-diagnosis"
OUT.mkdir(exist_ok=True)
HEAD = "6c18c023fab49f30c1887a927afb0a4093886214"
assert subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=SOURCE, text=True, encoding="utf-8").strip() == HEAD
changes = []


def edit(name, old, new):
    path = SOURCE / name
    text = path.read_text(encoding="utf-8")
    assert text.count(old) == 1, name
    path.write_text(text.replace(old, new), encoding="utf-8")
    changes.append({"file": name, "old": old, "new": new})


edit("scripts/node-workspace-host.mjs",
     '  return failure(error?.code==="ERR_SQLITE_ERROR" && [5,6].includes(error.errcode) ? "storage_busy" : "storage_failure");',
     '  const mapped=failure(error?.code==="ERR_SQLITE_ERROR" && [5,6].includes(error.errcode) ? "storage_busy" : "storage_failure");\n  mapped.diagnosticSqliteErrcode=Number.isInteger(error?.errcode)?error.errcode:null; return mapped;')
edit("scripts/node-workspace-host-worker.mjs",
     'parentPort.postMessage({saved:false,activated:false,released:false,code:error.code,head:workspace.head()});',
     'parentPort.postMessage({saved:false,activated:false,released:false,code:error.code,sqliteErrcode:error.diagnosticSqliteErrcode,head:workspace.head()});')
assertion = '    assert(noOpOutcomes.every(result=>result.saved && result.activated && result.released));'
edit("scripts/node-workspace-host-conformance.mjs", assertion,
     '    console.log("[DIAG-NOOP] "+JSON.stringify(noOpOutcomes.map(({saved,activated,released,code,sqliteErrcode,head})=>({saved,activated,released,code,sqliteErrcode,head}))));\n' + assertion)
(OUT / "instrumentation.json").write_text(json.dumps(changes, indent=2), encoding="utf-8")
environment = {key: value for key, value in os.environ.items() if not any(marker in key.upper() for marker in ("TOKEN", "SECRET", "PASSWORD", "CREDENTIAL", "API_KEY"))}
records = []
for index in range(10):
    result = subprocess.run(["node", "scripts/test-node-package.mjs", "--prebuilt"], cwd=SOURCE, env=environment,
                            capture_output=True, text=True, encoding="utf-8", errors="replace", timeout=600)
    (OUT / f"batch-{index:02}.stdout").write_text(result.stdout, encoding="utf-8")
    (OUT / f"batch-{index:02}.stderr").write_text(result.stderr, encoding="utf-8")
    rows = []
    for line in result.stdout.splitlines():
        if line.startswith("[DIAG-NOOP] "):
            rows.append(json.loads(line.removeprefix("[DIAG-NOOP] ")))
    record = {"batch": index, "returncode": result.returncode, "no_op_rows": rows}
    records.append(record)
    (OUT / "results.json").write_text(json.dumps({"source": HEAD, "batches": records}, indent=2), encoding="utf-8")
    print(json.dumps(record), flush=True)
