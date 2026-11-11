import hashlib
import unittest

from generate import GATES, canonical_json, catalog, make_scenario


class GeneratorTests(unittest.TestCase):
    def test_fixture_is_deterministic_and_hash_covers_payload(self):
        first = make_scenario("storage_workspace", 23, 10, "fresh", "mostly_new", "unicode")
        second = make_scenario("storage_workspace", 23, 10, "fresh", "mostly_new", "unicode")
        self.assertEqual(first, second)
        expected_hash = first.pop("fixtureSha256")
        self.assertEqual(hashlib.sha256(canonical_json(first)).hexdigest(), expected_hash)

    def test_finding_offsets_are_utf8_bytes_for_unicode(self):
        fixture = make_scenario("storage_workspace", 1, 10, "fresh", "mostly_new", "unicode")
        for item in fixture["inputs"]:
            raw = item["text"].encode("utf-8")
            for finding in item["findings"]:
                matched = finding["matchedText"].encode("utf-8")
                self.assertEqual(raw[finding["startByte"]:finding["endByte"]], matched)

    def test_reused_values_are_seeded_and_restore_expectations_are_exact(self):
        fixture = make_scenario("storage_workspace", 3, 100, "fresh", "repeated_existing", "batch")
        self.assertEqual(fixture["expected"]["initialMappingCount"], 100)
        self.assertEqual(fixture["expected"]["finalMappingCount"], 100)
        self.assertEqual(fixture["expected"]["seedDistinctValues"], 100)
        self.assertEqual(fixture["expected"]["trialDistinctValues"], 32)
        self.assertLessEqual(len(fixture["seedInputs"]), 32)
        self.assertLessEqual(len(fixture["inputs"]), 32)
        self.assertEqual(
            fixture["expected"]["restoreTexts"],
            [{"inputId": item["id"], "text": item["text"]} for item in fixture["inputs"]],
        )

    def test_large_targets_fit_each_core_prepare_budget(self):
        for reuse in ("mostly_new", "repeated_existing"):
            fixture = make_scenario("storage_workspace", 1, 10_000, "long_history", reuse, "table")
            for phase in (fixture["seedInputs"], fixture["inputs"]):
                self.assertLessEqual(sum(len(item["text"].encode()) for item in phase), 1_048_576)
                self.assertLessEqual(sum(len(item["findings"]) for item in phase), 10_000)
                self.assertLessEqual(len(phase), 32)
            self.assertEqual(
                {key: fixture["expected"]["history"][key] for key in ("operations", "conversations", "messagesPerConversation")},
                {"operations": 1_000, "conversations": 100, "messagesPerConversation": 10},
            )
            recipe = fixture["expected"]["history"]["recipe"]
            self.assertEqual(recipe["expectedBeforeTrial"]["pendingIntents"], 0)
            self.assertEqual(recipe["expectedBeforeTrial"]["operations"], 3_001 + (reuse == "repeated_existing"))
            self.assertEqual(recipe["operationOutcomes"]["kind"], "user_cancelled")
            self.assertEqual(recipe["conversations"], 100)

    def test_every_shape_fits_the_max_mapping_target_without_silent_truncation(self):
        for shape in ("chat", "statement", "batch", "unicode", "table"):
            for reuse in ("mostly_new", "repeated_existing"):
                with self.subTest(shape=shape, reuse=reuse):
                    fixture = make_scenario("storage_workspace", 11, 10_000, "fresh", reuse, shape)
                    self.assertEqual(fixture["expected"]["trialFindingCount"], 10_000)
                    self.assertEqual(sum(len(item["findings"]) for item in fixture["inputs"]), 10_000)
                    self.assertLessEqual(fixture["expected"]["trialSourceBytes"], 1_048_576)
                    self.assertLessEqual(len(fixture["inputs"]), 32)
                    for item in fixture["inputs"]:
                        raw = item["text"].encode("utf-8")
                        for finding in item["findings"]:
                            self.assertEqual(raw[finding["startByte"]:finding["endByte"]], finding["matchedText"].encode("utf-8"))

    def test_catalog_covers_requested_dimensions(self):
        result = catalog(7)
        self.assertEqual(result["scenarioCount"], 160)
        self.assertEqual({case["mode"] for case in result["scenarios"]}, {"storage_workspace", "complete_extension"})
        self.assertEqual({case["mappingTarget"] for case in result["scenarios"]}, {10, 100, 1_000, 10_000})
        self.assertEqual({case["inputShape"] for case in result["scenarios"]}, {"chat", "statement", "batch", "unicode", "table"})
        self.assertEqual(len(GATES), 9)


if __name__ == "__main__":
    unittest.main()
