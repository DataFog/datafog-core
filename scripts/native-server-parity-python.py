"""Actual installed native Python + existing encrypted SQLite host; synthetic only."""
from __future__ import annotations
import hashlib
import json
import multiprocessing
import os
from pathlib import Path
import signal
import sqlite3
import sys
import tempfile
import time

import datafog_core as api
sys.path.insert(0,str(Path(__file__).resolve().parents[1]/"examples/native-python-host"))
from host import EncryptedStore,Scope,HostError


def public_id(value): return bytes([value])*16

def head(value): return {"revision":str(value.revision),"commitId":list(value.commit_id)}

def snapshot(workspace):
    return {"head":head(workspace.head()),"namespaceHex":workspace.namespace().hex(),"bundleHex":workspace.export_bundle().hex(),"policy":workspace.policy(),"counts":workspace.counts(),"operations":workspace.operations(),"historyCoverage":workspace.history_coverage()}

def state(store):
    with store.open() as workspace: return snapshot(workspace)

def staged(workspace,config):
    if config.get("noop"): return workspace.prepare("ordinary",[],public_id(config["operationByte"]))
    definition=config["definition"]
    findings=[api.WorkspaceFinding(value["entityType"],value["matchedText"],api.TextRange(value["range"]["start"],value["range"]["end"]),source=value["source"],needs_review=value["needsReview"]) for value in definition["findings"]]
    return workspace.prepare_with_operation(definition["text"],findings,{"id":public_id(definition["operationByte"]),"occurredAtMs":definition["occurredAtMs"]})

def row(store):
    with store.connection() as db:
        return db.execute("SELECT * FROM workspaces WHERE tenant=? AND workspace=?",(store.scope.tenant,store.scope.workspace)).fetchone()

def row_digest(store):
    return hashlib.sha256(json.dumps([value.hex() if isinstance(value,bytes) else value for value in row(store)],separators=(",",":")).encode()).hexdigest()

def error_value(error):
    assert isinstance(error,HostError) and str(error)==error.code
    return {"code":error.code,"message":str(error)}

def child(pipe,config):
    store=EncryptedStore(Path(config["database"]),Scope(**config["scope"]),config["key"])
    try:
        with store.open() as workspace:
            assert workspace.export_bundle().hex()==config["seedHex"]
            with staged(workspace,config) as plan:
                candidate=plan.export_candidate_bundle();assert candidate.hex()==config["candidateHex"]
                parent,saved=plan.expected_head(),plan.candidate_head()
                if not config.get("crashAt"):
                    pipe.send({"ready":True,"parent":head(parent)});assert pipe.recv()=="save"
                checks=0
                def cancelled():
                    nonlocal checks
                    checks+=1
                    if config.get("crashAt")=="before_commit" and checks==2:
                        Path(config["phaseFile"]).write_text("before_commit")
                        while True: time.sleep(60)
                    return False
                activated=False
                try:
                    acknowledgment=store.compare_and_save(candidate,parent,saved,cancelled=cancelled)
                    if config.get("crashAt")=="after_commit":
                        Path(config["phaseFile"]).write_text("after_commit")
                        while True: time.sleep(60)
                    workspace.activate_saved(plan,parent,saved);activated=True
                    result={"ok":True,"head":head(workspace.head()),"acknowledgment":acknowledgment,"activated":activated}
                except HostError as error: result={"ok":False,"error":error_value(error),"activated":activated}
                result.update({"operationId":list(public_id(config["operationByte"] if config.get("noop") else config["definition"]["operationByte"])),"candidateBundleHex":config["candidateHex"],"saveCalls":1})
                pipe.send(result)
    finally: pipe.close()

def receive(pipe):
    if not pipe.poll(15): raise RuntimeError("native server child timeout")
    return pipe.recv()

def race(configs,processes):
    context=multiprocessing.get_context("spawn");pending=[]
    for config in configs:
        parent,other=context.Pipe();process=context.Process(target=child,args=(other,config));processes.append(process);process.start();other.close();pending.append((process,parent))
    try:
        ready=[receive(pipe) for _,pipe in pending];assert all(value["ready"] for value in ready)
        for _,pipe in pending: pipe.send("save")
        return {"ready":ready,"outcomes":[receive(pipe) for _,pipe in pending]}
    finally:
        for _,pipe in pending: pipe.close()

def crash(config,processes):
    context=multiprocessing.get_context("spawn");parent,other=context.Pipe();process=context.Process(target=child,args=(other,config));processes.append(process);process.start();other.close()
    try:
        phase=Path(config["phaseFile"])
        for _ in range(1500):
            if phase.exists() and phase.read_text()==config["crashAt"]: break
            if not process.is_alive(): raise RuntimeError("native server child exited before boundary")
            time.sleep(.01)
        else: raise RuntimeError("native server crash boundary timeout")
        os.kill(process.pid,signal.SIGKILL);process.join(10);assert not process.is_alive()
        return {"boundary":config["crashAt"],"processExit":{"code":process.exitcode,"signal":"SIGKILL"}}
    finally: parent.close()

def run(fixture,oracle):
    cases=[];processes=[]
    with tempfile.TemporaryDirectory(prefix="datafog-native-server-parity-") as directory:
        try:
            for scenario in fixture["cases"]:
                database=Path(directory)/(scenario["id"]+".sqlite");key=os.urandom(32);scope=Scope(public_id(11),public_id(fixture["namespaceByte"]),public_id(12));store=EncryptedStore(database,scope,key)
                with api.Workspace.create_with_operation(public_id(fixture["namespaceByte"]),{"id":public_id(fixture["bootstrapOperationByte"]),"occurredAtMs":0}) as seed:
                    assert seed.export_bundle().hex()==oracle["seed"]["bundleHex"];store.create(seed)
                result={"id":scenario["id"],"before":state(store),"steps":[]}
                def record(call,value): result["steps"].append({"call":call,"value":value})
                with store.open() as workspace:
                    definition=fixture["candidateDefinitions"]["a"]
                    with staged(workspace,{"definition":definition}) as plan:
                        candidate=plan.export_candidate_bundle();assert candidate.hex()==oracle["a"]["bundleHex"]
                        parent,saved=plan.expected_head(),plan.candidate_head();before_digest=row_digest(store)
                        def attempt(expected=parent,fault=None):
                            try:return {"ok":True,"acknowledgment":store.compare_and_save(candidate,expected,saved,fault=fault)}
                            except HostError as error:return {"ok":False,"error":error_value(error)}
                        name=scenario["id"]
                        if name=="acknowledged-save":
                            record("open",snapshot(workspace));record("save",attempt());workspace.activate_saved(plan,parent,saved);record("activate",snapshot(workspace));record("reopen",state(store))
                        elif name=="full-parent-conflict":
                            record("wrong-revision",attempt(api.WorkspaceHead(parent.revision+1,parent.commit_id)));record("wrong-commit-id",attempt(api.WorkspaceHead(parent.revision,public_id(99))));record("unchanged",{"rowBeforeSha256":before_digest,"rowAfterSha256":row_digest(store),"local":snapshot(workspace)})
                        elif name=="abort-rollback":
                            record("save-aborted",attempt(fault="abort_after_update"));record("unchanged",{"rowBeforeSha256":before_digest,"rowAfterSha256":row_digest(store),"local":snapshot(workspace)})
                        elif name=="lost-acknowledgment":
                            record("save-uncertain",{**attempt(fault="lose_ack"),"saveCalls":1,"local":snapshot(workspace)});record("reopen-committed",state(store))
                        elif name.endswith("-race"):
                            noop=name=="process-noop-race"
                            configs=[{"database":str(database),"scope":{"tenant":scope.tenant,"workspace":scope.workspace,"key_id":scope.key_id},"key":key,"seedHex":oracle["seed"]["bundleHex"],"definition":fixture["candidateDefinitions"][key_name],"candidateHex":oracle["noop" if noop else key_name]["bundleHex"],"noop":noop,"operationByte":5+index} for index,key_name in enumerate(["a","b"])]
                            evidence=race(configs,processes);record("two-processes-ready",evidence["ready"]);record("race",evidence["outcomes"]);record("reopen-same-head" if noop else "reopen-winner",state(store))
                        elif name.startswith("process-crash-"):
                            crash_at="before_commit" if name=="process-crash-before-commit" else "after_commit"
                            evidence=crash({"database":str(database),"scope":{"tenant":scope.tenant,"workspace":scope.workspace,"key_id":scope.key_id},"key":key,"seedHex":oracle["seed"]["bundleHex"],"definition":definition,"candidateHex":oracle["a"]["bundleHex"],"crashAt":crash_at,"phaseFile":str(Path(directory)/(name+".phase"))},processes)
                            record("child-at-boundary",{"boundary":evidence["boundary"]});record("killed",evidence["processExit"]);record("reopen-original" if crash_at=="before_commit" else "reopen-committed",state(store))
                        elif name=="authentication-and-isolation":
                            original=row(store);envelope=original[-1];altered=bytearray(envelope);altered[12]^=1
                            with store.connection() as db:db.execute("UPDATE workspaces SET snapshot=? WHERE tenant=? AND workspace=?",(bytes(altered),scope.tenant,scope.workspace))
                            try:
                                try: state(store);raise RuntimeError("tamper accepted")
                                except HostError as error: record("tampered-ciphertext",error_value(error))
                            finally:
                                with store.connection() as db:db.execute("UPDATE workspaces SET snapshot=? WHERE tenant=? AND workspace=?",(envelope,scope.tenant,scope.workspace))
                            with store.connection() as db:db.execute("UPDATE workspaces SET revision=? WHERE tenant=? AND workspace=?",("1",scope.tenant,scope.workspace))
                            try:
                                try:state(store);raise RuntimeError("modified head accepted")
                                except HostError as error:record("tampered-head-metadata",error_value(error))
                            finally:
                                with store.connection() as db:db.execute("UPDATE workspaces SET revision=? WHERE tenant=? AND workspace=?",(original[3],scope.tenant,scope.workspace))
                            try:state(EncryptedStore(database,scope,bytes(value^255 for value in key)));raise RuntimeError("wrong key accepted")
                            except HostError as error:record("wrong-key",error_value(error))
                            try:state(EncryptedStore(database,Scope(public_id(99),scope.workspace,scope.key_id),key));raise RuntimeError("other tenant accepted")
                            except HostError as error:record("other-tenant",error_value(error))
                            record("restored-row",{"rowBeforeSha256":before_digest,"rowAfterSha256":row_digest(store)})
                        else:raise RuntimeError("unknown native host scenario")
                result["after"]=state(store);cases.append(result)
        finally:
            for process in processes:
                if process.is_alive():os.kill(process.pid,signal.SIGKILL)
                process.join(10)
                if process.is_alive():raise RuntimeError("native server child cleanup timeout")
    return {"schemaVersion":1,"profile":fixture["profile"],"sdk":"python","cases":cases}

if __name__=="__main__":
    try:
        fixture=json.loads(Path(sys.argv[1]).read_text());oracle=json.loads(Path(sys.argv[2]).read_text())
        print(json.dumps(run(fixture,oracle),ensure_ascii=False,allow_nan=False))
    except Exception:
        sys.stderr.write("native server Python qualification failed\n");raise SystemExit(1) from None
