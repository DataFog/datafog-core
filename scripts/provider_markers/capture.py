"""Additive offline desktop capture; historical web/API captures stay unchanged."""
from __future__ import annotations

from datetime import datetime, timezone
import json
import math
from pathlib import Path

from scripts.provider_markers import preparation as frozen


def validate_capture(metadata: dict, response: str, directory: Path):
    """Validate local data only; this never creates a chat or sends a request."""
    if isinstance(metadata, dict) and metadata.get("executionSurface") in ("web", "api"):
        return frozen.validate_capture(metadata, response, directory)
    return validate_desktop_capture(metadata, response, directory)


# The original generator pins preparation.py byte-for-byte. Keep its public capture
# path intact; metadata validation evolves here, with legacy positive/negative
# parity tests and the same closed fields for the desktop addition.
def validate_desktop_capture(metadata: dict, response: str, directory: Path):
    if not isinstance(metadata, dict) or set(metadata) != frozen.METADATA_KEYS or type(metadata["schemaVersion"]) is not int or metadata["schemaVersion"] != 1 or metadata["recordKind"] not in ("provider_capture", "protocol_test"):
        raise ValueError("capture metadata fields differ from proposal")
    if metadata["provider"] != "claude" or metadata["executionSurface"] != "desktop":
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
    frozen.verify(directory)
    manifest_bytes = (directory / "manifest.json").read_bytes()
    manifest = json.loads(manifest_bytes)
    matches = [task for task in manifest["tasks"] if task["id"] == metadata["promptId"]]
    if len(matches) != 1 or metadata["promptSha256"] != matches[0]["promptSha256"] or metadata["fixtureManifestSha256"] != frozen.digest(manifest_bytes):
        raise ValueError("capture prompt/fixture provenance mismatch")
    response_bytes = response.encode("utf-8", errors="strict")
    api, _ = frozen.native()
    gold = json.loads((directory / "gold/workspace.json").read_text())
    with api.Workspace.open_bundle(bytes.fromhex(gold["bundleHex"])) as workspace:
        try:
            report = workspace.restore_report(response)
            analysis = {"textAvailable": report["text"] is not None, "restoredMarkers": report["restoredMarkers"], "issues": report["issues"]}
        except api.DataFogWorkspaceError as error:
            analysis = {"code": error.code, "textAvailable": False}
    return {"schemaVersion": 1, "metadata": metadata, "response": {"text": response, "sha256": frozen.digest(response_bytes), "utf8Bytes": len(response_bytes)},
            "coreAnalysis": analysis, "coreWitness": frozen.provenance(),
            "captureWitness": {"collectorSha256": frozen.digest(Path(__file__).read_bytes()), "executionSurface": "desktop"},
            "qualification": {"state": "unreviewed", "g1Pass": False, "preservationRate": None}}



def main():
    import argparse
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("directory", type=Path)
    parser.add_argument("--metadata", type=Path, required=True)
    parser.add_argument("--response", type=Path, required=True)
    args = parser.parse_args()
    metadata = json.loads(args.metadata.read_bytes())
    response = args.response.read_bytes().decode("utf-8", errors="strict")
    print(json.dumps(validate_capture(metadata, response, args.directory), ensure_ascii=False, allow_nan=False))


if __name__ == "__main__":
    main()
