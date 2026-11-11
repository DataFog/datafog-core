import unittest
import json
import tempfile
from pathlib import Path

from analyze import MIN_INDEPENDENT_TRIALS_FOR_PERCENTILES, REQUIRED_GATE_NAMES, ReceiptError, load_receipts, summarize


def receipt(trial, profile, *, admission="admitted", gate="pass", duration=10.0, exclusion=None):
    result = {
        "runKind": "final", "runId": "run-1", "scenarioId": "scenario-1", "backend": "sqlite",
        "trialId": trial, "profileId": profile,
        "admission": {"status": admission, "reason": "limit" if admission == "rejected" else None},
        "correctness": {"gates": {
            name: {"status": gate if name == "exact_restore_after_browser_restart" else "pass"}
            for name in REQUIRED_GATE_NAMES
        }},
        "timings": {"restore": {"durationMs": duration, "status": "measured"}},
    }
    if exclusion:
        result["exclusionReason"] = exclusion
    return result


class AnalyzerTests(unittest.TestCase):
    def test_interpolated_percentiles_use_independent_trial_durations(self):
        self.assertEqual(MIN_INDEPENDENT_TRIALS_FOR_PERCENTILES, 100)
        rows = summarize([receipt(f"t{i}", f"p{i}", duration=i) for i in range(100)])
        interval = rows[0]["intervals"][0]
        self.assertEqual(interval["n"], 100)
        self.assertEqual(interval["p50Ms"], 49.5)
        self.assertAlmostEqual(interval["p95Ms"], 94.05)
        self.assertEqual(interval["percentileStatus"], "reported")
        self.assertEqual(rows[0]["failureRate"], 0)

    def test_failed_unsupported_and_rejected_trials_are_retained_but_not_timed(self):
        rows = summarize([
            receipt("t1", "p1", duration=1),
            receipt("t2", "p2", gate="unsupported", duration=100),
            receipt("t3", "p3", admission="rejected", duration=200),
            receipt("t4", "p4", exclusion="operator marked trial incomplete", duration=300),
        ])
        row = rows[0]
        self.assertEqual(row["trialCount"], 4)
        self.assertEqual(row["failedTrialCount"], 1)
        self.assertEqual(row["failureRate"], 0.25)
        self.assertEqual(row["unsupportedTrialCount"], 1)
        self.assertEqual(row["unsupportedRate"], 0.25)
        self.assertEqual(row["excludedTrialCount"], 1)
        self.assertEqual(row["excludedRate"], 0.25)
        self.assertEqual(row["eligibleTrialCount"], 1)
        self.assertEqual(row["intervals"][0]["n"], 1)
        self.assertIsNone(row["intervals"][0]["p50Ms"])
        self.assertIsNone(row["intervals"][0]["p95Ms"])
        self.assertEqual(row["intervals"][0]["percentileStatus"], "exploratory_insufficient_independent_trials")
        self.assertEqual(len(row["exclusions"]), 3)
        self.assertEqual({item["classification"] for item in row["exclusions"]}, {"failed", "unsupported", "excluded"})

    def test_profile_reuse_is_rejected_as_non_independent(self):
        one = receipt("t1", "p1")
        two = receipt("t2", "p1")
        with self.assertRaisesRegex(ReceiptError, "profileId reused"):
            # The loader checks this before summarize; express the same invariant here
            # so an adapter cannot silently flatten correlated rows into extra samples.
            from analyze import validate_independent_profiles
            validate_independent_profiles([one, two])

    def test_loader_fails_closed_when_a_required_correctness_gate_is_missing(self):
        item = receipt("t1", "p1")
        del item["correctness"]["gates"]["backup_recovery_preserves_mappings_and_authority"]
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "receipts.jsonl"
            path.write_text(json.dumps(item) + "\n", encoding="utf-8")
            with self.assertRaisesRegex(ReceiptError, "missing required correctness gates"):
                load_receipts([path], "final")


if __name__ == "__main__":
    unittest.main()
