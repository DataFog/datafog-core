"""Offline rejection tests for the reference-import candidate source binding."""
import copy
import hashlib
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

spec = importlib.util.spec_from_file_location("legacy_gate", Path(__file__).with_name("verify-fogvault-legacy.py"))
gate = importlib.util.module_from_spec(spec)
spec.loader.exec_module(gate)

class PreparedInputsTests(unittest.TestCase):
    def test_exact_inputs_and_actual_artifact_bytes_required(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            archive = root/"candidate.tgz"
            archive.write_bytes(b"synthetic candidate")
            proof = {"schema":"datafog-fogvault-preparation/v1","pass":True,"fogvault_commit":gate.REV,
                     "core_build_inputs":{"core":"exact bytes"},"artifacts":{
                         k:{"path":str(archive),"sha256":hashlib.sha256(archive.read_bytes()).hexdigest()}
                         for k in ("core_tarball","vault_tarball")}}
            marker = root/"prepared.json"
            old = gate.core_build_inputs
            try:
                gate.core_build_inputs = lambda _: {"core":"exact bytes"}
                marker.write_text(json.dumps(proof));gate.validate(marker)
                for key, value in (("schema","wrong"),("pass",False),("fogvault_commit","wrong"),
                                   ("core_build_inputs",{"core":"stale bytes"})):
                    broken = copy.deepcopy(proof);broken[key]=value;marker.write_text(json.dumps(broken))
                    with self.subTest(key=key),self.assertRaises(ValueError):gate.validate(marker)
                marker.write_text(json.dumps(proof));archive.write_bytes(b"changed")
                with self.assertRaises(ValueError):gate.validate(marker)
                archive.unlink()
                with self.assertRaises(OSError):gate.validate(marker)
            finally:
                gate.core_build_inputs = old

if __name__ == "__main__":unittest.main()
