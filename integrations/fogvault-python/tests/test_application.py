"""Installed application owner contracts using actual files and child processes."""
import json
import os
from pathlib import Path
import re
import queue
import threading
import secrets
import subprocess
import sys
import tempfile
import unittest

import datafog_fogvault as fv


def finding(text):
    return {"entityType": "EMAIL", "matchedText": text,
            "range": {"unit": "utf8_bytes", "start": 0, "end": len(text.encode())}}


class ApplicationVaultTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="datafog-application-")
        self.directory = str(Path(self.temp.name).resolve() / "vault")
        self.owners = []

    def tearDown(self):
        for owner in self.owners:
            owner.close()
        self.temp.cleanup()

    def open(self, **options):
        owner = fv.ApplicationVault.open("application-tests", directory=self.directory, **options)
        self.owners.append(owner)
        return owner

    def test_application_restart_encryption_and_explicit_protection(self):
        owner = self.open(allow_file_key=True)
        self.assertEqual(owner.protection_mode, "file_key")
        secret = "sentinel-vault-never-plaintext@example.test"
        operation = secrets.token_bytes(16)
        marker = owner.prepare_and_commit(secret, [finding(secret)], operation, True)
        self.assertNotIn(secret, marker)
        repeated_operation = secrets.token_bytes(16)
        self.assertEqual(owner.prepare_and_commit(secret, [finding(secret)], repeated_operation, True), marker)
        identity = owner.vault_id
        owner.close()
        reopened = self.open(allow_file_key=True)
        self.assertEqual(reopened.vault_id, identity)
        self.assertEqual(reopened.restore(marker), secret)
        self.assertEqual(reopened.operations()[-2]["id"], list(operation))
        self.assertEqual(reopened.operations()[-1]["id"], list(repeated_operation))
        self.assertIsInstance(reopened.operations()[-1]["occurredAtMs"], int)
        for path in Path(self.directory).iterdir():
            if path.is_file():
                self.assertNotIn(secret.encode(), path.read_bytes())
        with self.assertRaises(fv.FogVaultWorkspaceError) as caught:
            self.open()
        self.assertNotIn(secret, str(caught.exception))
        self.assertEqual(caught.exception.code, "key_required")

    def test_application_four_process_writers_refresh_long_lived_reader(self):
        reader = self.open(allow_file_key=True)
        script = r'''
import json, secrets, sys
import datafog_fogvault as fv
w = fv.ApplicationVault.open("application-tests", directory=sys.argv[1], allow_file_key=True)
for i in range(12):
    text = f"writer-{sys.argv[2]}-{i}@example.test"
    finding = {"entityType":"EMAIL", "matchedText":text,
               "range":{"unit":"utf8_bytes","start":0,"end":len(text.encode())}}
    result = w.prepare_and_commit(text, [finding], secrets.token_bytes(16), True)
    print(json.dumps([result, text]), flush=True)
    if sys.stdin.readline() != "continue\n":
        raise RuntimeError("missing reader acknowledgment")
w.close()
'''
        children = [subprocess.Popen([sys.executable, "-c", script, self.directory, str(i)],
                    stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                    text=True, bufsize=1) for i in range(4)]
        committed = queue.Queue()
        def collect(index, child):
            try:
                for line in child.stdout:
                    committed.put((index, json.loads(line), None))
            finally:
                code = child.wait()
                committed.put((index, None, (code, child.stderr.read())))
        threads = [threading.Thread(target=collect, args=(i, child), daemon=True)
                   for i, child in enumerate(children)]
        for thread in threads:
            thread.start()
        own_commits = 0
        try:
            received, finished = 0, set()
            counts = [0] * len(children)
            while len(finished) < len(children):
                index, payload, exited = committed.get(timeout=45)
                if exited is not None:
                    code, errors = exited
                    self.assertEqual(code, 0, errors)
                    self.assertEqual(counts[index], 12, "writer stopped before all committed markers")
                    finished.add(index)
                    continue
                marker, value = payload
                counts[index] += 1
                # Writer remains alive waiting for this acknowledgment while
                # this long-lived reader refreshes, restores and writes itself.
                self.assertIsNone(children[index].poll())
                self.assertEqual(reader.restore(marker), value)
                if received % 6 == 0:
                    text = f"long-lived-{own_commits}@example.test"
                    own_marker = reader.prepare_and_commit(text, [finding(text)], secrets.token_bytes(16), True)
                    self.assertEqual(reader.restore(own_marker), text)
                    own_commits += 1
                children[index].stdin.write("continue\n")
                children[index].stdin.flush()
                received += 1
            self.assertEqual(received, 48)
            for child in children:
                child.stdin.close()
            self.assertEqual(reader.head().revision, 48 + own_commits)
            self.assertEqual(len(reader.operations()), 49 + own_commits)
        finally:
            for child in children:
                if child.poll() is None:
                    child.kill()
                child.wait()
            for thread in threads:
                thread.join(timeout=5)
            for child in children:
                for stream in (child.stdin, child.stdout, child.stderr):
                    if stream and not stream.closed:
                        stream.close()

    def test_application_forget_revokes_after_restart(self):
        owner = self.open(allow_file_key=True)
        secret = "forgotten-sentinel@example.test"
        marker = owner.prepare_and_commit(secret, [finding(secret)], secrets.token_bytes(16), True)
        code = re.search(r"_([A-Z0-9]{6})\]\]", marker).group(1)
        owner.forget([code], secrets.token_bytes(16))
        owner.close()
        reopened = self.open(allow_file_key=True)
        report = reopened.restore_report(marker)
        self.assertIsNone(report["text"])
        self.assertEqual(report["issues"][0]["kind"], "expired")
        with self.assertRaises(fv.FogVaultWorkspaceError) as caught:
            reopened.restore(marker)
        self.assertNotIn(secret, str(caught.exception))

    def test_application_approval_wrong_key_and_owner_permissions(self):
        owner = self.open(key=bytes([71]) * 32)
        self.assertEqual(owner.protection_mode, "external_key")
        secret = "approval-sentinel@example.test"
        before = owner.head().revision
        with self.assertRaises(fv.FogVaultWorkspaceError):
            owner.prepare_and_commit(secret, [finding(secret)], secrets.token_bytes(16), False)
        self.assertEqual(owner.head().revision, before)
        with self.assertRaises(fv.FogVaultWorkspaceError) as caught:
            self.open(key=bytes([72]) * 32)
        self.assertNotIn(secret, str(caught.exception))
        self.assertNotIn(self.directory, str(caught.exception))
        if os.name == "posix":
            self.assertEqual(Path(self.directory).stat().st_mode & 0o077, 0)
            for path in Path(self.directory).iterdir():
                self.assertEqual(path.stat().st_mode & 0o077, 0)
            encrypted = Path(self.directory) / "workspace.enc"
            encrypted.chmod(0o644)
            with self.assertRaises(fv.FogVaultWorkspaceError) as caught:
                owner.restore("ordinary")
            self.assertEqual(caught.exception.code, "unsafe_permissions")
            encrypted.chmod(0o600)
