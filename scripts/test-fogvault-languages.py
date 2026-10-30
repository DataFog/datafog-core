"""Fail-closed negative evidence tests for native language qualification."""
import copy
import importlib.util
from pathlib import Path
import tempfile
import unittest

spec = importlib.util.spec_from_file_location("language_gate", Path(__file__).with_name("verify-fogvault-languages.py"))
gate = importlib.util.module_from_spec(spec)
spec.loader.exec_module(gate)


class EvidenceTests(unittest.TestCase):
    def test_all_required_language_cases_pass(self):
        for names in (gate.NODE_CASES, gate.PYTHON_CASES):
            gate.cases([{"id": n, "status": "pass", "executed": True} for n in names], names)
        self.assertEqual(len(gate.NODE_CASES), 11)
        self.assertEqual(len(gate.PYTHON_CASES), 18)

    def test_missing_duplicate_skipped_failed_and_malformed_reject(self):
        for names in (gate.NODE_CASES, gate.PYTHON_CASES):
            original = [{"id": n, "status": "pass", "executed": True} for n in sorted(names)]
            for kind in ("missing", "duplicate", "skipped", "failed", "unrelated", "malformed"):
                value = copy.deepcopy(original)
                if kind == "missing": value.pop()
                if kind == "duplicate": value[0] = value[1]
                if kind == "skipped": value[0]["executed"] = False
                if kind == "failed": value[0]["status"] = "fail"
                if kind == "unrelated": value[0]["id"] = "unrelated_passing_test"
                if kind == "malformed": value[0] = None
                with self.subTest(kind=kind), self.assertRaises(ValueError):
                    gate.cases(value, names)

    def test_interop_requires_three_stages_and_source_stability(self):
        value = [{"id": n, "pass": True, "executed": True} for n in gate.INTEROP_CASES]
        gate.cases(value, gate.INTEROP_CASES, "pass")
        for case in value:
            broken = copy.deepcopy(value)
            next(c for c in broken if c["id"] == case["id"])["pass"] = False
            with self.assertRaises(ValueError): gate.cases(broken, gate.INTEROP_CASES, "pass")

    def test_failed_unexecuted_wrong_schema_or_malformed_report_reject(self):
        valid = {"schema": "expected", "pass": True, "executed": True}
        gate.object_report(valid, "expected")
        for invalid in (None, [], {}, {**valid, "pass": False}, {**valid, "executed": False}, {**valid, "schema": "wrong"}):
            with self.assertRaises(ValueError): gate.object_report(invalid, "expected")

    def test_changed_or_missing_actual_artifact_reject(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "candidate"
            path.write_bytes(b"actual candidate bytes")
            proof = {"path": str(path), "sha256": gate.sha(path)}
            gate.artifact(proof)
            path.write_bytes(b"replaced candidate")
            with self.assertRaises(ValueError): gate.artifact(proof)
            path.unlink()
            with self.assertRaises(OSError): gate.artifact(proof)

    def test_same_commit_dirty_source_bytes_change_identity(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            gate.subprocess.run(["git", "init", "--quiet", str(root)], check=True)
            source = root / "integrations/fogvault-python/src/lib.rs"
            source.parent.mkdir(parents=True)
            source.write_text("original")
            saved_root = gate.ROOT
            try:
                gate.ROOT = root
                before = gate.identity()
                source.write_text("changed without a commit")
                self.assertNotEqual(before, gate.identity())
            finally:
                gate.ROOT = saved_root

    def test_producer_source_subset_rejects_changed_missing_or_extra_bytes(self):
        original = gate.identity
        try:
            gate.identity = lambda: {"crates/core/src/lib.rs": "current", "unrelated": "value"}
            gate.source_matches({"crates/core/src/lib.rs": "current"}, ("crates/core/",))
            for invalid in (None, {}, {"crates/core/src/lib.rs": "old"},
                            {"crates/core/src/lib.rs": "current", "removed": "old"}):
                with self.assertRaises(ValueError): gate.source_matches(invalid, ("crates/core/",))
        finally:
            gate.identity = original

    def test_interop_bound_to_exact_producer_report_bytes(self):
        original = gate.validate_producers
        try:
            gate.validate_producers = lambda *_: None
            with tempfile.TemporaryDirectory() as directory:
                node, python = Path(directory) / "node.json", Path(directory) / "python.json"
                node.write_text("node candidate")
                python.write_text("python candidate")
                proof = {"schema": "datafog-fogvault-native-interop/v1", "pass": True, "executed": True,
                         "checks": [{"id": n, "pass": True, "executed": True} for n in gate.INTEROP_CASES],
                         "node_proof_sha256": gate.sha(node), "python_proof_sha256": gate.sha(python)}
                gate.validate({}, {}, proof, node, python)
                for path in (node, python):
                    prior = path.read_bytes()
                    path.write_bytes(b"different producer with same pass claim")
                    with self.assertRaises(ValueError): gate.validate({}, {}, proof, node, python)
                    path.write_bytes(prior)
        finally:
            gate.validate_producers = original


if __name__ == "__main__":
    unittest.main()
