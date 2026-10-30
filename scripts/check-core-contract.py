#!/usr/bin/env python3
"""Check the reviewed Core contract inventory against committed source and fixtures.

This source check complements, and never replaces, installed four-SDK parity.
"""
import ast
import json
import hashlib
from pathlib import Path
import re
import sys

ROOT = Path(__file__).resolve().parents[1]
MANIFEST = "docs/contracts/core-v1.json"


def javascript_exports(source):
    names = set(re.findall(r"^export (?:declare )?(?:async )?(?:function|class|const|let|var) (\w+)", source, re.M))
    for block in re.findall(r"^export \{([^}]+)\}", source, re.M):
        for item in block.split(","):
            item = item.strip()
            if item:
                names.add(item.split(" as ")[-1].strip())
    return sorted(names)


def python_exports(source):
    module = ast.parse(source)
    for node in module.body:
        if isinstance(node, ast.Assign) and any(isinstance(target, ast.Name) and target.id == "__all__" for target in node.targets):
            names = ast.literal_eval(node.value)
            if len(names) != len(set(names)):
                raise ValueError("duplicate Python __all__ export")
            return sorted(names)
    raise ValueError("Python __all__ missing")


def pyo3_exports(root, entry):
    """Inventory explicit PyO3 registrations, excluding automatic module metadata."""
    names = {}
    for path in [entry["runtime"], *entry.get("runtimeSources", [])]:
        source = (root / path).read_text(encoding="utf-8")
        for attributes, rust_name in re.findall(r"#\[pyclass\(([^]]*)\)\]\s*(?:#\[[^]]+\]\s*)*(?:pub )?struct (\w+)", source):
            public_name = re.search(r'name = "([^"\n]+)"', attributes)
            names[rust_name] = public_name[1] if public_name else rust_name
    source = (root / entry["runtime"]).read_text(encoding="utf-8")
    registrations = re.findall(r"\bm\.add_class::<([\w:]+)>\(\)", source)
    exports = [names[name.split("::")[-1]] for name in registrations]
    exports.extend(re.findall(r'\bm\.add\(\s*"([^"\n]+)"', source))
    return sorted(exports)


def declaration_sha256(source):
    """Hash reviewed public declarations with only newline style normalized."""
    return hashlib.sha256(source.replace("\r\n", "\n").replace("\r", "\n").encode("utf-8")).hexdigest()


def same(actual, expected, label):
    if actual != expected:
        raise ValueError(f"{label} differs from contract inventory: actual={actual!r}, expected={expected!r}")


def validate(root=ROOT, manifest=None):
    root = Path(root)
    manifest = json.loads((root / MANIFEST).read_text(encoding="utf-8")) if manifest is None else manifest
    same(manifest["schemaVersion"], 1, "inventory schema")
    same(manifest["workspaceContractVersion"], 1, "workspace contract")
    source = (root / "crates/core/src/workspace.rs").read_text(encoding="utf-8")
    version = re.search(r"pub const WORKSPACE_CONTRACT_VERSION: u32 = (\d+);", source)
    same(int(version[1]) if version else None, manifest["workspaceContractVersion"], "Rust workspace identifier")
    errors = source.split("impl ErrorCode {", 1)[1].split("\n}", 1)[0]
    same(sorted(re.findall(r'Self::\w+ => "(\w+)"', errors)), manifest["workspaceErrorCodes"], "workspace error codes")
    for sdk, entry in {**manifest["bindings"], **manifest.get("integrationBindings", {})}.items():
        runtime = (root / entry["runtime"]).read_text(encoding="utf-8")
        declarations = (root / entry["declarations"]).read_text(encoding="utf-8")
        same(declaration_sha256(declarations), entry["declarationsSha256"], f"{sdk} public signatures and types")
        if entry.get("runtimeKind") == "pyo3":
            exports = pyo3_exports(root, entry)
            declared = sorted(node.name if isinstance(node, (ast.ClassDef, ast.FunctionDef)) else node.target.id for node in ast.parse(declarations).body if isinstance(node, (ast.ClassDef, ast.FunctionDef)) or isinstance(node, ast.AnnAssign) and isinstance(node.target, ast.Name))
        elif sdk == "python":
            exports = python_exports(runtime)
            declared = sorted(node.name for node in ast.parse(declarations).body if isinstance(node, (ast.ClassDef, ast.FunctionDef, ast.AsyncFunctionDef)))
        else:
            exports = javascript_exports(runtime)
            declared = javascript_exports(declarations)
        same(exports, entry["runtimeExports"], f"{sdk} runtime exports")
        same(sorted(set(declared) - set(exports)), entry["declarationOnlyExports"], f"{sdk} declaration-only values")
        missing = sorted(set(exports) - set(declared))
        if missing:
            raise ValueError(f"{sdk} runtime exports without declarations: {missing}")
    for entry in manifest["behaviorFixtures"]:
        fixture = json.loads((root / entry["path"]).read_text(encoding="utf-8"))
        same(fixture["schemaVersion"], 1, entry["path"] + " schema")
        same(fixture.get("requiredMethods", []), entry["requiredMethods"], entry["path"] + " required methods")
        same([case["id"] for case in fixture["cases"]], entry["caseIds"], entry["path"] + " cases")
    snapshot = manifest["snapshot"]
    same(snapshot["writerVersion"], 3, "snapshot writer")
    same(snapshot["readerVersions"], [1, 2, 3], "snapshot readers")
    if "snapshot_version: 3," not in source:
        raise ValueError("new workspace snapshot writer differs")
    codec = (root / "crates/core/src/workspace/codec.rs").read_text(encoding="utf-8")
    decoder_versions = re.search(r"require\(matches!\(\s*\(snapshot_version, fields\),(.+?)\)\)\?;", codec, re.S)
    if not decoder_versions:
        raise ValueError("snapshot reader inventory requires review")
    same([(int(version), int(fields)) for version, fields in re.findall(r"\((\d+), Some\((\d+)\)\)", decoder_versions[1])], [(1, 7), (2, 8), (3, 11)], "snapshot reader map shapes")
    same([key["id"] for key in snapshot["keys"]], list(range(11)), "snapshot keys")
    for key in snapshot["keys"]:
        fixture = json.loads((root / key["missingFieldFixture"]).read_text(encoding="utf-8"))
        names = [case["id"] for case in fixture["rejections"]]
        if f"missing_top_key_{key['id']}" not in names:
            raise ValueError(f"snapshot key {key['id']} lacks its indexed missing-field rejection")
    for entry in manifest.get("rustIntegrations", []):
        for path in [entry["source"], *entry["tests"]]:
            if not (root / path).is_file():
                raise ValueError(f"missing Rust integration contract evidence: {path}")
    for path in manifest["relatedContracts"]:
        if not (root / path).is_file():
            raise ValueError(f"missing related contract: {path}")
    return manifest


if __name__ == "__main__":
    try:
        validate()
    except (ValueError, KeyError, OSError, SyntaxError) as error:
        print(f"Core contract inventory failed: {error}", file=sys.stderr)
        sys.exit(1)
    print("Core v1 contract inventory matches committed runtime exports, declarations, error codes and fixture coverage")
