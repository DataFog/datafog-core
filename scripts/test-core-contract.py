#!/usr/bin/env python3
"""Prove inventory drift fails; actual behavior stays covered by SDK parity."""
import copy
import importlib.util
import json
from pathlib import Path
import shutil
import tempfile
import unittest

SPEC = importlib.util.spec_from_file_location("core_contract", Path(__file__).with_name("check-core-contract.py"))
CHECK = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CHECK)


class ContractInventoryTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.manifest = json.loads((CHECK.ROOT / CHECK.MANIFEST).read_text(encoding="utf-8"))
        paths = {"crates/core/src/workspace.rs", "crates/core/src/workspace/codec.rs", *self.manifest["relatedContracts"]}
        for binding in {**self.manifest["bindings"], **self.manifest.get("integrationBindings", {})}.values():
            paths.update((binding["runtime"], binding["declarations"], *binding.get("runtimeSources", [])))
        for entry in self.manifest.get("rustIntegrations", []):
            paths.update((entry["source"], *entry["tests"]))
        paths.update(entry["path"] for entry in self.manifest["behaviorFixtures"])
        paths.update(key["missingFieldFixture"] for key in self.manifest["snapshot"]["keys"])
        for relative in paths:
            target = self.root / relative
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(CHECK.ROOT / relative, target)

    def test_current_inventory(self):
        CHECK.validate(self.root, self.manifest)

    def test_contract_drift_rejected(self):
        for mutate in (
            lambda m: m.update(workspaceContractVersion=2),
            lambda m: m["workspaceErrorCodes"].pop(),
            lambda m: m["bindings"]["node"]["runtimeExports"].pop(),
            lambda m: m["behaviorFixtures"][0]["caseIds"].pop(),
            lambda m: m["behaviorFixtures"][0]["requiredMethods"].pop(),
            lambda m: m["snapshot"]["keys"].pop(),
            lambda m: m["snapshot"].update(readerVersions=[3]),
            lambda m: m["relatedContracts"].append("missing-contract.md"),
        ):
            with self.subTest(mutation=mutate):
                manifest = copy.deepcopy(self.manifest)
                mutate(manifest)
                with self.assertRaises(ValueError):
                    CHECK.validate(self.root, manifest)

    def test_false_public_declaration_rejected(self):
        path = self.root / self.manifest["bindings"]["node"]["declarations"]
        with path.open("a", encoding="utf-8") as stream:
            stream.write("\nexport declare function internalTransport(): void;\n")
        with self.assertRaisesRegex(ValueError, "public signatures and types"):
            CHECK.validate(self.root, self.manifest)

    def test_method_signature_and_type_field_drift_rejected(self):
        for sdk, old, new in (
            ("node", "revision: string", "revision: number"),
            ("wasm", "workspaceContractVersion(): number", "workspaceContractVersion(): string"),
            ("python", "workspace_contract_version() -> int", "workspace_contract_version() -> str"),
        ):
            with self.subTest(sdk=sdk):
                path = self.root / self.manifest["bindings"][sdk]["declarations"]
                original = path.read_text(encoding="utf-8")
                self.assertIn(old, original)
                path.write_text(original.replace(old, new, 1), encoding="utf-8")
                with self.assertRaisesRegex(ValueError, "public signatures and types"):
                    CHECK.validate(self.root, self.manifest)
                path.write_text(original, encoding="utf-8")

    def test_integration_signatures_and_runtime_registrations_cannot_drift(self):
        entry = self.manifest["integrationBindings"]["fogvault-python"]
        path = self.root / entry["runtime"]
        source = path.read_text(encoding="utf-8")
        self.assertIn("m.add_class::<PreparedChange>()?;", source)
        path.write_text(source.replace("m.add_class::<PreparedChange>()?;", ""), encoding="utf-8")
        with self.assertRaisesRegex(ValueError, "runtime exports"):
            CHECK.validate(self.root, self.manifest)
        path.write_text(source, encoding="utf-8")
        for sdk, entry in self.manifest["integrationBindings"].items():
            with self.subTest(sdk=sdk):
                path = self.root / entry["declarations"]
                source = path.read_text(encoding="utf-8")
                path.write_text(source + "\n// changed public declaration\n" if sdk != "fogvault-python" else source + "\n# changed public stub\n", encoding="utf-8")
                with self.assertRaisesRegex(ValueError, "public signatures and types"):
                    CHECK.validate(self.root, self.manifest)
                path.write_text(source, encoding="utf-8")

    def test_newline_style_is_not_contract_drift(self):
        source = "first\nsecond\n"
        self.assertEqual(CHECK.declaration_sha256(source), CHECK.declaration_sha256(source.replace("\n", "\r\n")))

    def test_missing_field_rejection_cannot_disappear(self):
        path = self.root / self.manifest["snapshot"]["keys"][10]["missingFieldFixture"]
        fixture = json.loads(path.read_text(encoding="utf-8"))
        fixture["rejections"] = [case for case in fixture["rejections"] if case["id"] != "missing_top_key_10"]
        path.write_text(json.dumps(fixture), encoding="utf-8")
        with self.assertRaisesRegex(ValueError, "key 10"):
            CHECK.validate(self.root, self.manifest)


if __name__ == "__main__":
    unittest.main()
