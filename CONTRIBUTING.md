# Contributing

Follow [AGENTS.md](AGENTS.md) for repository rules and PR writing guidance.

## Changelog policy

Effective October 22, 2026, every PR that changes user-facing
behavior must add a concise bullet under `[Unreleased]` in `CHANGELOG.md`. Earlier
changes are not backfilled; their existing release notes remain in `docs/releases/`.

Use `Added`, `Changed`, `Fixed`, `Deprecated`, `Removed`, or `Security` as appropriate.
Describe observable behavior, including API, binding, configuration, installation,
and security changes. Record only what the PR implements. Keep entries unreleased
until release preparation moves them to a dated version heading.
A PR that only moves existing entries into a release section may use the exemption below.

CI checks the complete committed PR against its actual target branch, including
stacked PRs. An entry in an earlier commit of the same PR counts; follow-up commits
do not need duplicate entries. Entries from a parent PR do not cover a child PR.
Heading-only, blank, and historical-release edits do not satisfy the check.
Reviewers must check that the entry describes the change accurately.

## Exemptions

Tests, internal refactoring, and documentation or tooling changes with no user-facing
effect may use the `no-changelog` label. Include a nonempty reason in the PR body:

```text
Changelog exemption: Adds regression tests without changing runtime behavior.
```

Both the label and reason are required. Reviewers confirm the exemption. Bot PRs
follow the same policy. CI reruns when the description, labels, commits, or target
branch change.

## Checking locally

```sh
python3 scripts/test-changelog.py
python3 scripts/check-changelog.py --base origin/main
```

The checker reads committed changes. Fetch the target branch first, and use that
branch for `--base` when checking a stacked PR. Missing base refs and malformed
event data fail the check. Local checks do not consult GitHub labels unless an
event file is supplied with `--event`.

Once the workflow lands on `main`, add `Changelog policy` to the required checks
in GitHub branch rules. The workflow alone does not prevent merging a failed check.
