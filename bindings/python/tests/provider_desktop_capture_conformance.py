"""Offline additive desktop capture, with actual installed Core and legacy parity."""
from pathlib import Path
import copy
import json
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[3]
if str(ROOT) not in sys.path:
    sys.path.insert(0, str(ROOT))
from scripts.provider_markers import preparation, capture


class ProviderDesktopCaptureTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.directory = ROOT / "fixtures/provider-markers"
        cls.manifest = json.loads((cls.directory / "manifest.json").read_bytes())
        cls.gold = json.loads((cls.directory / "gold/workspace.json").read_bytes())
        cls.api, _ = preparation.native()

    def metadata(self, surface="desktop"):
        sampling = dict.fromkeys(("temperature", "topP", "seed", "maxOutputTokens"))
        usage = dict.fromkeys(("inputTokens", "outputTokens", "costAmount", "currency", "tokenizerId", "tokenizerVersion"))
        unavailable = ["reportedModelId", "modelVersion", "providerRequestId"]
        unavailable += [f"{name}.{key}" for name, values in (("sampling", sampling), ("usage", usage)) for key in values]
        return {"schemaVersion": 1, "recordKind": "protocol_test", "provider": "claude", "executionSurface": surface,
                "modelLabel": "Fable 5.1 High", "reportedModelId": None, "modelVersion": None,
                "capturedAt": "2026-10-29T00:00:00Z", "promptId": "summarize", "promptSha256": self.manifest["tasks"][0]["promptSha256"],
                "fixtureManifestSha256": preparation.digest((self.directory / "manifest.json").read_bytes()),
                "sampling": sampling, "unavailableMetadata": unavailable, "usage": usage,
                "providerRequestId": None, "responseTruncated": False}

    def test_web_api_capture_outputs_remain_exactly_equal_to_historical_collector(self):
        marker = self.gold["items"][0]["marker"]
        for surface in ["web", "api"]:
            for provider in ["chatgpt", "claude", "gemini"]:
                for kind in ["protocol_test", "provider_capture"]:
                    for response in ["ordinary prose\r\n", "👋 " + marker, marker + " [[PERSON_BAD]]"]:
                        with self.subTest(surface=surface, provider=provider, kind=kind, response=response):
                            metadata = self.metadata(surface); metadata.update(provider=provider, recordKind=kind)
                            old = preparation.validate_capture(metadata, response, self.directory)
                            new = capture.validate_capture(metadata, response, self.directory)
                            self.assertEqual(preparation.canonical(new), preparation.canonical(old))
                            self.assertNotIn("captureWitness", new)

    def test_desktop_keeps_real_surface_opaque_visible_label_and_explicit_missing_identity(self):
        metadata = self.metadata()
        before = copy.deepcopy(metadata)
        result = capture.validate_capture(metadata, self.gold["items"][0]["marker"], self.directory)
        self.assertEqual(result["metadata"], before)
        self.assertEqual(metadata, before)
        self.assertEqual(result["metadata"]["executionSurface"], "desktop")
        self.assertEqual(result["metadata"]["modelLabel"], "Fable 5.1 High")
        for key in ["reportedModelId", "modelVersion", "providerRequestId"]:
            self.assertIsNone(result["metadata"][key]); self.assertIn(key, result["metadata"]["unavailableMetadata"])
        self.assertEqual(result["captureWitness"]["executionSurface"], "desktop")
        self.assertEqual(result["captureWitness"]["collectorSha256"], preparation.digest((ROOT / "scripts/provider_markers/capture.py").read_bytes()))
        self.assertEqual(result["qualification"], {"state": "unreviewed", "g1Pass": False, "preservationRate": None})
        with self.assertRaises(ValueError): preparation.validate_capture(metadata, "ordinary", self.directory)

    def test_closed_fields_types_null_accounting_and_hashes_fail_on_all_surfaces(self):
        changes = [
            lambda m: m.pop("modelVersion"), lambda m: m.update(apiKey="forbidden"),
            lambda m: m.update(appVersion="unreviewed extra field"), lambda m: m.update(schemaVersion=True),
            lambda m: m.update(recordKind="qualified"), lambda m: m.update(modelLabel=" "),
            lambda m: m.update(reportedModelId=False), lambda m: m.update(modelVersion=5.1),
            lambda m: m.update(providerRequestId=[]), lambda m: m.update(capturedAt="yesterday"),
            lambda m: m.update(capturedAt="2026-10-29T00:00:00+01:00"), lambda m: m.update(responseTruncated=0),
            lambda m: m.update(unavailableMetadata=[]), lambda m: m["unavailableMetadata"].append("unknown"),
            lambda m: m["unavailableMetadata"].append(m["unavailableMetadata"][0]),
            lambda m: m["unavailableMetadata"].append(False), lambda m: m["sampling"].update(secret="forbidden"),
            lambda m: m["sampling"].pop("seed"), lambda m: m["sampling"].update(temperature=float("nan")),
            lambda m: m["sampling"].update(topP=True), lambda m: m["sampling"].update(seed=0.5),
            lambda m: m["usage"].update(inputTokens=-1), lambda m: m["usage"].update(outputTokens=0.5),
            lambda m: m["usage"].update(costAmount=float("inf")), lambda m: m["usage"].update(currency=" "),
            lambda m: m["usage"].update(tokenizerVersion=False), lambda m: m.update(promptId="unknown"),
            lambda m: m.update(promptSha256="0" * 64), lambda m: m.update(fixtureManifestSha256="0" * 64),
        ]
        for surface in ["web", "api", "desktop"]:
            for index, change in enumerate(changes):
                with self.subTest(surface=surface, mutation=index):
                    metadata = self.metadata(surface); change(metadata)
                    with self.assertRaises(ValueError): capture.validate_capture(metadata, "synthetic", self.directory)
                    if surface != "desktop":
                        with self.assertRaises(ValueError): preparation.validate_capture(metadata, "synthetic", self.directory)
        for invalid in [None, [], {}, {"executionSurface": "unknown"}]:
            with self.assertRaises(ValueError): capture.validate_capture(invalid, "synthetic", self.directory)

    def test_desktop_does_not_guess_other_providers_or_project_unknown_surfaces(self):
        for provider, surface in [("chatgpt", "desktop"), ("gemini", "desktop"), ("other", "desktop"), ("claude", "Desktop"), ("claude", "desktop_web"), ("claude", None)]:
            metadata = self.metadata(); metadata.update(provider=provider, executionSurface=surface)
            with self.assertRaises(ValueError): capture.validate_capture(metadata, "synthetic", self.directory)

    def test_genuinely_observed_public_metadata_must_not_be_listed_as_unavailable(self):
        metadata = self.metadata()
        for key in ["reportedModelId", "modelVersion", "providerRequestId"]:
            metadata[key] = "synthetic observed identity"
            metadata["unavailableMetadata"].remove(key)
        for name, values in [("sampling", {"temperature": 0.0, "topP": 1.0, "seed": 0, "maxOutputTokens": 4000}),
                             ("usage", {"inputTokens": 0, "outputTokens": 0, "costAmount": 0.0, "currency": "USD", "tokenizerId": "synthetic tokenizer", "tokenizerVersion": "synthetic version"})]:
            for key, value in values.items():
                metadata[name][key] = value; metadata["unavailableMetadata"].remove(f"{name}.{key}")
        self.assertEqual(metadata["unavailableMetadata"], [])
        self.assertEqual(capture.validate_capture(metadata, "ordinary", self.directory)["metadata"], metadata)
        metadata["unavailableMetadata"].append("reportedModelId")
        with self.assertRaises(ValueError): capture.validate_capture(metadata, "ordinary", self.directory)

    def test_core_analysis_retains_offsets_and_no_restored_private_values(self):
        marker = self.gold["items"][0]["marker"]
        for response in ["\r\n" + marker + "\r\n", "👋 e\u0301 " + marker + " " + marker[2:-2], "ordinary truncated output [[PERSON_BAD]]"]:
            metadata = self.metadata(); metadata["responseTruncated"] = True
            result = capture.validate_capture(metadata, response, self.directory)
            self.assertEqual(result["response"], {"text": response, "sha256": preparation.digest(response.encode()), "utf8Bytes": len(response.encode())})
            self.assertTrue(result["metadata"]["responseTruncated"])
            with self.api.Workspace.open_bundle(bytes.fromhex(self.gold["bundleHex"])) as workspace:
                actual = workspace.restore_report(response)
            self.assertEqual(result["coreAnalysis"], {"textAvailable": actual["text"] is not None, "restoredMarkers": actual["restoredMarkers"], "issues": actual["issues"]})
            self.assertNotIn("text", result["coreAnalysis"])
            self.assertNotIn(self.gold["items"][0]["original"], json.dumps(result))
            self.assertFalse(result["qualification"]["g1Pass"])

    def test_schema_addition_does_not_change_historical_closed_metadata(self):
        old = json.loads((ROOT / "scripts/provider_markers/capture-metadata.schema.json").read_bytes())
        new = json.loads((ROOT / "scripts/provider_markers/capture-with-desktop.schema.json").read_bytes())
        self.assertEqual(set(new["properties"]), preparation.METADATA_KEYS)
        self.assertEqual(new["required"], old["required"])
        self.assertFalse(new["additionalProperties"])
        self.assertEqual(old["properties"]["executionSurface"]["enum"], ["web", "api"])
        self.assertEqual(new["properties"]["executionSurface"]["enum"], ["web", "api", "desktop"])
        for key in preparation.METADATA_KEYS - {"executionSurface"}:
            self.assertEqual(new["properties"][key], old["properties"][key])
        self.assertEqual(new["allOf"][0]["then"]["properties"]["provider"]["const"], "claude")

    def test_cli_preserves_utf8_crlf_and_web_api_json_bytes_and_rejects_invalid_utf8(self):
        raw = ("👋 synthetic\r\n" + self.gold["items"][0]["marker"] + "\r\n").encode()
        with tempfile.TemporaryDirectory() as temporary:
            metadata = Path(temporary) / "metadata.json"; response = Path(temporary) / "response.txt"
            response.write_bytes(raw)
            for surface in ["web", "api", "desktop"]:
                metadata.write_bytes(preparation.canonical(self.metadata(surface)))
                command = [sys.executable, "-m", "scripts.provider_markers.capture", str(self.directory), "--metadata", str(metadata), "--response", str(response)]
                actual = subprocess.check_output(command, cwd=ROOT)
                parsed = json.loads(actual)
                self.assertEqual(parsed["response"]["text"].encode(), raw)
                self.assertEqual(parsed["response"]["sha256"], preparation.digest(raw))
                if surface != "desktop":
                    previous = subprocess.check_output([sys.executable, "-m", "scripts.provider_markers.preparation", "capture", str(self.directory), "--metadata", str(metadata), "--response", str(response)], cwd=ROOT)
                    self.assertEqual(actual, previous)
                response.write_bytes(b"\xff")
                invalid = subprocess.run(command, cwd=ROOT, capture_output=True)
                self.assertNotEqual(invalid.returncode, 0); self.assertEqual(invalid.stdout, b"")
                response.write_bytes(raw)


def verify_provider_desktop_capture():
    result = unittest.TextTestRunner(verbosity=1).run(unittest.defaultTestLoader.loadTestsFromTestCase(ProviderDesktopCaptureTests))
    if not result.wasSuccessful():
        raise AssertionError("offline desktop provider capture failed")


if __name__ == "__main__":
    verify_provider_desktop_capture()
