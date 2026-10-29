"""Actual installed native Python transport for the shared streaming command profile."""
import json
import sys
import datafog_core as api


def error_value(error):
    return {"code":error.code,"findingIndex":error.finding_index,"inputIndex":error.input_index,"markerIssues":error.marker_issues,"reviewConflicts":error.review_conflicts,"delivered":getattr(error,"delivered",None),"complete":getattr(error,"complete",None)}


def seed():
    workspace=api.Workspace.create(bytes([1])*16,bytes([2])*16)
    plan=workspace.prepare("Jane",[api.WorkspaceFinding("PERSON","Jane",api.TextRange(0,4))],bytes([3])*16)
    marker=plan.preview()["text"]
    workspace.activate_saved(plan,plan.expected_head(),plan.candidate_head())
    return workspace,marker


def run(fixture):
    if fixture["schemaVersion"]!=1 or fixture["profile"]!="workspace-streaming-v1" or not fixture["cases"]:
        raise ValueError("invalid streaming fixture")
    for method in ("buffered_restore","progressive_restore"):
        if not callable(getattr(api.Workspace,method,None)):
            raise ValueError("mandatory native streaming method unavailable")
    cases=[]
    for scenario in fixture["cases"]:
        modes={}
        for mode in ("buffered","progressive"):
            workspace,marker=seed()
            stream=getattr(workspace,mode+"_restore")(scenario["encoding"])
            steps=[]
            try:
                for step in scenario["steps"]:
                    op=step["op"]
                    try:
                        value=None
                        if op=="feed":
                            source=bytes(step["bytes"]) if "bytes" in step else step["text"].replace("{marker}",marker).replace("{bareMarker}",marker[2:-2]).replace("{oversizedPrefix}","[[PERSON_"+"A"*100).encode("utf8")
                            value=stream.feed(source)
                        elif op=="feedUtf16":value=stream.feed_utf16(step["units"])
                        elif op=="finalize":value=stream.finalize()
                        elif op=="reset":stream.reset()
                        elif op=="closeStream":stream.close()
                        elif op=="closeWorkspace":workspace.close()
                        elif op=="advanceWorkspace":
                            plan=workspace.prepare("Bob",[api.WorkspaceFinding("PERSON","Bob",api.TextRange(0,3))],bytes([4])*16)
                            workspace.activate_saved(plan,plan.expected_head(),plan.candidate_head())
                        else:raise ValueError("unsupported mandatory streaming operation")
                        steps.append({"op":op,"ok":True,"value":value})
                    except api.DataFogWorkspaceError as error:
                        if str(error)!=error.code:raise ValueError("unstructured streaming failure") from None
                        steps.append({"op":op,"ok":False,"error":error_value(error)})
            finally:
                stream.close()
                workspace.close()
            modes[mode]={"steps":steps}
        cases.append({"id":scenario["id"],"modes":modes})
    return {"schemaVersion":1,"profile":fixture["profile"],"cases":cases}


if __name__=="__main__":
    try:
        with open(sys.argv[1],encoding="utf8") as source:fixture=json.load(source)
        json.dump(run(fixture),sys.stdout,ensure_ascii=False,allow_nan=False,sort_keys=True)
    except Exception:
        sys.stderr.write("native Python streaming parity runner failed\n")
        raise SystemExit(1) from None
