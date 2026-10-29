"""Mandatory operation profile against the actual installed native workspace."""
from __future__ import annotations
import json
from pathlib import Path
import unittest
from datafog_core import DataFogWorkspaceError, Workspace, WorkspaceHead, WorkspaceInput, WorkspaceReviewInput, WorkspaceFinding, TextRange

ROOT = Path(__file__).resolve().parents[3]
MAX_TIME = 253402300799999

def public_id(value): return bytes([value]) * 16

def context(value, time=0): return {"id": public_id(value), "occurredAtMs": time}

def finding(text="Jane"):
    return WorkspaceFinding("PERSON", text, TextRange(0, len(text.encode())))

def save(workspace, plan):
    candidate = plan.export_candidate_bundle()
    with Workspace.open_bundle(candidate) as opened:
        assert opened.head() == plan.candidate_head()
    workspace.activate_saved(plan, plan.expected_head(), plan.candidate_head())
    return candidate

class WorkspaceOperationTests(unittest.TestCase):
    def setUp(self): self.workspace = Workspace.create_with_operation(public_id(201), context(202))
    def tearDown(self): self.workspace.close()
    def error(self, code, call):
        with self.assertRaises(DataFogWorkspaceError) as caught: call()
        self.assertEqual((caught.exception.code, str(caught.exception)), (code, code))
        return caught.exception

    def test_all_operation_methods_are_native_and_creation_records_only_known_time(self):
        w = self.workspace
        for name in ("create_with_operation", "operations", "operations_page", "counts", "history_coverage", "prepare_with_operation", "prepare_batch_with_operation", "prepare_recovery_with_operation", "prepare_outcome"):
            self.assertTrue(callable(getattr(Workspace, name)))
        self.assertEqual(w.operations(), [{"id": list(public_id(202)), "kind": "create", "occurredAtMs": 0, "preparedInputs": 0, "transformedSpans": 0}])
        self.assertEqual(w.counts(), {"operations": 1, "preparedInputs": 0, "transformedSpans": 0, "confirmedHandoffs": 0, "reportedBlockedAttempts": 0})
        self.assertEqual(w.history_coverage(), {"recordedHistoryOnly": True, "preUpgradeHistory": "not_applicable"})
        with Workspace.create(public_id(1), public_id(2)) as legacy_create:
            self.assertIsNone(legacy_create.operations()[0]["occurredAtMs"])
        with Workspace.create_with_operation(public_id(3), context(4, MAX_TIME)) as latest:
            self.assertEqual(latest.operations()[0]["occurredAtMs"], MAX_TIME)
        self.error("invalid_operation", lambda: Workspace.create_with_operation(public_id(3), context(4, MAX_TIME + 1)))

    def test_batch_candidate_failed_save_activation_and_reopen_accounting(self):
        w = self.workspace; before = w.export_bundle(); head = w.head()
        plan = w.prepare_batch_with_operation([WorkspaceInput("👋 Jane", [WorkspaceFinding("PERSON", "Jane", TextRange(5, 9))]), WorkspaceInput("ordinary", [])], context(203, MAX_TIME))
        marker_text = plan.previews()[0]["text"]
        with Workspace.open_bundle(plan.export_candidate_bundle()) as pending:
            self.assertEqual(pending.counts()["operations"], 2)
            self.assertEqual(pending.counts()["preparedInputs"], 2)
            self.assertEqual(pending.counts()["transformedSpans"], 1)
            self.assertEqual(pending.operations()[-1]["occurredAtMs"], MAX_TIME)
            self.assertEqual(pending.restore(marker_text), "👋 Jane")
        self.assertEqual((w.export_bundle(), w.head()), (before, head))
        self.error("workspace_conflict", lambda: w.activate_saved(plan, head, WorkspaceHead(999, public_id(203))))
        self.assertEqual(w.export_bundle(), before)
        saved = save(w, plan)
        self.assertEqual(w.counts()["operations"], 2)
        self.error("inactive_plan", plan.preview)
        with Workspace.open_bundle(saved) as reopened:
            self.assertEqual(reopened.counts(), w.counts())
            self.assertEqual(reopened.operations(), w.operations())
            self.assertEqual(reopened.restore(marker_text), "👋 Jane")
            self.error("operation_reused", lambda: reopened.prepare_with_operation("", [], context(203)))

    def test_explicit_nochange_preparation_is_staged_legacy_noop_and_discard_stay_inert(self):
        w = self.workspace; before = w.export_bundle(); head = w.head()
        legacy = w.prepare("ordinary", [], public_id(203))
        self.assertEqual(legacy.export_candidate_bundle(), before)
        self.assertEqual(legacy.expected_head(), legacy.candidate_head())
        save(w, legacy)
        explicit = w.prepare_with_operation("ordinary", [], context(203, 1))
        self.assertNotEqual(explicit.expected_head(), explicit.candidate_head())
        self.assertEqual(w.export_bundle(), before)
        explicit.discard()
        self.assertEqual((w.export_bundle(), w.head()), (before, head))
        saved = save(w, w.prepare_with_operation("ordinary", [], context(203, 0)))
        self.assertNotEqual(saved, before)
        self.assertEqual(w.counts()["preparedInputs"], 1)
        self.assertEqual(w.counts()["transformedSpans"], 0)
        self.assertEqual(w.operations()[-1]["occurredAtMs"], 0)
        self.assertEqual(w.history_coverage()["preUpgradeHistory"], "not_applicable")

    def test_explicit_outcomes_are_content_free_host_assertions_without_delivery_authority(self):
        w = self.workspace
        for number, kind, count in ((203, "copy_confirmed", 0), (204, "user_cancelled", 0), (205, "blocked_input", 10000), (206, "restore_blocked", 2)):
            before = w.export_bundle(); previous = w.counts()
            plan = w.prepare_outcome({"kind": kind, "issueCount": count}, context(number, 3))
            self.assertEqual(plan.previews(), [])
            self.error("invalid_batch", plan.preview)
            self.assertEqual((w.export_bundle(), w.counts()), (before, previous))
            save(w, plan)
        self.assertEqual(w.counts(), {"operations": 5, "preparedInputs": 0, "transformedSpans": 0, "confirmedHandoffs": 0, "reportedBlockedAttempts": 2})
        for record in w.operations():
            self.assertEqual(set(record), {"id", "kind", "occurredAtMs", "preparedInputs", "transformedSpans"})
        self.assertNotIn("issueCount", json.dumps(w.operations()))

    def test_v1_v2_reads_noops_and_upgrade_keep_unknown_history_aliases_and_choices(self):
        counter = next(iter(json.loads((ROOT / "fixtures/workspace/counters.json").read_text()).values()))
        policy = json.loads((ROOT / "fixtures/workspace/policy-v2.json").read_text())
        for fixture in (counter, policy):
            raw = bytes.fromhex(fixture["bundleHex"])
            with self.subTest(version=fixture.get("version")), Workspace.open_bundle(raw) as w:
                before_head = w.head()
                self.assertEqual(w.operations(), [])
                self.assertEqual(w.counts()["operations"], 0)
                self.assertEqual(w.history_coverage(), {"recordedHistoryOnly": True, "preUpgradeHistory": "unknown"})
                self.assertEqual(w.export_bundle(), raw)
                with w.prepare("ordinary", [], public_id(210)) as no_op:
                    self.assertEqual(no_op.export_candidate_bundle(), raw)
                abandoned = w.prepare_outcome({"kind": "user_cancelled", "issueCount": 0}, context(211))
                abandoned.discard()
                self.assertEqual((w.export_bundle(), w.head()), (raw, before_head))
                plan = w.prepare_outcome({"kind": "user_cancelled", "issueCount": 0}, context(211))
                self.assertEqual(plan.snapshot_format(), {"currentVersion": 2 if fixture is policy else 1, "candidateVersion": 3})
                saved = save(w, plan)
                self.assertEqual(w.history_coverage()["preUpgradeHistory"], "unknown")
                self.assertEqual(w.operations()[0]["kind"], "user_cancelled")
                with Workspace.open_bundle(saved) as reopened:
                    self.assertEqual(reopened.history_coverage(), w.history_coverage())
                    if fixture is policy:
                        self.assertEqual(reopened.policy()["choices"][0]["value"], "May")
                    self.assertEqual(reopened.restore(fixture["marker"]), "Jane")

    def test_legacy_recorded_mutations_and_explicit_recovery_keep_unknown_time(self):
        w = self.workspace
        plan = w.prepare("Jane", [finding()], public_id(203)); marker = plan.preview()["text"]; save(w, plan)
        self.assertIsNone(w.operations()[-1]["occurredAtMs"])
        self.assertEqual(w.operations()[-1]["preparedInputs"], 1)
        before = w.export_bundle()
        recovery = w.prepare_recovery_with_operation(context(204, 1))
        self.assertEqual(recovery.previews(), [])
        self.assertEqual(w.export_bundle(), before)
        save(w, recovery)
        self.assertEqual(w.operations()[-1]["kind"], "recovery")
        self.assertEqual(w.operations()[-1]["occurredAtMs"], 1)
        self.assertEqual(w.restore(marker), "Jane")
        self.assertEqual(w.counts()["transformedSpans"], 1)
        self.error("operation_reused", lambda: w.prepare_recovery_with_operation(context(203)))

    def test_strict_context_outcome_transport_rejects_callbacks_and_bad_numbers_without_mutation(self):
        w = self.workspace; before = w.export_bundle(); calls = []
        class CustomInt:
            def __index__(self): calls.append("index"); return 1
        class CustomString:
            def __str__(self): calls.append("str"); return "copy_confirmed"
        class Mapping(dict):
            def __getitem__(self, key): calls.append("getitem"); return super().__getitem__(key)
        bad_contexts = [None, {}, {"id": public_id(203)}, {**context(203), "text": "forbidden"}, Mapping(context(203))]
        bad_contexts += [context(203, value) for value in (-1, .5, float("nan"), float("inf"), MAX_TIME + 1, "0", None, True, CustomInt())]
        for value in bad_contexts:
            with self.subTest(context_type=type(value).__name__):
                self.error("invalid_operation", lambda: w.prepare_with_operation("", [], value))
        for value in (b"", bytes(15), bytes(17), bytearray(16), list(range(16)), "0" * 16):
            self.error("invalid_operation_id", lambda: w.prepare_with_operation("", [], {"id": value, "occurredAtMs": 0}))
        for value in (None, {}, {"kind": "blocked_input", "issueCount": 0}, {"kind": "copy_confirmed", "issueCount": 1}, {"kind": "user_cancelled", "issueCount": 1}, {"kind": "restore_blocked", "issueCount": 10001}, {"kind": "sent", "issueCount": 0}, {"kind": CustomString(), "issueCount": 0}, {"kind": "copy_confirmed", "issueCount": CustomInt()}, {"kind": "copy_confirmed", "issueCount": False}, {"kind": "copy_confirmed", "issueCount": 0, "text": "forbidden"}):
            self.error("invalid_operation", lambda: w.prepare_outcome(value, context(203)))
        self.assertEqual(calls, [])
        self.assertEqual(w.export_bundle(), before)

    def test_owned_pages_are_explicit_complete_bounded_reads_without_hidden_writes(self):
        w = self.workspace
        for value in range(203, 210): save(w, w.prepare_outcome({"kind": "user_cancelled", "issueCount": 0}, context(value)))
        before = w.export_bundle(); full = w.operations(); head = w.head(); rows = []; offset = 0
        while True:
            page = w.operations_page(offset, 3)
            self.assertEqual((page["offset"], page["total"]), (offset, len(full)))
            rows += page["records"]
            if page["nextOffset"] is None: break
            self.assertEqual(page["nextOffset"], offset + len(page["records"]))
            offset = page["nextOffset"]
        self.assertEqual(rows, full)
        self.assertEqual(w.operations_page(len(full), 100000), {"records": [], "total": len(full), "offset": len(full), "nextOffset": None})
        page = w.operations_page(0, 1); page["records"][0]["id"][0] = 0; page["records"][0]["occurredAtMs"] = None
        self.assertEqual(w.operations_page(0, 1)["records"], full[:1])
        calls = []
        class Index:
            def __index__(self): calls.append("index"); return 1
        for offset, limit in ((len(full)+1, 1), (0, 0), (0, 100001), (-1, 1), (0, -1), (0, .5), (True, 1), (0, False), (2**100, 1), (0, Index())):
            self.error("resource_limit", lambda: w.operations_page(offset, limit))
        self.assertEqual(calls, [])
        self.assertEqual((w.export_bundle(), w.head(), w.operations()), (before, head, full))
        fixture = json.loads((ROOT / "fixtures/workspace/policy-v2.json").read_text()); raw = bytes.fromhex(fixture["bundleHex"])
        with Workspace.open_bundle(raw) as old:
            self.assertEqual(old.operations_page(0, 1), {"records": [], "total": 0, "offset": 0, "nextOffset": None})
            self.assertEqual(old.export_bundle(), raw)
        w.close(); self.error("closed_workspace", lambda: w.operations_page(0, 1))

    def test_history_only_replays_preserve_legacy_error_codes_and_explicit_distinction(self):
        w = self.workspace
        for value in (203, 204): save(w, w.prepare_outcome({"kind": "user_cancelled", "issueCount": 0}, context(value)))
        before = w.export_bundle(); head = w.head(); counts = w.counts()
        operation_id = public_id(203)
        legacy_calls = (
            lambda: w.prepare("", [], operation_id),
            lambda: w.prepare_batch([WorkspaceInput("", [])], operation_id),
            lambda: w.prepare_review([WorkspaceReviewInput("")], operation_id),
            lambda: w.prepare_recovery(operation_id),
            lambda: w.prepare_policy_changes({"rules": [{"entityType": "CUSTOM", "rule": "hide"}], "choices": []}, operation_id),
        )
        for call in legacy_calls: self.error("invalid_operation_id", call)
        explicit_calls = (
            lambda: w.prepare_with_operation("", [], context(203)),
            lambda: w.prepare_batch_with_operation([WorkspaceInput("", [])], context(203)),
            lambda: w.prepare_recovery_with_operation(context(203)),
            lambda: w.prepare_outcome({"kind": "user_cancelled", "issueCount": 0}, context(203)),
        )
        for call in explicit_calls: self.error("operation_reused", call)
        self.assertEqual((w.export_bundle(), w.head(), w.counts()), (before, head, counts))

    def test_validation_indices_owned_reads_plan_slots_and_close_lifecycle(self):
        w = self.workspace; before = w.export_bundle()
        wrong = WorkspaceFinding("PERSON", "wrong", TextRange(0, 4))
        error = self.error("invalid_finding", lambda: w.prepare_batch_with_operation([WorkspaceInput("Jane", [finding()]), WorkspaceInput("Jane", [wrong])], context(203)))
        self.assertEqual((error.input_index, error.finding_index), (1, 0))
        self.error("invalid_batch", lambda: w.prepare_batch_with_operation([], context(203)))
        self.assertEqual(w.export_bundle(), before)
        copies = w.operations(); copies[0]["id"][0] = 0; copies[0]["occurredAtMs"] = 1
        w.counts()["operations"] = 999
        w.history_coverage()["preUpgradeHistory"] = "unknown"
        self.assertEqual(w.operations()[0]["id"], list(public_id(202)))
        self.assertEqual(w.operations()[0]["occurredAtMs"], 0)
        self.assertEqual(w.counts()["operations"], 1)
        plans = [w.prepare_outcome({"kind": "user_cancelled", "issueCount": 0}, context(203)) for _ in range(32)]
        self.error("resource_limit", lambda: w.prepare_with_operation("", [], context(204)))
        plans.pop().discard()
        fresh = w.prepare_with_operation("", [], context(204)); save(w, fresh)
        for plan in plans: self.error("inactive_plan", plan.previews)
        self.assertEqual(w.counts()["operations"], 2)
        closed_plan = w.prepare_recovery_with_operation(context(205))
        w.close()
        for call in (w.operations, w.counts, w.history_coverage, closed_plan.previews): self.error("closed_workspace", call)


def verify_operations():
    result = unittest.TextTestRunner(verbosity=2).run(unittest.defaultTestLoader.loadTestsFromTestCase(WorkspaceOperationTests))
    if not result.wasSuccessful(): raise AssertionError("native workspace operation profile failed")

if __name__ == "__main__": verify_operations()
