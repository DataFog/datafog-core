"""Run deterministic workspace commands against the actual installed native Python API."""
import json
import sys
import datafog_core as api


def canonical(value):
    if isinstance(value, list):
        return [canonical(item) for item in value]
    if isinstance(value, dict):
        result = {key: canonical(item) for key, item in sorted(value.items())}
        if "revision" in result and "commitId" in result:
            result["revision"] = str(result["revision"])
        if "entityType" in result and "matchedText" in result and result.get("range", {}).get("unit") == "utf8_bytes":
            for key in ("confidence", "detectorName", "detectorVersion"):
                result.setdefault(key, None)
        return result
    return value


def head(value):
    return {"revision": str(value.revision), "commitId": list(value.commit_id)}


def text_value(command):
    if "textRepeat" in command:
        repeated=command["textRepeat"]
        if not isinstance(repeated["value"],str) or not isinstance(repeated["count"],int) or not 0 <= repeated["count"] <= 2*1024*1024:
            raise ValueError("invalid fixture text repetition")
        return repeated["value"]*repeated["count"]
    return command["text"]


def run(fixture):
    if fixture["schemaVersion"] != 1 or not fixture["cases"]:
        raise ValueError("invalid workspace fixture")
    if not all(hasattr(api, name) for name in ("Workspace", "WorkspaceFinding", "WorkspaceInput", "WorkspaceReviewInput", "WorkspaceReviewCorrection", "DataFogWorkspaceError")):
        raise ValueError("native workspace API unavailable")
    aliases={"openBundle":"open_bundle","exportBundle":"export_bundle","prepareBatch":"prepare_batch","prepareReview":"prepare_review","preparePolicyChanges":"prepare_policy_changes","prepareRecovery":"prepare_recovery","restoreReport":"restore_report","activateSaved":"activate_saved","snapshotFormat":"snapshot_format","policyPreview":"policy_preview","exportCandidateBundle":"export_candidate_bundle"}
    for method in fixture["requiredMethods"]:
        if method=="plan.heads":
            if not all(hasattr(api.WorkspacePreparedChange,name) for name in ("expected_head","candidate_head")):
                raise ValueError("mandatory plan head methods unavailable")
        else:
            owner=api.WorkspacePreparedChange if method.startswith("plan.") else api.Workspace
            name=method.split(".")[-1]
            if not hasattr(owner,aliases.get(name,name)):
                raise ValueError("mandatory native workspace method unavailable")
    output = []
    supported = {"create", "openBundle", "head", "export", "close", "inspectPlan", "discard", "activateSaved", "recall", "review", "restore", "restoreReport", "restorePlanText", "restoreReportPlanText", "prepare", "prepareBatch", "prepareReview", "preparePolicyChanges", "prepareRecovery"}
    for scenario in fixture["cases"]:
        workspaces, plans, plan_texts, bundles, reviews = {}, {}, {}, {}, {}
        steps = []

        def snapshot(workspace):
            return {"head": head(workspace.head()), "namespaceHex": workspace.namespace().hex(), "bundleHex": workspace.export_bundle().hex(), "policy": workspace.policy()}

        def inspect(plan):
            return {"heads": {"expectedHead": head(plan.expected_head()), "candidateHead": head(plan.candidate_head())}, "previews": plan.previews(), "snapshotFormat": plan.snapshot_format(), "policyPreview": plan.policy_preview(), "candidateBundleHex": plan.export_candidate_bundle().hex()}

        def finding(value):
            span = value["range"]
            if span["unit"] != "utf8_bytes":
                raise ValueError("unsupported fixture range encoding")
            return api.WorkspaceFinding(value["entityType"], value["matchedText"], api.TextRange(span["start"], span["end"]), confidence=value.get("confidence"), detector_name=value.get("detectorName"), detector_version=value.get("detectorVersion"), source=value.get("source", "scanner"), needs_review=value.get("needsReview", True))

        def review_input(value):
            corrections = []
            for correction in value.get("corrections", []):
                key = correction.get("candidateKey")
                if "candidateFrom" in correction:
                    selector = correction["candidateFrom"]
                    matches = [candidate for candidate in reviews[selector["review"]]["candidates"] if candidate["finding"]["entityType"] == selector["entityType"] and candidate["byteRange"] == {"start": selector["start"], "end": selector["end"]}]
                    if len(matches) != 1:
                        raise ValueError("ambiguous fixture candidate")
                    key = matches[0]["key"]
                corrections.append(api.WorkspaceReviewCorrection(key, finding(correction["replacement"])))
            return api.WorkspaceReviewInput(value["text"], scanner_findings=[finding(item) for item in value["scannerFindings"]], user_selections=[finding(item) for item in value["userSelections"]], corrections=corrections)

        if not scenario["commands"]:
            raise ValueError("empty fixture scenario")
        for command in scenario["commands"]:
            call = command["call"]
            if call not in supported:
                raise ValueError("unsupported mandatory fixture command")
            try:
                operation = bytes([command["operationByte"]]) * 16 if "operationByte" in command else None
                if call == "create":
                    workspace = api.Workspace.create(bytes([command["namespaceByte"]]) * 16, operation)
                    workspaces[command["workspace"]] = workspace
                    value = snapshot(workspace)
                elif call == "openBundle":
                    workspace = api.Workspace.open_bundle(bytes.fromhex(command["inlineBundleHex"]) if "inlineBundleHex" in command else bundles[command["bundle"]])
                    workspaces[command["workspace"]] = workspace
                    value = snapshot(workspace)
                elif call == "head":
                    value = head(workspaces[command["workspace"]].head())
                elif call == "export":
                    workspace = workspaces[command["workspace"]]
                    bundles[command["bundle"]] = workspace.export_bundle()
                    value = snapshot(workspace)
                elif call == "close":
                    workspaces[command["workspace"]].close()
                    value = None
                elif call == "inspectPlan":
                    value = inspect(plans[command["plan"]])
                elif call == "discard":
                    plans[command["plan"]].discard()
                    value = None
                elif call == "activateSaved":
                    workspace, plan = workspaces[command["workspace"]], plans[command["plan"]]
                    workspace.activate_saved(plan, plan.expected_head(), plan.expected_head() if command.get("savedHeadMode")=="parent" else plan.candidate_head())
                    value = snapshot(workspace)
                elif call == "recall":
                    value = workspaces[command["workspace"]].recall(text_value(command))
                elif call == "review":
                    value = workspaces[command["workspace"]].review(review_input(command["input"]))
                    reviews[command["review"]] = value
                elif call in ("restore", "restoreReport", "restorePlanText", "restoreReportPlanText"):
                    text = plan_texts[command["plan"]][command["inputIndex"]] if call.endswith("PlanText") else text_value(command)
                    text += command.get("suffix", "")
                    workspace = workspaces[command["workspace"]]
                    value = workspace.restore_report(text) if call.startswith("restoreReport") else workspace.restore(text)
                else:
                    workspace = workspaces[command["workspace"]]
                    if call == "prepare":
                        plan = workspace.prepare(text_value(command), [finding(item) for item in command["findings"]], operation)
                    elif call == "prepareBatch":
                        plan = workspace.prepare_batch([api.WorkspaceInput(item["text"], [finding(item) for item in item["findings"]]) for item in command["inputs"]], operation)
                    elif call == "prepareReview":
                        plan = workspace.prepare_review([review_input(item) for item in command["inputs"]], operation)
                    elif call == "preparePolicyChanges":
                        plan = workspace.prepare_policy_changes(command["edits"], operation)
                    elif call == "prepareRecovery":
                        plan = workspace.prepare_recovery(operation)
                    else:
                        raise ValueError("unsupported mandatory fixture command")
                    value = inspect(plan)
                    plans[command["plan"]] = plan
                    plan_texts[command["plan"]] = [preview["text"] for preview in value["previews"]]
                if "expectCode" in command:
                    raise ValueError("required operation failure did not occur")
                steps.append({"call": call, "ok": True, "value": canonical(value)})
            except api.DataFogWorkspaceError as error:
                if error.code != command.get("expectCode") or str(error) != error.code:
                    raise ValueError("unexpected workspace operation failure") from None
                steps.append({"call": call, "ok": False, "error": canonical({"code": error.code, "inputIndex": error.input_index, "findingIndex": error.finding_index, "markerIssues": error.marker_issues, "reviewConflicts": error.review_conflicts})})
        for workspace in workspaces.values():
            workspace.close()
        output.append({"id": scenario["id"], "steps": steps})
    return canonical({"schemaVersion": 1, "profile": fixture["profile"], "cases": output})


if __name__ == "__main__":
    try:
        with open(sys.argv[1], encoding="utf8") as fixture_file:
            fixture = json.load(fixture_file)
        json.dump(run(fixture), sys.stdout, ensure_ascii=False, allow_nan=False, sort_keys=True)
    except Exception:
        sys.stderr.write("native Python workspace parity runner failed\n")
        raise SystemExit(1) from None
