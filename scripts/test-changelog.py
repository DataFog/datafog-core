"""Exercise the changelog policy against committed PR diffs."""

import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

CHECKER = Path(__file__).with_name("check-changelog.py")
INITIAL = "# Changelog\n\n## [Unreleased]\n\n## [0.4.4] - 2026-10-01\n\n- Earlier behavior.\n"


class ChangelogPolicy(unittest.TestCase):
    def setUp(self):
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        self.repo = Path(directory.name)
        self.git("init", "-b", "main")
        self.git("config", "user.name", "Test")
        self.git("config", "user.email", "test@example.com")
        self.write("CHANGELOG.md", INITIAL)
        self.write("behavior.txt", "original\n")
        self.commit("base")
        self.git("switch", "-c", "feature")

    def git(self, *args):
        return subprocess.check_output(
            ["git", "-C", str(self.repo), *args], text=True, stderr=subprocess.PIPE
        ).strip()

    def write(self, name, text):
        (self.repo / name).write_text(text, encoding="utf-8")

    def commit(self, message):
        self.git("add", ".")
        self.git("commit", "-m", message)

    def entry(self):
        self.write("CHANGELOG.md", INITIAL.replace(
            "## [Unreleased]\n", "## [Unreleased]\n\n### Added\n\n- New behavior.\n"
        ))

    def check(self, expected, *args, event=None):
        command = [sys.executable, str(CHECKER), "--base", "main", *args]
        if event is not None:
            event_path = self.repo / "event.json"
            event_path.write_text(json.dumps(event), encoding="utf-8")
            command.extend(["--event", str(event_path)])
        result = subprocess.run(command, cwd=self.repo, capture_output=True, text=True)
        self.assertEqual(result.returncode, expected, result.stdout + result.stderr)

    def test_committed_entry_covers_the_whole_pr(self):
        self.entry()
        self.commit("entry")
        self.write("behavior.txt", "changed\n")
        self.commit("implementation")
        self.check(0)

    def test_uncommitted_and_staged_entries_do_not_cover_behavior(self):
        self.write("behavior.txt", "changed\n")
        self.commit("implementation")
        self.entry()
        self.check(1)
        self.git("add", "CHANGELOG.md")
        self.check(1)
        self.commit("entry")
        self.check(0)

    def test_blank_heading_deletion_and_historical_edits_do_not_count(self):
        for text in (
            "",
            INITIAL + "\n",
            INITIAL.replace("## [Unreleased]\n", "## [Unreleased]\n\n### Added\n\n- \n"),
            INITIAL.replace("Earlier behavior.", "Reworded historical behavior."),
        ):
            with self.subTest(text=text):
                self.git("reset", "--hard", "main")
                self.write("behavior.txt", "changed\n")
                self.write("CHANGELOG.md", text)
                self.commit("no new entry")
                self.check(1)

    def test_renaming_changelog_does_not_count(self):
        self.git("mv", "CHANGELOG.md", "history.md")
        self.commit("rename")
        self.check(1)

    def test_exemption_requires_both_label_and_nonempty_reason(self):
        self.write("behavior.txt", "changed\n")
        self.commit("implementation")
        for labels, body, expected in (
            ([], "Changelog exemption: Tests only.", 1),
            ([{"name": "no-changelog"}], "", 1),
            ([{"name": "no-changelog"}], "Changelog exemption:   \nOther text", 1),
            ([{"name": "no-changelog"}], "Changelog exemption: Regression tests only.", 0),
        ):
            with self.subTest(labels=labels, body=body):
                self.check(expected, event={"pull_request": {"labels": labels, "body": body}})

    def test_missing_base_and_malformed_events_fail(self):
        self.check(1, "--base", "missing")
        self.write("behavior.txt", "changed\n")
        self.commit("implementation")
        self.check(1, event={})
        self.write("event.json", "invalid json")
        self.check(1, "--event", str(self.repo / "event.json"))

    def test_base_branch_entry_does_not_cover_feature(self):
        self.git("switch", "main")
        self.entry()
        self.commit("unrelated base entry")
        self.git("switch", "feature")
        self.write("behavior.txt", "changed\n")
        self.commit("feature")
        self.check(1)

    def test_stacked_pr_uses_its_actual_target(self):
        self.entry()
        self.commit("parent slice")
        self.git("branch", "parent")
        self.write("behavior.txt", "child change\n")
        self.commit("child slice")
        self.check(0)
        self.check(1, "--base", "parent")
        self.write("CHANGELOG.md", (self.repo / "CHANGELOG.md").read_text().replace(
            "- New behavior.\n", "- New behavior.\n- Child behavior.\n"
        ))
        self.commit("child entry")
        self.check(0, "--base", "parent")

    def test_initial_changelog_with_entry_counts(self):
        self.git("switch", "main")
        self.git("rm", "CHANGELOG.md")
        self.commit("base without changelog")
        self.git("switch", "feature")
        self.git("merge", "--ff-only", "main")
        self.entry()
        self.commit("initial changelog")
        self.check(0)

    def test_bots_use_the_same_explicit_policy(self):
        self.write("behavior.txt", "changed\n")
        self.commit("bot update")
        self.check(1, event={"pull_request": {
            "user": {"login": "dependabot[bot]", "type": "Bot"}, "labels": [], "body": ""
        }})

    def test_no_changes_pass(self):
        self.check(0)


if __name__ == "__main__":
    unittest.main()
