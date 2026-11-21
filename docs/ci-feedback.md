# Measuring CI feedback

Use `python3 scripts/report-ci-feedback.py --pr 286 --pr 287 --queue-run 44659384965 --save-snapshot /tmp/ci-input.json > /tmp/ci-report.json` with authenticated `gh`. Calls are read-only. Repeat `--pr` or `--queue-run` for an explicitly selected cohort. Replay without GitHub access using `--snapshot /tmp/ci-input.json`; retain snapshots alongside reports to make observations reproducible.

The JSON separates docs, code and merge-queue batches. PR scope uses the conservative prose classifier, including the previous filename of renamed files. It retrieves current/final PR-head runs and checks that the head did not change while fetching files. Workflow creations within 60 seconds form one observed batch. Later Changelog-only events are excluded because label/body edits do not constitute new technical feedback. Multiple technical batches remain visible; older batches are excluded. Rerun, cancelled, incomplete, mixed-head and duplicate-workflow batches carry explicit exclusions. Original-attempt jobs are fetched even when retries exist.

`eligible` means the observed batch passed these accounting filters, **not** that all expected workflows ran or branch protection passed. API results cannot establish workflows that should have triggered but never did. Inspect workflow revisions and selection before treating a batch as complete. Selected PRs are not a representative random population; the report deliberately does not publish p50/p95.

Feedback time runs from first workflow creation to last observed job completion. Waiting time runs from each job's creation to start; it includes scheduling delay but does not identify the responsible capacity limit. Summed execution seconds are neither billed minutes nor elapsed feedback time. Skipped jobs do not count as allocations; unfinished jobs with no valid execution interval are reported as exclusions. The last-finishing job is evidence of the tail, not a reconstructed dependency critical path. GitHub timestamps have second resolution.

Do not pool docs and code, PRs and queues, historical and current workflow revisions, or cancelled pushes with successful first attempts. For capacity decisions, collect a stable period and report cohort sizes, exclusions, runner labels and actual billing separately. A current-head snapshot cannot reconstruct historical PR files or identify every push from timestamps alone.

Run regression fixtures with `python3 scripts/test-ci-feedback.py`.
