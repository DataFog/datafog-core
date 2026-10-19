"""Verify that publication fails closed, without loading models or calling GitHub."""
import contextlib
import io
from pathlib import Path
import runpy
import subprocess
import unittest
from unittest.mock import patch

module = runpy.run_path(str(Path(__file__).with_name("check-model-integration.py")))
check = module["check"]
platforms = ("ubuntu-24.04", "ubuntu-24.04-arm", "macos-14", "macos-15-intel", "windows-latest")


class PublicationCheck(unittest.TestCase):
    def run_check(self, runs=None, jobs=None):
        if runs is None:
            runs = [{"id": 1, "head_sha": "release", "conclusion": "success", "html_url": "https://example.test/run"}]
        if jobs is None:
            jobs = [{"name": f"installed-model ({backend}, {platform})", "conclusion": "success"}
                    for backend in ("datafog", "gliner2") for platform in platforms]
        replies = [json_bytes([{"workflow_runs": runs}]), json_bytes([{"jobs": jobs[:5]}, {"jobs": jobs[5:]}])]
        with patch("subprocess.check_output", side_effect=replies) as call, contextlib.redirect_stdout(io.StringIO()):
            check("DataFog/datafog-core", "release")
        self.assertIn("head_sha=release", call.call_args_list[0].args[0][-1])

    def test_both_models_all_five_platforms_pass(self):
        self.run_check()

    def test_missing_failed_unfinished_cancelled_and_wrong_commit_block(self):
        cases = [[], [{"head_sha": "old", "conclusion": "success"}]]
        cases += [[{"head_sha": "release", "conclusion": conclusion}] for conclusion in (None, "failure", "cancelled", "skipped")]
        for runs in cases:
            with self.subTest(runs=runs), self.assertRaises(SystemExit):
                self.run_check(runs=runs)

    def test_each_missing_failed_or_skipped_platform_blocks(self):
        jobs = [{"name": f"installed-model ({backend}, {platform})", "conclusion": "success"}
                for backend in ("datafog", "gliner2") for platform in platforms]
        for index in range(len(jobs)):
            for conclusion in (None, "failure", "skipped", "cancelled"):
                bad = [dict(job) for job in jobs]
                bad[index]["conclusion"] = conclusion
                with self.subTest(index=index, conclusion=conclusion), self.assertRaises(SystemExit):
                    self.run_check(jobs=bad)
            with self.subTest(missing=index), self.assertRaises(SystemExit):
                self.run_check(jobs=jobs[:index] + jobs[index + 1:])

    def test_newer_failed_run_is_not_replaced_by_older_success(self):
        with self.assertRaises(SystemExit):
            self.run_check(runs=[{"head_sha": "release", "conclusion": "failure"},
                                 {"head_sha": "release", "conclusion": "success"}])

    def test_api_failure_blocks(self):
        with patch("subprocess.check_output", side_effect=subprocess.CalledProcessError(1, "gh")):
            with self.assertRaises(subprocess.CalledProcessError):
                check("DataFog/datafog-core", "release")


def json_bytes(value):
    import json
    return json.dumps(value)


if __name__ == "__main__":
    unittest.main()
