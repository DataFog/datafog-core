"""Block publication unless this commit passed both models on all native targets."""

import json
import os
import subprocess


def api(repository, endpoint):
    return json.loads(subprocess.check_output(
        ["gh", "api", "--paginate", "--slurp", f"repos/{repository}/{endpoint}"],
        text=True,
    ))


def check(repository, revision):
    pages = api(repository, f"actions/workflows/model-datafog.yml/runs?head_sha={revision}&per_page=1")
    runs = pages[0]["workflow_runs"]
    if not runs or runs[0]["head_sha"] != revision or runs[0]["conclusion"] != "success":
        raise SystemExit("Publication blocked: the latest model integration run for this commit is missing, unfinished or failed.")
    run = runs[0]
    pages = api(repository, f"actions/runs/{run['id']}/jobs?per_page=100")
    passed = {job["name"] for page in pages for job in page["jobs"] if job["conclusion"] == "success"}
    platforms = ("ubuntu-24.04", "ubuntu-24.04-arm", "macos-14", "macos-15-intel", "windows-latest")
    required = {f"installed-model ({backend}, {platform})" for backend in ("datafog", "gliner2") for platform in platforms}
    if not required <= passed:
        raise SystemExit("Publication blocked: both models must pass functional checks on all five platforms.")
    print(f"Model integration passed for {revision}: {run['html_url']}")


if __name__ == "__main__":
    check(os.environ["GITHUB_REPOSITORY"], os.environ["GITHUB_SHA"])
