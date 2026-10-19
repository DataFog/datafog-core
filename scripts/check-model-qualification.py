"""Require successful actual-model qualification for this exact release source.

GitHub CLI uses the job's existing token. This check never changes repository
state or substitutes qualification from an earlier commit.
"""
import json
import os
import subprocess

repository = os.environ["GITHUB_REPOSITORY"]
revision = os.environ["GITHUB_SHA"]
def api(endpoint):
    return json.loads(subprocess.check_output(["gh", "api", f"repos/{repository}/{endpoint}"], text=True))
runs = api(f"actions/workflows/model-datafog.yml/runs?head_sha={revision}&status=success")
platforms = ["ubuntu-24.04", "ubuntu-24.04-arm", "macos-14", "macos-15-intel", "windows-latest"]
required = {f"installed-model ({backend}, {host})" for backend in ("datafog", "gliner2") for host in platforms}
for run in runs["workflow_runs"]:
    jobs = api(f"actions/runs/{run['id']}/jobs?per_page=100")["jobs"]
    passed = {job["name"] for job in jobs if job["conclusion"] == "success"}
    if required <= passed:
        if path := os.environ.get("GITHUB_OUTPUT"):
            with open(path,"a") as stream:
                stream.write(f"run_id={run['id']}\n")
        print(f"Actual-model qualification passed for {revision}: {run['html_url']}")
        break
else:
    raise SystemExit("No successful two-backend/five-platform model qualification for this exact source; publication blocked.")
