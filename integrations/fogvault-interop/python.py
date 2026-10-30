"""Second installed consumer of the same synthetic native encrypted vault."""
import json
from pathlib import Path
import sys
from datafog_fogvault import NativeWorkspace, WorkspaceHead, FogVaultWorkspaceError

c = json.loads(Path(sys.argv[1]).read_text())
original = json.loads(Path(c["nodeResult"]).read_text())
tenant = c["tenant"]
ident = lambda name: bytes(c[name])
expected = WorkspaceHead(int(original["head"]["revision"]), bytes(original["head"]["commitId"]))
owner = NativeWorkspace.open_existing(tenant, c["control"], c["vault"], bytes(c["password"]),
    ident("recordId"), ident("namespace"), expected_head=expected)
try:
    assert owner.restore(tenant, original["marker"]) == c["source"]
    text = c["pythonSource"]
    value = c["pythonValue"]
    start = text.encode().index(value.encode())
    change = owner.prepare_review(tenant, [{"text": text, "scannerFindings": [{
        "entityType": "EMAIL", "matchedText": value,
        "range": {"unit": "utf8_bytes", "start": start, "end": start + len(value.encode())},
    }], "userSelections": [], "corrections": []}], ident("pythonOperation"))
    saved = owner.commit(tenant, change, True, ident("pythonRequest"))
    marker = saved["previews"][0]
    change = owner.prepare_conversation_send(tenant, [{"input": {
        "text": marker, "scannerFindings": [], "userSelections": [], "corrections": []},
        "conversation": {"id": ident("logicalId"), "surface": "synthetic-interop",
                         "siteId": None, "siteIdByteBudget": 64}}],
        {"id": ident("sendOperation"), "occurredAtMs": c["maximumTime"] - 1})
    intent = change.send_intents(tenant)[0]
    pending = owner.commit(tenant, change, True, ident("sendRequest"))
    try:
        owner.restore_in_conversation(tenant, pending["previews"][0], ident("logicalId"))
        raise AssertionError("Unconfirmed send unexpectedly restored")
    except FogVaultWorkspaceError as error:
        assert error.code == "conversation_pending"
    change = owner.prepare_observed_send(tenant, intent, ident("logicalId"),
        "synthetic-interop", "synthetic-interop-chat",
        {"id": ident("observeOperation"), "occurredAtMs": c["maximumTime"]})
    final = owner.commit(tenant, change, True, ident("observeRequest"))
    h = final["head"]
    Path(c["pythonResult"]).write_text(json.dumps({
        "head": {"revision": str(h.revision), "commitId": list(h.commit_id)},
        "marker": marker, "sentMarker": pending["previews"][0],
    }))
finally:
    owner.close(tenant)
