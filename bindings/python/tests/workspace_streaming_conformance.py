"""Real installed Python streaming handles; no Python decoder or marker substitute."""
from __future__ import annotations
import gc
import subprocess
import sys
import unittest
from datafog_core import (
    Workspace, WorkspaceFinding, TextRange, WorkspaceBufferedRestore,
    WorkspaceProgressiveRestore, DataFogWorkspaceError, DataFogWorkspaceStreamError,
)

def ident(value): return bytes([value]) * 16

def save(workspace, change):
    raw = change.export_candidate_bundle()
    with Workspace.open_bundle(raw) as reopened:
        assert reopened.head() == change.candidate_head()
    workspace.activate_saved(change, change.expected_head(), change.candidate_head())
    return raw

def units(text):
    value = text.encode("utf-16-le")
    return [int.from_bytes(value[i:i+2], "little") for i in range(0, len(value), 2)]

def counts(text, markers=0):
    return {"bytes":len(text.encode()), "codepoints":len(text), "utf16":len(text.encode("utf-16-le"))//2, "restoredMarkers":markers}

class StreamingTests(unittest.TestCase):
    def setUp(self):
        self.workspace = Workspace.create(ident(1), ident(2))
        plan = self.workspace.prepare("Jane", [WorkspaceFinding("PERSON", "Jane", TextRange(0, 4))], ident(3))
        self.marker = plan.preview()["text"]
        save(self.workspace, plan)
    def tearDown(self): self.workspace.close()
    def error(self, code, action, delivered=None):
        with self.assertRaises(DataFogWorkspaceError) as caught: action()
        error = caught.exception
        self.assertEqual(error.code, code)
        if delivered is not None:
            self.assertIsInstance(error, DataFogWorkspaceStreamError)
            self.assertEqual(error.delivered, delivered)
            self.assertIs(error.complete, False)
            self.assertNotIn("Jane", repr(error))
        return error
    def test_strict_atomic_and_progressive_every_utf8_utf16_split(self):
        w=self.workspace;before=w.export_bundle();head=w.head()
        source="👋 é "+self.marker+" done.";expected="👋 é Jane done."
        for encoding,data,method in [("utf8",source.encode(),"feed"),("utf16",units(source),"feed_utf16")]:
            for split in range(len(data)+1):
                with w.buffered_restore(encoding) as strict:
                    self.assertIsInstance(strict,WorkspaceBufferedRestore)
                    self.assertIsNone(getattr(strict,method)(data[:split]))
                    self.assertIsNone(getattr(strict,method)(data[split:]))
                    self.assertEqual(strict.finalize(),{"text":expected,"restoredMarkers":1,"issues":[]})
                    self.error("inactive_stream",strict.finalize)
                with w.progressive_restore(encoding) as stream:
                    self.assertIsInstance(stream,WorkspaceProgressiveRestore)
                    a=getattr(stream,method)(data[:split]);b=getattr(stream,method)(data[split:]);last=stream.finalize()
                    self.assertFalse(a["complete"]);self.assertFalse(b["complete"]);self.assertTrue(last["complete"])
                    self.assertEqual(a["text"]+b["text"]+last["text"],expected)
                    self.assertEqual(last["delivered"],counts(expected,1))
                    self.error("inactive_stream",stream.finalize,counts(expected,1))
        self.assertEqual((w.export_bundle(),w.head()),(before,head))
    def test_late_failure_retains_only_earlier_delivery_and_original_ranges(self):
        source="👋 "+self.marker+" "
        s=self.workspace.progressive_restore();first=s.feed(source.encode())
        self.assertEqual(first["text"],"👋 Jane ")
        error=self.error("invalid_marker",lambda:s.feed(b"discarded [[PERSON_BAD]]"),counts("👋 Jane ",1))
        issue=error.marker_issues[0]
        self.assertEqual(issue["byteRange"]["start"],len((source+"discarded ").encode()))
        self.assertEqual(issue["codepointRange"]["start"],len(source+"discarded "))
        self.error("inactive_stream",s.finalize,counts("👋 Jane ",1))
        s.reset();self.assertEqual(s.feed(b"fresh.")["text"],"fresh.");self.assertEqual(s.finalize()["delivered"],counts("fresh."))
        strict=self.workspace.buffered_restore();strict.feed(source.encode());strict.feed(b"discarded [[PERSON_BAD]]")
        report=strict.finalize();self.assertIsNone(report["text"]);self.assertNotIn("Jane",repr(report));self.assertTrue(report["issues"])
    def test_invalid_encoding_transport_and_incomplete_eof(self):
        for encoding,value,method in [("utf8",b"\xf0\x9f","feed"),("utf16",[0xD83D],"feed_utf16")]:
            for factory,progressive in [(self.workspace.buffered_restore,False),(self.workspace.progressive_restore,True)]:
                stream=factory(encoding);getattr(stream,method)(value)
                self.error("invalid_encoding",stream.finalize,counts("") if progressive else None)
                stream.reset();stream.close()
        strict=self.workspace.buffered_restore();strict.feed(b"[");self.assertEqual(strict.finalize()["text"],"[")
        progressive=self.workspace.progressive_restore();progressive.feed(b"[");self.error("invalid_marker",progressive.finalize,counts(""))
        class Indexer:
            def __index__(self):raise AssertionError("custom numeric callback ran")
        invalid=[("utf8","feed","raw text"),("utf8","feed",[65]),("utf8","feed",bytearray(b"x")),("utf16","feed_utf16",[True]),("utf16","feed_utf16",[1.5]),("utf16","feed_utf16",[-1]),("utf16","feed_utf16",[65536]),("utf16","feed_utf16",[Indexer()]),("utf8","feed_utf16",[]),("utf16","feed",b"")]
        for encoding,method,value in invalid:
            with self.subTest(encoding=encoding,method=method,value=repr(value)):
                stream=self.workspace.progressive_restore(encoding)
                self.error("invalid_encoding",lambda:getattr(stream,method)(value),counts(""))
                self.error("inactive_stream",stream.finalize,counts(""));stream.reset();stream.close()
        self.error("invalid_encoding",lambda:self.workspace.progressive_restore("utf-8"))
        for factory in (WorkspaceBufferedRestore,WorkspaceProgressiveRestore):
            with self.assertRaises(TypeError):factory()
    def test_parent_close_drop_changed_head_noop_and_failed_save(self):
        w=self.workspace;s=w.progressive_restore();strict=w.buffered_restore();s.feed(b"prefix.")
        no_op=w.prepare("ordinary",[],ident(4));save(w,no_op)
        self.assertEqual(s.feed(b"ok.")["text"],"ok.");self.assertIsNone(strict.feed(b"ordinary"))
        staged=w.prepare("May",[WorkspaceFinding("PERSON","May",TextRange(0,3))],ident(5))
        self.error("workspace_conflict",lambda:w.activate_saved(staged,w.head(),w.head()))
        self.assertEqual(s.feed(b"still.")["text"],"still.")
        staged.discard();self.assertEqual(s.feed(b"valid.")["text"],"valid.")
        changed=w.prepare("May",[WorkspaceFinding("PERSON","May",TextRange(0,3))],ident(6));save(w,changed)
        prior=counts("prefix.ok.still.valid.")
        self.error("workspace_conflict",s.finalize,prior);self.error("workspace_conflict",strict.finalize)
        s=w.progressive_restore();s.feed(b"old.");w.close();self.error("closed_workspace",s.reset,counts("old."))
        owner=Workspace.create(ident(8),ident(9));orphan=owner.progressive_restore();orphan.feed(b"before.")
        del owner;gc.collect();self.error("closed_workspace",orphan.finalize,counts("before."))
        with self.assertRaisesRegex(RuntimeError,"outer"):
            with Workspace.create(ident(10),ident(11)) as owner:
                with owner.progressive_restore() as child:raise RuntimeError("outer")
        self.error("closed_workspace",child.finalize,counts(""))
    def test_resource_failure_is_one_atomic_feed(self):
        stream=self.workspace.progressive_restore();stream.feed(b"before.")
        self.error("resource_limit",lambda:stream.feed(b"x"*(16*1024*1024+1)),counts("before."))
        self.error("inactive_stream",stream.finalize,counts("before."))
        stream.reset();stream.close()
        # Bound the native copy of an oversized Python list while still letting
        # one Core call reject the whole chunk, even with a surrogate near EOF.
        utf16=self.workspace.progressive_restore("utf16")
        value=[65]*(16*1024*1024)+[0xD83D,0xDE00,65]
        self.error("resource_limit",lambda:utf16.feed_utf16(value),counts(""))
    def test_error_constructor_reentry_has_no_native_mutex_or_json_callback(self):
        code='''
import json
from datafog_core import Workspace,DataFogWorkspaceStreamError
w=Workspace.create(bytes([1])*16,bytes([2])*16);s=w.progressive_restore();s.feed(b"before.")
original=DataFogWorkspaceStreamError.__init__
entered=[]
def reenter(self,*args):
    s.reset();entered.append(True);original(self,*args)
DataFogWorkspaceStreamError.__init__=reenter
json.loads=lambda *a,**k:(_ for _ in ()).throw(AssertionError("dynamic json callback"))
try:s.feed(b"[[PERSON_BAD]]")
except DataFogWorkspaceStreamError as error:
    assert error.code=="invalid_marker" and error.delivered["bytes"]==7 and error.complete is False
else:raise AssertionError("missing failure")
assert entered and s.feed(b"after.")["text"]=="after."
print("stream reentry passed")
'''
        result=subprocess.run([sys.executable,"-c",code],capture_output=True,text=True,timeout=10)
        self.assertEqual(result.returncode,0,result.stdout+result.stderr);self.assertIn("stream reentry passed",result.stdout)

def verify_streaming():
    result=unittest.TextTestRunner(verbosity=2).run(unittest.defaultTestLoader.loadTestsFromTestCase(StreamingTests))
    if not result.wasSuccessful():raise AssertionError("installed Python streaming conformance failed")

if __name__=="__main__":verify_streaming()
