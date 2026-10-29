"""Offline initial G1 task scorecard; no provider execution or aggregate decision."""
from __future__ import annotations

from collections import Counter
import json
from pathlib import Path
import re

from scripts.provider_markers import preparation

REVIEW_URL = "https://github.com/DataFog/datafog-core/blob/bb4e2eeec349590f50d8d7c017123c2aadf31905/docs/verification/provider-marker-preservation.md#proposed-reference-and-omission-denominator"
# This only locates bracketed candidates. Core owns all marker recognition,
# label/code tolerance and unknown/damaged classification.
CANDIDATES = re.compile(r"\[\[[^\[\]]*\]\]")


def reviewed_scorecard(directory: Path, manifest: dict) -> dict:
    review = json.loads((directory / "scorecard-review.json").read_text())
    expected = {
        "schemaVersion": 1, "state": "approved_initial_baseline",
        "approvedOn": "2026-10-29", "approvalContext": "Explicit human reply, approximately 14:22 UTC",
        "reviewUrl": REVIEW_URL,
        "fixtureManifestSha256": preparation.digest((directory / "manifest.json").read_bytes()),
        "requiredReferenceSlots": 170,
        "tasks": [{key: task[key] for key in ("id", "requiredMarkerIds", "permittedOmittedMarkerIds")} for task in manifest["tasks"]],
        "rules": {"requiredOmissionsFail": True, "repetitionOffsetsOmission": False,
                  "returnedOptionalMarkersRequireStrictRestore": True, "wholeResponseFailureRestorationCredit": 0},
        "unapproved": ["providerModels", "sampling", "repetitions", "aggregation", "trueMultiTurn", "providerExecution"],
    }
    # Canonical JSON preserves bool/integer distinctions in this approval witness.
    if preparation.canonical(review) != preparation.canonical(expected):
        raise ValueError("scorecard differs from the approved initial baseline")
    return review


def score_response(directory: Path, task_id: str, response: str) -> dict:
    if type(task_id) is not str or type(response) is not str:
        raise ValueError("task and response must be strings")
    witness = preparation.verify(directory)["source"]
    manifest = json.loads((directory / "manifest.json").read_bytes())
    review = reviewed_scorecard(directory, manifest)
    tasks = [task for task in manifest["tasks"] if task["id"] == task_id]
    if len(tasks) != 1:
        raise ValueError("unknown reviewed task")
    task = tasks[0]
    gold = json.loads((directory / "gold/workspace.json").read_bytes())
    items = {row["original"]: row for row in gold["items"]}
    if len(items) != len(gold["items"]):
        raise ValueError("synthetic reference originals must be unique for attribution")
    api, _ = preparation.native()
    counts = Counter()
    exact = tolerated = 0
    with api.Workspace.open_bundle(bytes.fromhex(gold["bundleHex"])) as workspace:
        try:
            full = workspace.restore_report(response)
        except api.DataFogWorkspaceError as error:
            # No attribution claim can be made when Core cannot analyze the input.
            return {
                "schemaVersion": 1, "taskId": task_id, "attributionComplete": False,
                "requiredReferences": len(task["requiredMarkerIds"]),
                "successfulRequiredReferences": 0, "taskScorecardSatisfied": False,
                "coreAnalysis": {"code": error.code, "textAvailable": False,
                                 "markerIssues": error.marker_issues, "inputIndex": error.input_index,
                                 "findingIndex": error.finding_index, "reviewConflicts": error.review_conflicts},
                **context(directory, response, witness, review),
            }
        for match in CANDIDATES.finditer(response):
            token = match.group()
            report = workspace.restore_report(token)
            if report["restoredMarkers"] == 0:
                continue
            row = items.get(report["text"])
            if report["restoredMarkers"] != 1 or report["issues"] or row is None:
                raise ValueError("Core token attribution differs from the reviewed fixture")
            counts[row["id"]] += 1
            if token == row["marker"]:
                exact += 1
            else:
                tolerated += 1
        if sum(counts.values()) != full["restoredMarkers"]:
            raise ValueError("located occurrences differ from the full Core report")
    required = task["requiredMarkerIds"]
    optional = task["permittedOmittedMarkerIds"]
    observed = [identifier for identifier in required if counts[identifier]]
    missing = [identifier for identifier in required if not counts[identifier]]
    strict = full["text"] is not None and not full["issues"]
    return {
        "schemaVersion": 1, "taskId": task_id, "attributionComplete": True,
        "requiredReferences": len(required), "observedRequiredReferences": len(observed),
        "observedRequiredMarkerIds": observed, "missingRequiredMarkerIds": missing,
        "observedOptionalMarkerIds": [identifier for identifier in optional if counts[identifier]],
        "occurrences": [{"markerId": row["id"], "count": counts[row["id"]]} for row in gold["items"] if counts[row["id"]]],
        "repeatedOccurrences": sum(max(0, count - 1) for count in counts.values()),
        "verbatimOccurrences": exact, "coreToleratedOccurrences": tolerated,
        "successfulRequiredReferences": len(observed) if strict else 0,
        "taskScorecardSatisfied": strict and not missing,
        "coreAnalysis": {"textAvailable": strict, "restoredMarkers": full["restoredMarkers"], "issues": full["issues"]},
        **context(directory, response, witness, review),
    }


def context(directory, response, witness, review):
    raw = response.encode("utf-8", errors="strict")
    return {
        "state": "offline_task_analysis", "responseSha256": preparation.digest(raw), "responseUtf8Bytes": len(raw),
        "fixtureManifestSha256": review["fixtureManifestSha256"],
        "scorecardSha256": preparation.digest((directory / "scorecard-review.json").read_bytes()),
        "scorerSha256": preparation.digest(Path(__file__).read_bytes()), "reviewUrl": review["reviewUrl"],
        "coreWitness": witness,
        "qualification": {"providerRunsExecuted": 0, "preservationRate": None, "g1Pass": False},
    }


def main():
    import argparse
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("directory", type=Path)
    parser.add_argument("--task", choices=tuple(preparation.TASKS), required=True)
    parser.add_argument("--response", type=Path, required=True)
    args = parser.parse_args()
    print(json.dumps(score_response(args.directory, args.task, args.response.read_bytes().decode("utf-8", errors="strict")), ensure_ascii=False, allow_nan=False))


if __name__ == "__main__":
    main()
