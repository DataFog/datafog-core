"""Exercise fail-closed aggregation and guard the complete workflow inventory."""

import copy
import importlib.util
import json
import io
import os
from pathlib import Path
import re
import unittest
from unittest.mock import patch
from urllib.error import URLError

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location("gate", ROOT / "scripts/check-merge-gate.py")
gate = importlib.util.module_from_spec(spec)
spec.loader.exec_module(gate)


class MergeGateTests(unittest.TestCase):
    def results(self, event="merge_group"):
        return {
            name: {"result": "success" if event == "merge_group" or name == "policy" else "skipped"}
            for name in ("policy", *gate.TECHNICAL_WORKFLOWS)
        }

    def test_complete_queue_passes(self):
        gate.validate("merge_group", self.results())

    def test_every_unsuccessful_dependency_blocks_queue(self):
        for name in self.results():
            for state in ("failure", "cancelled", "skipped", "timed_out", "neutral", None):
                with self.subTest(name=name, state=state):
                    results = self.results()
                    results[name]["result"] = state
                    with self.assertRaises(ValueError):
                        gate.validate("merge_group", results)

    def test_missing_extra_and_malformed_results_fail(self):
        variants = [None, [], {}, {**self.results(), "unknown": {"result": "success"}}]
        for name in self.results():
            missing = self.results()
            del missing[name]
            variants.append(missing)
            malformed = self.results()
            malformed[name] = "success"
            variants.append(malformed)
        for results in variants:
            with self.subTest(results=results), self.assertRaises(ValueError):
                gate.validate("merge_group", results)

    def test_pr_admission_requires_policy_and_does_not_claim_full_execution(self):
        results = self.results("pull_request")
        gate.validate("pull_request", results)
        for state in ("skipped", "failure", "cancelled"):
            invalid = copy.deepcopy(results)
            invalid["policy"]["result"] = state
            with self.assertRaises(ValueError):
                gate.validate("pull_request", invalid)
        with self.assertRaises(ValueError):
            gate.validate("push", self.results())

    def test_workflow_inventory_and_dependencies_are_complete(self):
        directory = ROOT / ".github/workflows"
        # change-scope is a scheduling helper, not an independently required check.
        excluded = {"changelog", "model-candidates", "merge-gate", "change-scope"}
        technical = {
            path.stem for path in directory.glob("*.yml")
            if path.stem not in excluded and not path.stem.startswith("publish-")
        }
        self.assertEqual(technical, set(gate.TECHNICAL_WORKFLOWS))
        workflow = (directory / "merge-gate.yml").read_text()
        dependencies = json.loads(re.search(r"^    needs: (\[.*\])$", workflow, re.MULTILINE)[1])
        self.assertEqual(set(dependencies), {"policy", *technical})
        self.assertEqual(len(dependencies), len(set(dependencies)))
        calls = re.findall(r"^    uses: \./.github/workflows/(.*)\.yml$", workflow, re.MULTILINE)
        self.assertEqual(set(calls), technical)
        self.assertEqual(len(calls), len(technical))
        self.assertIn("    if: always()\n", workflow)
        for name in technical:
            source = (directory / f"{name}.yml").read_text()
            self.assertIn("  workflow_call:\n", source, name)
            self.assertNotIn("continue-on-error:", source, name)
            self.assertIn(f"  {name}:\n    if: github.event_name == 'merge_group'\n", workflow)
            if "id: scope" in source:
                self.assertIn('if [[ "$GITHUB_EVENT_NAME" == merge_group ]]; then code=true; fi', source)


class JobVerificationTests(unittest.TestCase):
    def jobs(self):
        jobs = [
            {"id": index, "run_id": 123, "run_attempt": 2, "head_sha": "abc",
             "name": "policy" if name == "policy" else f"{name} / technical child",
             "status": "completed", "conclusion": "success"}
            for index, name in enumerate(("policy", *gate.TECHNICAL_WORKFLOWS), 1)
        ]
        jobs.append({"id": 100, "run_id": 123, "run_attempt": 2, "head_sha": "abc",
                     "name": "Full technical suite", "runner_name": "gate-runner",
                     "status": "in_progress", "conclusion": None})
        return jobs

    def check(self, jobs):
        gate.validate_jobs(jobs, 123, 2, "abc", "gate-runner")

    def test_successful_children_pass(self):
        self.check(self.jobs())

    def test_green_workflow_with_skipped_child_fails(self):
        gate.validate("merge_group", MergeGateTests().results())
        jobs = self.jobs()
        jobs.append(dict(jobs[1], id=101, name="application-vaults / another child", conclusion="skipped"))
        with self.assertRaisesRegex(ValueError, "Technical job did not succeed"):
            self.check(jobs)

    def test_every_non_success_child_and_unfinished_child_fails(self):
        for state in ("failure", "cancelled", "skipped", "neutral", "timed_out", None):
            with self.subTest(state=state), self.assertRaises(ValueError):
                jobs = self.jobs()
                jobs[1]["conclusion"] = state
                self.check(jobs)
        jobs = self.jobs()
        jobs[1]["status"] = "in_progress"
        with self.assertRaises(ValueError):
            self.check(jobs)

    def test_missing_workflow_or_gate_and_duplicate_jobs_fail(self):
        for index in range(len(self.jobs())):
            jobs = self.jobs()
            del jobs[index]
            with self.subTest(index=index), self.assertRaises(ValueError):
                self.check(jobs)
        with self.assertRaises(ValueError):
            self.check(self.jobs() + [self.jobs()[0]])
        with self.assertRaises(ValueError):
            self.check(self.jobs() + [dict(self.jobs()[-1], id=101)])

    def test_exact_gate_identity_and_attempt_are_required(self):
        for field, value in (("runner_name", "other"), ("name", "Full technical suite / other"),
                             ("status", "completed"), ("conclusion", "success")):
            jobs = self.jobs()
            jobs[-1][field] = value
            with self.subTest(field=field), self.assertRaises(ValueError):
                self.check(jobs)
        for field, value in (("run_id", 999), ("run_attempt", 1), ("head_sha", "old"),
                             ("id", None), ("name", None), ("name", "unknown / child")):
            jobs = self.jobs()
            jobs[0][field] = value
            with self.subTest(field=field), self.assertRaises(ValueError):
                self.check(jobs)

    def response(self, payload):
        return io.BytesIO(json.dumps(payload).encode())

    def test_paginated_current_attempt_api(self):
        jobs = self.jobs()
        with patch.object(gate, "urlopen", side_effect=[
            self.response({"total_count": len(jobs), "jobs": jobs[:10]}),
            self.response({"total_count": len(jobs), "jobs": jobs[10:]}),
        ]) as fetch:
            received = gate.read_jobs("owner/repo", 123, 2, "token", "https://api.github.com")
        self.check(received)
        self.assertEqual(fetch.call_count, 2)
        for page, call in enumerate(fetch.call_args_list, 1):
            self.assertEqual(call.args[0].full_url,
                             f"https://api.github.com/repos/owner/repo/actions/runs/123/attempts/2/jobs?per_page=100&page={page}")
            self.assertEqual(call.kwargs["timeout"], 30)

    def test_missing_inconsistent_and_malformed_api_data_fail(self):
        for payload in (None, [], {}, {"total_count": 0, "jobs": []}, {"total_count": 1},
                        {"total_count": 1, "jobs": []}, {"total_count": 1, "jobs": [1, 2]},
                        {"total_count": "1", "jobs": [{}]}):
            with patch.object(gate, "urlopen", return_value=self.response(payload)):
                with self.subTest(payload=payload), self.assertRaises(ValueError):
                    gate.read_jobs("owner/repo", 123, 2, "token", "https://api.github.com")
        for second in ({"total_count": 3, "jobs": [{}]}, {"total_count": 2, "jobs": []}):
            with patch.object(gate, "urlopen", side_effect=[
                self.response({"total_count": 2, "jobs": [{}]}), self.response(second)
            ]), self.assertRaises(ValueError):
                gate.read_jobs("owner/repo", 123, 2, "token", "https://api.github.com")

    def test_api_errors_fail_closed_and_pr_never_calls_api(self):
        env = {"GITHUB_EVENT_NAME": "merge_group", "GATE_RESULTS": json.dumps(MergeGateTests().results()),
               "GITHUB_RUN_ID": "123", "GITHUB_RUN_ATTEMPT": "2", "GITHUB_REPOSITORY": "owner/repo",
               "GH_TOKEN": "token", "GITHUB_API_URL": "https://api.github.com"}
        for error in (URLError("unavailable"), ValueError("bad JSON"), TimeoutError("timeout")):
            with patch.dict(os.environ, env, clear=True), patch.object(gate, "read_jobs", side_effect=error):
                self.assertEqual(gate.main(), 1)
        env = {"GITHUB_EVENT_NAME": "pull_request",
               "GATE_RESULTS": json.dumps(MergeGateTests().results("pull_request"))}
        with patch.dict(os.environ, env, clear=True), patch.object(gate, "read_jobs") as fetch:
            self.assertEqual(gate.main(), 0)
            fetch.assert_not_called()


if __name__ == "__main__":
    unittest.main()
