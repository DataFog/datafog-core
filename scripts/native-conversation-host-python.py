"""Installed Python SDK + encrypted SQLite C2 qualification; controlled synthetic callbacks only."""
from __future__ import annotations
import hashlib
import json
import multiprocessing
import os
from pathlib import Path
import signal
import sys
import tempfile
import time

import datafog_core as api
sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "examples/native-python-host"))
from host import EncryptedStore, Scope, HostError

_trace = {"transportCalls": 0, "resolutionPreparations": 0}


def public_id(value): return bytes([value]) * 16

def head(value): return {"revision": str(value.revision), "commitId": list(value.commit_id)}

def snapshot(workspace):
    return {"head": head(workspace.head()), "namespaceHex": workspace.namespace().hex(),
        "bundleHex": workspace.export_bundle().hex(), "policy": workspace.policy(),
        "counts": workspace.counts(), "operations": workspace.operations(), "historyCoverage": workspace.history_coverage()}

def state(store):
    with store.open() as workspace: return snapshot(workspace)

def error_value(error):
    assert isinstance(error, (HostError, api.DataFogWorkspaceError)) and str(error) == error.code
    value = {"type": "host" if isinstance(error, HostError) else "core", "code": error.code, "message": str(error)}
    if isinstance(error, api.DataFogWorkspaceError):
        value.update(inputIndex=error.input_index, findingIndex=error.finding_index,
            markerIssues=error.marker_issues, reviewConflicts=error.review_conflicts)
    return value

def row_digest(store):
    with store.connection() as db:
        row = db.execute("SELECT * FROM workspaces WHERE tenant=? AND workspace=?", (store.scope.tenant, store.scope.workspace)).fetchone()
    return hashlib.sha256(json.dumps([value.hex() if isinstance(value, bytes) else value for value in row], separators=(",", ":")).encode()).hexdigest()

def operation(value): return {"id": public_id(value), "occurredAtMs": value}

def review(value):
    findings = [api.WorkspaceFinding(item["entityType"], item["matchedText"], api.TextRange(item["range"]["start"], item["range"]["end"]), source=item["source"], needs_review=item["needsReview"]) for item in value["userSelections"]]
    return api.WorkspaceReviewInput(value["text"], scanner_findings=[], user_selections=findings)

def prepare(workspace, fixture, which):
    key, number = {"a": ("a", 3), "b": ("b", 5), "followup": ("followup", 11)}[which]
    value = fixture["inputs"][key]
    plan = workspace.prepare_conversation_send([{"input": review(value["input"]), "conversation": value["conversation"]}], operation(number))
    return plan, plan.send_intents()[0], plan.preview()["text"]

def persist(store, workspace, plan, fault=None, expected=None, checkpoint=None):
    parent, candidate = plan.expected_head(), plan.candidate_head()
    actual_parent = parent if expected is None else expected
    bundle = plan.export_candidate_bundle()
    receipt = {"expectedHead": head(actual_parent), "candidateHead": head(candidate), "candidateBundleHex": bundle.hex(), "saveCalls": 1, "activated": False}
    calls = 0
    def cancelled():
        nonlocal calls
        calls += 1
        if checkpoint is not None and calls == 2: checkpoint("before_commit")
        return False
    try:
        receipt["acknowledgment"] = store.compare_and_save(bundle, actual_parent, candidate,
            cancelled=cancelled, fault=fault)
        if checkpoint is not None: checkpoint("after_commit")
        workspace.activate_saved(plan, parent, candidate)
        receipt.update(ok=True, activated=True, localHead=head(workspace.head()))
    except (HostError, api.DataFogWorkspaceError) as failure:
        receipt.update(ok=False, error=error_value(failure), localHead=head(workspace.head()))
    return receipt

def association(binding, witness, callback):
    return all(callback.get(key) == value for key, value in binding.items()) and all(
        callback.get(key) == witness[key] for key in ("scope", "logicalId", "surface", "siteId"))

def dispatch(store, fixture, intent, output, kind="observed_sent", callback_changes=None):
    # Inspect the actual durable store immediately before the controlled callback.
    with store.open() as durable:
        assert durable.send_intent_status(intent) == "pending"
        before = snapshot(durable)
    scope = {"tenant": list(store.scope.tenant), "workspace": list(store.scope.workspace), "key_id": list(store.scope.key_id)}
    binding = {"scope": scope, "preparationId": list(intent["preparationId"]), "inputIndex": intent["inputIndex"],
        "logicalId": list(public_id(10)), "surface": fixture["surface"], "payloadSha256": hashlib.sha256(output.encode()).hexdigest()}
    witness = {"scope": scope, "logicalId": binding["logicalId"], "surface": fixture["surface"], "siteId": fixture["siteA"]}
    _trace["transportCalls"] += 1
    received = {**binding, "siteId": witness["siteId"], "kind": kind}
    result = {"attempt": binding, "siteVerification": witness, "durableBeforeCallback": before}
    if callback_changes is None: result["received"] = received
    else: result["callbacks"] = [{**received, **change} for change in callback_changes]
    return result

def resolution(workspace, evidence, intent, number, *, not_sent=False):
    assert association(evidence["attempt"], evidence["siteVerification"], evidence["received"])
    assert evidence["received"]["kind"] == ("definitely_not_sent" if not_sent else "observed_sent")
    if not_sent: plan = workspace.prepare_definitely_not_sent(intent, public_id(10), operation(number))
    else:
        received = evidence["received"]
        plan = workspace.prepare_observed_send(intent, public_id(10), received["surface"], received["siteId"], operation(number))
    _trace["resolutionPreparations"] += 1
    return plan

def scoped_error(workspace, text, logical=10):
    try: workspace.restore_in_conversation(text, public_id(logical))
    except api.DataFogWorkspaceError as failure: return error_value(failure)
    raise RuntimeError("scoped rejection missing")

def receive(pipe):
    if not pipe.poll(20): raise RuntimeError("conversation child timeout")
    return pipe.recv()

def child(pipe, config):
    fixture = config["fixture"]
    store = EncryptedStore(Path(config["database"]), Scope(**config["scope"]), config["key"])
    events = []
    try:
        with store.open() as workspace:
            role = config["role"]
            if role == "confirmation":
                intent, evidence = config["intent"], config["evidence"]
                assert association(evidence["attempt"], evidence["siteVerification"], evidence["received"])
                plan = resolution(workspace, evidence, intent, config["resolutionByte"])
            else:
                plan, intent, output = prepare(workspace, fixture, config.get("which", "a"))
            with plan:
                parent, candidate = plan.expected_head(), plan.candidate_head()
                candidate_hex = plan.export_candidate_bundle().hex()
                if config.get("crashStage") is None:
                    pipe.send({"ready": True, "parent": head(parent), "candidateHead": head(candidate), "candidateBundleHex": candidate_hex})
                    assert pipe.recv() == "save"
                def boundary(stage, subject):
                    def checkpoint(phase):
                        if config.get("crashStage") == stage and config.get("crashAt") == phase:
                            checkpoint_file = Path(config["phaseFile"])
                            temporary_file = checkpoint_file.with_suffix(".tmp")
                            temporary_file.write_text(json.dumps({"stage": stage, "boundary": phase, "events": events, "trace": dict(_trace),
                                "expectedHead": head(subject.expected_head()),
                                "candidateHead": head(subject.candidate_head()), "candidateBundleHex": subject.export_candidate_bundle().hex()}))
                            temporary_file.replace(checkpoint_file)
                            while True: time.sleep(60)
                    return checkpoint
                saved = persist(store, workspace, plan, checkpoint=boundary("confirmation" if role == "confirmation" else "pending", plan))
                if role == "confirmation":
                    pipe.send({"save": saved, "transportEvents": [], "resolutionEvidence": evidence, "trace": dict(_trace)});return
                if not saved["ok"]:
                    pipe.send({"save": saved, "transportEvents": [], "confirmation": None, "trace": dict(_trace)});return
                evidence = dispatch(store, fixture, intent, output);events.append(evidence)
                confirmation = resolution(workspace, evidence, intent, 4 if config.get("which", "a") == "a" else 6)
                with confirmation:
                    confirmed = persist(store, workspace, confirmation, checkpoint=boundary("confirmation", confirmation))
                pipe.send({"save": saved, "transportEvents": events, "confirmation": confirmed, "trace": dict(_trace)})
    finally: pipe.close()

def start_child(context, config, processes):
    parent, other = context.Pipe();process = context.Process(target=child, args=(other, config));processes.append(process);process.start();other.close()
    return process, parent

def run(fixture, oracle):
    global _trace
    cases, processes = [], []
    with tempfile.TemporaryDirectory(prefix="datafog-conversation-host-") as directory:
        try:
            for scenario in fixture["cases"]:
                _trace = {"transportCalls": 0, "resolutionPreparations": 0}
                name = scenario["id"];database = Path(directory) / (name + ".sqlite")
                scope = Scope(public_id(11), public_id(1), public_id(12));key = os.urandom(32);store = EncryptedStore(database, scope, key)
                initial = "twoConfirmed" if name == "existing-chat-pause" else "seed"
                with api.Workspace.open_bundle(bytes.fromhex(oracle["states"][initial]["bundleHex"])) as bootstrap: store.create(bootstrap)
                result = {"id": name, "before": state(store), "steps": []}
                def record(call, value): result["steps"].append({"call": call, "value": value})
                with store.open() as workspace:
                    if name.startswith("process-") or name.startswith("crash-"):
                        config = {"fixture": fixture, "database": str(database), "scope": {"tenant": scope.tenant, "workspace": scope.workspace, "key_id": scope.key_id}, "key": key}
                        context = multiprocessing.get_context("spawn")
                        if name == "process-confirmation-race":
                            with prepare(workspace, fixture, "a")[0] as pending:
                                intent = pending.send_intents()[0];output = pending.preview()["text"];record("pending-saved", persist(store, workspace, pending))
                            evidence = dispatch(store, fixture, intent, output);record("transport-observed", evidence)
                            configs = [{**config, "role": "confirmation", "resolutionByte": number, "intent": intent, "evidence": evidence} for number in [4, 8]]
                        elif name == "process-pending-race": configs = [{**config, "role": "pending", "which": which} for which in ["a", "b"]]
                        else:
                            stage = "pending" if name.startswith("crash-pending-") else "confirmation"
                            phase = "before_commit" if name.endswith("before") else "after_commit"
                            phase_file = Path(directory) / (name + ".phase")
                            process, pipe = start_child(context, {**config, "role": "pending", "which": "a", "crashStage": stage, "crashAt": phase, "phaseFile": str(phase_file)}, processes)
                            try:
                                for _ in range(2000):
                                    if phase_file.exists(): break
                                    if not process.is_alive(): raise RuntimeError("conversation child exited before crash checkpoint")
                                    time.sleep(.01)
                                else: raise RuntimeError("conversation crash checkpoint timeout")
                                record("child-at-boundary", json.loads(phase_file.read_text()));os.kill(process.pid, signal.SIGKILL);process.join(10)
                                assert not process.is_alive();record("killed", {"code": process.exitcode, "signal": "SIGKILL"})
                                record(scenario["steps"][-1], state(store))
                            finally: pipe.close()
                            result["after"] = state(store);result["trace"] = dict(_trace);cases.append(result);continue
                        pending = [start_child(context, item, processes) for item in configs]
                        try:
                            record("two-processes-ready", [receive(pipe) for _, pipe in pending])
                            for _, pipe in pending: pipe.send("save")
                            outcomes = [receive(pipe) for _, pipe in pending]
                            for (process, _), outcome in zip(pending, outcomes):
                                process.join(10);assert process.exitcode == 0
                                outcome["processExit"] = {"code": process.exitcode, "signal": None}
                            record("race", outcomes)
                            record("reopen-exact-winner", state(store))
                        finally:
                            for _, pipe in pending: pipe.close()
                        result["after"] = state(store);result["trace"] = dict(_trace);cases.append(result);continue
                    which = "followup" if name == "existing-chat-pause" else "a"
                    with prepare(workspace, fixture, which)[0] as plan:
                        intent, output = plan.send_intents()[0], plan.preview()["text"]
                        if name == "pending-save-rollback":
                            record("save-failed", persist(store, workspace, plan, fault="abort_after_update"));record("no-dispatch", {"transportEvents": [], "resolutionCalls": 0});record("reopen-original", state(store))
                        else:
                            if name == "existing-chat-pause": record("two-confirmed", snapshot(workspace))
                            record("followup-saved" if name == "existing-chat-pause" else "pending-saved", persist(store, workspace, plan))
                            if name == "existing-chat-pause":
                                def paused():
                                    with store.open() as reopened:
                                        return {"snapshot": snapshot(reopened), "plain": scoped_error(reopened, "old plain"), "old": scoped_error(reopened, oracle["plans"]["sendA"]["previews"][0]["text"]),
                                            "other": reopened.restore_in_conversation(oracle["plans"]["otherSend"]["previews"][0]["text"], public_id(11))}
                                record("reopen-paused", paused())
                                with workspace.prepare_recovery_with_operation(operation(13)) as recovered: record("recovery-saved", persist(store, workspace, recovered))
                                record("still-paused", paused())
                                stopped_evidence = dispatch(store, fixture, intent, output, "definitely_not_sent")
                                with resolution(workspace, stopped_evidence, intent, 14, not_sent=True) as stopped: record("not-sent-saved", {"observation": stopped_evidence, "save": persist(store, workspace, stopped)})
                                with store.open() as reopened:
                                    record("reopen-resumed", {"snapshot": snapshot(reopened), "old": reopened.restore_in_conversation(oracle["plans"]["sendA"]["previews"][0]["text"], public_id(10)), "otherNotIssued": scoped_error(reopened, oracle["plans"]["otherSend"]["previews"][0]["text"])})
                            else:
                                kind = "definitely_not_sent" if name == "definitely-not-sent" else "uncertain" if name == "uncertain-transport" else "observed_sent"
                                if name in ["callback-mismatch", "context-and-tenant-isolation"]:
                                    changes = [{"preparationId": list(public_id(99))}, {"logicalId": list(public_id(11))}, {"siteId": "wrong_site"}]
                                    if name == "context-and-tenant-isolation":
                                        foreign_scope = Scope(public_id(99), scope.workspace, scope.key_id);foreign = EncryptedStore(database, foreign_scope, os.urandom(32))
                                        with api.Workspace.open_bundle(bytes.fromhex(oracle["states"]["seed"]["bundleHex"])) as seed: foreign.create(seed)
                                        before = row_digest(foreign)
                                        own_scope = {"tenant": list(scope.tenant), "workspace": list(scope.workspace), "key_id": list(scope.key_id)}
                                        changes = [{"scope": {**own_scope, "tenant": list(public_id(99))}}, {"scope": {**own_scope, "workspace": list(public_id(99))}}]
                                    evidence = dispatch(store, fixture, intent, output, kind, changes)
                                    for callback in evidence["callbacks"]: assert not association(evidence["attempt"], evidence["siteVerification"], callback)
                                    value = {**evidence, "transportCalls": _trace["transportCalls"], "resolutionCalls": _trace["resolutionPreparations"]}
                                    record("foreign-callbacks" if name == "context-and-tenant-isolation" else "callbacks-received", value)
                                    if name == "context-and-tenant-isolation": record("foreign-store-unchanged", {"before": before, "after": row_digest(foreign), "snapshot": state(foreign)})
                                    if name == "callback-mismatch": record("no-confirmation", {"resolutionCalls": 0})
                                    else: record("scoped-unknown", scoped_error(workspace, "plain", 99))
                                    record("unchanged-pending" if name == "context-and-tenant-isolation" else "reopen-pending", state(store))
                                elif name == "uncertain-transport":
                                    evidence = dispatch(store, fixture, intent, output, kind)
                                    record("transport-uncertain", evidence);record("reopen-pending", state(store))
                                else:
                                    evidence = dispatch(store, fixture, intent, output, kind)
                                    record("transport-not-sent" if kind == "definitely_not_sent" else "transport-observed", evidence)
                                    with resolution(workspace, evidence, intent, 7 if kind == "definitely_not_sent" else 4, not_sent=kind == "definitely_not_sent") as confirmed:
                                        if name == "full-parent-conflict":
                                            parent = confirmed.expected_head();digest = row_digest(store)
                                            record("wrong-revision", persist(store, workspace, confirmed, expected=api.WorkspaceHead(parent.revision + 1, parent.commit_id)))
                                            record("wrong-commit-id", persist(store, workspace, confirmed, expected=api.WorkspaceHead(parent.revision, public_id(99))))
                                            record("unchanged-pending", {"snapshot": state(store), "rowBeforeSha256": digest, "rowAfterSha256": row_digest(store)})
                                        else:
                                            fault = "abort_after_update" if name == "confirmation-save-rollback" else "lose_ack" if name == "confirmation-lost-ack" else None
                                            saved = persist(store, workspace, confirmed, fault=fault)
                                            record("confirmation-failed" if fault == "abort_after_update" else "confirmation-uncertain" if fault else "resolution-saved" if kind == "definitely_not_sent" else "confirmation-saved", saved)
                                            if name == "confirmation-save-rollback":
                                                record("reopen-pending", state(store))
                                                with store.open() as reopened, resolution(reopened, evidence, intent, 4) as retry: record("retry-metadata-only", {"save": persist(store, reopened, retry), "transportCalls": 1, "received": evidence["received"]})
                                            record("reopen-not-sent" if kind == "definitely_not_sent" else "reopen-confirmed", state(store))
                                            if name == "confirmation-lost-ack":
                                                with store.open() as reopened:
                                                    try: reopened.prepare_observed_send(intent, public_id(10), fixture["surface"], fixture["siteA"], operation(15))
                                                    except api.DataFogWorkspaceError as failure: record("replay-rejected", error_value(failure))
                                                    else: raise RuntimeError("resolution replay accepted")
                result["after"] = state(store);result["trace"] = dict(_trace);cases.append(result)
        finally:
            for process in processes:
                if process.is_alive(): os.kill(process.pid, signal.SIGKILL)
                process.join(10)
                if process.is_alive(): raise RuntimeError("conversation child cleanup timeout")
    return {"schemaVersion": 1, "profile": fixture["profile"], "sdk": "python", "cases": cases}

if __name__ == "__main__":
    try:
        fixture = json.loads(Path(sys.argv[1]).read_text());oracle = json.loads(Path(sys.argv[2]).read_text())
        print(json.dumps(run(fixture, oracle), ensure_ascii=False, allow_nan=False))
    except Exception:
        sys.stderr.write("native Python conversation host qualification failed\n");raise SystemExit(1) from None
