"""Require successful queue checks; PRs validate the gate before queue admission."""

import json
import os
import sys
from urllib.request import Request, urlopen


TECHNICAL_WORKFLOWS = (
    "advisory", "application-vaults", "browser-model", "browser-v1-1",
    "credential-stores", "devcontainer", "dfv-check", "dfv-models",
    "fogvault-integration", "fogvault-native-languages", "fuzz", "model-datafog",
    "native-python-host", "node-binding", "python-binding", "rust-quality",
    "wasm-binding", "workspace-contract", "workspace-integration",
)


def validate(event_name, results):
    if event_name not in {"pull_request", "merge_group"}:
        raise ValueError(f"Unsupported gate event: {event_name}")
    expected = {"policy", *TECHNICAL_WORKFLOWS}
    if not isinstance(results, dict) or set(results) != expected:
        raise ValueError("Gate results must contain exactly every configured dependency")
    for name, result in results.items():
        required = "success"
        if event_name == "pull_request" and name != "policy":
            required = "skipped"
        if not isinstance(result, dict) or result.get("result") != required:
            raise ValueError(f"{name}: expected {required}, received {result!r}")



def read_jobs(repository, run_id, attempt, token, api_url):
    """Read this attempt only; verify pagination did not omit or repeat jobs."""
    jobs = []
    total = None
    for page in range(1, 101):
        url = (f"{api_url}/repos/{repository}/actions/runs/{run_id}/attempts/{attempt}"
               f"/jobs?per_page=100&page={page}")
        request = Request(url, headers={
            "Authorization": f"Bearer {token}",
            "Accept": "application/vnd.github+json",
            "X-GitHub-Api-Version": "2022-11-28",
        })
        with urlopen(request, timeout=30) as response:
            payload = json.load(response)
        if not isinstance(payload, dict):
            raise ValueError("Jobs API returned a malformed response")
        count = payload.get("total_count")
        batch = payload.get("jobs")
        if type(count) is not int or count <= 0 or not isinstance(batch, list) or not batch:
            raise ValueError("Jobs API returned missing or malformed job data")
        if total is not None and total != count:
            raise ValueError("Jobs API total changed during pagination")
        total = count
        jobs.extend(batch)
        if len(jobs) >= total:
            if len(jobs) != total:
                raise ValueError("Jobs API returned more jobs than its declared total")
            return jobs
    raise ValueError("Jobs API pagination exceeded the bounded page limit")


def validate_jobs(jobs, run_id, attempt, head_sha, runner_name):
    """Reusable workflow success does not guarantee every child job succeeded."""
    expected = {"policy", *TECHNICAL_WORKFLOWS}
    covered = set()
    identifiers = set()
    current_gate = []
    for job in jobs:
        if not isinstance(job, dict) or type(job.get("id")) is not int:
            raise ValueError("Jobs API returned a malformed job identity")
        if job["id"] in identifiers:
            raise ValueError("Jobs API returned duplicate jobs")
        identifiers.add(job["id"])
        if (job.get("run_id"), job.get("run_attempt"), job.get("head_sha")) != (run_id, attempt, head_sha):
            raise ValueError("Jobs API returned a job from another run, attempt, or commit")
        name = job.get("name")
        if name == "Full technical suite":
            if (job.get("runner_name") != runner_name or job.get("status") != "in_progress"
                    or job.get("conclusion") is not None):
                raise ValueError("Final gate job does not match the executing runner")
            current_gate.append(job["id"])
            continue
        if not isinstance(name, str):
            raise ValueError("Jobs API returned a job without a name")
        workflow = name.split(" / ", 1)[0]
        if workflow not in expected:
            raise ValueError(f"Unexpected job in merge gate: {name}")
        if job.get("status") != "completed" or job.get("conclusion") != "success":
            raise ValueError(f"Technical job did not succeed: {name}: {job.get('conclusion')}")
        covered.add(workflow)
    if len(current_gate) != 1:
        raise ValueError("Jobs API must contain exactly one executing final gate job")
    if covered != expected:
        raise ValueError(f"Missing technical workflow coverage: {sorted(expected - covered)}")


def main():
    try:
        event = os.environ["GITHUB_EVENT_NAME"]
        validate(event, json.loads(os.environ["GATE_RESULTS"]))
        if event == "merge_group":
            run_id = int(os.environ["GITHUB_RUN_ID"])
            attempt = int(os.environ["GITHUB_RUN_ATTEMPT"])
            jobs = read_jobs(os.environ["GITHUB_REPOSITORY"], run_id, attempt,
                             os.environ["GH_TOKEN"], os.environ["GITHUB_API_URL"])
            validate_jobs(jobs, run_id, attempt, os.environ["GITHUB_SHA"], os.environ["RUNNER_NAME"])
    except (KeyError, ValueError, TypeError, AttributeError, OSError) as error:
        print(f"Merge gate failed: {error}", file=sys.stderr)
        return 1
    print("Every dependency has the required result for this event.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
