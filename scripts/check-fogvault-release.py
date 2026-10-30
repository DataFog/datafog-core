"""Read-only v1+ publication gate for exact-commit Core/FogVault qualification.

Unlike the older model gate, this gate has no documentation-only ancestry
exception. Only same-repository push or manual workflow runs qualify; PR
merge-checkout results are insufficient. A later commit and an unfinished or failed rerun require fresh green
qualification, even if an earlier run or a similarly named check passed.
"""
import json
import os
from pathlib import Path
import re
import subprocess
import tomllib

ROOT = Path(__file__).resolve().parents[1]
PLATFORMS = ("ubuntu-24.04", "macos-15", "windows-2025")
REQUIRED = {
    "fogvault-integration.yml": {
        *(f"Core–FogVault native / {platform}" for platform in PLATFORMS),
        *(f"Core–FogVault joint / {engine}" for engine in ("chromium", "firefox", "msedge")),
    },
    "fogvault-native-languages.yml": {
        *(f"Core–FogVault Node/Python / {platform}" for platform in PLATFORMS),
    },
}


def api(repository, endpoint):
    return json.loads(subprocess.check_output(
        ["gh", "api", "--paginate", "--slurp", f"repos/{repository}/{endpoint}"], text=True,
    ))


def blocked(message):
    raise ValueError("Publication blocked: " + message)


def check(repository, revision, version, fetch=api):
    if not isinstance(version, str) or not re.fullmatch(r"(0|[1-9][0-9]*)\.[0-9]+\.[0-9]+(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?", version):
        blocked("invalid Core package version")
    if int(version.split(".")[0]) == 0:
        return {"required": False, "version": version,
                "reason": "Core pre-v1: joint FogVault release qualification is not required"}
    if not re.fullmatch(r"[0-9a-f]{40}", revision) or not re.fullmatch(r"[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+", repository):
        blocked("invalid repository or exact commit identity")
    proofs = []
    for workflow, required_jobs in REQUIRED.items():
        metadata = fetch(repository, f"actions/workflows/{workflow}")
        expected_path = ".github/workflows/" + workflow
        if (not isinstance(metadata, list) or len(metadata) != 1
                or metadata[0].get("path") != expected_path
                or not isinstance(metadata[0].get("id"), int)):
            blocked("workflow identity is missing or mismatched: " + workflow)
        workflow_id = metadata[0]["id"]
        pages = fetch(repository, f"actions/workflows/{workflow}/runs?head_sha={revision}&per_page=1")
        runs = [run for page in pages for run in page["workflow_runs"]]
        if not runs:
            blocked("no exact-commit workflow run: " + workflow)
        run = runs[0]  # Latest attempt only; never fall back to older green runs.
        if (run.get("head_sha") != revision or run.get("workflow_id") != workflow_id
                or run.get("path") != expected_path
                or run.get("head_repository", {}).get("full_name", "").casefold() != repository.casefold()
                or run.get("event") not in {"push", "workflow_dispatch"}
                or run.get("status") != "completed" or run.get("conclusion") != "success"
                or not isinstance(run.get("id"), int)):
            blocked("latest exact workflow run is not completed and successful: " + workflow)
        pages = fetch(repository, f"actions/runs/{run['id']}/jobs?filter=latest&per_page=100")
        jobs = [job for page in pages for job in page["jobs"]]
        for name in required_jobs:
            matches = [job for job in jobs if job.get("name") == name]
            if (len(matches) != 1 or matches[0].get("run_id") != run["id"]
                    or matches[0].get("status") != "completed"
                    or matches[0].get("conclusion") != "success"):
                blocked("missing, duplicate, unfinished or failed required scope: " + name)
        proofs.append({"workflow": expected_path, "workflow_id": workflow_id,
                       "run_id": run["id"], "head_sha": revision,
                       "required_scopes": sorted(required_jobs), "url": run.get("html_url")})
    return {"required": True, "version": version, "head_sha": revision, "proofs": proofs}


def main():
    version = tomllib.loads((ROOT / "crates/core/Cargo.toml").read_text())["package"]["version"]
    try:
        result = check(os.environ.get("GITHUB_REPOSITORY", ""), os.environ.get("GITHUB_SHA", ""), version)
    except (ValueError, KeyError, TypeError, AttributeError, OSError, subprocess.SubprocessError) as error:
        raise SystemExit(str(error)) from error
    print(json.dumps(result, sort_keys=True))


if __name__ == "__main__":
    main()
