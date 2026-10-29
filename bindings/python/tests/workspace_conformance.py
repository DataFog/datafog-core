"""Exercise the real installed native wheel, without a Python workspace substitute."""
from __future__ import annotations
import gc
import json
import unittest
import subprocess
import sys
import textwrap
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path
from datafog_core import (
    Workspace, WorkspacePreparedChange, WorkspaceHead, WorkspaceFinding, WorkspaceInput,
    WorkspaceReviewInput, WorkspaceReviewCorrection, DataFogWorkspaceError, TextRange, scan,
)
ROOT = Path(__file__).resolve().parents[3]

def public_id(value): return bytes([value]) * 16

def finding(text, label="PERSON", start=0, end=None, **kwargs):
    raw = text.encode()
    end = len(raw) if end is None else end
    return WorkspaceFinding(label, raw[start:end].decode(), TextRange(start, end), **kwargs)

def save(workspace, change):
    # The binding trusts the host's durable-save receipt. These tests inspect the
    # actual candidate independently; the encrypted transactional host is W6.
    candidate = change.export_candidate_bundle()
    with Workspace.open_bundle(candidate) as reopened:
        assert reopened.head() == change.candidate_head()
    workspace.activate_saved(change, change.expected_head(), change.candidate_head())
    return candidate

def edits(value="May", disposition="not_sensitive", label="PERSON"):
    return {"rules": [], "choices": [{"entityType": label, "value": value, "disposition": disposition}]}

class WorkspaceTests(unittest.TestCase):
    def setUp(self): self.workspace = Workspace.create(public_id(1), public_id(2))
    def tearDown(self): self.workspace.close()
    def error(self, code, operation):
        with self.assertRaises(DataFogWorkspaceError) as caught: operation()
        error = caught.exception
        self.assertEqual(error.code, code)
        self.assertEqual(str(error), code)
        return error
    def test_candidate_save_reopen_and_strict_restoration(self):
        w = self.workspace
        before, head = w.export_bundle(), w.head()
        p = w.prepare("Jane Smith", [finding("Jane Smith")], public_id(3))
        self.assertIsInstance(p, WorkspacePreparedChange)
        marker = p.preview()["text"]
        self.assertNotEqual(marker, "Jane Smith")
        self.assertEqual((w.export_bundle(), w.head()), (before, head))
        self.error("invalid_marker", lambda: w.restore(marker))
        changed = p.preview(); changed["text"] = "tampered"
        self.assertEqual(p.preview()["text"], marker)
        self.error("workspace_conflict", lambda: w.activate_saved(p, head, head))
        self.assertEqual(w.export_bundle(), before)
        saved = save(w, p)
        self.error("inactive_plan", p.preview)
        self.assertEqual(w.restore(marker), "Jane Smith")
        with Workspace.open_bundle(saved) as r:
            self.assertEqual(r.namespace(), public_id(1))
            self.assertEqual(r.restore(marker), "Jane Smith")
            report = r.restore_report(marker + " [[PERSON_ZZZZZZ]]")
            self.assertIsNone(report["text"])
            self.assertEqual(report["restoredMarkers"], 1)
            self.assertEqual(report["issues"][0]["kind"], "unknown")
            self.assertNotIn("Jane", repr(report))
    def test_close_drop_discard_and_context_lifetimes(self):
        w = self.workspace
        p = w.prepare("Jane", [finding("Jane")], public_id(3))
        p.discard(); self.error("inactive_plan", p.discard); self.error("inactive_plan", p.preview)
        with w.prepare("Jane", [finding("Jane")], public_id(3)) as p: self.assertTrue(p.preview())
        self.error("inactive_plan", p.export_candidate_bundle)
        p = w.prepare("Jane", [finding("Jane")], public_id(3))
        w.close(); self.error("closed_workspace", p.preview); self.error("closed_workspace", p.discard); self.error("closed_workspace", w.export_bundle)
        other = Workspace.create(public_id(8), public_id(9))
        p = other.prepare("Jane", [finding("Jane")], public_id(10))
        del other; gc.collect(); self.error("closed_workspace", p.preview)
        with self.assertRaisesRegex(RuntimeError, "outer failure"):
            with Workspace.create(public_id(8), public_id(9)) as other:
                raise RuntimeError("outer failure")
        self.error("closed_workspace", other.head)
    def test_live_plan_slots_gc_and_changed_head_cleanup(self):
        w = self.workspace
        plans = [w.prepare("Jane", [finding("Jane")], public_id(3)) for _ in range(32)]
        self.error("resource_limit", lambda: w.prepare("Jane", [finding("Jane")], public_id(4)))
        dropped = plans.pop(); del dropped; gc.collect()
        replacement = w.prepare("Jane", [finding("Jane")], public_id(4))
        save(w, replacement)
        for p in plans: self.error("inactive_plan", p.preview)
        fresh = [w.prepare("Jane", [finding("Jane")], public_id(5)) for _ in range(32)]
        for p in fresh: p.discard()
        self.assertTrue(w.prepare("Jane", [finding("Jane")], public_id(5)).preview())
    def test_noop_activation_keeps_siblings_foreign_activation_rejected(self):
        w = self.workspace
        p = w.prepare("ordinary", [], public_id(3))
        sibling = w.prepare("Jane", [finding("Jane")], public_id(4))
        self.assertEqual(p.expected_head(), p.candidate_head())
        save(w, p); self.assertTrue(sibling.preview())
        with Workspace.create(public_id(9), public_id(10)) as other:
            self.error("workspace_conflict", lambda: other.activate_saved(sibling, sibling.expected_head(), sibling.candidate_head()))
        self.assertTrue(sibling.preview())
    def test_policy_explicit_actions_permanent_aliases_and_recovery(self):
        w = self.workspace
        p = w.prepare("May", [finding("May")], public_id(3)); marker=p.preview()["text"];save(w,p)
        self.error("invalid_policy", lambda:w.prepare_policy_changes(edits(disposition="keep"),public_id(4)))
        before=w.export_bundle();p=w.prepare_policy_changes(edits(),public_id(4))
        self.assertEqual(p.policy_preview()["choices"][0]["after"],"not_sensitive")
        self.assertEqual(w.export_bundle(),before);save(w,p)
        self.assertEqual(w.recall("MAY")["findings"],[])
        self.assertEqual(w.prepare("MAY",[finding("MAY")],public_id(5)).preview()["text"],"MAY")
        self.assertEqual(w.restore(marker),"May")
        candidate=save(w,w.prepare_recovery(public_id(6)))
        with Workspace.open_bundle(candidate) as r:
            self.assertEqual(r.policy()["choices"][0]["disposition"],"not_sensitive")
            self.assertEqual(r.restore(marker),"May")
            save(r,r.prepare_policy_changes(edits(disposition=None),public_id(7)))
            self.assertEqual(len(r.recall("MAY")["findings"]),1)
            self.assertEqual(r.prepare("MAY",[finding("MAY")],public_id(8)).preview()["text"],marker)
    def test_malformed_policy_containers_are_bounded_and_inert(self):
        w=self.workspace;before=w.export_bundle()
        cycle={};cycle["rules"]=cycle;cycle["choices"]=[]
        for value in [cycle,{"rules":[{"entityType":"CUSTOM"}],"choices":[]},
                      {"rules":[],"choices":[{"entityType":"PERSON","value":"May"}]},
                      {"rules":[],"choices":[],"unknown":True},edits(disposition=object()),edits(value="\ud800")]:
            self.error("invalid_policy",lambda value=value:w.prepare_policy_changes(value,public_id(3)))
        self.assertEqual(w.export_bundle(),before)
        self.error("resource_limit",lambda:w.prepare_policy_changes({"rules":[],"choices":[edits()["choices"][0]]*10001},public_id(3)))
    def test_unicode_scanner_adapter_offsets_and_typed_scope(self):
        w=self.workspace;text="👋 José and 123-45-6789"
        name=finding(text,start=5,end=10)
        native=next(f for f in scan(text) if f.entity_type=="SSN")
        converted=WorkspaceFinding.from_finding(native)
        self.assertEqual(converted.byte_range.start,native.byte_range.start)
        p=w.prepare(text,[name,converted],public_id(3));preview=p.preview()
        t=preview["transformations"][0]
        self.assertEqual(t["sourceByteRange"],{"start":5,"end":10})
        self.assertEqual(t["sourceCodepointRange"],{"start":2,"end":6})
        self.assertEqual(t["sourceUtf16Range"],{"start":3,"end":7})
        save(w,p)
        recalled=w.recall("👋 JOSÉ")
        self.assertEqual(recalled["findings"][0]["utf16Range"],{"start":3,"end":7})
        self.assertEqual(w.restore(preview["text"]),text)
    def test_batch_validation_errors_keep_original_indices(self):
        w=self.workspace;before=w.export_bundle()
        wrong=WorkspaceFinding("PERSON","wrong",TextRange(0,4))
        error=self.error("invalid_finding",lambda:w.prepare_batch([WorkspaceInput("Jane",[finding("Jane")]),WorkspaceInput("Jane",[wrong])],public_id(3)))
        self.assertEqual((error.input_index,error.finding_index),(1,0));self.assertEqual(w.export_bundle(),before)
        p=w.prepare_batch([WorkspaceInput("Jane",[finding("Jane")]),WorkspaceInput("JANE",[finding("JANE")])],public_id(3))
        self.assertEqual(p.previews()[0]["text"],p.previews()[1]["text"])
        self.error("invalid_batch",p.preview)
        self.error("resource_limit",lambda:w.prepare_batch([WorkspaceInput("",[])]*33,public_id(4)))
    def test_combined_review_corrections_and_fail_closed_overlap(self):
        w=self.workspace;text="👋 abcdef"
        first=finding(text,start=5,end=9);second=finding(text,label="CUSTOM",start=7,end=11)
        inputs=WorkspaceReviewInput(text,scanner_findings=[first],user_selections=[second])
        before=w.export_bundle();review=w.review(inputs)
        self.assertTrue(review["conflicts"]);self.assertFalse(any(c["selected"] for c in review["candidates"]))
        error=self.error("review_conflict",lambda:w.prepare_review([inputs],public_id(3)))
        self.assertEqual(error.input_index,0);self.assertTrue(error.review_conflicts);self.assertNotIn("abcdef",str(error))
        key=next(c["key"] for c in review["candidates"] if c["finding"]["entityType"]=="PERSON")
        correction=WorkspaceReviewCorrection(key,finding(text,start=5,end=11))
        corrected=WorkspaceReviewInput(text,scanner_findings=[first],user_selections=[second],corrections=[correction])
        resolved=w.review(corrected);self.assertEqual(resolved["conflicts"],[])
        self.assertEqual(w.export_bundle(),before)
        p=w.prepare_review([corrected],public_id(3));save(w,p)
        self.error("workspace_conflict",lambda:w.review(corrected))
    def test_full_uint64_legacy_bytes_ids_and_bounds(self):
        fixture=json.loads((ROOT/"fixtures/workspace/counters.json").read_text())
        for case in fixture.values():
            raw=bytes.fromhex(case["bundleHex"])
            with Workspace.open_bundle(raw) as w:
                self.assertEqual(w.export_bundle(),raw)
                self.assertEqual(w.head().revision,int(case["revision"]))
                p=w.prepare("Jane",[finding("Jane")],public_id(201))
                self.assertEqual(p.export_candidate_bundle(),raw)
                self.assertEqual(p.snapshot_format(),{"currentVersion":1,"candidateVersion":1})
                self.assertEqual(w.restore(case["marker"]),"Jane")
        self.assertEqual(WorkspaceHead(2**64-1,public_id(7)).revision,2**64-1)
        self.error("invalid_operation_id",lambda:Workspace.create(b"short",public_id(2)))
        self.error("invalid_snapshot",lambda:Workspace.open_bundle(b"bad"))
        self.error("resource_limit",lambda:WorkspaceInput("x"*(1024*1024+1),[]))
    def test_aggregate_native_material_is_bounded_before_cloning(self):
        w=self.workspace;before=w.export_bundle()
        text="x"*(1024*1024);large=finding(text)
        self.error("resource_limit",lambda:w.prepare(text,[large]*17,public_id(3)))
        correction=WorkspaceReviewCorrection("a"*64,large)
        self.error("resource_limit",lambda:WorkspaceReviewInput(text,corrections=[correction]*17))
        item=WorkspaceInput(text,[large])
        self.error("resource_limit",lambda:w.prepare_batch([item]*9,public_id(3)))
        self.assertEqual(w.export_bundle(),before)

    def test_error_normalization_allows_reentry_without_native_locks(self):
        # Each regression gets a killable subprocess: a held non-reentrant native
        # mutex must fail this test rather than hang the CI process forever.
        cases={
            "plan": """
                p=w.prepare("Jane",[f],public_id(3));p.discard()
                def reenter():
                    try:p.expected_head()
                    except DataFogWorkspaceError as error:assert error.code=="inactive_plan"
                def trigger():p.preview()
                expected="inactive_plan"
            """,
            "stage": """
                wrong=WorkspaceFinding("PERSON","wrong",TextRange(0,4))
                def reenter():w.prepare("ordinary",[],public_id(4)).discard()
                def trigger():w.prepare("Jane",[wrong],public_id(3))
                expected="invalid_finding"
            """,
            "capacity": """
                plans=[w.prepare("Jane",[f],public_id(3)) for _ in range(32)]
                def reenter():
                    plans.pop().discard()
                    w.prepare("ordinary",[],public_id(4)).discard()
                def trigger():w.prepare("Jane",[f],public_id(4))
                expected="resource_limit"
            """,
            "activation": """
                p=w.prepare("Jane",[f],public_id(3));parent=p.expected_head()
                def reenter():assert p.expected_head()==parent
                def trigger():w.activate_saved(p,parent,parent)
                expected="workspace_conflict"
            """,
        }
        prelude="""
import json
from datafog_core import Workspace,WorkspaceFinding,TextRange,DataFogWorkspaceError
public_id=lambda value:bytes([value])*16
w=Workspace.create(public_id(1),public_id(2))
f=WorkspaceFinding("PERSON","Jane",TextRange(0,4))
before=w.export_bundle()
"""
        tail="""
original_init=DataFogWorkspaceError.__init__
original_loads=json.loads
entered=False
def hooked_init(self,*args,**kwargs):
    global entered
    if not entered:
        entered=True
        reenter()
    original_init(self,*args,**kwargs)
def decoder_must_not_run(*args,**kwargs):
    raise AssertionError("workspace views must not call a mutable Python JSON decoder")
DataFogWorkspaceError.__init__=hooked_init
json.loads=decoder_must_not_run
try:
    try:trigger()
    except DataFogWorkspaceError as error:assert error.code==expected
    else:raise AssertionError("missing canonical workspace error")
finally:
    DataFogWorkspaceError.__init__=original_init
    json.loads=original_loads
assert entered
assert w.export_bundle()==before
w.close()
print("reentry passed")
"""
        for name,case in cases.items():
            with self.subTest(name=name):
                result=subprocess.run([sys.executable,"-c",prelude+textwrap.dedent(case)+tail],capture_output=True,text=True,timeout=10)
                self.assertEqual(result.returncode,0,result.stdout+result.stderr)
                self.assertIn("reentry passed",result.stdout)

    def test_threads_are_serialized_and_independent_workers_reopen(self):
        w=self.workspace;p=w.prepare("Jane",[finding("Jane")],public_id(3));marker=p.preview()["text"];raw=save(w,p)
        def read(_):
            with Workspace.open_bundle(raw) as independent:
                return independent.restore(marker),w.recall("JANE")["findings"][0]["finding"]["matchedText"]
        with ThreadPoolExecutor(max_workers=4) as executor:
            self.assertEqual(list(executor.map(read,range(24))),[("Jane","JANE")]*24)


def verify_workspace():
    result=unittest.TextTestRunner(verbosity=2).run(unittest.defaultTestLoader.loadTestsFromTestCase(WorkspaceTests))
    if not result.wasSuccessful(): raise AssertionError("installed native workspace conformance failed")

if __name__ == "__main__": verify_workspace()
