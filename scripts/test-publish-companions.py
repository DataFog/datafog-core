"""Publication tests exercise changed bytes, unsafe archives and failed evidence."""
import io
import json
import os
import subprocess
import shutil
from pathlib import Path
import tarfile
import tempfile
import unittest
from unittest.mock import patch
import zipfile
import importlib.util
from types import SimpleNamespace

spec = importlib.util.spec_from_file_location("publisher", Path(__file__).with_name("publish-companions.py"))
publisher = importlib.util.module_from_spec(spec)
spec.loader.exec_module(publisher)


def tar(path, files):
    with tarfile.open(path, "w:gz") as archive:
        for name, data in files.items():
            info = tarfile.TarInfo(name)
            info.size = len(data)
            archive.addfile(info, io.BytesIO(data))


class Publication(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.root = Path(self.directory.name)

    def tearDown(self):
        self.directory.cleanup()

    def npm(self, name="@datafog/core-fogvault-browser", version="1.0.0"):
        result = self.root / "package.tgz"
        tar(result, {"package/package.json": json.dumps({"name": name, "version": version}).encode()})
        return result

    def test_dry_run_cannot_upload(self):
        archive = self.npm()
        with patch.object(publisher, "json_read", return_value=None), patch.object(publisher, "run") as run:
            result = publisher.publish_npm(archive, "@datafog/core-fogvault-browser", "1.0.0", False)
        self.assertIn("--dry-run", run.call_args.args[0])
        self.assertIn("--ignore-scripts", run.call_args.args[0])
        self.assertFalse(result["published"])

    def test_wrong_package_never_runs_publish(self):
        with patch.object(publisher, "run") as run:
            with self.assertRaises(ValueError):
                publisher.publish_npm(self.npm(version="2.0.0"), "@datafog/core-fogvault-browser", "1.0.0", True)
        run.assert_not_called()

    def test_existing_identical_archive_is_verified_without_reupload(self):
        archive = self.npm()
        with patch.object(publisher, "json_read", return_value={"dist": {"tarball": "registry-file"}}), patch.object(publisher, "read", return_value=archive.read_bytes()), patch.object(publisher, "run") as run:
            result = publisher.publish_npm(archive, "@datafog/core-fogvault-browser", "1.0.0", True)
        self.assertTrue(result["published"])
        run.assert_not_called()

    def test_existing_different_archive_hard_stops(self):
        with patch.object(publisher, "json_read", return_value={"dist": {"tarball": "registry-file"}}), patch.object(publisher, "read", return_value=b"different"), patch.object(publisher, "run") as run:
            with self.assertRaises(ValueError):
                publisher.publish_npm(self.npm(), "@datafog/core-fogvault-browser", "1.0.0", True)
        run.assert_not_called()

    def test_archive_traversal_cannot_escape(self):
        stream = io.BytesIO()
        with zipfile.ZipFile(stream, "w") as archive:
            archive.writestr("../outside", "wrong")
        with self.assertRaises(ValueError):
            publisher.unpack_zip(stream.getvalue(), self.root)
        self.assertFalse((self.root.parent / "outside").exists())

    def test_fog_token_bootstrap_cannot_claim_core_oidc_provenance(self):
        archive = self.npm(name="@datafog/fogvault-browser", version="0.1.0")
        with patch.dict(publisher.os.environ, {"NODE_AUTH_TOKEN": "synthetic", "ACTIONS_ID_TOKEN_REQUEST_TOKEN": "synthetic-oidc"}), patch.object(publisher, "json_read", return_value=None), patch.object(publisher, "run") as run:
            publisher.publish_npm(archive, "@datafog/fogvault-browser", "0.1.0", False)
        env = run.call_args.kwargs["env"]
        self.assertNotIn("ACTIONS_ID_TOKEN_REQUEST_TOKEN", env)
        self.assertEqual(env["NPM_CONFIG_PROVENANCE"], "false")

    def test_github_artifact_digest_and_head_are_required(self):
        metadata = {"id": 1, "name": "required", "digest": "sha256:wrong", "expired": False,
                    "workflow_run": {"head_sha": "a" * 40}}
        api = SimpleNamespace(api=lambda *args: [{"artifacts": [metadata]}])
        with patch.dict(publisher.os.environ, {"GITHUB_SHA": "a" * 40}), patch.object(publisher, "module", return_value=api), patch.object(publisher.subprocess, "check_output", return_value=b"wrong archive"):
            with self.assertRaises(ValueError):
                publisher.download_artifact(1, "required", self.root / "artifact")
            metadata["workflow_run"]["head_sha"] = "b" * 40
            with self.assertRaises(ValueError):
                publisher.download_artifact(1, "required", self.root / "artifact")

    def test_unreviewed_branch_cannot_upload(self):
        with patch.dict(publisher.os.environ, {"GITHUB_REPOSITORY": publisher.REPO, "GITHUB_REF": "refs/heads/feature"}), patch("sys.argv", ["publisher", "--group", "fog-rust", "--execute"]), patch.object(publisher, "run") as run:
            with self.assertRaises(ValueError):
                publisher.main()
        run.assert_not_called()

    def test_symlinked_artifact_is_rejected(self):
        stream = io.BytesIO()
        with zipfile.ZipFile(stream, "w") as archive:
            info = zipfile.ZipInfo("link")
            info.external_attr = 0o120777 << 16
            archive.writestr(info, "target")
        with self.assertRaises(ValueError):
            publisher.unpack_zip(stream.getvalue(), self.root)

    def test_missing_changed_or_ambiguous_proof_is_rejected(self):
        with self.assertRaises(ValueError):
            publisher.report(self.root, "required")
        (self.root / "one.json").write_text(json.dumps({"schema": "required", "pass": False}))
        with self.assertRaises(ValueError):
            publisher.report(self.root, "required")
        (self.root / "one.json").write_text(json.dumps({"schema": "required", "pass": True}))
        self.assertTrue(publisher.report(self.root, "required")["pass"])
        (self.root / "two.json").write_text((self.root / "one.json").read_text())
        with self.assertRaises(ValueError):
            publisher.report(self.root, "required")

    def test_qualified_archive_hash_required(self):
        path = self.root / "wheel.whl"
        path.write_bytes(b"qualified")
        receipt = {"sha256": publisher.digest(path.read_bytes()), "bytes": path.stat().st_size}
        self.assertEqual(publisher.qualified_file(self.root, receipt, ".whl"), path)
        path.write_bytes(b"changed")
        with self.assertRaises(ValueError):
            publisher.qualified_file(self.root, receipt, ".whl")

    def test_crate_source_manifest_and_dependency_evidence(self):
        source = self.root / "source"
        source.mkdir()
        manifest = b'[package]\nname="example"\nversion="1.0.0"\n'
        (source / "Cargo.toml").write_bytes(manifest)
        qualified = self.root / "qualified" / "example-1.0.0.crate"
        actual = self.root / "actual" / qualified.name
        qualified.parent.mkdir()
        actual.parent.mkdir()
        files = {"example-1.0.0/Cargo.toml": manifest, "example-1.0.0/Cargo.toml.orig": manifest, "example-1.0.0/Cargo.lock": b'version=4\npackage=[]\n', "example-1.0.0/src/lib.rs": b'pub fn valid() {}\n'}
        tar(qualified, files)
        tar(actual, files)
        publisher.compare_crate(qualified, actual, source, "a" * 40, {}, {})
        files["example-1.0.0/src/lib.rs"] = b'pub fn changed() {}\n'
        tar(actual, files)
        with self.assertRaises(ValueError):
            publisher.compare_crate(qualified, actual, source, "a" * 40, {}, {})
        files["example-1.0.0/src/lib.rs"] = b'pub fn valid() {}\n'
        files["example-1.0.0/Cargo.lock"] = b'version=4\n[[package]]\nname="unreviewed"\nversion="1.0.0"\nsource="registry+https://github.com/rust-lang/crates.io-index"\nchecksum="bad"\n'
        tar(actual, files)
        with self.assertRaises(ValueError):
            publisher.compare_crate(qualified, actual, source, "a" * 40, {}, {})

    def test_normal_cargo_repackages_broad_include_only_after_generated_orig_is_omitted(self):
        source = self.root / "source"
        (source / "src").mkdir(parents=True)
        (source / "Cargo.toml").write_text(
            '[package]\nname="publication-fixture"\nversion="1.0.0"\nedition="2021"\ninclude=["**"]\n')
        (source / "src/lib.rs").write_text('pub fn fixture() -> u8 { 1 }\n')
        environment = dict(os.environ, CARGO_TARGET_DIR=str(self.root / "build"))
        def cargo(*arguments):
            return subprocess.run(["cargo", *arguments], cwd=self.root, env=environment,
                                  text=True, capture_output=True, timeout=120)
        result = cargo("generate-lockfile", "--offline", "--manifest-path", str(source / "Cargo.toml"))
        self.assertEqual(result.returncode, 0, result.stderr)
        result = cargo("package", "--locked", "--offline", "--manifest-path", str(source / "Cargo.toml"))
        self.assertEqual(result.returncode, 0, result.stderr)
        qualified = self.root / "qualified/publication-fixture-1.0.0.crate"
        qualified.parent.mkdir()
        shutil.copyfile(self.root / "build/package/publication-fixture-1.0.0.crate", qualified)
        prefix = "publication-fixture-1.0.0/"
        files = publisher.tar_files(qualified)
        self.assertIn(prefix + "Cargo.toml.orig", files)
        broken = self.root / "broken"
        for name, content in files.items():
            target = broken / name.removeprefix(prefix)
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(content)
        result = cargo("package", "--locked", "--offline", "--manifest-path", str(broken / "Cargo.toml"))
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("reserved file name Cargo.toml.orig", result.stderr)
        repaired = self.root / "repaired"
        publisher.stage_crate_sources(qualified, repaired, prefix)
        self.assertFalse((repaired / "Cargo.toml.orig").exists())
        self.assertFalse((repaired / ".cargo_vcs_info.json").exists())
        self.assertEqual((repaired / "Cargo.lock").read_bytes(), files[prefix + "Cargo.lock"])
        self.assertEqual((repaired / "src/lib.rs").read_bytes(), files[prefix + "src/lib.rs"])
        result = cargo("package", "--locked", "--offline", "--manifest-path", str(repaired / "Cargo.toml"))
        self.assertEqual(result.returncode, 0, result.stderr)
        actual = self.root / "build/package/publication-fixture-1.0.0.crate"
        publisher.compare_crate(qualified, actual, repaired, "a" * 40, {}, {})

    def test_public_python_release_requires_complete_exact_hashes(self):
        (self.root / "receipt.json").write_text(json.dumps({"revision": "a" * 40, "group": "core-python-integration", "packages": [{"name": "one.whl", "sha256": "right"}]}))
        with patch.dict(publisher.os.environ, {"GITHUB_SHA": "a" * 40}), patch.object(publisher, "json_read", return_value={"urls": [{"filename": "one.whl", "digests": {"sha256": "right"}}]}):
            publisher.verify_python(self.root)
        with patch.dict(publisher.os.environ, {"GITHUB_SHA": "a" * 40}), patch.object(publisher, "json_read", return_value={"urls": []}):
            with self.assertRaises(ValueError):
                publisher.verify_python(self.root)


if __name__ == "__main__":
    unittest.main()
