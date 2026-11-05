"""Execute the actual model workflow's archive gate for RC, beta and stable candidates."""
import io
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tarfile
import tempfile
import textwrap
import unittest
from zipfile import ZipFile

ROOT = Path(__file__).resolve().parents[1]
WORKFLOW = ROOT / ".github/workflows/model-datafog.yml"


def candidate_script():
    source = WORKFLOW.read_text(encoding="utf-8")
    step = source.split("      - name: Select and verify exact current candidate archives\n", 1)[1]
    match = re.search(r"          python - <<'PYTHON'\n(.*?)\n          PYTHON", step, re.S)
    if match is None:
        raise ValueError("model candidate verification script missing")
    script = textwrap.dedent(match[1])
    compile(script, str(WORKFLOW) + ":candidate-artifacts", "exec")
    return script


def tar_metadata(path, name, value):
    data = value.encode()
    with tarfile.open(path, "w:gz") as archive:
        member = tarfile.TarInfo(name)
        member.size = len(data)
        archive.addfile(member, io.BytesIO(data))


class CandidateVersions(unittest.TestCase):
    def run_gate(self, semver, pep440, *, node_version=None, archive_mismatch=None):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for name in ("scripts", "bindings/node", "bindings/python", "crates/core", "candidate-common", "candidate-native", "candidate-node"):
                (root / name).mkdir(parents=True)
            shutil.copyfile(ROOT / "scripts/check-release.py", root / "scripts/check-release.py")
            node_version = semver if node_version is None else node_version
            (root / "bindings/node/package.json").write_text(json.dumps({"version": node_version}))
            (root / "bindings/python/pyproject.toml").write_text(f'[project]\nversion="{pep440}"\n')
            (root / "crates/core/Cargo.toml").write_text(f'[package]\nversion="{semver}"\n')
            crate = f"datafog-core-{semver}"
            inner_version = "0.0.0" if archive_mismatch == "crate" else semver
            tar_metadata(root / "candidate-common" / (crate + ".crate"), crate + "/Cargo.toml",
                         f'[package]\nname="datafog-core"\nversion="{inner_version}"\n')
            wheel = root / "candidate-native" / f"datafog_core-{pep440}-py3-none-any.whl"
            with ZipFile(wheel, "w") as archive:
                inner_version = "0.0.0" if archive_mismatch == "wheel" else pep440
                archive.writestr(f"datafog_core-{pep440}.dist-info/METADATA",
                                 f"Metadata-Version: 2.4\nName: datafog-core\nVersion: {inner_version}\n\n")
            for name in ("@datafog/node", "@datafog/node-linux-x64-gnu"):
                filename = name.removeprefix("@").replace("/", "-") + "-" + node_version + ".tgz"
                inner_version = "0.0.0" if archive_mismatch == "npm" else node_version
                tar_metadata(root / "candidate-node" / filename, "package/package.json",
                             json.dumps({"name": name, "version": inner_version}))
            output = root / "outputs"
            env = dict(os.environ, NPM_PLATFORM="linux-x64-gnu", GITHUB_OUTPUT=str(output))
            result = subprocess.run([sys.executable, "-c", candidate_script()], cwd=root, env=env,
                                    capture_output=True, text=True)
            return result, output.read_text() if output.exists() else ""

    def test_actual_workflow_accepts_matching_rc_beta_and_stable_archive_identities(self):
        for semver, pep440 in (("1.1.0-rc.1", "1.1.0rc1"), ("1.1.0-beta.2", "1.1.0b2"), ("1.1.0", "1.1.0")):
            with self.subTest(version=semver):
                result, output = self.run_gate(semver, pep440)
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertIn(f"crate=candidate-common/datafog-core-{semver}.crate\n", output)
                self.assertIn(f"wheel=candidate-native/datafog_core-{pep440}-py3-none-any.whl\n", output)
                self.assertIn(f"node=candidate-node/datafog-node-{semver}.tgz\n", output)

    def test_different_python_or_node_candidate_versions_remain_blocking(self):
        for semver, pep440, node in (("1.1.0-rc.1", "1.1.0rc2", None),
                                     ("1.1.0-rc.1", "1.1.0rc1", "1.1.0-rc.2"),
                                     ("1.1.0", "1.1.0rc1", None),
                                     ("1.1.0-beta.2", "1.1.0b3", None)):
            with self.subTest(semver=semver, python=pep440, node=node):
                result, output = self.run_gate(semver, pep440, node_version=node)
                self.assertNotEqual(result.returncode, 0)
                self.assertIn("Core, Node and Python candidate versions disagree", result.stderr)
                self.assertEqual(output, "")

    def test_matching_versions_do_not_bypass_packed_metadata_checks(self):
        for mismatch in ("crate", "wheel", "npm"):
            with self.subTest(mismatch=mismatch):
                result, output = self.run_gate("1.1.0-rc.1", "1.1.0rc1", archive_mismatch=mismatch)
                self.assertNotEqual(result.returncode, 0)
                self.assertIn("filename and metadata disagree", result.stderr)
                self.assertEqual(output, "")


if __name__ == "__main__":
    unittest.main()
