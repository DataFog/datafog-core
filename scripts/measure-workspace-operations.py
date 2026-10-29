#!/usr/bin/env python3
"""Synthetic review experiment in an idle isolated worktree; selects no launch cap."""
from __future__ import annotations
import hashlib
import json
from pathlib import Path
import platform
import re
import subprocess

ROOT=Path(__file__).resolve().parents[1]
SOURCE=ROOT/"crates/core/src/workspace/operations.rs"
OUTPUT=ROOT/"target/operation-costs"
INPUTS=["crates/core/src/workspace.rs","crates/core/src/workspace/operations.rs",
        "crates/core/src/workspace/codec.rs","crates/core/src/workspace/policy.rs",
        "crates/core/examples/workspace_operation_cost.rs"]

def sha(data):return hashlib.sha256(data).hexdigest()
def git(*args):return subprocess.check_output(["git",*args],cwd=ROOT,text=True).strip()

def main():
    OUTPUT.mkdir(parents=True,exist_ok=True)
    original=SOURCE.read_text()
    pattern=r"pub const MAX_OPERATIONS: usize = [0-9_]+;"
    if len(re.findall(pattern,original))!=1:raise RuntimeError("operation proposal constant changed")
    manifest={"scope":"synthetic unapproved capacity experiments, not a selected shipping limit",
              "baselineCommit":git("rev-parse","HEAD"),"baselineDirty":bool(git("status","--porcelain")),
              "system":platform.system(),"arch":platform.machine(),
              "rust":subprocess.check_output(["rustc","--version"],cwd=ROOT,text=True).strip(),
              "baselineInputs":{name:sha((ROOT/name).read_bytes()) for name in INPUTS},"profiles":[]}
    try:
        for cap in (10_000,100_000,300_000):
            profile=re.sub(pattern,f"pub const MAX_OPERATIONS: usize = {cap};",original)
            SOURCE.write_text(profile)
            with (OUTPUT/f"cap-{cap}.json").open("w") as output,(OUTPUT/f"cap-{cap}.log").open("w") as log:
                subprocess.run(["cargo","run","-p","datafog-core","--release","--example","workspace_operation_cost"],cwd=ROOT,stdout=output,stderr=log,check=True)
            report=json.loads((OUTPUT/f"cap-{cap}.json").read_text())
            if report["experimentalRecordCap"]!=cap:raise RuntimeError("experiment cap mismatch")
            manifest["profiles"].append({"cap":cap,"operationsSourceSha256":sha(profile.encode()),
                                         "report":f"cap-{cap}.json","reportSha256":sha((OUTPUT/f"cap-{cap}.json").read_bytes())})
    finally:
        SOURCE.write_text(original)
    if sha(SOURCE.read_bytes())!=manifest["baselineInputs"]["crates/core/src/workspace/operations.rs"]:
        raise RuntimeError("proposal source was not restored")
    # Do not leave an experimental default-feature library/example as the last build.
    subprocess.run(["cargo","build","-p","datafog-core","--release","--example","workspace_operation_cost"],cwd=ROOT,check=True)
    manifest["restoredSourceSha256"]=sha(SOURCE.read_bytes())
    (OUTPUT/"manifest.json").write_text(json.dumps(manifest,indent=2)+"\n")
    print("Synthetic capacity reports saved; original unapproved proposal limit restored.")

if __name__=="__main__":main()
