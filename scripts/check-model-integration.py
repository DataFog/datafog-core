"""Require the native model checks, allowing later README-only commits.

Intel macOS jobs are advisory: they run with continue-on-error, are not
required here, and publication does not wait for them to finish.
"""

import json
import os
import subprocess


def api(repository, endpoint):
    return json.loads(subprocess.check_output(
        ["gh", "api", "--paginate", "--slurp", f"repos/{repository}/{endpoint}"],
        text=True,
    ))


REQUIRED_PLATFORMS = ("ubuntu-24.04", "ubuntu-24.04-arm", "macos-14", "windows-latest")
REQUIRED_JOBS = {f"installed-model ({backend}, {platform})"
                 for backend in ("datafog", "gliner2", "gliner2-general") for platform in REQUIRED_PLATFORMS}


def check(repository, revision):
    tested_revision = revision
    while True:
        pages = api(repository, f"actions/workflows/model-datafog.yml/runs?head_sha={tested_revision}&per_page=1")
        runs = pages[0]["workflow_runs"]
        if runs:
            break
        # Checkout is shallow in publication workflows. Use GitHub's immutable
        # commit data, including every page of changed files, to prove each hop.
        pages = api(repository, f"commits/{tested_revision}?per_page=100")
        commit = pages[0]
        files = [file for page in pages for file in page["files"]]
        if (commit["sha"] != tested_revision
                or len(commit["parents"]) != 1
                or len(files) != 1
                or files[0].get("filename") != "README.md"
                or files[0].get("status") != "modified"):
            raise SystemExit("Publication blocked: no model integration run; untested changes must modify only README.md.")
        tested_revision = commit["parents"][0]["sha"]
    run = runs[0]
    # A completed run must have succeeded; advisory jobs cannot fail it. A run
    # still in progress counts only once every required job below has passed.
    if run["head_sha"] != tested_revision or (
            run.get("status") == "completed" and run["conclusion"] != "success"):
        raise SystemExit("Publication blocked: the latest model integration run for the tested commit failed or was cancelled.")
    pages = api(repository, f"actions/runs/{run['id']}/jobs?per_page=100")
    passed = {job["name"] for page in pages for job in page["jobs"] if job["conclusion"] == "success"}
    if not REQUIRED_JOBS <= passed:
        raise SystemExit("Publication blocked: DataFog, PII GLiNER2 and general GLiNER2 must pass functional checks on every required platform.")
    suffix = "" if revision == tested_revision else f" (README-only changes since {tested_revision})"
    print(f"Model integration passed for {revision}{suffix}: {run['html_url']}")


if __name__ == "__main__":
    check(os.environ["GITHUB_REPOSITORY"], os.environ["GITHUB_SHA"])
