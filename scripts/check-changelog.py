"""Require a committed Unreleased entry or explicit exemption across a PR."""

import argparse
import json
import os
from pathlib import Path
import re
import subprocess
import sys


def git(*args):
    return subprocess.check_output(
        ["git", *args], text=True, encoding="utf-8", stderr=subprocess.PIPE
    )


def exempt(event_path):
    pr = json.loads(Path(event_path).read_text(encoding="utf-8"))["pull_request"]
    if not any(label["name"] == "no-changelog" for label in pr["labels"]):
        return False
    reason = re.search(
        r"^Changelog exemption:[ \t]*([^\r\n]+)", pr.get("body") or "", re.MULTILINE
    )
    if reason is None or not reason.group(1).strip():
        raise ValueError(
            "The no-changelog label requires 'Changelog exemption: <reason>' in the PR body."
        )
    print("Explicit changelog exemption found.")
    return True


def has_entry(base):
    patch = git(
        "diff", "--no-ext-diff", "--no-renames", "--unified=0", base, "HEAD", "--", "CHANGELOG.md"
    )
    added = set()
    line_number = None
    for line in patch.splitlines():
        hunk = re.match(r"^@@ -\d+(?:,\d+)? \+(\d+)(?:,\d+)? @@", line)
        if hunk:
            line_number = int(hunk.group(1))
        elif line_number is not None and line.startswith("+"):
            added.add(line_number)
            line_number += 1
        elif line_number is not None and line.startswith(" "):
            line_number += 1
    if not added:
        return False
    unreleased = False
    for number, line in enumerate(git("show", "HEAD:CHANGELOG.md").splitlines(), 1):
        if line.startswith("## "):
            unreleased = line.strip() == "## [Unreleased]"
        if unreleased and number in added and re.match(r"^- \S", line):
            return True
    return False


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base", default=os.environ.get("CHANGELOG_BASE", "origin/main"))
    parser.add_argument("--event", help="GitHub pull_request event JSON for explicit exemptions")
    args = parser.parse_args()
    try:
        base = git("merge-base", args.base, "HEAD").strip()
        if not git("diff", "--name-only", "-z", base, "HEAD"):
            print("No changes to check.")
            return 0
        if args.event and exempt(args.event):
            return 0
        if has_entry(base):
            print("Unreleased changelog entry found.")
            return 0
        print(
            "Add an entry under [Unreleased] in CHANGELOG.md, or use the explicit "
            "exemption described in CONTRIBUTING.md.", file=sys.stderr
        )
    except (subprocess.CalledProcessError, OSError, ValueError, KeyError, TypeError) as error:
        print(f"Cannot validate changelog: {error}", file=sys.stderr)
        print("Fetch the PR base, or set CHANGELOG_BASE to its local ref.", file=sys.stderr)
    return 1


if __name__ == "__main__":
    sys.exit(main())
