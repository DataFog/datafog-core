"""Prove prose classification and the PR-only matrix optimization boundary."""
import importlib.util
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('scope', ROOT / 'scripts/ci-change-scope.py')
scope = importlib.util.module_from_spec(spec)
spec.loader.exec_module(scope)
WORKFLOWS = {
    'fogvault-integration': ('native', 'joint'),
    'fogvault-native-languages': ('installed-native-languages',),
    'application-vaults': ('application-vault',),
    'credential-stores': ('credential-store',),
}


class ScopeTests(unittest.TestCase):
    def classify(self, paths):
        with patch.object(scope.subprocess, 'run', return_value=subprocess.CompletedProcess([], 0, b'\0'.join(paths) + b'\0')):
            return scope.requires_checks('pull_request', 'base', 'head')

    def test_explicit_prose_only(self):
        self.assertFalse(self.classify([b'README.md', b'docs/guides/email-boundaries.mdx', b'.cursor/skills/verify-datafog/features/entities.md']))
        for path in (b'crates/core/src/lib.rs', b'docs/contracts/core-v1.json',
                     b'docs/contracts/launch-api-python-example.py', b'docs/contracts/core-v1.md',
                     b'docs/integrations/evidence/probe.rs', b'docs/docs.json',
                     b'.github/workflows/change-scope.yml', b'bindings/python/README.md', b'unknown.md', b'.cursor/skills/verify-datafog/script.py'):
            with self.subTest(path=path):
                self.assertTrue(self.classify([b'README.md', path]))

    def test_nul_paths(self):
        self.assertFalse(self.classify([b'docs/a b\nc.md']))
        self.assertTrue(self.classify([b'docs/a.md\nscript.py']))

    def test_unknown_base_and_empty_fail_conservatively(self):
        for base in ('', 'missing', '0' * 40):
            with patch.object(scope.subprocess, 'run', side_effect=subprocess.CalledProcessError(128, ['git'])):
                self.assertTrue(scope.requires_checks('pull_request', base, 'head'))
        with patch.object(scope.subprocess, 'run', return_value=subprocess.CompletedProcess([], 0, b'')):
            self.assertTrue(scope.requires_checks('pull_request', 'base', 'head'))

    def test_queue_push_manual_ignore_classifier(self):
        for event in ('merge_group', 'push', 'workflow_dispatch', 'workflow_call', ''):
            with patch.object(scope.subprocess, 'run', side_effect=AssertionError('must not classify')):
                self.assertTrue(scope.requires_checks(event, 'base', 'head'))

    def test_real_git_deletion_and_rename_both_endpoints(self):
        with tempfile.TemporaryDirectory() as directory:
            def git(*args):
                return subprocess.check_output(['git', '-C', directory, *args]).decode().strip()
            git('init', '-q')
            git('config', 'user.name', 'Scope test')
            git('config', 'user.email', 'scope@example.invalid')
            source = Path(directory) / 'program.py'
            source.write_text('print("example")\n')
            git('add', '.')
            git('commit', '-qm', 'base')
            base = git('rev-parse', 'HEAD')
            source.rename(Path(directory) / 'README.md')
            git('add', '-A')
            git('commit', '-qm', 'rename')
            head = git('rev-parse', 'HEAD')
            original = scope.subprocess.run
            def run(args, **kwargs):
                return original(['git', '-C', directory, *args[1:]], **kwargs)
            with patch.object(scope.subprocess, 'run', side_effect=run):
                self.assertTrue(scope.requires_checks('pull_request', base, head))
                self.assertTrue(scope.requires_checks('pull_request', head, base))

    def test_matrix_wiring_preserves_full_non_pr_runs(self):
        for name, jobs in WORKFLOWS.items():
            source = (ROOT / f'.github/workflows/{name}.yml').read_text()
            self.assertIn("  scope:\n    uses: ./.github/workflows/change-scope.yml\n", source)
            for job in jobs:
                self.assertIn(f"  {job}:\n    needs: scope\n    if: ${{{{ !cancelled() && (github.event_name != 'pull_request' || needs.scope.outputs.code != 'false') }}}}\n", source)
            self.assertNotIn("  scope:\n    if:", source)
            self.assertNotIn('continue-on-error:', source)
        native = (ROOT / '.github/workflows/fogvault-native-languages.yml').read_text()
        self.assertIn("if: ${{ !cancelled() && needs.installed-native-languages.result == 'success' }}", native)
        helper = (ROOT / '.github/workflows/change-scope.yml').read_text()
        self.assertIn('if [[ "$EVENT" != pull_request ]]; then', helper)
        self.assertIn("echo 'code=true'", helper)
        self.assertIn("if: github.event_name == 'pull_request'", helper)
        non_pr, pr = helper.split('          else\n')
        self.assertNotIn('python3 ', non_pr)
        self.assertIn('python3 scripts/test-ci-change-scope.py', pr)


if __name__ == '__main__':
    unittest.main()
