"""Check typing metadata and downstream code against the installed wheel."""

import ast
import importlib.metadata
import inspect
import json
import subprocess
import sys
import tempfile
from pathlib import Path

import datafog_core

VALID = """
from typing import TYPE_CHECKING
from typing_extensions import assert_type
from datafog_core import (
    DataFogConfigurationError, DataFogFindingError, DataFogInternalError,
    DataFogKeyProviderError, TextRange, Finding, Transformation, TransformResult,
    Restoration, RestoreResult, FieldMapping, StructuredFinding,
    StructuredScanResult, StructuredTransformation, StructuredTransformResult,
    StructuredRestoration, StructuredRestoreResult, PrivacyManager,
    scan, transform, scan_and_transform, discover_fields, scan_structured,
    transform_structured, scan_and_transform_structured,
)
if TYPE_CHECKING:
    from datafog_core import (
        _TransformationConfig, _JsonDocument, _TokenizeItem, _TokenizeResult,
        _RestoreItem, _RestoreResult, _ResolvedKey,
    )

config: _TransformationConfig = {
    "default": {"strategy": "mask", "character": "*", "reveal": {"direction": "last", "count": 4}},
    "entities": ["EMAIL"],
    "overrides": {
        "PHONE": {"strategy": "remove"},
        "PERSON": {"strategy": "pseudonymize", "key_ref": "names", "key_version": "1"},
    },
    "allow": {"exact": {"EMAIL": ["safe@example.com"]}, "regex": {"EMAIL": [{"pattern": "safe", "case_sensitive": False}]}},
}
findings = scan("jane@example.com", {"locale": "en-US"})
assert_type(findings, list[Finding])
finding = findings[0]
assert_type(finding.entity_type, str)
assert_type(finding.matched_text, str)
assert_type(finding.byte_range, TextRange)
assert_type(finding.byte_range.start, int)
assert_type(finding.codepoint_range.end, int)
assert_type(finding.confidence, float | None)
assert_type(finding.detector_name, str)
assert_type(finding.detector_version, str | None)
manual = Finding("EMAIL", "jane@example.com", TextRange(0, 16), TextRange(0, 16), "custom", confidence=0.9)
assert_type(transform("jane@example.com", (manual,), config), TransformResult)
result = scan_and_transform("jane@example.com", {"scan": {"locale": "en-US"}, "transform": config})
assert_type(result.text, str)
assert_type(result.transformations, list[Transformation])
assert_type(result.transformations[0].source_byte_range, TextRange)
assert_type(result.transformations[0].output_codepoint_range, TextRange)
assert_type(result.transformations[0].resolved_key_version, str | None)
assert_type(result.transformations[0].resolved_token_version, str | None)
assert_type(scan("", None), list[Finding])
data: _JsonDocument = {"name": "Jane", "users": [None, True, 1, 1.5, {"email": "jane@example.com"}]}
assert_type(discover_fields(data, {"discover_person": False, "mappings": {"/name": "PERSON"}, "exclude": ["/users"]}), list[FieldMapping])
structured = scan_structured(data)
assert_type(structured, StructuredScanResult)
assert_type(structured.mappings, list[FieldMapping])
assert_type(structured.mappings[0].rule, str)
assert_type(structured.findings, list[StructuredFinding])
assert_type(structured.findings[0].finding, Finding)
assert_type(structured.findings[0].path, str)
located = StructuredFinding("/email", manual)
assert_type(transform_structured(data, [located], config), StructuredTransformResult)
structured_result = scan_and_transform_structured(data, {"transform": config})
assert_type(structured_result.data, _JsonDocument)
assert_type(structured_result.transformations, list[StructuredTransformation])
assert_type(structured_result.transformations[0].transformation, Transformation)

class KeyProvider:
    async def resolve_key(self, ref: str, version: str | None) -> _ResolvedKey:
        return {"key": bytes(range(32)), "resolved_version": version or "1"}

class AttributeKey:
    key: bytes = bytes(range(32))
    resolved_version: str = "1"

class AttributeKeyProvider:
    async def resolve_key(self, ref: str, version: str | None) -> AttributeKey:
        return AttributeKey()

class TokenProvider:
    async def tokenize_batch(self, scope: str, items: list[_TokenizeItem]) -> list[_TokenizeResult]:
        return [{"id": item["id"], "payload": b"token", "resolved_version": "1"} for item in items]
    async def restore_batch(self, scope: str, items: list[_RestoreItem]) -> list[_RestoreResult]:
        return [{"id": item["id"], "value": "restored"} for item in items]

PrivacyManager(AttributeKeyProvider())
async def use_manager() -> None:
    manager = PrivacyManager(provider=KeyProvider(), token_provider=TokenProvider())
    assert_type(await manager.transform("text", findings, config), TransformResult)
    assert_type(await manager.scan_and_transform("text", {"transform": config}), TransformResult)
    restored = await manager.restore("text", {"scope": "tenant"})
    assert_type(restored, RestoreResult)
    assert_type(restored.restorations, list[Restoration])
    assert_type(restored.restorations[0].token_ref, str)
    assert_type(restored.restorations[0].output_byte_range, TextRange)
    assert_type(await manager.transform_structured(data, structured.findings, config), StructuredTransformResult)
    assert_type(await manager.scan_and_transform_structured(data, {"transform": {"default": {"strategy": "tokenize", "token_ref": "names"}}}, {"scope": "tenant"}), StructuredTransformResult)
    restored_data = await manager.restore_structured(data, {"scope": "tenant"})
    assert_type(restored_data, StructuredRestoreResult)
    assert_type(restored_data.data, _JsonDocument)
    assert_type(restored_data.restorations, list[StructuredRestoration])
    assert_type(restored_data.restorations[0].restoration, Restoration)

for error in (DataFogConfigurationError(), DataFogFindingError(), DataFogInternalError(), DataFogKeyProviderError()):
    assert_type(error.code, str)
    assert_type(error.reason, str | None)
    assert_type(error.path, str | None)
    assert_type(error.finding_index, int | None)
"""

# Each line must produce a diagnostic; a missing stub or Any cannot satisfy this.
INVALID = """
from datafog_core import scan, transform, scan_structured, PrivacyManager, TransformResult
scan(123)  # error
scan("text", {"locale": 123})  # error
scan("text", {"max_bytes": 100})  # error
scan("text")[0].entity_type = "OTHER"  # error
scan("text")[0].byte_range.start = 1  # error
transform("text", ["not a finding"], {"default": {"strategy": "redact"}})  # error
transform("text", [], {"default": {"strategy": "invalid"}})  # error
transform("text", [], {"default": {"strategy": "pseudonymize"}})  # error
transform("text", [], {"default": {"strategy": "mask", "reveal": {"direction": "middle", "count": 4}}})  # error
scan_structured({"name": "Jane"}, {"mappings": {"/name": "EMAIL"}})  # error
scan_structured("not a document")  # error
PrivacyManager(object())  # error
PrivacyManager().restore("text")  # error
PrivacyManager().restore("text", {"scope": 12})  # error
TransformResult()  # error
"""


def verify_metadata() -> None:
    distribution = importlib.metadata.distribution("datafog-core")
    files = {str(path) for path in distribution.files or []}
    assert "datafog_core/py.typed" in files, files
    assert "datafog_core/__init__.pyi" in files, files
    stub = Path(distribution.locate_file("datafog_core/__init__.pyi"))
    tree = ast.parse(stub.read_text())
    declarations = {
        node.name: node
        for node in tree.body
        if isinstance(node, (ast.ClassDef, ast.FunctionDef))
        and not node.name.startswith("_")
    }
    native_module = importlib.import_module("datafog_core.datafog_core")
    exports = set(native_module.__all__)
    assert set(datafog_core.__all__) == exports
    assert set(declarations) == exports, (
        set(declarations) - exports,
        exports - set(declarations),
    )
    for name, declaration in declarations.items():
        value = getattr(datafog_core, name)
        if isinstance(declaration, ast.ClassDef):
            # Every native read-only field needs a corresponding typed property.
            properties = {
                member.name
                for member in declaration.body
                if isinstance(member, ast.FunctionDef)
                and any(
                    isinstance(d, ast.Name) and d.id == "property"
                    for d in member.decorator_list
                )
            }
            native = {
                key
                for key, member in vars(value).items()
                if (
                    inspect.isgetsetdescriptor(member)
                    or inspect.ismemberdescriptor(member)
                )
                and not key.startswith("_")
            }
            assert properties == native, (name, properties, native)


def main() -> None:
    verify_metadata()
    with tempfile.TemporaryDirectory(prefix="datafog-typing-") as temporary:
        root = Path(temporary)
        (root / "valid.py").write_text(VALID)
        (root / "invalid.py").write_text(INVALID)
        (root / "pyrightconfig.json").write_text(
            json.dumps(
                {
                    "typeCheckingMode": "strict",
                    "pythonVersion": "3.10",
                    "reportPrivateUsage": False,
                }
            )
        )
        for checker in ("mypy", "pyright"):
            command = [sys.executable, "-m", checker]
            if checker == "mypy":
                command += [
                    "--strict",
                    "--python-version",
                    "3.10",
                    "--no-incremental",
                    "--show-column-numbers",
                ]
            else:
                command += ["--pythonpath", sys.executable, "--outputjson"]
            valid = subprocess.run(
                command + ["valid.py"],
                cwd=root,
                capture_output=True,
                text=True,
                check=False,
            )
            assert valid.returncode == 0, (
                f"{checker} valid usage failed:\n{valid.stdout}\n{valid.stderr}"
            )
            invalid = subprocess.run(
                command + ["invalid.py"],
                cwd=root,
                capture_output=True,
                text=True,
                check=False,
            )
            assert invalid.returncode == 1, (
                f"{checker} expected type errors:\n{invalid.stdout}\n{invalid.stderr}"
            )
            expected = {
                number
                for number, line in enumerate(INVALID.splitlines(), 1)
                if "# error" in line
            }
            if checker == "pyright":
                diagnostics = json.loads(invalid.stdout)["generalDiagnostics"]
                actual = {
                    item["range"]["start"]["line"] + 1
                    for item in diagnostics
                    if item["severity"] == "error"
                }
            else:
                actual = {
                    int(line.split(":")[1])
                    for line in invalid.stdout.splitlines()
                    if line.startswith("invalid.py:") and ": error:" in line
                }
            assert actual == expected, (
                f"{checker}: expected lines {expected}, got {actual}\n{invalid.stdout}"
            )
            print(f"{checker}: valid usage passes; all invalid cases rejected.")
    print("Installed typing metadata covers every runtime export and native property.")


if __name__ == "__main__":
    main()
