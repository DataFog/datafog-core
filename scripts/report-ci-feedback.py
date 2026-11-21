"""Read-only CI feedback accounting. See docs/ci-feedback.md for scope and limits."""
import argparse
from datetime import datetime, timezone
import importlib.util
import json
import math
from pathlib import Path
import subprocess
from urllib.parse import quote

SPEC = importlib.util.spec_from_file_location("scope", Path(__file__).with_name("ci-change-scope.py"))
SCOPE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(SCOPE)


def timestamp(value):
    if not isinstance(value, str):
        raise ValueError("missing timestamp")
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


def collect(repository, prs, queue_runs, recent_prs=0):
    prs = list(dict.fromkeys(prs))
    queue_runs = list(dict.fromkeys(queue_runs))
    root = f"repos/{repository}"
    snapshot = {"repository": repository, "collected_at": datetime.now(timezone.utc).isoformat(), "subjects": []}
    if recent_prs:
        recent = []
        for page in range(1, (recent_prs + 99) // 100 + 1):
            recent.extend(api(f"{root}/pulls?state=all&sort=created&direction=desc&per_page=100&page={page}"))
        prs = list(dict.fromkeys(prs + [p['number'] for p in recent[:recent_prs]]))
    snapshot['selection'] = {"recent_prs": recent_prs, "pr_numbers": prs, "queue_run_ids": queue_runs}
    for number in prs:
        pr = api(f"{root}/pulls/{number}")
        files = pages(f"{root}/pulls/{number}/files")
        if len(files) != pr['changed_files']:
            raise ValueError('PR file listing incomplete or changed during collection')
        runs = pages(f"{root}/actions/runs?event=pull_request&head_sha={pr['head']['sha']}", "workflow_runs")
        # A head SHA may occur on multiple PRs. Do not silently attribute such
        # a run to this PR when GitHub supplies a different association.
        associations = pages(f"{root}/pulls?state=all&head={quote(pr['head']['label'], safe='')}")
        matching = [p['number'] for p in associations if p['head']['sha'] == pr['head']['sha']
                    and p['head']['ref'] == pr['head']['ref'] and p['head']['repo'] and pr['head']['repo']
                    and p['head']['repo']['full_name'] == pr['head']['repo']['full_name']]
        for run in runs:
            run['pr_identity_verified'] = any(p['number'] == number for p in run.get('pull_requests', [])) or (
                not run.get('pull_requests') and matching == [number]
                and run['head_branch'] == pr['head']['ref']
                and run['head_repository']['full_name'] == pr['head']['repo']['full_name'])
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
        if not batches:
            rows.append({"pr": subject.get('pr'), "head_sha": subject['head_sha'], "cohort": 'unknown',
                         "eligible": False, "exclusions": ['no_technical_runs'], "jobs": [],
                         "observed_feedback_seconds": None, "outcome": 'unobserved'})
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
            if any(r['conclusion'] not in {'success', 'failure'} for r in batch):
                reasons.append('cancelled_or_other_run')
            if subject.get('pr') and any(not r.get('pr_identity_verified', False) for r in batch):
                reasons.append('unverified_pr_identity')
            jobs = []
            for run in batch:
                if not run['jobs']:
                    reasons.append('missing_jobs')
                for job in run['jobs']:
                    if job['conclusion'] == 'skipped':
                        continue
                    if job['conclusion'] not in {'success', 'failure'}:
                        reasons.append('cancelled_or_other_job')
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
            contexts = []
            for run in subject['runs']:
                if abs((timestamp(run['created_at']) - timestamp(batch[0]['created_at'])).total_seconds()) > 60:
                    continue
                for job in run['jobs']:
                    if job.get('name') in {'Changelog policy', 'Full technical suite'}:
                        try:
                            context_seconds = seconds(batch[0]['created_at'], job.get('completed_at'))
                        except (ValueError, TypeError):
                            context_seconds = None
                        contexts.append({"feedback_seconds": context_seconds, "name": job['name'], "conclusion": job['conclusion'],
                            "completed_at": job.get('completed_at'), "url": job['html_url'],
                            "run_attempt": run['run_attempt']})
            rows.append({"pr": subject.get('pr'), "head_sha": subject['head_sha'], "cohort": cohort,
                "outcome": ('other' if any(r['conclusion'] not in {'success', 'failure'} for r in batch) else
                            'failure' if any(r['conclusion'] == 'failure' for r in batch) else 'success'),
                "observed_required_contexts": contexts,
                "eligible": not reasons, "exclusions": sorted(set(reasons)),
                "run_ids": [r['id'] for r in batch], "policy_run_ids_excluded": policy,
                "observed_feedback_seconds": seconds(batch[0]['created_at'], last['completed_at']) if last else None,
                "allocations_observed": len(jobs), "execution_seconds_sum": sum(j['execution_seconds'] for j in jobs),
                "last_finishing_job": last, "jobs": jobs})
    summaries = {}
    for cohort in ('docs', 'code', 'queue'):
        selected = [r for r in rows if r['cohort'] == cohort]
        valid = [r for r in selected if r['eligible']]
        def distribution(values):
            ordered = sorted(values)
            return {"n": len(ordered), "p50_seconds": ordered[math.ceil(len(ordered) * .5) - 1] if len(ordered) >= 5 else None,
                    "p95_seconds": ordered[math.ceil(len(ordered) * .95) - 1] if len(ordered) >= 20 else None}
        summaries[cohort] = {"observed_batches": len(selected), "excluded_batches": len(selected)-len(valid),
            "completed_first_attempts": distribution([r['observed_feedback_seconds'] for r in valid]),
            "success_only": distribution([r['observed_feedback_seconds'] for r in valid if r['outcome'] == 'success']),
            "failed_first_attempts": sum(r['outcome'] == 'failure' for r in valid)}
    return {"cohort_summaries": summaries, "selection": snapshot.get('selection'), "repository": snapshot['repository'], "collected_at": snapshot['collected_at'], "batches": rows,
        "limitations": ["Observed technical workflows only; does not prove expected workflow completeness or required-check satisfaction.",
            "PR batches use a 60-second creation window; inspect multiple batches rather than treating them as independent pushes.",
            "Current/final PR heads only; PR files classify scope at collection time, not historical workflow selection.",
            "Waiting is job creation to start; runner availability and account concurrency cannot be distinguished.",
            "Summed execution is neither billed usage nor wall time; last-finishing job is not a dependency critical-path reconstruction.",
            "Percentiles use nearest rank: p50 suppressed below n=5, p95 below n=20. Descriptive selected-cohort statistics, not population estimates; workflow revisions may differ."]}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--repo', default='DataFog/datafog-core')
    parser.add_argument('--recent-prs', type=int, default=0, help='Newest N PRs by creation, all states (maximum 1000)')
    parser.add_argument('--pr', type=int, action='append', default=[])
    parser.add_argument('--queue-run', type=int, action='append', default=[])
    parser.add_argument('--snapshot', type=Path, help='Replay saved API data without network access')
    parser.add_argument('--save-snapshot', type=Path)
    args = parser.parse_args()
    if not 0 <= args.recent_prs <= 1000:
        parser.error("--recent-prs must be between 0 and 1000")
    if not (args.snapshot or args.pr or args.queue_run or args.recent_prs):
        parser.error('select --pr, --queue-run, or --snapshot')
    snapshot = json.loads(args.snapshot.read_text()) if args.snapshot else collect(args.repo, args.pr, args.queue_run, args.recent_prs)
    if args.save_snapshot:
        args.save_snapshot.write_text(json.dumps(snapshot, indent=2) + '\n')
    print(json.dumps(report(snapshot), indent=2))


if __name__ == '__main__':
    main()
