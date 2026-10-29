"""Current operation backup fields through the actual installed native wheel."""
from pathlib import Path
import json
from datafog_core import Workspace, DataFogWorkspaceError

ROOT = Path(__file__).resolve().parents[3]

def verify_backup_operation_fields():
    fixture = json.loads((ROOT / "fixtures/workspace/backup-operation-fields.json").read_text())
    for name in ("current", "legacyUpgraded"):
        gold = fixture[name]; raw = bytes.fromhex(gold["bundleHex"])
        with Workspace.open_bundle(raw) as workspace:
            assert workspace.export_bundle() == raw
            assert workspace.operations() == gold["operations"]
            assert workspace.counts() == gold["counts"]
            assert workspace.history_coverage() == gold["historyCoverage"]
            assert workspace.operations_page(0, 1000)["records"] == gold["operations"]
            for alias in gold["aliases"]: assert workspace.restore(alias["marker"]) == alias["value"]
            with workspace.prepare_recovery(bytes([111]) * 16) as plan:
                candidate = plan.export_candidate_bundle()
                with Workspace.open_bundle(candidate) as pending:
                    assert pending.operations()[:-1] == gold["operations"]
                assert workspace.export_bundle() == raw
                workspace.activate_saved(plan, plan.expected_head(), plan.candidate_head())
            assert workspace.operations()[:-1] == gold["operations"]
            assert workspace.operations()[-1]["kind"] == "recovery"
            assert workspace.operations()[-1]["occurredAtMs"] is None
            assert workspace.counts() == {**gold["counts"], "operations": gold["counts"]["operations"] + 1}
            assert workspace.history_coverage() == gold["historyCoverage"]
            with Workspace.open_bundle(workspace.export_bundle()) as reopened:
                assert reopened.operations() == workspace.operations()
                assert reopened.counts() == workspace.counts()
                for alias in gold["aliases"]: assert reopened.restore(alias["marker"]) == alias["value"]
    for vector in fixture["rejections"]:
        try:
            Workspace.open_bundle(bytes.fromhex(vector["bundleHex"])).close()
        except DataFogWorkspaceError as error:
            assert error.code == vector["code"] == str(error)
            assert error.finding_index is None and error.input_index is None
            assert not error.marker_issues and not error.review_conflicts
        else:
            raise AssertionError("malformed current operation backup accepted")
    print("Installed native Python current-v3 backup fields: two histories and 32 rejections pass.")

if __name__ == "__main__": verify_backup_operation_fields()
