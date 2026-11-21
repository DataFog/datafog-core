"""Read-only CI feedback accounting. See docs/ci-feedback.md for scope and limits."""
import argparse
from datetime import datetime, timezone
import importlib.util
import json
from pathlib import Path
import subprocess

SPEC = importlib.util.spec_from_file_location("scope", Path(__file__).with_name("ci-change-scope.py"))
SCOPE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(SCOPE)


def timestamp(value):
    return datetime.fromisoformat(value.replace("Z", "+00:00"))


def seconds(start, end):
    value = (timestamp(end) - timestamp(start)).total_seconds()
    if value < 0:
        raise ValueError("negative interval")
    return value


def api(path):
    return json.loads(subprocess.check_output(["gh", "api", path], text=True))


def pages(path, field=None):
    values = []
    for page in range(1, 101):
        payload = api(f"{path}{'&' if '?' in path else '?'}per_page=100&page={page}")
        batch = payload[field] if field else payload
        values.extend(batch)
        if len(batch) < 100:
            if field and payload.get("total_count") != len(values):
                raise ValueError("API pagination incomplete or changed during collection")
            return values
    raise ValueError("API pagination limit exceeded")


def collect(repository, prs, queue_runs):
    root = f"repos/{repository}"
    snapshot = {"repository": repository, "collected_at": datetime.now(timezone.utc).isoformat(), "subjects": []}
    for number in prs:
        pr = api(f"{root}/pulls/{number}")
        files = pages(f"{root}/pulls/{number}/files")
        if len(files) != pr['changed_files']:
            raise ValueError('PR file listing incomplete or changed during collection')
        runs = pages(f"{root}/actions/runs?event=pull_request&head_sha={pr['head']['sha']}", "workflow_runs")
        current = api(f"{root}/pulls/{number}")
        snapshot["subjects"].append({"pr": number, "head_sha": pr['head']['sha'],
            "current_head_sha": current['head']['sha'], "files": files, "runs": runs})
    for run_id in queue_runs:
        run = api(f"{root}/actions/runs/{run_id}")
        if run['event'] != 'merge_group':
            raise ValueError(f"{run_id} is not a merge queue run")
        snapshot['subjects'].append({"head_sha": run['head_sha'], "runs": [run]})
    for subject in snapshot['subjects']:
        for run in subject['runs']:
            # Fetch the original attempt even when a later retry now exists.
            run['jobs'] = pages(f"{root}/actions/runs/{run['id']}/attempts/1/jobs", "jobs")
        if subject.get('pr'):
            subject['current_head_sha'] = api(f"{root}/pulls/{subject['pr']}")['head']['sha']
    return snapshot


def report(snapshot):
    rows = []
    for subject in snapshot['subjects']:
        runs = sorted(subject['runs'], key=lambda run: run['created_at'])
        # Changelog can rerun on label/body changes without a push; it is policy,
        # not technical feedback, and has a separate required context.
        policy = [r['id'] for r in runs if r['name'] == 'Changelog']
        runs = [r for r in runs if r['name'] != 'Changelog']
        batches = []
        for run in runs:
            if not batches or seconds(batches[-1][0]['created_at'], run['created_at']) > 60:
                batches.append([])
            batches[-1].append(run)
        for batch in batches:
            reasons = []
            if subject.get('pr') and subject['head_sha'] != subject['current_head_sha']:
                reasons.append('stale_head')
            if batch is not batches[-1]:
                reasons.append('superseded_batch')
            if len({r['name'] for r in batch}) != len(batch):
                reasons.append('duplicate_workflow')
            if any(r['head_sha'] != subject['head_sha'] for r in batch):
                reasons.append('mixed_heads')
            if any(r['run_attempt'] != 1 for r in batch):
                reasons.append('retried_run')
            if any(r['status'] != 'completed' for r in batch):
                reasons.append('incomplete_run')
            if any(r['conclusion'] != 'success' for r in batch):
                reasons.append('unsuccessful_run')
            jobs = []
            for run in batch:
                if not run['jobs']:
                    reasons.append('missing_jobs')
                for job in run['jobs']:
                    if job['conclusion'] == 'skipped':
                        continue
                    if job['conclusion'] != 'success':
                        reasons.append('unsuccessful_job')
                    if job.get('run_attempt') != 1 or job.get('head_sha') != subject['head_sha']:
                        reasons.append('mixed_job_identity')
                    try:
                        wait = seconds(job['created_at'], job['started_at'])
                        active = seconds(job['started_at'], job['completed_at'])
                        if job['status'] != 'completed':
                            raise ValueError('incomplete job')
                    except (TypeError, ValueError, KeyError):
                        reasons.append('incomplete_job_timing')
                        continue
                    jobs.append({"id": job['id'], "name": job['name'], "url": job['html_url'],
                        "wait_seconds": wait, "execution_seconds": active,
                        "completed_at": job['completed_at'], "conclusion": job['conclusion'],
                        "runner_labels": job.get('labels', [])})
            cohort = 'queue'
            if subject.get('pr'):
                files = subject['files']
                cohort = 'docs' if files and all(SCOPE.prose(f['filename'].encode()) and
                    ('previous_filename' not in f or SCOPE.prose(f['previous_filename'].encode())) for f in files) else 'code'
            last = max(jobs, key=lambda j: j['completed_at']) if jobs else None
            rows.append({"pr": subject.get('pr'), "head_sha": subject['head_sha'], "cohort": cohort,
                "eligible": not reasons, "exclusions": sorted(set(reasons)),
                "run_ids": [r['id'] for r in batch], "policy_run_ids_excluded": policy,
                "observed_feedback_seconds": seconds(batch[0]['created_at'], last['completed_at']) if last else None,
                "allocations_observed": len(jobs), "execution_seconds_sum": sum(j['execution_seconds'] for j in jobs),
                "last_finishing_job": last, "jobs": jobs})
    return {"repository": snapshot['repository'], "collected_at": snapshot['collected_at'], "batches": rows,
        "limitations": ["Observed technical workflows only; does not prove expected workflow completeness or required-check satisfaction.",
            "PR batches use a 60-second creation window; inspect multiple batches rather than treating them as independent pushes.",
            "Current/final PR heads only; PR files classify scope at collection time, not historical workflow selection.",
            "Waiting is job creation to start; runner availability and account concurrency cannot be distinguished.",
            "Summed execution is neither billed usage nor wall time; last-finishing job is not a dependency critical-path reconstruction.",
            "No population percentiles: explicitly selected runs and changing workflow revisions are not a representative sample."]}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--repo', default='DataFog/datafog-core')
    parser.add_argument('--pr', type=int, action='append', default=[])
    parser.add_argument('--queue-run', type=int, action='append', default=[])
    parser.add_argument('--snapshot', type=Path, help='Replay saved API data without network access')
    parser.add_argument('--save-snapshot', type=Path)
    args = parser.parse_args()
    if not (args.snapshot or args.pr or args.queue_run):
        parser.error('select --pr, --queue-run, or --snapshot')
    snapshot = json.loads(args.snapshot.read_text()) if args.snapshot else collect(args.repo, args.pr, args.queue_run)
    if args.save_snapshot:
        args.save_snapshot.write_text(json.dumps(snapshot, indent=2) + '\n')
    print(json.dumps(report(snapshot), indent=2))


if __name__ == '__main__':
    main()
