"""Offline synthetic G1 preparation/capture integrity; no provider client or scorer."""
from __future__ import annotations
import hashlib
import importlib
import importlib.machinery
import importlib.metadata
import json
import math
from pathlib import Path
import subprocess
import sys
from datetime import datetime, timezone
import zipfile

ROOT = Path(__file__).resolve().parents[2]
TYPES = ("PERSON", "ADDRESS", "EMAIL", "PHONE", "IDENTIFICATION_NUMBER", "URL", "USERNAME", "ACCOUNT_NUMBER", "STREET_ADDRESS", "SSN")
NAMESPACE = bytes.fromhex("102132435465768798a9bacbdcedfe0f")
CREATE_ID = bytes.fromhex("2031425364758697a8b9cadbecfd0e1f")
TASKS = {
    "summarize": ((1, 2, 3, 4, 5), "Summarize the five cases in five bullets, one case per bullet. Include every pseudonym belonging to each case."),
    "reply": ((1, 3, 5), "Draft a short reply about cases C01, C03 and C05. Include every pseudonym belonging to those three cases. Other cases may be omitted."),
    "table": ((1, 2, 3, 4, 5), "Create a Markdown table with one row per case and one column per labelled field. Include all five cases and all pseudonyms."),
    "rewrite": ((2, 4), "Rewrite cases C02 and C04 as concise prose paragraphs. Include every pseudonym belonging to those two cases. Other cases may be omitted."),
    "followup": ((1, 5), "Earlier synthetic exchange: the records below were supplied, and the assistant acknowledged receipt. Follow-up request: compare cases C01 and C05 and include every pseudonym belonging to both. Other cases may be omitted."),
}

def digest(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()

def canonical(value) -> bytes:
    return (json.dumps(value, ensure_ascii=False, sort_keys=True, indent=2, allow_nan=False) + "\n").encode()

def native():
    api = importlib.import_module("datafog_core")
    extension = importlib.import_module("datafog_core.datafog_core")
    if not isinstance(extension.__spec__.loader, importlib.machinery.ExtensionFileLoader) or api.Workspace is not extension.Workspace:
        raise ValueError("actual installed native Core workspace binding required")
    return api, Path(extension.__file__)

def provenance(wheel: Path | None = None) -> dict:
    _, binary = native()
    version = importlib.metadata.version("datafog-core")
    binary_digest = digest(binary.read_bytes())
    def matches_binary(candidate):
        with zipfile.ZipFile(candidate) as archive:
            binaries = [name for name in archive.namelist() if name.endswith((".so", ".pyd"))]
            return len(binaries) == 1 and digest(archive.read(binaries[0])) == binary_digest
    if wheel is None:
        candidates = [candidate for directory in ("wheels", "rebuilt-wheels")
                      for candidate in (ROOT / "target" / directory).glob(f"datafog_core-{version}-*.whl")]
        matches = sorted(candidate for candidate in candidates if matches_binary(candidate))
        if not matches:
            raise ValueError("a local wheel matching the actual installed native binary is required")
        wheel = matches[0]
    if not matches_binary(wheel):
        raise ValueError("installed extension does not match the selected local wheel")
    # Source and binary identities are witnesses, not a claim about provider execution.
    if subprocess.run(["git", "diff", "--quiet", "HEAD", "--", "crates/core/src", "crates/core/Cargo.toml", "bindings/python/src", "bindings/python/python", "bindings/python/Cargo.toml", "Cargo.lock"], cwd=ROOT).returncode:
        raise ValueError("Core/Python runtime source must match its declared commit")
    git = lambda *args: subprocess.check_output(["git", *args], cwd=ROOT, text=True).strip()
    return {
        "repository": "DataFog/datafog-core", "sourceCommit": git("rev-parse", "HEAD"),
        "coreTree": git("rev-parse", "HEAD:crates/core"), "pythonTree": git("rev-parse", "HEAD:bindings/python"),
        "cargoLockSha256": digest((ROOT / "Cargo.lock").read_bytes()),
        "rustc": subprocess.check_output(["rustc", "--version"], text=True).strip(),
        "python": sys.version.split()[0], "distributionVersion": version,
        "wheelFilename": wheel.name, "wheelSha256": digest(wheel.read_bytes()),
        "nativeBinaryFilename": binary.name, "nativeBinarySha256": digest(binary.read_bytes()),
        "generatorSha256": digest(Path(__file__).read_bytes()),
    }

def original(kind: str, number: int) -> str:
    if kind == "EMAIL": return f"fixture{number:02d}@example.invalid"
    if kind == "URL": return f"https://fixture{number:02d}.invalid/demo"
    if kind == "USERNAME": return f"synthetic_user_{number:02d}"
    return f"SYNTHETIC {kind} VALUE {number:02d}"

def corpus():
    api, _ = native()
    workspace = api.Workspace.create(NAMESPACE, CREATE_ID)
    items = []
    if not set(TYPES).issubset(workspace.policy()["protectedTypes"]):
        raise ValueError("fixture types must be real current launch protected types")
    for case in range(1, 6):
        for kind in TYPES:
            ordinal = len(items) + 1
            value = original(kind, case)
            operation = bytes.fromhex("30415263748596a7") + ordinal.to_bytes(8, "big")
            finding = api.WorkspaceFinding(kind, value, api.TextRange(0, len(value.encode())))
            plan = workspace.prepare(value, [finding], operation)
            marker = plan.preview()["text"]
            candidate = plan.export_candidate_bundle()
            with api.Workspace.open_bundle(candidate) as opened:
                if opened.head() != plan.candidate_head(): raise ValueError("candidate receipt mismatch")
            workspace.activate_saved(plan, plan.expected_head(), plan.candidate_head())
            items.append({"id": f"M{ordinal:02d}", "case": f"C{case:02d}", "entityType": kind,
                          "marker": marker, "original": value, "operationIdHex": operation.hex()})
    return workspace, items

def prompt_text(task_id, rows):
    instructions = TASKS[task_id][1]
    records = []
    for case in range(1, 6):
        records.append(f"Case C{case:02d}; status ready; fictional priority {case}.")
        records.extend(f"{row['entityType']}: {row['marker']}" for row in rows if row["case"] == f"C{case:02d}")
    return ("Synthetic marker transport exercise. Preserve each referenced pseudonym exactly, including both pairs of square brackets, label and code. Do not replace or invent pseudonyms.\n\n"
            + instructions + "\n\nRecords:\n" + "\n".join(records) + "\n")

def audit_prompt(text, gold, workspace):
    forbidden = [gold["bundleHex"], gold["namespaceHex"], gold["createOperationIdHex"]]
    forbidden += [row["original"] for row in gold["items"]] + [row["operationIdHex"] for row in gold["items"]]
    normalized = " ".join(text.casefold().split())
    if any(" ".join(value.casefold().split()) in normalized for value in forbidden):
        raise ValueError("private fixture data reached a prompt")
    report = workspace.restore_report(text)
    if report["text"] is None or report["issues"] or report["restoredMarkers"] != 50:
        raise ValueError("prompt does not contain exactly the committed Core marker corpus")

def prepare(destination: Path, wheel: Path | None = None):
    workspace, rows = corpus()
    try:
        public = [{key: row[key] for key in ("id", "case", "entityType", "marker")} for row in rows]
        gold = {"schemaVersion": 1, "syntheticOnly": True, "namespaceHex": NAMESPACE.hex(),
                "createOperationIdHex": CREATE_ID.hex(), "bundleHex": workspace.export_bundle().hex(), "items": rows}
        files = {"markers.json": canonical({"schemaVersion": 1, "markers": public}), "gold/workspace.json": canonical(gold)}
        tasks = []
        for task_id, (cases, _) in TASKS.items():
            text = prompt_text(task_id, public)
            required = [row["id"] for row in rows if int(row["case"][1:]) in cases]
            omitted = [row["id"] for row in rows if row["id"] not in required]
            path = f"prompts/{task_id}.txt"
            files[path] = text.encode()
            tasks.append({"id": task_id, "path": path, "promptSha256": digest(files[path]),
                          "requiredMarkerIds": required, "permittedOmittedMarkerIds": omitted,
                          "utf8Bytes": len(files[path]), "codepoints": len(text), "utf16Units": len(text.encode("utf-16-le")) // 2})
        manifest = {"schemaVersion": 1, "state": "prepared_only", "fixtureTypes": list(TYPES), "distinctMarkers": 50,
                    "tasks": tasks, "files": {name: digest(data) for name, data in files.items()},
                    "denominatorStatus": "draft_requires_review", "draftRequiredReferenceSlotsPerProviderRepetition": sum(len(task["requiredMarkerIds"]) for task in tasks),
                    "providerModels": None, "sampling": None, "source": provenance(wheel)}
        files["manifest.json"] = canonical(manifest)
        files["preparation-report.json"] = canonical({"schemaVersion": 1, "state": "prepared_only", "coreFixtureVerified": True,
                                                       "providerRunsExecuted": 0, "preservationRate": None, "g1Pass": False,
                                                       "providerTokenCounts": None, "providerCosts": None})
        # Audit through Core before writing. No gold values/IDs/bundle go into prompts.
        for task in tasks:
            text = files[task["path"]].decode()
            audit_prompt(text, gold, workspace)
        destination.mkdir(parents=True, exist_ok=True)
        for path, data in files.items():
            target = destination / path
            if target.exists() and target.read_bytes() != data:
                raise ValueError("prepared artifact differs; use a new explicit destination for review")
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(data)
        return manifest
    finally:
        workspace.close()


def verify(directory: Path):
    api, _ = native()
    manifest = json.loads((directory / "manifest.json").read_text())
    gold = json.loads((directory / "gold/workspace.json").read_text())
    public = json.loads((directory / "markers.json").read_text())["markers"]
    if manifest["schemaVersion"] != 1 or manifest["state"] != "prepared_only" or len(public) != 50 or len({row["marker"] for row in public}) != 50 or manifest["source"]["generatorSha256"] != digest(Path(__file__).read_bytes()):
        raise ValueError("invalid prepared fixture")
    expected_files = {"markers.json", "gold/workspace.json"} | {f"prompts/{task}.txt" for task in TASKS}
    if set(manifest["files"]) != expected_files or [task["id"] for task in manifest["tasks"]] != list(TASKS):
        raise ValueError("prepared file/task inventory differs from proposal")
    if manifest["fixtureTypes"] != list(TYPES) or manifest["draftRequiredReferenceSlotsPerProviderRepetition"] != 170 or manifest["denominatorStatus"] != "draft_requires_review" or manifest["providerModels"] is not None or manifest["sampling"] is not None:
        raise ValueError("preparation must retain explicit draft qualification choices")
    for path, expected in manifest["files"].items():
        if path.startswith("/") or ".." in Path(path).parts or digest((directory / path).read_bytes()) != expected:
            raise ValueError("artifact integrity mismatch")
    current, generated = corpus()
    try:
        if public != [{key: row[key] for key in ("id", "case", "entityType", "marker")} for row in generated]:
            raise ValueError("Core-generated markers no longer match the prepared fixture")
        if gold["items"] != generated or gold["namespaceHex"] != NAMESPACE.hex() or gold["createOperationIdHex"] != CREATE_ID.hex() or gold["bundleHex"] != current.export_bundle().hex():
            raise ValueError("stored synthetic workspace differs from the actual Core fixture")
        with api.Workspace.open_bundle(bytes.fromhex(gold["bundleHex"])) as stored:
            for row in gold["items"]:
                if stored.restore(row["marker"]) != row["original"]: raise ValueError("gold mapping mismatch")
            for task in manifest["tasks"]:
                text = (directory / task["path"]).read_text()
                cases = TASKS[task["id"]][0]
                expected_required = [row["id"] for row in public if int(row["case"][1:]) in cases]
                if task["path"] != f"prompts/{task['id']}.txt" or text != prompt_text(task["id"], public) or task["requiredMarkerIds"] != expected_required or task["permittedOmittedMarkerIds"] != [row["id"] for row in public if row["id"] not in expected_required] or task["promptSha256"] != digest(text.encode()):
                    raise ValueError("prompt or reference requirements differ from proposal")
                if (task["utf8Bytes"], task["codepoints"], task["utf16Units"]) != (len(text.encode()), len(text), len(text.encode("utf-16-le")) // 2):
                    raise ValueError("prompt size observation mismatch")
                required, omitted = set(task["requiredMarkerIds"]), set(task["permittedOmittedMarkerIds"])
                if required & omitted or required | omitted != {row["id"] for row in public}: raise ValueError("reference partition mismatch")
                audit_prompt(text, gold, stored)
        prepared = json.loads((directory / "preparation-report.json").read_text())
        if prepared["providerRunsExecuted"] != 0 or prepared["g1Pass"] is not False or prepared["preservationRate"] is not None:
            raise ValueError("preparation cannot claim provider qualification")
        return {"state": "prepared_only", "coreFixtureVerified": True, "providerRunsExecuted": 0,
                "g1Pass": False, "preservationRate": None, "source": provenance()}
    finally:
        current.close()

METADATA_KEYS = {"schemaVersion", "recordKind", "provider", "executionSurface", "modelLabel", "reportedModelId", "modelVersion", "capturedAt", "promptId", "promptSha256", "fixtureManifestSha256", "sampling", "unavailableMetadata", "usage", "providerRequestId", "responseTruncated"}

def validate_capture(metadata: dict, response: str, directory: Path):
    if not isinstance(metadata, dict) or set(metadata) != METADATA_KEYS or type(metadata["schemaVersion"]) is not int or metadata["schemaVersion"] != 1 or metadata["recordKind"] not in ("provider_capture", "protocol_test"):
        raise ValueError("capture metadata fields differ from proposal")
    if metadata["provider"] not in ("chatgpt", "claude", "gemini") or metadata["executionSurface"] not in ("web", "api"):
        raise ValueError("provider identity missing")
    if not isinstance(metadata["modelLabel"], str) or not metadata["modelLabel"].strip(): raise ValueError("model label missing")
    for key in ("reportedModelId", "modelVersion", "providerRequestId"):
        if metadata[key] is not None and (not isinstance(metadata[key], str) or not metadata[key].strip()): raise ValueError("model/request metadata invalid")
    if not isinstance(metadata["capturedAt"], str): raise ValueError("capture time must be explicit UTC")
    timestamp = datetime.fromisoformat(metadata["capturedAt"].replace("Z", "+00:00"))
    if not metadata["capturedAt"].endswith("Z") or timestamp.utcoffset() != timezone.utc.utcoffset(timestamp): raise ValueError("capture time must be UTC")
    if type(metadata["responseTruncated"]) is not bool: raise ValueError("truncation state missing")
    unavailable = metadata["unavailableMetadata"]
    allowed_missing = {"reportedModelId", "modelVersion", "providerRequestId"} | {f"sampling.{key}" for key in ("temperature", "topP", "seed", "maxOutputTokens")} | {f"usage.{key}" for key in ("inputTokens", "outputTokens", "costAmount", "currency", "tokenizerId", "tokenizerVersion")}
    if not isinstance(unavailable, list) or any(not isinstance(key, str) or key not in allowed_missing for key in unavailable) or len(set(unavailable)) != len(unavailable): raise ValueError("unavailable metadata invalid")
    for object_name, keys in (("sampling", {"temperature", "topP", "seed", "maxOutputTokens"}), ("usage", {"inputTokens", "outputTokens", "costAmount", "currency", "tokenizerId", "tokenizerVersion"})):
        value = metadata[object_name]
        if not isinstance(value, dict) or set(value) != keys: raise ValueError("unknown capture configuration/usage field")
        for key, item in value.items():
            if item is None:
                if f"{object_name}.{key}" not in metadata["unavailableMetadata"]: raise ValueError("missing metadata needs an explicit unavailable entry")
            elif key in ("currency", "tokenizerId", "tokenizerVersion"):
                if not isinstance(item, str) or not item.strip(): raise ValueError("usage identity invalid")
            elif type(item) not in (int, float) or not math.isfinite(item):
                raise ValueError("sampling/usage must be finite public numeric data")
            elif key in ("seed", "maxOutputTokens", "inputTokens", "outputTokens") and type(item) is not int:
                raise ValueError("integer capture setting required")
            if key in ("inputTokens", "outputTokens") and item is not None and item < 0: raise ValueError("token counts cannot be negative")
    for key in ("reportedModelId", "modelVersion", "providerRequestId"):
        if metadata[key] is None and key not in metadata["unavailableMetadata"]: raise ValueError("missing identity needs an explicit unavailable entry")
    actual_missing = {key for key in ("reportedModelId", "modelVersion", "providerRequestId") if metadata[key] is None}
    actual_missing |= {f"{name}.{key}" for name in ("sampling", "usage") for key, value in metadata[name].items() if value is None}
    if set(unavailable) != actual_missing: raise ValueError("unavailable metadata must describe exactly the missing observations")
    verify(directory)
    manifest_bytes = (directory / "manifest.json").read_bytes()
    manifest = json.loads(manifest_bytes)
    matches = [task for task in manifest["tasks"] if task["id"] == metadata["promptId"]]
    if len(matches) != 1 or metadata["promptSha256"] != matches[0]["promptSha256"] or metadata["fixtureManifestSha256"] != digest(manifest_bytes):
        raise ValueError("capture prompt/fixture provenance mismatch")
    response_bytes = response.encode("utf-8", errors="strict")
    api, _ = native()
    gold = json.loads((directory / "gold/workspace.json").read_text())
    with api.Workspace.open_bundle(bytes.fromhex(gold["bundleHex"])) as workspace:
        try:
            report = workspace.restore_report(response)
            analysis = {"textAvailable": report["text"] is not None, "restoredMarkers": report["restoredMarkers"], "issues": report["issues"]}
        except api.DataFogWorkspaceError as error:
            analysis = {"code": error.code, "textAvailable": False}
    return {"schemaVersion": 1, "metadata": metadata, "response": {"text": response, "sha256": digest(response_bytes), "utf8Bytes": len(response_bytes)},
            "coreAnalysis": analysis, "coreWitness": provenance(),
            "qualification": {"state": "unreviewed", "g1Pass": False, "preservationRate": None}}


def main():
    import argparse
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("operation", choices=("prepare", "verify", "capture"))
    parser.add_argument("directory", type=Path)
    parser.add_argument("--metadata", type=Path, help="local metadata JSON for capture")
    parser.add_argument("--response", type=Path, help="local UTF-8 provider response, preserved verbatim")
    args = parser.parse_args()
    if args.operation == "capture":
        if args.metadata is None or args.response is None:
            parser.error("capture requires --metadata and --response")
        result = validate_capture(json.loads(args.metadata.read_text()), args.response.read_bytes().decode("utf-8", errors="strict"), args.directory)
    else:
        if args.metadata is not None or args.response is not None:
            parser.error("metadata/response apply only to capture")
        result = prepare(args.directory) if args.operation == "prepare" else verify(args.directory)
    print(json.dumps(result, ensure_ascii=False, allow_nan=False))

if __name__ == "__main__":
    main()
