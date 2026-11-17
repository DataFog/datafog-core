"""Qualification-harness guards; these tests do not qualify a credential provider."""
import copy
import importlib.util
import json
from pathlib import Path
import sys
import tempfile
import time
import unittest

SPEC = importlib.util.spec_from_file_location("credential_qualification", Path(__file__).with_name("credential-qualification-run.py"))
QUAL = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(QUAL)


class EvidenceTests(unittest.TestCase):
    def test_nonzero_child_is_never_qualified(self):
        with tempfile.TemporaryDirectory() as temporary:
            runner = QUAL.Runner(Path(sys.executable), Path(temporary))
            with self.assertRaises(QUAL.QualificationError):
                runner.command("failed-child", [sys.executable, "-c", "raise SystemExit(7)"])
            self.assertEqual(runner.steps[-1]["status"], "FAIL")
            self.assertEqual(runner.steps[-1]["exit_code"], 7)

    def test_malformed_or_incomplete_provider_report_cannot_pass(self):
        valid = {"schema": QUAL.SCHEMA, "status": "PASS", "mode": "full", "real_provider": True,
                 "backend": QUAL.BACKENDS[QUAL.platform.system()], "cleanup_status": "PASS",
                 "cases": [{"id": name, "status": "PASS"} for name in sorted(QUAL.FULL_CASES)]}
        samples = [valid]
        for field, value in (("status", "FAIL"), ("real_provider", False), ("backend", "file_key"),
                             ("mode", "verify"), ("cleanup_status", "FAIL"), ("schema", "other")):
            changed = copy.deepcopy(valid)
            changed[field] = value
            samples.append(changed)
        for change in ("missing", "duplicate", "skipped"):
            changed = copy.deepcopy(valid)
            if change == "missing":
                changed["cases"].pop()
            elif change == "duplicate":
                changed["cases"].append(changed["cases"][0])
            else:
                changed["cases"][0]["status"] = "SKIP"
            samples.append(changed)
        samples.append({})
        with tempfile.TemporaryDirectory() as temporary:
            for index, sample in enumerate(samples):
                directory = Path(temporary) / str(index)
                directory.mkdir()
                runner = QUAL.Runner(Path("unused"), directory)
                def fixture_command(phase, arguments, **kwargs):
                    # A parser fixture only: real qualification never replaces
                    # commands or the actual platform credential provider.
                    runner.steps.append({"id": phase, "status": "PASS"})
                    Path(arguments[-1]).write_text(json.dumps(sample))
                runner.command = fixture_command
                if index == 0:
                    runner.probe("provider")
                else:
                    with self.assertRaises(QUAL.QualificationError):
                        runner.probe("provider")

    def test_timeout_stops_parent_and_its_writer_child(self):
        with tempfile.TemporaryDirectory() as temporary:
            heartbeat = Path(temporary) / "heartbeat"
            child = "import pathlib,sys,time\np=pathlib.Path(sys.argv[1])\nwhile True:\n with p.open('a') as stream: stream.write('tick\\n'); stream.flush()\n time.sleep(0.01)"
            parent = ("import subprocess,sys,time\n"
                      "subprocess.Popen([sys.executable,'-u','-c'," + repr(child) + "," + repr(str(heartbeat)) + "])\n"
                      "time.sleep(120)\n")
            runner = QUAL.Runner(Path(sys.executable), Path(temporary))
            with self.assertRaisesRegex(QUAL.QualificationError, "owned-tree_timeout"):
                runner.command("owned-tree", [sys.executable, "-c", parent], timeout=5)
            self.assertEqual(runner.steps[-1]["termination_status"], "PASS")
            before = heartbeat.read_bytes()
            self.assertTrue(before)
            time.sleep(0.1)
            self.assertEqual(heartbeat.read_bytes(), before)

    def test_disposable_macos_mode_is_rejected_on_regular_desktop(self):
        with tempfile.TemporaryDirectory() as temporary:
            runner = QUAL.Runner(Path("unused"), Path(temporary))
            # CI executes this test too; remove its hosted authorization markers
            # only in this process while checking the guard.
            old = QUAL.os.environ.pop("GITHUB_ACTIONS", None)
            try:
                with self.assertRaisesRegex(QUAL.QualificationError, "disposable_macos_runner_required"):
                    QUAL.disposable_macos_keychain(runner)
                self.assertEqual(runner.steps, [])
            finally:
                if old is not None:
                    QUAL.os.environ["GITHUB_ACTIONS"] = old


if __name__ == "__main__":
    unittest.main()
