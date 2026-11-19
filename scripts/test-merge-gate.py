"""Exercise fail-closed aggregation and guard the complete workflow inventory."""

import copy
import importlib.util
import json
from pathlib import Path
import re
import unittest

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
        excluded = {"changelog", "model-candidates", "merge-gate"}
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


if __name__ == "__main__":
    unittest.main()
