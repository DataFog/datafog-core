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
    def run_check(self, runs=None, jobs=None, commits=None, ancestor_runs=None):
        if runs is None:
            runs = [{"id": 1, "head_sha": "release", "conclusion": "success", "html_url": "https://example.test/run"}]
        if jobs is None:
            jobs = [{"name": f"installed-model ({backend}, {platform})", "conclusion": "success"}
                    for backend in ("datafog", "gliner2") for platform in platforms]
        if commits is None:
            commits = {"release": self.commit("release", "parent", "crates/core/src/lib.rs")}
        run_results = {"release": runs, **(ancestor_runs or {})}

        def reply(arguments, **kwargs):
            endpoint = arguments[-1].split("repos/DataFog/datafog-core/")[1]
            if endpoint.startswith("actions/workflows/model-datafog.yml/runs?head_sha="):
                revision = endpoint.split("head_sha=")[1].split("&")[0]
                return json_bytes([{"workflow_runs": run_results[revision]}])
            if endpoint.startswith("commits/"):
                revision = endpoint.split("commits/")[1].split("?")[0]
                commit = commits[revision]
                return json_bytes(commit if isinstance(commit, list) else [commit])
            if endpoint == "actions/runs/1/jobs?per_page=100":
                return json_bytes([{"jobs": jobs[:5]}, {"jobs": jobs[5:]}])
            raise AssertionError(f"unexpected API endpoint: {endpoint}")

        with patch("subprocess.check_output", side_effect=reply) as call, contextlib.redirect_stdout(io.StringIO()) as output:
            check("DataFog/datafog-core", "release")
        self.assertIn("head_sha=release", call.call_args_list[0].args[0][-1])
        return output.getvalue()

    @staticmethod
    def commit(revision, parent, filename="README.md", status="modified"):
        return {"sha": revision, "parents": [{"sha": parent}],
                "files": [{"filename": filename, "status": status}]}

    @staticmethod
    def successful_run(revision):
        return [{"id": 1, "head_sha": revision, "conclusion": "success", "html_url": "https://example.test/run"}]

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

    def test_readme_only_commits_reuse_latest_passing_ancestor(self):
        output = self.run_check(runs=[], commits={
            "release": self.commit("release", "readme-parent"),
            "readme-parent": self.commit("readme-parent", "tested"),
        }, ancestor_runs={"readme-parent": [], "tested": self.successful_run("tested")})
        self.assertIn("README-only changes since tested", output)

    def test_other_files_and_readme_additions_deletions_or_renames_block(self):
        for filename, status in [
            ("crates/core/src/lib.rs", "modified"),
            ("Cargo.lock", "modified"),
            ("bindings/python/python/datafog_core/_gliner2_worker.py", "modified"),
            ("scripts/check-model-integration.py", "modified"),
            ("docs/index.mdx", "modified"),
            ("README.md", "added"),
            ("README.md", "removed"),
            ("README.md", "renamed"),
        ]:
            with self.subTest(filename=filename, status=status), self.assertRaises(SystemExit):
                self.run_check(runs=[], commits={"release": self.commit("release", "tested", filename, status)},
                               ancestor_runs={"tested": self.successful_run("tested")})

    def test_every_page_of_changed_files_is_checked(self):
        readme = self.commit("release", "tested")
        code = self.commit("release", "tested", "crates/core/src/lib.rs")
        for commits in [[{**readme, "files": readme["files"] + code["files"]}], [readme, code]]:
            with self.subTest(commits=commits), self.assertRaises(SystemExit):
                self.run_check(runs=[], commits={"release": commits},
                               ancestor_runs={"tested": self.successful_run("tested")})

    def test_merges_root_commits_empty_commits_and_wrong_commits_block(self):
        for changed in [
            {"parents": []},
            {"parents": [{"sha": "tested"}, {"sha": "other"}]},
            {"files": []},
            {"sha": "other"},
        ]:
            with self.subTest(changed=changed), self.assertRaises(SystemExit):
                self.run_check(runs=[], commits={"release": {**self.commit("release", "tested"), **changed}},
                               ancestor_runs={"tested": self.successful_run("tested")})

    def test_readme_does_not_hide_untested_code_changes_in_ancestor(self):
        with self.assertRaises(SystemExit):
            self.run_check(runs=[], commits={
                "release": self.commit("release", "code-parent"),
                "code-parent": self.commit("code-parent", "tested", "crates/core/src/lib.rs"),
            }, ancestor_runs={"code-parent": [], "tested": self.successful_run("tested")})

    def test_readme_does_not_bypass_failed_or_incomplete_model_results(self):
        jobs = [{"name": "installed-model (datafog, ubuntu-24.04)", "conclusion": "success"}]
        with self.assertRaises(SystemExit):
            self.run_check(runs=[], jobs=jobs, commits={"release": self.commit("release", "tested")},
                           ancestor_runs={"tested": self.successful_run("tested")})
        for revision in ["release", "tested"]:
            for conclusion in (None, "failure", "cancelled", "skipped"):
                runs = [{**self.successful_run(revision)[0], "conclusion": conclusion}]
                with self.subTest(revision=revision, conclusion=conclusion), self.assertRaises(SystemExit):
                    self.run_check(runs=runs if revision == "release" else [],
                                   commits={"release": self.commit("release", "tested")},
                                   ancestor_runs={"tested": runs if revision == "tested" else self.successful_run("tested")})

    def test_readme_ancestry_api_failure_blocks(self):
        replies = [json_bytes([{"workflow_runs": []}]), subprocess.CalledProcessError(1, "gh")]
        with patch("subprocess.check_output", side_effect=replies), self.assertRaises(subprocess.CalledProcessError):
            check("DataFog/datafog-core", "release")


def json_bytes(value):
    import json
    return json.dumps(value)


if __name__ == "__main__":
    unittest.main()
