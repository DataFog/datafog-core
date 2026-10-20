"""Integration-contract checks for provisioning; no model-quality evaluation."""
import hashlib
import importlib.util
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

SCRIPT = Path(__file__).with_name("provision-gliner2-model.py")
SPEC = importlib.util.spec_from_file_location("provision_gliner2", SCRIPT)
PROVISION = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(PROVISION)


class ManifestProvisioning(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name)
        self.source = self.root / "source"
        self.source.mkdir()
        files = {}
        for name in ("config.json", "encoder_config/config.json", "tokenizer.json",
                     "tokenizer_config.json", "model.safetensors"):
            path = self.source / name
            path.parent.mkdir(parents=True, exist_ok=True)
            data = ("synthetic integration fixture: " + name).encode()
            path.write_bytes(data)
            files[name] = {"sha256": hashlib.sha256(data).hexdigest(), "bytes": len(data)}
        self.manifest = {"schema_version": 1,
                         "model": {"backend": "gliner2", "id": "synthetic/general-model",
                                   "revision": "integration-fixture"},
                         "platform": "portable-cpu", "files": files}
        self.manifest_path = self.root / "model.json"
        self.destination = self.root / "bundle"

    def provision(self):
        self.manifest_path.write_text(json.dumps(self.manifest))
        return subprocess.run([sys.executable, str(SCRIPT), "--source", str(self.source),
                               "--destination", str(self.destination),
                               "--manifest", str(self.manifest_path)],
                              capture_output=True, text=True)

    def test_identity_assets_and_emitted_digest_round_trip(self):
        result = self.provision()
        self.assertEqual(result.returncode, 0, result.stderr)
        receipt = json.loads(result.stdout)
        data = (self.destination / "bundle-manifest.json").read_bytes()
        self.assertEqual(json.loads(data), self.manifest)
        self.assertEqual(receipt["manifest_sha256"], hashlib.sha256(data).hexdigest())
        for name in self.manifest["files"]:
            self.assertEqual((self.destination / name).read_bytes(), (self.source / name).read_bytes())

    def test_changed_assets_do_not_leave_a_partial_bundle(self):
        (self.source / "model.safetensors").write_bytes(b"changed")
        result = self.provision()
        self.assertNotEqual(result.returncode, 0)
        self.assertFalse(self.destination.exists())
        self.assertFalse(list(self.root.glob(".gliner2-install-*")))

    def test_local_model_selection_generates_manifest_without_handwritten_hashes(self):
        result = subprocess.run([
            sys.executable, str(SCRIPT), "--source", str(self.source),
            "--destination", str(self.destination),
            "--model-id", "synthetic/another-model", "--revision", "selected-revision",
        ], capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        manifest = json.loads((self.destination / "bundle-manifest.json").read_text())
        self.assertEqual(manifest["model"]["id"], "synthetic/another-model")
        self.assertEqual(manifest["model"]["revision"], "selected-revision")
        self.assertEqual(manifest["files"], self.manifest["files"])

    def test_backend_mismatch_is_rejected(self):
        self.manifest["model"]["backend"] = "spacy"
        self.assertNotEqual(self.provision().returncode, 0)
        self.assertFalse(self.destination.exists())

    def test_model_identity_flags_require_nonempty_complete_identity(self):
        for flags in (["--model-id", "synthetic/model"],
                      ["--model-id", "", "--revision", ""]):
            result = subprocess.run([
                sys.executable, str(SCRIPT), "--source", str(self.source),
                "--destination", str(self.destination), *flags,
            ], capture_output=True, text=True)
            self.assertNotEqual(result.returncode, 0)
            self.assertFalse(self.destination.exists())

    def test_parent_traversal_is_rejected(self):
        self.manifest["files"]["../outside"] = {"sha256": "0" * 64, "bytes": 0}
        self.assertNotEqual(self.provision().returncode, 0)
        self.assertFalse(self.destination.exists())

    def test_missing_required_model_file_is_rejected(self):
        del self.manifest["files"]["model.safetensors"]
        self.assertNotEqual(self.provision().returncode, 0)

    def test_legacy_lock_asset_contract_remains_supported(self):
        root = SCRIPT.parent.parent
        lock = json.loads((root / "crates/core/src/models/gliner2-lock.json").read_text())
        for name, identity in lock["files"].items():
            PROVISION.validate_asset(name, identity)


if __name__ == "__main__":
    unittest.main()
