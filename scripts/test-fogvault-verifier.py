"""Negative evidence tests for the optional integration gate."""
import copy
import importlib.util
import json
from pathlib import Path
import subprocess
import tempfile
import unittest

spec = importlib.util.spec_from_file_location("integration_gate", Path(__file__).with_name("verify-fogvault-integration.py"))
gate = importlib.util.module_from_spec(spec)
spec.loader.exec_module(gate)


class EvidenceTests(unittest.TestCase):
    def browser(self):
        return {"schema": "datafog-fogvault-browser-proof/v1", "pass": True, "executed": True,
                "typecheck_executed": True, "required_cases": sorted(gate.BROWSER_CASES),
                "installed_assets": {"synthetic": {"index.js": "fixture"}},
                "result": {"pass": True, "executed": True, "userAgent": "synthetic browser",
                           "cases": [{"id": name, "status": "pass", "executed": True}
                                     for name in sorted(gate.BROWSER_CASES)]}}

    def test_complete_browser_evidence_passes(self):
        gate.validate_browser(self.browser())

    def test_browser_missing_duplicate_skipped_or_failed_cases_reject(self):
        original = self.browser()
        variants = []
        for kind in ["missing", "duplicate", "skipped", "failed"]:
            value = copy.deepcopy(original)
            cases = value["result"]["cases"]
            if kind == "missing": cases.pop()
            if kind == "duplicate": cases[0] = cases[1]
            if kind == "skipped": cases[0]["executed"] = False
            if kind == "failed": cases[0]["status"] = "fail"
            variants.append(value)
        for value in variants:
            with self.subTest(value=value), self.assertRaises(ValueError):
                gate.validate_browser(value)

    def test_malformed_browser_and_missing_typecheck_reject(self):
        for value in [None, [], {}, {**self.browser(), "typecheck_executed": False},
                      {**self.browser(), "result": []}]:
            with self.subTest(value=value), self.assertRaises(ValueError):
                gate.validate_browser(value)

    def test_rust_required_names_are_not_replaced_by_unrelated_passing_tests(self):
        log = "test unrelated ... ok\ntest result: ok. 1 passed; 0 failed; 0 ignored;"
        for suite in gate.RUST_CASES:
            with self.assertRaises(ValueError): gate.validate_rust(log, suite)

    def test_rust_complete_names_pass_and_one_missing_rejects(self):
        for suite, names in gate.RUST_CASES.items():
            lines = [f"test tests::{name} ... ok" for name in sorted(names)]
            summary = f"\ntest result: ok. {len(names)} passed; 0 failed; 0 ignored;"
            gate.validate_rust("\n".join(lines) + summary, suite)
            with self.assertRaises(ValueError): gate.validate_rust("\n".join(lines[1:]) + summary, suite)

    def test_rust_zero_or_ignored_cases_reject(self):
        for log in ["", "test result: ok. 0 passed; 0 failed; 0 ignored;",
                    "test result: ok. 8 passed; 0 failed; 1 ignored;"]:
            with self.assertRaises(ValueError): gate.validate_rust(log, "native")

    def test_node_zero_skipped_or_failed_cases_reject(self):
        gate.require_node_tests("# tests 7\n# fail 0\n# skipped 0\n# cancelled 0\n")
        for log in ["", "# tests 0\n# fail 0", "# tests 1\n# fail 1",
                    "# tests 1\n# fail 0\n# skipped 1"]:
            with self.assertRaises(ValueError): gate.require_node_tests(log)

    def test_same_commit_dirty_runtime_and_changed_artifact_reject(self):
        gate.BASE.mkdir(parents=True, exist_ok=True)
        with tempfile.TemporaryDirectory(dir=gate.BASE) as directory:
            root = Path(directory)
            subprocess.run(["git", "init", "--quiet", str(root)], check=True)
            runtime = root / "bindings/wasm/index.js"
            runtime.parent.mkdir(parents=True)
            runtime.write_text("original runtime")
            subprocess.run(["git", "add", "."], cwd=root, check=True)
            artifact = root / "candidate.tgz"
            artifact.write_bytes(b"synthetic candidate identity")
            item = {"path": str(artifact), "sha256": gate.digest(artifact), "bytes": artifact.stat().st_size}
            prepared = {"pass": True, "fogvault_commit": gate.VAULT_REV,
                        "core_build_inputs": gate.core_build_inputs(root),
                        "artifacts": {"core_tarball": item, "vault_tarball": item}}
            gate.validate_prepared(prepared, root)
            runtime.write_text("changed without any commit")
            with self.assertRaises(ValueError): gate.validate_prepared(prepared, root)
            runtime.write_text("original runtime")
            artifact.write_bytes(b"changed tarball")
            with self.assertRaises(ValueError): gate.validate_prepared(prepared, root)


if __name__ == "__main__":
    unittest.main()
