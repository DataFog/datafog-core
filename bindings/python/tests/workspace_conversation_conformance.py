"""Mandatory native counterpart of the actual shared conversation SDK corpus."""
import unittest
from datafog_core import Workspace, WorkspaceReviewInput, WorkspaceFinding, TextRange, DataFogWorkspaceError


def identifier(value): return bytes([value]) * 16
def operation(value): return {"id": identifier(value), "occurredAtMs": value}
def context(value, site=None): return {"id": identifier(value), "surface": "synthetic_chat", "siteId": site, "siteIdByteBudget": 128}
def review(text, mark=False):
    return WorkspaceReviewInput(text, user_selections=[WorkspaceFinding("PERSON", text, TextRange(0, len(text.encode())))] if mark else [])
def save(workspace, plan):
    with Workspace.open_bundle(plan.export_candidate_bundle()) as opened:
        assert opened.head() == plan.candidate_head()
    workspace.activate_saved(plan, plan.expected_head(), plan.candidate_head())


class ConversationTests(unittest.TestCase):
    def setUp(self): self.workspace = Workspace.create_with_operation(identifier(1), operation(2))
    def tearDown(self): self.workspace.close()
    def error(self, code, call):
        with self.assertRaises(DataFogWorkspaceError) as caught: call()
        self.assertEqual((caught.exception.code, str(caught.exception)), (code, code))
        return caught.exception
    def send(self, logical, text, preparation, confirmation, site):
        w = self.workspace
        with w.prepare_conversation_send([{"input": review(text, True), "conversation": context(logical)}], operation(preparation)) as plan:
            marker, intent = plan.preview()["text"], plan.send_intents()[0]
            save(w, plan)
        with w.prepare_observed_send(intent, identifier(logical), "synthetic_chat", site, operation(confirmation)) as plan: save(w, plan)
        return marker, intent

    def test_advertised_methods_and_partial_batch_whole_chat_pause_persisted_resolution(self):
        w = self.workspace
        for method in ("prepare_conversation_send", "conversation", "conversation_for_site", "send_intent_status", "prepare_observed_send", "prepare_definitely_not_sent", "restore_in_conversation", "restore_report_in_conversation", "buffered_restore_in_conversation", "progressive_restore_in_conversation"):
            self.assertTrue(callable(getattr(w, method)))
        definition = {"input": review("Synthetic 👋 Person", True), "conversation": context(10)}
        with w.prepare_conversation_send([definition, definition], operation(3)) as plan:
            intents = plan.send_intents(); marker = plan.previews()[0]["text"]
            self.assertEqual([item["inputIndex"] for item in intents], [0, 1])
            self.error("conversation_unknown", lambda: w.conversation(identifier(10)))
            self.assertEqual(w.counts()["confirmedHandoffs"], 0)
            save(w, plan)
        self.assertEqual(w.conversation(identifier(10))["pendingIntents"], 2)
        for text in ("plain", marker): self.error("conversation_pending", lambda: w.restore_in_conversation(text, identifier(10)))
        for method in (w.buffered_restore_in_conversation, w.progressive_restore_in_conversation): self.error("conversation_pending", lambda: method(identifier(10)))
        self.assertEqual(w.restore(marker), "Synthetic 👋 Person")
        before = w.export_bundle()
        self.error("invalid_send_intent", lambda: w.prepare_observed_send(intents[0], identifier(11), "synthetic_chat", "thread_a", operation(4)))
        self.error("invalid_conversation", lambda: w.prepare_observed_send(intents[0], identifier(10), "synthetic_chat", "a" * 129, operation(4)))
        self.assertEqual(w.export_bundle(), before)
        with w.prepare_observed_send(intents[0], identifier(10), "synthetic_chat", "thread_a", operation(4)): pass
        self.assertEqual(w.send_intent_status(intents[0]), "pending")
        with w.prepare_observed_send(intents[0], identifier(10), "synthetic_chat", "thread_a", operation(5)) as plan: save(w, plan)
        self.assertEqual(w.counts()["confirmedHandoffs"], 1)
        self.error("conversation_pending", lambda: w.restore_in_conversation(marker, identifier(10)))
        raw = w.export_bundle(); w.close(); self.workspace = w = Workspace.open_bundle(raw)
        self.assertEqual([w.send_intent_status(item) for item in intents], ["observed_sent", "pending"])
        with w.prepare_recovery_with_operation(operation(6)) as plan: save(w, plan)
        self.error("conversation_pending", lambda: w.restore_in_conversation("old plain reply", identifier(10)))
        with w.prepare_observed_send(intents[1], identifier(10), "synthetic_chat", "thread_a", operation(7)) as plan: save(w, plan)
        self.assertEqual(w.counts()["confirmedHandoffs"], 2)
        self.assertEqual(w.conversation(identifier(10))["issuedMarkers"], 1)
        self.assertEqual(w.conversation_for_site("synthetic_chat", "thread_a"), identifier(10))
        self.assertEqual(w.restore_in_conversation(marker, identifier(10)), "Synthetic 👋 Person")
        self.error("invalid_send_intent", lambda: w.prepare_observed_send(intents[1], identifier(10), "synthetic_chat", "thread_a", operation(8)))

    def test_scoped_unicode_atomic_and_progressive_restoration_with_owned_views(self):
        w = self.workspace
        marker, intent = self.send(10, "Synthetic 👋 Person", 3, 4, "thread_a")
        other, _ = self.send(11, "Second Synthetic", 5, 6, "thread_b")
        state = w.conversation(identifier(10)); state["pendingIntents"] = 999; state["conversation"]["id"][0] = 255
        self.assertEqual(w.conversation(identifier(10))["pendingIntents"], 0)
        self.assertEqual(w.send_intent_status(intent), "observed_sent")
        text = "👋 " + other
        report = w.restore_report_in_conversation(text, identifier(10))
        self.assertIsNone(report["text"]); issue = report["issues"][0]
        self.assertEqual(issue["kind"], "not_issued_in_conversation")
        self.assertEqual([issue[key]["start"] for key in ("byteRange", "codepointRange", "utf16Range")], [5, 2, 3])
        with w.buffered_restore_in_conversation(identifier(10)) as buffered:
            buffered.feed((marker + " " + other).encode())
            self.assertIsNone(buffered.finalize()["text"])
        with w.progressive_restore_in_conversation(identifier(10)) as stream:
            safe = stream.feed("safe 👋 ".encode())
            failure = self.error("invalid_marker", lambda: stream.feed(other.encode()))
            self.assertFalse(failure.complete); self.assertEqual(failure.delivered, safe["delivered"])
            self.assertEqual(failure.marker_issues[0]["kind"], "not_issued_in_conversation")
        for method in (w.buffered_restore_in_conversation, w.progressive_restore_in_conversation):
            with method(identifier(10), "utf16") as stream:
                raw = ("👋 " + marker).encode("utf-16-le"); units = [int.from_bytes(raw[index:index+2], "little") for index in range(0, len(raw), 2)]
                result = stream.feed_utf16(units[:1]); second = stream.feed_utf16(units[1:]); final = stream.finalize()
                output = final["text"] if result is None else result["text"] + second["text"] + final["text"]
                self.assertEqual(output, "👋 Synthetic 👋 Person")
        active = w.progressive_restore_in_conversation(identifier(10)); w.close()
        self.error("closed_workspace", lambda: active.feed(b"A")); active.close()

    def test_definite_non_delivery_resolves_only_its_saved_member_without_counting_send(self):
        w = self.workspace
        marker, _ = self.send(10, "Synthetic Person", 3, 4, "thread_a")
        with w.prepare_conversation_send([{"input": review(marker), "conversation": context(10, "thread_a")}], operation(5)) as plan:
            intent = plan.send_intents()[0]; save(w, plan)
        before = w.counts()["confirmedHandoffs"]
        self.error("conversation_pending", lambda: w.restore_in_conversation(marker, identifier(10)))
        with w.prepare_definitely_not_sent(intent, identifier(10), operation(6)) as plan: save(w, plan)
        self.assertEqual(w.send_intent_status(intent), "definitely_not_sent")
        self.assertEqual(w.counts()["confirmedHandoffs"], before)
        self.assertEqual(w.restore_in_conversation(marker, identifier(10)), "Synthetic Person")

    def test_exact_builtin_transport_rejects_callbacks_bool_extra_missing_and_owned_intents(self):
        w = self.workspace; source = review("Synthetic", True)
        class Arbitrary(dict):
            def __getitem__(self, key): raise AssertionError("private callback")
        for bad in [None, Arbitrary(context(10)), {**context(10), "extra": True}, {key:value for key,value in context(10).items() if key != "siteId"}, {**context(10), "siteIdByteBudget": True}, {**context(10), "id": [True] * 16}]:
            failure = self.error("invalid_conversation", lambda: w.prepare_conversation_send([{"input":source,"conversation":bad}], operation(3)))
            self.assertEqual(failure.input_index, 0)
        for bad in [None, {}, (), iter(())]:
            self.error("invalid_batch", lambda: w.prepare_conversation_send(bad, operation(3)))
        with w.prepare_conversation_send([{"input":source,"conversation":context(10)}], operation(3)) as plan:
            intents = plan.send_intents(); intents[0]["inputIndex"] = 31
            self.assertEqual(plan.send_intents()[0]["inputIndex"], 0)
            real = plan.send_intents()[0]; save(w, plan)
        for bad in [None, {**real,"inputIndex":True}, {**real,"inputIndex":32}, {**real,"extra":True}, {**real,"preparationId":[1.5]*16}]:
            self.error("invalid_send_intent", lambda: w.send_intent_status(bad))
        raw = w.export_bundle()
        self.error("resource_limit", lambda: w.prepare_conversation_send([{"input":source,"conversation":context(10)}] * 33, operation(4)))
        self.assertEqual(w.export_bundle(), raw)


def verify_conversations():
    result = unittest.TextTestRunner(verbosity=2).run(unittest.defaultTestLoader.loadTestsFromTestCase(ConversationTests))
    if not result.wasSuccessful(): raise AssertionError("native conversation conformance failed")

if __name__ == "__main__": verify_conversations()
