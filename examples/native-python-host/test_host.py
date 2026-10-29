"""Actual installed-SDK + encrypted SQLite/process qualification; invented text only."""
from __future__ import annotations
import copy
import hashlib
import json
import multiprocessing
import os
from pathlib import Path
import platform
import sqlite3
import subprocess
import datafog_core.datafog_core as native_sdk
import tempfile
import unittest
import cryptography
from datafog_core import Workspace, WorkspaceHead, WorkspaceFinding, TextRange, DataFogWorkspaceError
from host import EncryptedStore, Scope, RequestContext, HostError, prepare_and_handoff, restore_response


def public_id(value): return bytes([value]) * 16

def finding(text): return WorkspaceFinding("PERSON",text,TextRange(0,len(text.encode())))

def worker(pipe, path, scope, key, request_id, text, barrier=False, fault=None, cancel_phase=None, crash=False, pressure=False):
    cancelled=False
    def checkpoint(phase):
        nonlocal cancelled
        if phase=="prepared" and barrier:
            pipe.send({"phase":"prepared"})
            if pipe.recv()!="continue": raise HostError("cancelled")
        if phase==cancel_phase: cancelled=True
        if phase=="after_save" and crash: os._exit(23)
    try:
        store=EncryptedStore(Path(path),scope,key)
        if pressure:
            with store.open() as workspace:
                for iteration in range(16):
                    text=f"Synthetic Person {iteration}"
                    plans=[workspace.prepare(text,[finding(text)],os.urandom(16)) for _ in range(32)]
                    try: workspace.prepare(text,[finding(text)],os.urandom(16))
                    except DataFogWorkspaceError as error:
                        if error.code!="resource_limit":raise
                    else: raise HostError("unbounded_plans")
                    change=plans[0];parent=change.expected_head();saved=change.candidate_head()
                    store.compare_and_save(change.export_candidate_bundle(),parent,saved)
                    workspace.activate_saved(change,parent,saved)
                    for old in plans:
                        try:old.preview()
                        except DataFogWorkspaceError as error:
                            if error.code!="inactive_plan":raise
                        else:raise HostError("stale_plan_survived")
            pipe.send({"status":"pressure_passed","maximumLive":32,"cycles":16,"released":False})
            return
        result=prepare_and_handoff(store,RequestContext(scope,request_id),text,[finding(text)],
            approved=True,cancelled=lambda:cancelled,checkpoint=checkpoint,fault=fault)
        pipe.send({"status":"saved","request":request_id.hex(),"released":True,"text":result["text"]})
    except (HostError,DataFogWorkspaceError) as error:
        pipe.send({"status":error.code,"request":request_id.hex(),"released":False})
    finally: pipe.close()


class HostTests(unittest.TestCase):
    def setUp(self):
        self.tmp=tempfile.TemporaryDirectory(prefix="datafog-native-host-")
        self.path=Path(self.tmp.name)/"workspace.sqlite"
        self.scope=Scope(public_id(1),public_id(2),public_id(3))
        self.key=os.urandom(32)
        self.store=EncryptedStore(self.path,self.scope,self.key)
        with Workspace.create(self.scope.workspace,public_id(4)) as workspace: self.store.create(workspace)
        self.context=RequestContext(self.scope,public_id(5))
        self.processes=[]
    def tearDown(self):
        for process in self.processes:
            if process.is_alive(): process.terminate()
            process.join(5)
        self.tmp.cleanup()
    def error(self,code,operation):
        with self.assertRaises((HostError,DataFogWorkspaceError)) as caught: operation()
        self.assertEqual(caught.exception.code,code)
        self.assertEqual(str(caught.exception),code)
    def run_request(self,text="Ada Example",**kwargs):
        self.context=RequestContext(self.scope,os.urandom(16))
        return prepare_and_handoff(self.store,self.context,text,[finding(text)],approved=True,**kwargs)
    def process(self,request=6,text="Ada Example",scope=None,key=None,**kwargs):
        ctx=multiprocessing.get_context("spawn");parent,child=ctx.Pipe()
        process=ctx.Process(target=worker,args=(child,str(self.path),scope or self.scope,key or self.key,public_id(request),text),kwargs=kwargs)
        process.start();child.close();self.processes.append(process)
        return process,parent
    def receive(self,pipe):
        self.assertTrue(pipe.poll(15),"native worker did not return bounded status")
        return pipe.recv()
    def test_two_independent_processes_race_one_complete_parent(self):
        first,a=self.process(request=6,text="Ada Example",barrier=True)
        second,b=self.process(request=7,text="Bea Example",barrier=True)
        self.assertEqual(self.receive(a),{"phase":"prepared"});self.assertEqual(self.receive(b),{"phase":"prepared"})
        a.send("continue");b.send("continue")
        results=[self.receive(a),self.receive(b)]
        self.assertEqual(sorted(row["status"] for row in results),["saved","workspace_conflict"])
        self.assertEqual(sum(row["released"] for row in results),1)
        winner=next(row for row in results if row["released"])
        with self.store.open() as reopened:
            self.assertIn(reopened.restore(winner["text"]),["Ada Example","Bea Example"])
            self.assertEqual(reopened.head().revision,1)
        for process in [first,second]: process.join(10);self.assertEqual(process.exitcode,0)
        retry=self.run_request("Cy Example")
        self.assertTrue(retry["text"].startswith("[[PERSON_"))
        with self.store.open() as reopened: self.assertEqual(reopened.head().revision,2)
    def test_two_worker_plan_pressure_stays_bounded_and_reclaims_stale_handles(self):
        second_scope=Scope(self.scope.tenant,public_id(50),public_id(51));second_key=os.urandom(32)
        store=EncryptedStore(self.path,second_scope,second_key)
        with Workspace.create(second_scope.workspace,public_id(52)) as workspace:store.create(workspace)
        first,a=self.process(pressure=True)
        second,b=self.process(request=7,scope=second_scope,key=second_key,pressure=True)
        for pipe in [a,b]:
            self.assertEqual(self.receive(pipe),{"status":"pressure_passed","maximumLive":32,"cycles":16,"released":False})
        for process in [first,second]:process.join(15);self.assertEqual(process.exitcode,0)

    def test_both_head_components_participate_in_real_cas(self):
        with self.store.open() as workspace:
            before=workspace.export_bundle();parent=workspace.head()
            with workspace.prepare("Ada Example",[finding("Ada Example")],public_id(10)) as change:
                for wrong in [WorkspaceHead(parent.revision,public_id(80)),WorkspaceHead(parent.revision+1,parent.commit_id)]:
                    self.error("workspace_conflict",lambda:self.store.compare_and_save(change.export_candidate_bundle(),wrong,change.candidate_head()))
                with self.store.open() as unchanged:self.assertEqual(unchanged.export_bundle(),before)
    def test_malformed_metadata_is_sanitized_and_bounded(self):
        with sqlite3.connect(self.path) as db:
            original=db.execute("SELECT revision FROM workspaces").fetchone()[0]
            for bad in ["0"*5000,"01","-1","18446744073709551616"]:
                db.execute("UPDATE workspaces SET revision=?",(bad,));db.commit()
                self.error("invalid_envelope",self.store.open)
            db.execute("UPDATE workspaces SET revision=?",(original,))

    def test_legacy_upgrade_requires_explicit_notice_acknowledgment(self):
        root=Path(__file__).resolve().parents[2]
        raw=bytes.fromhex(json.loads((root/"fixtures/workspace/counters.json").read_text())["high"]["bundleHex"])
        with Workspace.open_bundle(raw) as legacy:
            scope=Scope(public_id(60),legacy.namespace(),public_id(61))
            store=EncryptedStore(self.path,scope,os.urandom(32));store.create(legacy)
        context=RequestContext(scope,public_id(62));text="Another Synthetic Person"
        self.error("upgrade_notice_required",lambda:prepare_and_handoff(store,context,text,[finding(text)],approved=True))
        with store.open() as unchanged:self.assertEqual(unchanged.export_bundle(),raw)
        result=prepare_and_handoff(store,context,text,[finding(text)],approved=True,acknowledge_upgrade=True)
        with store.open() as upgraded:
            self.assertEqual(upgraded.policy()["snapshotVersion"],2)
            self.assertEqual(upgraded.restore(result["text"]),text)

    def test_real_transaction_abort_never_activates_or_releases(self):
        with self.store.open() as before: raw=before.export_bundle();head=before.head()
        self.error("storage_failure",lambda:self.run_request(fault="abort_after_update"))
        with self.store.open() as after:
            self.assertEqual(after.export_bundle(),raw);self.assertEqual(after.head(),head)
        result=self.run_request()
        with self.store.open() as after: self.assertEqual(after.restore(result["text"]),"Ada Example")
    def test_lost_acknowledgment_persists_without_handoff(self):
        self.error("save_uncertain",lambda:self.run_request(fault="lose_ack"))
        with self.store.open() as reopened:
            self.assertEqual(reopened.head().revision,1)
            recalled=reopened.recall("Ada Example")["findings"]
            self.assertEqual(len(recalled),1)
        # A fresh request and explicit approval reuse the durable alias safely.
        result=self.run_request()
        with self.store.open() as reopened:
            self.assertEqual(reopened.head().revision,1)
            self.assertEqual(reopened.restore(result["text"]),"Ada Example")
    def test_cancellation_before_and_after_commit_blocks_handoff(self):
        for phase,revision in [("before_save",0),("after_save",1),("after_activation",1)]:
            cancelled=False
            def checkpoint(current):
                nonlocal cancelled
                if current==phase: cancelled=True
            self.error("cancelled",lambda:self.run_request(cancelled=lambda:cancelled,checkpoint=checkpoint))
            with self.store.open() as reopened: self.assertEqual(reopened.head().revision,revision)
    def test_actual_process_death_after_commit_and_fresh_restart(self):
        process,pipe=self.process(crash=True)
        process.join(15);self.assertEqual(process.exitcode,23)
        self.assertFalse(pipe.poll() and self._has_message(pipe))
        process,pipe=self.process(request=7)
        result=self.receive(pipe);process.join(15)
        self.assertEqual((process.exitcode,result["status"]),(0,"saved"))
        with self.store.open() as reopened:
            self.assertEqual(reopened.head().revision,1)
            self.assertEqual(reopened.restore(result["text"]),"Ada Example")
    @staticmethod
    def _has_message(pipe):
        try: pipe.recv();return True
        except EOFError:return False
    def test_encrypted_bytes_and_authenticated_metadata(self):
        result=self.run_request()
        content=self.path.read_bytes()
        for secret in [b"Ada Example",self.key,b"synthetic_v1",b"protected_v1"]: self.assertNotIn(secret,content)
        for column,replacement in [("revision","2"),("commit_id",public_id(40)),("snapshot",b"x"*40)]:
            with sqlite3.connect(self.path) as db:
                old=db.execute(f"SELECT {column} FROM workspaces").fetchone()[0]
                db.execute(f"UPDATE workspaces SET {column}=?",(replacement,))
            self.error("invalid_envelope",self.store.open)
            with sqlite3.connect(self.path) as db: db.execute(f"UPDATE workspaces SET {column}=?",(old,))
        self.error("invalid_envelope",EncryptedStore(self.path,self.scope,os.urandom(32)).open)
        with self.store.open() as reopened: self.assertEqual(reopened.restore(result["text"]),"Ada Example")
    def test_ciphertext_substitution_fails_even_with_same_test_key(self):
        self.run_request()
        other_scope=Scope(public_id(70),self.scope.workspace,self.scope.key_id)
        other=EncryptedStore(self.path,other_scope,self.key)
        with Workspace.create(other_scope.workspace,public_id(71)) as workspace:other.create(workspace)
        with sqlite3.connect(self.path) as db:
            revision,commit,ciphertext=db.execute("SELECT revision,commit_id,snapshot FROM workspaces WHERE tenant=?",(self.scope.tenant,)).fetchone()
            db.execute("UPDATE workspaces SET revision=?,commit_id=?,snapshot=? WHERE tenant=?",(revision,commit,ciphertext,other_scope.tenant))
        self.error("invalid_envelope",other.open)

    def test_tenant_workspace_key_and_request_isolation(self):
        response=self.run_request()
        other_scope=Scope(public_id(10),self.scope.workspace,public_id(11))
        other=EncryptedStore(self.path,other_scope,os.urandom(32))
        with Workspace.create(other_scope.workspace,public_id(12)) as workspace: other.create(workspace)
        other_context=RequestContext(other_scope,public_id(13))
        self.error("invalid_context",lambda:restore_response(other,other_context,self.context,{"text":response["text"]}))
        self.error("invalid_context",lambda:restore_response(self.store,self.context,RequestContext(self.scope,public_id(14)),{"text":response["text"]}))
        # Even bypassing the correlation check, a tenant's fresh mapping store
        # cannot guess another tenant's committed secret from a marker.
        self.error("invalid_marker",lambda:restore_response(other,other_context,other_context,{"text":response["text"]}))
        wrong_key_id=EncryptedStore(self.path,Scope(self.scope.tenant,self.scope.workspace,public_id(99)),self.key)
        self.error("invalid_envelope",wrong_key_id.open)
    def test_nonstreaming_tool_envelope_and_server_owned_context(self):
        response=self.run_request()
        body={"text":"Reply to "+response["text"],"tool_calls":[{"id":"tool-1","type":"tool_use","input":{"name":response["text"]}}]}
        before=copy.deepcopy(body)
        restored=restore_response(self.store,self.context,self.context,body)
        self.assertEqual(restored["text"],"Reply to Ada Example")
        self.assertEqual(restored["tool_calls"],before["tool_calls"]);self.assertEqual(body,before)
        self.error("invalid_marker",lambda:restore_response(self.store,self.context,self.context,{"text":"[[PERSON_ZZZZZZ]]"}))
    def test_chat_history_without_mapping_store_fails(self):
        response=self.run_request()
        missing=EncryptedStore(self.path,Scope(self.scope.tenant,public_id(30),self.scope.key_id),self.key)
        context=RequestContext(missing.scope,public_id(31))
        self.error("state_unavailable",lambda:restore_response(missing,context,context,{"text":response["text"],"history":"Ada Example"}))


def main():
    result=unittest.TextTestRunner(verbosity=2).run(unittest.defaultTestLoader.loadTestsFromTestCase(HostTests))
    if not result.wasSuccessful(): raise SystemExit(1)
    # Receipt is metadata only; fixture text, keys, plaintext snapshots and tool
    # contents never enter the on-disk qualification report.
    receipt={"result":"passed","tests":result.testsRun,"python":platform.python_version(),
        "cryptography":cryptography.__version__,"sqlite":sqlite3.sqlite_version,
        "workerStart":"spawn","workers":2,"maxLivePlansPerHandle":32,
        "scope":"synthetic native Python encrypted SQLite/process host; nonstreaming only"}
    root=Path(__file__).resolve().parents[2]
    receipt["sourceCommit"]=subprocess.check_output(["git","rev-parse","HEAD"],cwd=root,text=True).strip()
    receipt["sourceDirty"]=bool(subprocess.check_output(["git","status","--porcelain"],cwd=root))
    receipt["nativeSdkSha256"]=hashlib.sha256(Path(native_sdk.__file__).read_bytes()).hexdigest()
    receipt["inputs"]={name:hashlib.sha256((root/"examples/native-python-host"/name).read_bytes()).hexdigest() for name in ["host.py","test_host.py","requirements.txt"]}
    path=root/"target/python-host-proof.json"
    path.parent.mkdir(exist_ok=True);path.write_text(json.dumps(receipt,indent=2)+"\n")
    print("Native Python encrypted-host proof passed.")

if __name__=="__main__":main()
