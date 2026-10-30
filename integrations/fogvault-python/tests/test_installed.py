"""Synthetic installed-wheel contracts; never a physical-provider qualification."""
import gc
import json
from pathlib import Path
import secrets
import tempfile
import threading
import unittest

import datafog_fogvault as fv

TENANT = "synthetic-tenant-a"
PASSWORD = b"synthetic-only Python native integration password"

def identity():
    while True:
        value = secrets.token_bytes(16)
        if any(value):
            return value

def finding(text):
    value = "alice@example.test"
    offset = text.encode().index(value.encode())
    return {"entityType": "EMAIL", "matchedText": value,
            "range": {"unit": "utf8_bytes", "start": offset, "end": offset + len(value)}}

class InstalledOwnerTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="datafog-SYNTHETIC-python-")
        self.root = Path(self.temp.name)
        self.record, self.namespace = identity(), identity()
        self.control, self.vault = str(self.root / "independently-protected-control"), str(self.root / "vault.sqlite")
        self.owners = []

    def tearDown(self):
        for owner in self.owners:
            owner.close(TENANT)
        self.temp.cleanup()

    def create(self, **options):
        owner = fv.NativeWorkspace.create_new(TENANT, self.control, self.vault, PASSWORD,
            self.record, self.namespace, identity(), identity(), **options)
        self.owners.append(owner)
        return owner

    def reopen(self, **options):
        owner = fv.NativeWorkspace.open_existing(TENANT, self.control, self.vault, PASSWORD,
            self.record, self.namespace, **options)
        self.owners.append(owner)
        return owner

    def rejected(self, run, code=None):
        with self.assertRaises(fv.FogVaultWorkspaceError) as error:
            run()
        if code is not None:
            self.assertEqual(error.exception.code, code)
        self.assertNotIn(PASSWORD.decode(), str(error.exception))
        self.assertNotIn("alice@example.test", str(error.exception))
        self.assertNotIn(self.vault, str(error.exception))
        return error.exception

    def committed(self, owner, text="🦀 Email alice@example.test"):
        plan = owner.prepare_review(TENANT, [{"text": text, "scannerFindings": [finding(text)], "userSelections": []}], identity())
        preview = plan.previews(TENANT)[0]
        self.assertNotIn("alice@example.test", preview["text"])
        return owner.commit(TENANT, plan, True, identity()), plan, preview

    def test_installed_native_extension_not_source_wrapper(self):
        self.assertTrue(any(Path(fv.__file__).parent.glob("*.so")) or any(Path(fv.__file__).parent.glob("*.pyd")))
        self.assertIn("site-packages", str(fv.__file__))
        self.assertEqual(fv.__version__, "0.0.0")
        with self.assertRaises(TypeError):
            fv.NativeWorkspace()
        with self.assertRaises(TypeError):
            fv.PreparedChange()

    def test_create_review_commit_lock_reopen_read_and_unicode_offsets(self):
        owner = self.create()
        saved, plan, preview = self.committed(owner)
        self.assertEqual(saved["recordRevision"], 2)
        self.assertEqual(saved["previews"], [preview["text"]])
        transform = preview["transformations"][0]
        self.assertEqual(transform["sourceByteRange"]["start"], len("🦀 Email ".encode()))
        self.assertEqual(transform["sourceCodepointRange"]["start"], len("🦀 Email "))
        self.assertEqual(transform["sourceUtf16Range"]["start"], len("🦀 Email ".encode("utf-16-le")) // 2)
        owner.lock(TENANT)
        self.rejected(lambda: plan.previews(TENANT), "owner_closed")
        reopened = self.reopen(expected_head=saved["head"])
        self.assertEqual(reopened.restore(TENANT, saved["previews"][0]), "🦀 Email alice@example.test")

    def test_only_literal_true_approves_and_rejection_preserves_pending_plan(self):
        owner = self.create()
        text = "Email alice@example.test"
        plan = owner.prepare(TENANT, text, [finding(text)], identity())
        for approved in [False, 1, "true", None, object()]:
            self.rejected(lambda: owner.commit(TENANT, plan, approved, identity()), "approval_required")
        self.assertEqual(owner.record_revision(TENANT), 1)
        owner.commit(TENANT, plan, True, identity())
        self.rejected(lambda: owner.commit(TENANT, plan, True, identity()))

    def test_tenant_and_thread_boundaries_are_controlled(self):
        owner = self.create()
        self.rejected(lambda: owner.head("synthetic-other-tenant"), "wrong_tenant")
        self.rejected(lambda: owner.close("synthetic-other-tenant"), "wrong_tenant")
        result = []
        def wrong_thread():
            try:
                owner.head(TENANT)
            except BaseException as error:
                result.append(error)
        thread = threading.Thread(target=wrong_thread)
        thread.start(); thread.join()
        self.assertEqual(len(result), 1)
        self.assertIsInstance(result[0], fv.FogVaultWorkspaceError)
        self.assertEqual(result[0].code, "wrong_thread")
        self.assertEqual(owner.head(TENANT).revision, 0)

    def test_foreign_plan_and_conflicting_writer_reject(self):
        owner = self.create()
        other = self.reopen()
        text = "Email alice@example.test"
        plan = owner.prepare(TENANT, text, [finding(text)], identity())
        self.rejected(lambda: other.commit(TENANT, plan, True, identity()), "foreign_plan")
        stale = other.prepare(TENANT, text, [finding(text)], identity())
        owner.commit(TENANT, plan, True, identity())
        self.rejected(lambda: other.commit(TENANT, stale, True, identity()), "Conflict")
        self.rejected(lambda: other.restore(TENANT, "plain"), "owner_closed")

    def test_wrong_password_identity_and_full_head_reject(self):
        owner = self.create()
        head = owner.head(TENANT)
        owner.close(TENANT)
        self.rejected(lambda: fv.NativeWorkspace.open_existing(TENANT, self.control, self.vault,
            b"wrong synthetic password", self.record, self.namespace), "Authentication")
        self.rejected(lambda: fv.NativeWorkspace.open_existing(TENANT, self.control, self.vault,
            PASSWORD, self.record, identity()), "binding_mismatch")
        self.rejected(lambda: self.reopen(expected_head=fv.WorkspaceHead(head.revision, identity())), "binding_mismatch")

    def test_create_never_reopens_or_overwrites_existing_paths(self):
        owner = self.create()
        saved, _, _ = self.committed(owner)
        owner.close(TENANT)
        self.rejected(lambda: self.create(), "Exists")
        reopened = self.reopen()
        self.assertEqual(reopened.restore(TENANT, saved["previews"][0]), "🦀 Email alice@example.test")

    def test_heads_are_lossless_python_ints_and_strict_bytes(self):
        value = fv.WorkspaceHead(2**64-1, identity())
        self.assertIs(type(value.revision), int)
        self.assertEqual(value.revision, 2**64-1)
        self.assertIs(type(value.commit_id), bytes)
        for bad in [True, 1.0, "1", -1, 2**64]:
            self.rejected(lambda: fv.WorkspaceHead(bad, identity()), "invalid_head")

    def test_streams_revoke_on_lock_and_saved_head_change(self):
        owner = self.create()
        saved, _, _ = self.committed(owner)
        stream = owner.restore_stream(TENANT)
        self.assertIsNone(stream.feed(TENANT, saved["previews"][0].encode()))
        self.assertEqual(stream.finalize(TENANT)["text"], "🦀 Email alice@example.test")
        stale = owner.restore_stream(TENANT)
        self.committed(owner, "Other alice@example.test")
        # A same-head no-op preparation may preserve streams by Core contract.
        text = "Email alice@example.test and bob@example.test"
        f = {"entityType":"EMAIL", "matchedText":"bob@example.test", "range":{"unit":"utf8_bytes", "start":text.index("bob"), "end":len(text)}}
        changed = owner.prepare(TENANT, text, [f], identity())
        owner.commit(TENANT, changed, True, identity())
        self.rejected(lambda: stale.feed(TENANT, b"plain"))
        active = owner.restore_stream(TENANT)
        owner.lock(TENANT)
        self.rejected(lambda: active.feed(TENANT, b"plain"), "owner_closed")

    def test_owner_gc_revokes_surviving_children(self):
        owner = self.create()
        text = "Email alice@example.test"
        plan = owner.prepare(TENANT, text, [finding(text)], identity())
        stream = owner.restore_stream(TENANT)
        self.owners.remove(owner)
        del owner
        gc.collect()
        self.rejected(lambda: plan.previews(TENANT), "owner_closed")
        self.rejected(lambda: stream.feed(TENANT, b"plain"), "owner_closed")

    def test_conversation_pending_then_observed_send_remains_explicit(self):
        owner = self.create()
        logical = identity(); text = "Email alice@example.test"
        plan = owner.prepare_conversation_send(TENANT, [{"input":{"text":text,"scannerFindings":[finding(text)],"userSelections":[]},
            "conversation":{"id":logical,"surface":"synthetic-no-network","siteId":None,"siteIdByteBudget":64}}], {"id":identity(),"occurredAtMs":1})
        intent = plan.send_intents(TENANT)[0]
        saved = owner.commit(TENANT, plan, True, identity())
        self.assertEqual(owner.send_intent_status(TENANT, intent), "pending")
        self.rejected(lambda: owner.restore_in_conversation(TENANT, saved["previews"][0], logical), "conversation_pending")
        owner.close(TENANT); owner = self.reopen()
        self.assertEqual(owner.send_intent_status(TENANT, intent), "pending")
        observed = owner.prepare_observed_send(TENANT, intent, logical, "synthetic-no-network", "synthetic-chat", {"id":identity(),"occurredAtMs":2})
        owner.commit(TENANT, observed, True, identity())
        self.assertEqual(owner.restore_in_conversation(TENANT, saved["previews"][0], logical), text)
        self.rejected(lambda: owner.restore_in_conversation(TENANT, saved["previews"][0], identity()))

    def test_progressive_failure_has_no_failed_call_text(self):
        owner = self.create()
        stream = owner.restore_stream(TENANT, progressive=True)
        result = stream.feed(TENANT, b"ordinary ")
        self.assertFalse(result["complete"])
        error = self.rejected(lambda: stream.feed(TENANT, b"\xff"))
        self.assertIs(error.complete, False)
        self.assertIn("bytes", error.delivered)
        self.assertFalse(hasattr(error, "text"))

    def test_unsupported_profile_and_malformed_transport_fail_before_creation(self):
        self.rejected(lambda: self.create(profile="password_and_device"), "unsupported_profile")
        self.assertFalse(Path(self.control).exists())
        owner = self.create()
        bad = [{"text":"SECRET synthetic text", "unexpected":"SECRET"}]
        error = self.rejected(lambda: owner.prepare_review(TENANT, bad, identity()), "invalid_input")
        self.assertNotIn("SECRET", str(error))
        self.assertEqual(owner.record_revision(TENANT), 1)

    def test_reports_counts_and_policy_delegate_to_core(self):
        owner = self.create()
        self.assertEqual(owner.counts(TENANT)["operations"], 1)
        self.assertEqual(owner.operations(TENANT)[0]["kind"], "create")
        self.assertEqual(owner.history_coverage(TENANT)["recordedHistoryOnly"], True)
        self.assertIn("rules", owner.policy(TENANT))
        report = owner.restore_report(TENANT, "[[EMAIL_UNKNOWN]]")
        self.assertIsNone(report["text"])
        self.assertTrue(report["issues"])

    def test_read_only_review_recall_batches_and_policy_preview(self):
        owner = self.create()
        text = "Email alice@example.test"
        reviewed = owner.review(TENANT, {"text": text, "scannerFindings": [finding(text)], "userSelections": []})
        self.assertTrue(reviewed["candidates"])
        self.assertEqual(owner.record_revision(TENANT), 1)
        batch = owner.prepare_batch(TENANT, [{"text": text, "findings": [finding(text)]},
            {"text": text, "findings": [finding(text)]}], identity())
        saved = owner.commit(TENANT, batch, True, identity())
        self.assertEqual(len(saved["previews"]), 2)
        self.assertTrue(owner.recall(TENANT, "ALICE@example.test")["findings"])
        policy = owner.prepare_policy_changes(TENANT, {"rules": [{"entityType": "OPAQUE_TAG", "rule": "keep"}], "choices": []}, identity())
        self.assertIsNotNone(policy.policy_preview(TENANT))
        policy.discard(TENANT)

    def test_authenticated_recovery_uses_new_destination_and_preserves_source(self):
        fixture = Path(__file__).resolve().parents[3] / "fixtures/workspace/backup-current-fields.json"
        gold = json.loads(fixture.read_text(encoding="utf-8"))["current"]
        raw = bytes.fromhex(gold["bundleHex"])
        source_head = fv.WorkspaceHead(int(gold["head"]["revision"]), bytes(gold["head"]["commitId"]))
        args = (TENANT, self.control, self.vault, PASSWORD, self.record, raw, bytes([41])*16,
                source_head, identity(), identity())
        self.rejected(lambda: fv.NativeWorkspace.import_authenticated_bundle(*args, False), "approval_required")
        self.assertFalse(Path(self.control).exists())
        owner = fv.NativeWorkspace.import_authenticated_bundle(*args, True)
        self.owners.append(owner)
        self.assertEqual(owner.head(TENANT).revision, source_head.revision + 1)
        for alias in gold["aliases"]:
            self.assertEqual(owner.restore(TENANT, alias["marker"]), alias["value"])
        self.assertEqual(raw, bytes.fromhex(gold["bundleHex"]))
        self.rejected(lambda: fv.NativeWorkspace.import_authenticated_bundle(*args, True), "Exists")

    def test_restart_reconciliation_returns_only_receipt(self):
        owner = self.create()
        saved, _, _ = self.committed(owner)
        request = bytes(saved["receipt"]["requestId"])
        owner.close(TENANT)
        receipt = fv.NativeWorkspace.reconcile_existing(TENANT, self.control, self.vault, request)
        self.assertEqual(bytes(receipt["requestId"]), request)
        self.assertEqual(set(receipt), {"generation", "requestId", "headDigest"})
        reopened = self.reopen()
        self.assertEqual(reopened.restore(TENANT, saved["previews"][0]), "🦀 Email alice@example.test")

    def test_malformed_feed_transport_terminates_response_and_utf16_is_exact(self):
        owner = self.create()
        stream = owner.restore_stream(TENANT, progressive=True)
        delivered = stream.feed(TENANT, b"ordinary ")["delivered"]
        error = self.rejected(lambda: stream.feed(TENANT, "not bytes"), "invalid_encoding")
        self.assertEqual(error.delivered, delivered)
        self.assertIs(error.complete, False)
        self.rejected(lambda: stream.feed(TENANT, b"later"))
        stream.reset(TENANT)
        self.assertFalse(stream.feed(TENANT, b"fresh ")["complete"])
        stream.close(TENANT)
        strict = owner.restore_stream(TENANT, "utf16")
        text = "🦀 ordinary"
        encoded = text.encode("utf-16-le")
        units = [int.from_bytes(encoded[i:i+2], "little") for i in range(0, len(encoded), 2)]
        strict.feed_utf16(TENANT, units)
        self.assertEqual(strict.finalize(TENANT)["text"], text)
        for malformed in [[True], [1.0], [65536], [-1], "units"]:
            bad = owner.restore_stream(TENANT, "utf16")
            self.rejected(lambda: bad.feed_utf16(TENANT, malformed), "invalid_encoding")
            self.rejected(lambda: bad.finalize(TENANT))

if __name__ == "__main__":
    import argparse
    import sys
    import time
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--report", type=Path)
    args = parser.parse_args()
    if args.report and args.report.exists():
        raise SystemExit("Use a fresh installed-test report path")
    cases = []
    class EvidenceResult(unittest.TextTestResult):
        def startTest(self, test):
            self.started = time.monotonic()
            super().startTest(test)
        def record(self, test, status, executed=True):
            cases.append({"id":test._testMethodName,"status":status,"executed":executed,
                          "duration_seconds":time.monotonic()-self.started})
        def addSuccess(self,test):
            self.record(test,"pass"); super().addSuccess(test)
        def addFailure(self,test,error):
            self.record(test,"fail"); super().addFailure(test,error)
        def addError(self,test,error):
            self.record(test,"error"); super().addError(test,error)
        def addSkip(self,test,reason):
            self.record(test,"skip",False); super().addSkip(test,reason)
    result = unittest.TextTestRunner(verbosity=2,resultclass=EvidenceResult).run(
        unittest.defaultTestLoader.loadTestsFromTestCase(InstalledOwnerTests))
    passed = result.wasSuccessful() and not result.skipped and len(cases)==result.testsRun
    if args.report:
        args.report.parent.mkdir(parents=True,exist_ok=True)
        args.report.write_text(json.dumps({"schema":"datafog-fogvault-python-cases/v1",
            "pass":passed,"executed":True,"cases":cases,"module_path":fv.__file__,
            "python":sys.version,"tests_run":result.testsRun},indent=2)+"\n",encoding="utf-8")
    raise SystemExit(0 if passed else 1)
