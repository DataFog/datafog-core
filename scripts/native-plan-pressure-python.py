"""Actual installed PyO3 handles; all retained wrappers and memory scopes are explicit."""
from __future__ import annotations
import gc
import hashlib
import json
import multiprocessing
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import tracemalloc

import datafog_core as api
sys.path.insert(0,str(Path(__file__).resolve().parents[1]/"examples/native-python-host"))
from host import EncryptedStore,Scope,HostError


def public_id(value):return bytes([value])*16

def operation(worker,cycle,handle,phase,slot):return {"id":bytes([250,worker,cycle,handle,phase,slot])+bytes(10),"occurredAtMs":0}

def failure(action,code):
    try:action()
    except (api.DataFogWorkspaceError,HostError) as error:
        assert error.code==code and str(error)==code
        return error.code
    else:raise AssertionError("required native plan failure missing")

def source_text(config):
    value="".join(hashlib.sha256(f"native-plan-pressure:{index}".encode()).hexdigest() for index in range((config["sourceBytes"]+63)//64))[:config["sourceBytes"]]
    assert hashlib.sha256(value.encode()).hexdigest()==config["sourceSha256"];return value

def sample():
    gc.collect();current,peak=tracemalloc.get_traced_memory()
    rss=int(subprocess.check_output(["ps","-o","rss=","-p",str(os.getpid())],text=True).strip())*1024
    return {"pid":os.getpid(),"rss":rss,"pythonTracedBytes":current,"pythonTracedPeakBytes":peak}

def worker(pipe,config,database,scope,key,index):
    tracemalloc.start();store=EncryptedStore(Path(database),Scope(**scope),key);source=source_text(config);retained=[]
    try:
        for cycle in range(config["cycles"]):
            handles=[store.open() for _ in range(config["handlesPerWorker"])];before=sample();plans=[];admission_errors=[]
            for handle_index,handle in enumerate(handles):
                rows=[handle.prepare_with_operation(source,[],operation(index,cycle,handle_index,0,slot)) for slot in range(config["plansPerHandle"])]
                admission_errors.append(failure(lambda:handle.prepare_with_operation(source,[],operation(index,cycle,handle_index,3,0)),"resource_limit"))
                discarded=rows.pop();discarded.discard();failure(discarded.expected_head,"inactive_plan");retained.append(discarded)
                rows.append(handle.prepare_with_operation(source,[],operation(index,cycle,handle_index,1,31)));plans.append(rows)
            held=sample();pipe.send({"phase":"held","worker":index,"cycle":cycle,"handles":len(handles),"admissionErrors":admission_errors,"namespaces":[handle.namespace().hex() for handle in handles],"heads":[{"revision":str(handle.head().revision),"commitId":list(handle.head().commit_id)} for handle in handles],"livePlans":sum(map(len,plans)),"before":before,"held":held})
            assert pipe.recv()=="release"
            chosen=plans[0][0];parent,saved=chosen.expected_head(),chosen.candidate_head();candidate=chosen.export_candidate_bundle()
            store.compare_and_save(candidate,parent,saved);handles[0].activate_saved(chosen,parent,saved)
            for plan in plans[0]:failure(plan.expected_head,"inactive_plan")
            recycled=[handles[0].prepare_with_operation(source,[],operation(index,cycle,0,2,slot)) for slot in range(32)]
            after_cleanup_code=failure(lambda:handles[0].prepare_with_operation(source,[],operation(index,cycle,0,3,1)),"resource_limit")
            for plan in recycled:plan.discard();failure(plan.expected_head,"inactive_plan")
            retained.extend(recycled)
            stale=plans[1][0];stale_parent,stale_saved=stale.expected_head(),stale.candidate_head()
            stale_cas_code=failure(lambda:store.compare_and_save(stale.export_candidate_bundle(),stale_parent,stale_saved),"workspace_conflict")
            assert stale.expected_head()==stale_parent
            retained.extend(plan for rows in plans for plan in rows)
            for handle in handles:handle.close()
            for rows in plans:
                for plan in rows:failure(plan.expected_head,"closed_workspace")
            with store.open() as reopened:
                counts=reopened.counts();assert counts["operations"]==cycle+2 and counts["preparedInputs"]==cycle+1 and counts["transformedSpans"]==0
            pipe.send({"phase":"released","worker":index,"cycle":cycle,"retainedWrappers":len(retained),"counts":counts,"after":sample(),"discardSlotReused":True,"changedHeadSlotsReused":32,"staleIndependentHandleRejected":True,"staleCasCode":stale_cas_code,"afterCleanupAdmissionCode":after_cleanup_code})
            assert pipe.recv()=="next"
        for plan in retained:failure(plan.expected_head,"closed_workspace")
        retained_wrappers=len(retained);after=sample();retained.clear()
        pipe.send({"phase":"done","worker":index,"retainedWrappers":retained_wrappers,"retainedReferenceListAfterDrop":len(retained),"after":after,"afterDrop":sample()})
    finally:pipe.close()

def receive(pipe):
    if not pipe.poll(30):raise RuntimeError("native plan pressure child timeout")
    return pipe.recv()

def run(config):
    assert config["workers"]==2 and config["handlesPerWorker"]==2 and config["plansPerHandle"]==32
    processes=[];pipes=[];cycles=[]
    with tempfile.TemporaryDirectory(prefix="datafog-python-plan-pressure-") as directory:
        database=str(Path(directory)/"pressure.sqlite")
        try:
            context=multiprocessing.get_context("spawn")
            for index in range(2):
                scope=Scope(public_id(11),public_id(20+index),public_id(12));key=os.urandom(32);store=EncryptedStore(Path(database),scope,key)
                with api.Workspace.create_with_operation(scope.workspace,{"id":public_id(2),"occurredAtMs":0}) as seed:store.create(seed)
                parent,other=context.Pipe();process=context.Process(target=worker,args=(other,config,database,{"tenant":scope.tenant,"workspace":scope.workspace,"key_id":scope.key_id},key,index));processes.append(process);process.start();other.close();pipes.append(parent)
            for cycle in range(config["cycles"]):
                held=[receive(pipe) for pipe in pipes];assert all(value["phase"]=="held" and value["cycle"]==cycle and value["livePlans"]==64 for value in held)
                released=[]
                for pipe in pipes:
                    pipe.send("release");value=receive(pipe);assert value["phase"]=="released" and value["cycle"]==cycle;released.append(value);pipe.send("next")
                cycles.append({"cycle":cycle,"observedLivePlansAtBarrier":sum(value["livePlans"] for value in held),"workers":[{"held":value,"released":released[index]} for index,value in enumerate(held)]})
            done=[receive(pipe) for pipe in pipes];assert all(value["phase"]=="done" for value in done)
            for process in processes:process.join(10);assert process.exitcode==0
            return {"schemaVersion":1,"profile":config["profile"],"sdk":"python","cycles":cycles,"done":done,"exits":[{"code":process.exitcode,"signal":None} for process in processes],"scope":"per-handle admission only; RSS/Python tracemalloc exclude exact Rust registry accounting and are not a host-wide budget"}
        finally:
            for process in processes:
                if process.is_alive():process.kill()
                process.join(10)
            for pipe in pipes:pipe.close()

if __name__=="__main__":
    try:print(json.dumps(run(json.loads(Path(sys.argv[1]).read_text())),allow_nan=False))
    except Exception:sys.stderr.write("native plan pressure qualification failed\n");raise SystemExit(1) from None
