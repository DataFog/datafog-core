"""Offline fail-closed tests for exact-commit v1 publication evidence."""
import copy
import importlib.util
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location("release_gate", Path(__file__).with_name("check-fogvault-release.py"))
gate = importlib.util.module_from_spec(spec)
spec.loader.exec_module(gate)
REPOSITORY = "DataFog/datafog-core"
REVISION = "a" * 40


def fixture():
    responses = {}
    for index, (workflow, names) in enumerate(gate.REQUIRED.items(), 1):
        path = ".github/workflows/" + workflow
        responses[f"actions/workflows/{workflow}"] = [{"id": index, "path": path}]
        responses[f"actions/workflows/{workflow}/runs?head_sha={REVISION}&per_page=1"] = [{"workflow_runs": [{
            "id": index * 10, "workflow_id": index, "path": path, "head_sha": REVISION,
            "event": "push", "head_repository": {"full_name": REPOSITORY},
            "status": "completed", "conclusion": "success", "html_url": "https://example.test/run",
        }]}]
        jobs = [{"run_id": index * 10, "name": name, "status": "completed", "conclusion": "success"}
                for name in sorted(names)]
        # Force the checker to consume every page, not just the first job page.
        responses[f"actions/runs/{index * 10}/jobs?filter=latest&per_page=100"] = [
            {"jobs": jobs[:1]}, {"jobs": jobs[1:]}]
    return responses


def run(responses, version="1.0.0"):
    def fetch(repository, endpoint):
        if repository != REPOSITORY:
            raise AssertionError("Unexpected repository")
        return responses[endpoint]
    return gate.check(REPOSITORY, REVISION, version, fetch)


class ReleaseEvidenceTests(unittest.TestCase):
    def test_beta3_is_explicitly_nonrequired_and_does_not_fetch(self):
        self.assertEqual(run({}, "0.5.0-beta.3")["required"], False)

    def test_exact_completed_stable_and_manual_runs_pass_all_eleven_scopes(self):
        for event in ("push", "workflow_dispatch"):
            for version in ("1.0.0", "1.0.0-beta.1", "2.0.0"):
                data = fixture()
                for key, pages in data.items():
                    if "/runs?" in key:
                        pages[0]["workflow_runs"][0]["event"] = event
                result = run(data, version)
                self.assertTrue(result["required"])
                self.assertEqual(sum(len(p["required_scopes"]) for p in result["proofs"]), 11)
                self.assertTrue(all(p["head_sha"] == REVISION for p in result["proofs"]))

    def test_browser_sdk_extraction_is_required_at_exact_head(self):
        self.assertEqual(gate.REQUIRED["browser-model.yml"], {"extraction"})
        data = fixture()
        key = f"actions/workflows/browser-model.yml/runs?head_sha={REVISION}&per_page=1"
        data[key][0]["workflow_runs"][0]["head_sha"] = "b" * 40
        with self.assertRaises(ValueError):
            run(data)

    def test_browser_extraction_executes_the_sdk_gate_and_retains_its_evidence(self):
        text = (gate.ROOT / ".github/workflows/browser-model.yml").read_text()
        marker = "      - name: Qualify installed SDK contracts and approved-value recall\n"
        self.assertEqual(text.count(marker), 1)
        step = text.split(marker, 1)[1].split("      - ", 1)[0]
        self.assertIn("run: cargo dfv check --core-sdk --json", step)
        self.assertIn("working-directory: .", step)
        self.assertNotIn("if:", step)
        self.assertNotIn("continue-on-error", step)
        self.assertIn("target/core-sdk-qualification/", text)
        self.assertIn("target/dfv/runs/", text)
        self.assertIn("workflow_dispatch:", text)
        for source in ("bindings/**", "integrations/**", "crates/**", "scripts/**", "fixtures/**", "docs/contracts/**"):
            self.assertEqual(text.count(f'      - "{source}"'), 2)

    def test_archive_and_unified_node_proofs_are_part_of_required_jobs(self):
        joint = (gate.ROOT / ".github/workflows/fogvault-integration.yml").read_text(encoding="utf-8")
        step = joint.split("      - name: Qualify registry-native Core and FogVault archives\n", 1)[1].split("      - ", 1)[0]
        self.assertIn("if: matrix.engine == 'chromium'", step)
        self.assertIn("python scripts/qualify-native-registry.py", step)
        self.assertNotIn("continue-on-error", step)
        self.assertIn("target/native-registry-*/run/registry/*.crate", joint)
        self.assertNotIn("consumer-cargo-home", joint)
        self.assertIn("Core–FogVault unified Node archive", gate.REQUIRED["fogvault-native-languages.yml"])
        native = (gate.ROOT / ".github/workflows/fogvault-native-languages.yml").read_text(encoding="utf-8")
        assembly = native.split("  assemble-node:\n", 1)[1]
        self.assertIn("needs: installed-native-languages", assembly)
        self.assertIn("--revision \"$GITHUB_SHA\"", assembly)
        self.assertIn("--archive target/native-assembly/*.tgz --binary-root", assembly)
        self.assertNotIn("continue-on-error", assembly)

    def test_each_missing_os_or_browser_scope_blocks(self):
        for workflow, names in gate.REQUIRED.items():
            for name in names:
                data = fixture()
                for key, pages in data.items():
                    if key.endswith("/jobs?filter=latest&per_page=100"):
                        for page in pages:
                            page["jobs"] = [j for j in page["jobs"] if j["name"] != name]
                with self.subTest(workflow=workflow, name=name), self.assertRaises(ValueError):
                    run(data)

    def test_wrong_head_workflow_repository_and_pr_only_green_block(self):
        for field, wrong in (("head_sha", "b" * 40), ("workflow_id", 999),
                             ("path", ".github/workflows/unrelated.yml"),
                             ("head_repository", {"full_name": "other/fork"}),
                             ("event", "pull_request")):
            for workflow in gate.REQUIRED:
                data = fixture()
                key = f"actions/workflows/{workflow}/runs?head_sha={REVISION}&per_page=1"
                data[key][0]["workflow_runs"][0][field] = wrong
                with self.subTest(field=field, workflow=workflow), self.assertRaises(ValueError):
                    run(data)

    def test_in_progress_failed_cancelled_and_missing_runs_block_without_fallback(self):
        for workflow in gate.REQUIRED:
            for status, conclusion in (("in_progress", None), ("queued", None),
                                       ("completed", "failure"), ("completed", "cancelled"),
                                       ("completed", "skipped")):
                data = fixture()
                key = f"actions/workflows/{workflow}/runs?head_sha={REVISION}&per_page=1"
                latest = data[key][0]["workflow_runs"][0]
                older = copy.deepcopy(latest)
                latest.update(status=status, conclusion=conclusion)
                data[key][0]["workflow_runs"].append(older)
                with self.subTest(status=status, conclusion=conclusion), self.assertRaises(ValueError):
                    run(data)
            data = fixture()
            data[f"actions/workflows/{workflow}/runs?head_sha={REVISION}&per_page=1"] = [{"workflow_runs": []}]
            with self.assertRaises(ValueError): run(data)

    def test_failed_unfinished_duplicate_and_foreign_run_jobs_block(self):
        for change in ("failure", "in_progress", "duplicate", "foreign_run"):
            data = fixture()
            jobs = data["actions/runs/10/jobs?filter=latest&per_page=100"][0]["jobs"]
            if change == "failure": jobs[0]["conclusion"] = "failure"
            if change == "in_progress": jobs[0]["status"] = "in_progress"
            if change == "duplicate": jobs.append(copy.deepcopy(jobs[0]))
            if change == "foreign_run": jobs[0]["run_id"] = 999
            with self.subTest(change=change), self.assertRaises(ValueError): run(data)

    def test_missing_or_wrong_workflow_metadata_cannot_use_check_names(self):
        for metadata in ([], [{"id": 1, "path": ".github/workflows/unrelated.yml"}], [{"path": ".github/workflows/fogvault-integration.yml"}]):
            data = fixture()
            data["actions/workflows/fogvault-integration.yml"] = metadata
            with self.assertRaises(ValueError): run(data)

    def test_invalid_versions_and_commit_identity_block(self):
        for version in ("", "latest", "1", "01.0.0"):
            with self.subTest(version=version), self.assertRaises(ValueError): run({}, version)
        with self.assertRaises(ValueError): gate.check(REPOSITORY, "main", "1.0.0", lambda *_: [])

    def test_all_publication_workflows_invoke_gate_without_an_if_exception(self):
        for platform in ("rust", "node", "python", "wasm"):
            text = (gate.ROOT / f".github/workflows/publish-{platform}.yml").read_text()
            self.assertEqual(text.count("run: python3 scripts/check-fogvault-release.py"), 1)
            step = text.split("      - name: Require exact-commit Core and FogVault qualification for v1+\n", 1)[1].split("      - ", 1)[0]
            self.assertNotIn("if:", step)
            self.assertIn("GH_TOKEN: ${{ github.token }}", step)
            self.assertLess(text.index("scripts/check-fogvault-release.py"), text.index("scripts/check-model-integration.py"))


if __name__ == "__main__":
    unittest.main()
