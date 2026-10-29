"""Offline G1 protocol/strict-restore proofs with the actual installed Core wheel."""
from pathlib import Path
import importlib.util
import json
import re
import tempfile
import unittest
import copy
import shutil
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[3]
spec = importlib.util.spec_from_file_location("provider_marker_preparation", ROOT / "scripts/provider_markers/preparation.py")
prep = importlib.util.module_from_spec(spec)
spec.loader.exec_module(prep)
if str(ROOT) not in sys.path:
    sys.path.insert(0, str(ROOT))
from scripts.provider_markers import scoring

class ProviderMarkerPreparationTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.temporary = tempfile.TemporaryDirectory(prefix="datafog-g1-offline-")
        cls.directory = Path(cls.temporary.name)
        prep.prepare(cls.directory)
        cls.manifest = json.loads((cls.directory / "manifest.json").read_text())
        cls.gold = json.loads((cls.directory / "gold/workspace.json").read_text())
        cls.api, _ = prep.native()

    @classmethod
    def tearDownClass(cls):
        cls.temporary.cleanup()

    def metadata(self):
        missing = ["reportedModelId", "modelVersion", "providerRequestId"]
        sampling = dict.fromkeys(("temperature", "topP", "seed", "maxOutputTokens"))
        usage = dict.fromkeys(("inputTokens", "outputTokens", "costAmount", "currency", "tokenizerId", "tokenizerVersion"))
        missing += ["sampling." + key for key in sampling] + ["usage." + key for key in usage]
        return {"schemaVersion": 1, "recordKind": "protocol_test", "provider": "chatgpt", "executionSurface": "web",
                "modelLabel": "synthetic protocol identity; no provider executed", "reportedModelId": None, "modelVersion": None,
                "capturedAt": "2026-10-29T00:00:00Z", "promptId": "summarize", "promptSha256": self.manifest["tasks"][0]["promptSha256"],
                "fixtureManifestSha256": prep.digest((self.directory / "manifest.json").read_bytes()),
                "sampling": sampling, "unavailableMetadata": missing, "usage": usage,
                "providerRequestId": None, "responseTruncated": False}

    def test_fifty_actual_core_markers_are_deterministic_typed_and_omission_sets_explicit(self):
        rows = self.gold["items"]
        self.assertEqual(len(rows), 50)
        self.assertEqual(len({row["marker"] for row in rows}), 50)
        self.assertEqual({kind: sum(row["entityType"] == kind for row in rows) for kind in prep.TYPES}, dict.fromkeys(prep.TYPES, 5))
        verified = prep.verify(self.directory)
        self.assertTrue(verified["coreFixtureVerified"])
        self.assertEqual(verified["providerRunsExecuted"], 0)
        self.assertFalse(verified["g1Pass"])
        self.assertIsNone(verified["preservationRate"])
        self.assertEqual([len(task["requiredMarkerIds"]) for task in self.manifest["tasks"]], [50, 30, 50, 20, 20])
        self.assertEqual(self.manifest["draftRequiredReferenceSlotsPerProviderRepetition"], 170)
        self.assertEqual(self.manifest["denominatorStatus"], "draft_requires_review")
        for key in ("wheelSha256", "nativeBinarySha256", "cargoLockSha256", "generatorSha256"):
            self.assertEqual(len(verified["source"][key]), 64)
        self.assertEqual(prep.verify(ROOT / "fixtures/provider-markers")["providerRunsExecuted"], 0)

    def test_core_restores_verbatim_case_crockford_and_keeps_all_coordinate_systems(self):
        with self.api.Workspace.open_bundle(bytes.fromhex(self.gold["bundleHex"])) as workspace:
            for row in self.gold["items"]:
                self.assertEqual(workspace.restore(row["marker"]), row["original"])
                self.assertEqual(workspace.restore(row["marker"].lower()), row["original"])
            row = next(row for row in self.gold["items"] if any(value in row["marker"].rsplit("_", 1)[1] for value in "01"))
            label, code = row["marker"][2:-2].rsplit("_", 1)
            tolerated = "[[" + label.lower() + "_" + code.replace("0", "O").replace("1", "l") + "]]"
            self.assertEqual(workspace.restore(tolerated), row["original"])
            prefix = "👋 e\u0301 "
            bare = row["marker"][2:-2]
            report = workspace.restore_report(prefix + bare)
            self.assertIsNone(report["text"])
            issue = report["issues"][0]
            self.assertEqual(issue["kind"], "unbracketed")
            self.assertEqual(issue["byteRange"], {"start": 9, "end": 9 + len(bare)})
            self.assertEqual(issue["codepointRange"], {"start": 5, "end": 5 + len(bare)})
            self.assertEqual(issue["utf16Range"], {"start": 6, "end": 6 + len(bare)})

    def test_core_rejects_damaged_wrong_label_foreign_and_unissued_markers_atomically(self):
        with self.api.Workspace.open_bundle(bytes.fromhex(self.gold["bundleHex"])) as workspace:
            before = workspace.export_bundle()
            row = self.gold["items"][0]
            pending = workspace.prepare("SYNTHETIC PENDING", [self.api.WorkspaceFinding("PERSON", "SYNTHETIC PENDING", self.api.TextRange(0, 17))], bytes([231]) * 16)
            unissued = pending.preview()["text"]
            self.assertNotIn(unissued, [value["marker"] for value in self.gold["items"]])
            code = row["marker"][2:-2].rsplit("_", 1)[1]
            wrong = "[[EMAIL_" + code + "]]"
            for bad, kind in ((unissued, "unknown"), (wrong, "unknown"), ("[[PERSON_BAD]]", "damaged"), (row["marker"][2:-2], "unbracketed")):
                report = workspace.restore_report(row["marker"] + " then " + bad)
                self.assertIsNone(report["text"])
                self.assertEqual(report["restoredMarkers"], 1)
                self.assertEqual(report["issues"][0]["kind"], kind)
                with self.assertRaises(self.api.DataFogWorkspaceError): workspace.restore(bad)
            pending.discard()
            with self.api.Workspace.create(bytes([232]) * 16, bytes([233]) * 16) as foreign:
                plan = foreign.prepare("SYNTHETIC FOREIGN", [self.api.WorkspaceFinding("PERSON", "SYNTHETIC FOREIGN", self.api.TextRange(0, 17))], bytes([234]) * 16)
                marker = plan.preview()["text"]
                foreign.activate_saved(plan, plan.expected_head(), plan.candidate_head())
                self.assertNotIn(marker, [value["marker"] for value in self.gold["items"]])
                self.assertIsNone(workspace.restore_report(marker)["text"])
            self.assertEqual(workspace.export_bundle(), before)

    def test_capture_requires_hashes_explicit_identity_and_public_configuration_only(self):
        metadata = self.metadata()
        record = prep.validate_capture(metadata, self.gold["items"][0]["marker"], self.directory)
        self.assertTrue(record["coreAnalysis"]["textAvailable"])
        self.assertNotIn("text", record["coreAnalysis"])
        self.assertFalse(record["qualification"]["g1Pass"])
        self.assertEqual(record["metadata"]["recordKind"], "protocol_test")
        for key in ("promptSha256", "fixtureManifestSha256"):
            invalid = copy.deepcopy(metadata); invalid[key] = "0" * 64
            with self.assertRaises(ValueError): prep.validate_capture(invalid, "synthetic", self.directory)
        for change in (lambda value: value.update(apiKey="must never be captured"), lambda value: value["unavailableMetadata"].clear(), lambda value: value["sampling"].update(temperature=float("nan")), lambda value: value.update(capturedAt="yesterday"), lambda value: value.update(g1Pass=True)):
            invalid = copy.deepcopy(metadata); change(invalid)
            with self.assertRaises(ValueError): prep.validate_capture(invalid, "synthetic", self.directory)

    def test_rehashed_prompt_partition_or_generator_edits_require_explicit_review(self):
        for change in ("prompt", "partition", "generator", "gold", "pass"):
            with self.subTest(change=change), tempfile.TemporaryDirectory() as temporary:
                directory = Path(temporary) / "packet"
                shutil.copytree(self.directory, directory)
                manifest = json.loads((directory / "manifest.json").read_text())
                if change == "prompt":
                    prompt = directory / manifest["tasks"][0]["path"]
                    prompt.write_text(prompt.read_text() + self.gold["items"][0]["original"])
                    manifest["files"][manifest["tasks"][0]["path"]] = prep.digest(prompt.read_bytes())
                    manifest["tasks"][0]["promptSha256"] = prep.digest(prompt.read_bytes())
                elif change == "partition":
                    task = manifest["tasks"][1]
                    task["requiredMarkerIds"][0], task["permittedOmittedMarkerIds"][0] = task["permittedOmittedMarkerIds"][0], task["requiredMarkerIds"][0]
                elif change == "generator":
                    manifest["source"]["generatorSha256"] = "0" * 64
                elif change == "gold":
                    gold = directory / "gold/workspace.json"
                    value = json.loads(gold.read_text()); value["items"][0]["original"] = "CHANGED SYNTHETIC VALUE"
                    gold.write_bytes(prep.canonical(value))
                    manifest["files"]["gold/workspace.json"] = prep.digest(gold.read_bytes())
                else:
                    report = directory / "preparation-report.json"
                    value = json.loads(report.read_text()); value["g1Pass"] = True
                    report.write_bytes(prep.canonical(value))
                (directory / "manifest.json").write_bytes(prep.canonical(manifest))
                with self.assertRaises(ValueError): prep.verify(directory)

    def test_local_capture_cli_keeps_response_bytes_and_schema_fields(self):
        schema = json.loads((ROOT / "scripts/provider_markers/capture-metadata.schema.json").read_text())
        self.assertEqual(set(schema["properties"]), prep.METADATA_KEYS)
        with tempfile.TemporaryDirectory() as temporary:
            metadata = Path(temporary) / "metadata.json"
            response = Path(temporary) / "response.txt"
            metadata.write_bytes(prep.canonical(self.metadata()))
            raw = ("Synthetic reply\r\n" + self.gold["items"][0]["marker"] + "\r\n").encode()
            response.write_bytes(raw)
            output = subprocess.check_output([sys.executable, str(ROOT / "scripts/provider_markers/preparation.py"), "capture", str(self.directory), "--metadata", str(metadata), "--response", str(response)], cwd=ROOT)
            captured = json.loads(output)
            self.assertEqual(captured["response"]["text"].encode(), raw)
            self.assertEqual(captured["response"]["sha256"], prep.digest(raw))
            self.assertEqual(captured["response"]["utf8Bytes"], len(raw))
            self.assertFalse(captured["qualification"]["g1Pass"])

    def test_empty_or_damaged_response_cannot_become_a_fake_provider_pass(self):
        for response in ("No markers returned.", '{"g1Pass":true}', "[[PERSON_BAD]]", self.gold["items"][0]["marker"]):
            record = prep.validate_capture(self.metadata(), response, self.directory)
            self.assertFalse(record["qualification"]["g1Pass"])
            self.assertIsNone(record["qualification"]["preservationRate"])
        missing = prep.validate_capture(self.metadata(), "No markers returned.", self.directory)
        self.assertTrue(missing["coreAnalysis"]["textAvailable"])
        self.assertEqual(missing["coreAnalysis"]["restoredMarkers"], 0)


class ProviderMarkerScoringTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.directory = ROOT / "fixtures/provider-markers"
        cls.manifest = json.loads((cls.directory / "manifest.json").read_bytes())
        cls.gold = json.loads((cls.directory / "gold/workspace.json").read_bytes())
        cls.rows = {row["id"]: row for row in cls.gold["items"]}
        cls.api, _ = prep.native()

    def response(self, task):
        return "\n".join(self.rows[identifier]["marker"] for identifier in task["requiredMarkerIds"])

    def score(self, task, response):
        value = scoring.score_response(self.directory, task["id"], response)
        self.assertEqual(value["qualification"], {"providerRunsExecuted": 0, "preservationRate": None, "g1Pass": False})
        self.assertNotIn("text", value["coreAnalysis"])
        for row in self.gold["items"]:
            self.assertNotIn(row["original"], json.dumps(value))
        return value

    def test_all_five_required_sets_score_without_aggregate_or_provider_pass(self):
        before = {name: prep.digest((self.directory / name).read_bytes()) for name in self.manifest["files"]}
        for task, expected in zip(self.manifest["tasks"], [50, 30, 50, 20, 20], strict=True):
            with self.subTest(task=task["id"]):
                value = self.score(task, self.response(task))
                self.assertEqual(value["requiredReferences"], expected)
                self.assertEqual(value["successfulRequiredReferences"], expected)
                self.assertEqual(value["missingRequiredMarkerIds"], [])
                self.assertEqual(value["observedOptionalMarkerIds"], [])
                self.assertTrue(value["taskScorecardSatisfied"])
        self.assertEqual(sum(len(task["requiredMarkerIds"]) for task in self.manifest["tasks"]), 170)
        self.assertEqual(before, {name: prep.digest((self.directory / name).read_bytes()) for name in self.manifest["files"]})

    def test_repetitions_cannot_offset_one_missing_reference_on_any_task(self):
        for task in self.manifest["tasks"]:
            with self.subTest(task=task["id"]):
                required = task["requiredMarkerIds"]
                response = " ".join(self.rows[value]["marker"] for value in required[:-1])
                response += " " + self.rows[required[0]]["marker"] * 5
                value = self.score(task, response)
                self.assertEqual(value["missingRequiredMarkerIds"], [required[-1]])
                self.assertEqual(value["observedRequiredReferences"], len(required) - 1)
                self.assertEqual(value["successfulRequiredReferences"], len(required) - 1)
                self.assertEqual(value["repeatedOccurrences"], 5)
                self.assertFalse(value["taskScorecardSatisfied"])

    def test_optional_markers_can_be_omitted_or_returned_without_enlarging_denominator(self):
        for task in self.manifest["tasks"]:
            value = self.score(task, (self.directory / task["path"]).read_text())
            self.assertTrue(value["taskScorecardSatisfied"])
            self.assertEqual(value["requiredReferences"], len(task["requiredMarkerIds"]))
            self.assertEqual(value["observedOptionalMarkerIds"], task["permittedOmittedMarkerIds"])

    def test_strict_failure_zeros_restoration_credit_despite_complete_observed_set(self):
        task = self.manifest["tasks"][1]
        optional = self.rows[task["permittedOmittedMarkerIds"][0]]["marker"]
        wrong_label = "[[FAKE_" + optional[2:-2].rsplit("_", 1)[1] + "]]"
        for invalid in [optional[2:-2], "[[PERSON_BAD]]", wrong_label]:
            with self.subTest(invalid=invalid):
                value = self.score(task, "👋 e\u0301 " + self.response(task) + " " + invalid)
                self.assertEqual(value["observedRequiredReferences"], 30)
                self.assertEqual(value["missingRequiredMarkerIds"], [])
                self.assertEqual(value["successfulRequiredReferences"], 0)
                self.assertFalse(value["coreAnalysis"]["textAvailable"])
                self.assertFalse(value["taskScorecardSatisfied"])
                with self.api.Workspace.open_bundle(bytes.fromhex(self.gold["bundleHex"])) as workspace:
                    actual = workspace.restore_report("👋 e\u0301 " + self.response(task) + " " + invalid)
                    self.assertEqual(value["coreAnalysis"]["issues"], actual["issues"])
                    self.assertTrue(all(set(issue) >= {"byteRange", "codepointRange", "utf16Range"} for issue in actual["issues"]))

    def test_foreign_unissued_and_case_crockford_tolerance_are_decided_by_core(self):
        task = self.manifest["tasks"][3]
        with self.api.Workspace.open_bundle(bytes.fromhex(self.gold["bundleHex"])) as workspace:
            before = workspace.export_bundle()
            pending = workspace.prepare("SYNTHETIC PENDING", [self.api.WorkspaceFinding("PERSON", "SYNTHETIC PENDING", self.api.TextRange(0, 17))], bytes([231]) * 16)
            unissued = pending.preview()["text"]
            value = self.score(task, self.response(task) + " " + unissued)
            self.assertEqual(value["successfulRequiredReferences"], 0)
            self.assertEqual(value["coreAnalysis"]["issues"][0]["kind"], "unknown")
            pending.discard()
            self.assertEqual(workspace.export_bundle(), before)
        with self.api.Workspace.create(bytes([232]) * 16, bytes([233]) * 16) as foreign:
            plan = foreign.prepare("SYNTHETIC FOREIGN", [self.api.WorkspaceFinding("PERSON", "SYNTHETIC FOREIGN", self.api.TextRange(0, 17))], bytes([234]) * 16)
            marker = plan.preview()["text"]
            foreign.activate_saved(plan, plan.expected_head(), plan.candidate_head())
            self.assertEqual(self.score(task, self.response(task) + " " + marker)["successfulRequiredReferences"], 0)
        transformed = []
        for identifier in task["requiredMarkerIds"]:
            marker = self.rows[identifier]["marker"]
            label, code = marker[2:-2].rsplit("_", 1)
            transformed.append("[[" + label.lower() + "_" + code.replace("0", "O").replace("1", "l") + "]]")
        value = self.score(task, " ".join(transformed))
        self.assertEqual(value["coreToleratedOccurrences"], 20)
        self.assertEqual(value["verbatimOccurrences"], 0)
        self.assertTrue(value["taskScorecardSatisfied"])

    def test_literal_originals_empty_prose_and_nested_brackets_do_not_fabricate_credit(self):
        task = self.manifest["tasks"][0]
        for response in ["", "ordinary prose", " ".join(row["original"] for row in self.gold["items"])]:
            value = self.score(task, response)
            self.assertEqual(value["observedRequiredReferences"], 0)
            self.assertEqual(len(value["missingRequiredMarkerIds"]), 50)
            self.assertFalse(value["taskScorecardSatisfied"])
        # Extra outer brackets do not conceal the actual Core-recognized inner tokens.
        nested = " ".join("[" + row["marker"] + "]" for row in self.gold["items"])
        self.assertTrue(self.score(task, nested)["taskScorecardSatisfied"])

    def test_approval_partition_and_occurrence_total_disagreement_fail_closed(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary) / "packet"
            shutil.copytree(self.directory, directory)
            path = directory / "scorecard-review.json"
            review = json.loads(path.read_text())
            review["tasks"][0]["requiredMarkerIds"].pop()
            path.write_bytes(prep.canonical(review))
            with self.assertRaisesRegex(ValueError, "approved initial baseline"):
                scoring.score_response(directory, "summarize", "ordinary prose")
        from unittest.mock import patch
        with patch.object(scoring, "CANDIDATES", re.compile(r"(?!)")):
            with self.assertRaisesRegex(ValueError, "full Core report"):
                scoring.score_response(self.directory, "summarize", self.response(self.manifest["tasks"][0]))
        with self.assertRaisesRegex(ValueError, "unknown reviewed task"):
            scoring.score_response(self.directory, "unreviewed", "")

    def test_offline_cli_preserves_raw_hash_and_has_no_provider_pass(self):
        task = self.manifest["tasks"][1]
        with tempfile.TemporaryDirectory() as temporary:
            response = Path(temporary) / "response.txt"
            raw = (self.response(task) + "\r\n").encode()
            response.write_bytes(raw)
            result = json.loads(subprocess.check_output([sys.executable, "-m", "scripts.provider_markers.scoring", str(self.directory), "--task", task["id"], "--response", str(response)], cwd=ROOT))
            self.assertEqual(result["responseSha256"], prep.digest(raw))
            self.assertEqual(result["responseUtf8Bytes"], len(raw))
            self.assertTrue(result["taskScorecardSatisfied"])
            self.assertFalse(result["qualification"]["g1Pass"])


def verify_provider_marker_preparation():
    suite = unittest.TestSuite(unittest.defaultTestLoader.loadTestsFromTestCase(case) for case in (ProviderMarkerPreparationTests, ProviderMarkerScoringTests))
    result = unittest.TextTestRunner(verbosity=1).run(suite)
    if not result.wasSuccessful(): raise AssertionError("offline provider marker preparation failed")

if __name__ == "__main__":
    verify_provider_marker_preparation()
