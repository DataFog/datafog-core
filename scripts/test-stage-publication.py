"""Prove exact archive retention, native order and disabled npm lifecycle hooks."""
import importlib.util
import io
import json
from pathlib import Path
import shutil
import tarfile
import tempfile
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location('stage_publication', Path(__file__).with_name('stage-publication.py'))
staging = importlib.util.module_from_spec(spec)
spec.loader.exec_module(staging)


def crate(path, version='1.0.0'):
    path.parent.mkdir(parents=True, exist_ok=True)
    data = ('[package]\nname="datafog-core"\nversion="' + version + '"\n').encode()
    with tarfile.open(path, 'w:gz') as archive:
        entry = tarfile.TarInfo('datafog-core-1.0.0/Cargo.toml')
        entry.size = len(data)
        archive.addfile(entry, io.BytesIO(data))


class PublicationInputs(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.root = Path(self.temporary.name)
        self.output = self.root / 'output'
        self.output.mkdir()
        source = self.root / 'crates/core'
        source.mkdir(parents=True)
        (source / 'Cargo.toml').write_text('[package]\nname="datafog-core"\nversion="1.0.0"\n')
        self.patcher = patch.object(staging, 'ROOT', self.root)
        self.patcher.start()

    def tearDown(self):
        self.patcher.stop()
        self.temporary.cleanup()

    def test_cargo_identical_temporary_copies_are_retained_exactly(self):
        archive = self.root / 'target/package/tmp-crate/datafog-core-1.0.0.crate'
        crate(archive)
        duplicate = self.root / 'target/package/tmp-registry' / archive.name
        duplicate.parent.mkdir(parents=True)
        shutil.copyfile(archive, duplicate)
        rows = staging.stage_rust(self.output)
        self.assertEqual(rows[0]['file'], archive.name)
        self.assertEqual((self.output / archive.name).read_bytes(), archive.read_bytes())

    def test_cargo_missing_or_conflicting_copies_fail(self):
        with self.assertRaises(ValueError):
            staging.stage_rust(self.output)
        archive = self.root / 'target/package/datafog-core-1.0.0.crate'
        crate(archive)
        crate(self.root / 'target/package/tmp-crate/datafog-core-1.0.0.crate', version='2.0.0')
        with self.assertRaises(ValueError):
            staging.stage_rust(self.output)

    def test_cargo_wrong_internal_identity_fails(self):
        crate(self.root / 'target/package/datafog-core-1.0.0.crate', version='2.0.0')
        with self.assertRaises(ValueError):
            staging.stage_rust(self.output)

    def npm_source(self, name, directory, optional=None):
        directory.mkdir(parents=True)
        metadata = {'name': name, 'version': '1.0.0', 'publishConfig': {'tag': 'latest'},
                    'scripts': {'prepack': 'node -e "require(\'fs\').writeFileSync(\'hook-ran\',\'bad\')"'}}
        if optional is not None:
            metadata['optionalDependencies'] = optional
        (directory / 'package.json').write_text(json.dumps(metadata))
        return metadata

    def test_node_packs_five_platforms_then_root_without_running_hooks(self):
        directory = self.root / 'bindings/node'
        dependencies = {'@datafog/node-' + target: '1.0.0' for target in staging.PLATFORMS}
        self.npm_source('@datafog/node', directory, dependencies)
        for target in staging.PLATFORMS:
            self.npm_source('@datafog/node-' + target, directory / 'npm' / target)
        rows = staging.stage_npm('node', self.output)
        self.assertEqual([row['name'] for row in rows], list(dependencies) + ['@datafog/node'])
        self.assertEqual(len(list(self.output.glob('*.tgz'))), 6)
        self.assertFalse(list(directory.rglob('hook-ran')))
        self.assertEqual((self.output / 'npm-tag.txt').read_text(), 'latest\n')
        with tarfile.open(self.output / rows[-1]['file'], 'r:gz') as archive:
            metadata = json.load(archive.extractfile('package/package.json'))
        self.assertEqual(metadata['optionalDependencies'], dependencies)

    def test_node_missing_platform_stops_before_packing(self):
        directory = self.root / 'bindings/node'
        self.npm_source('@datafog/node', directory)
        (directory / 'npm').mkdir()
        with self.assertRaises(ValueError):
            staging.stage_npm('node', self.output)
        self.assertFalse(list(self.output.glob('*.tgz')))

    def test_wasm_archives_once_without_running_hooks(self):
        directory = self.root / 'bindings/wasm'
        self.npm_source('@datafog/wasm', directory)
        rows = staging.stage_npm('wasm', self.output)
        self.assertEqual(len(rows), 1)
        self.assertFalse((directory / 'hook-ran').exists())

    def test_receipt_and_checksum_manifest_bind_exact_archive_bytes(self):
        archive = self.root / 'target/package/datafog-core-1.0.0.crate'
        crate(archive)
        with patch.object(staging.subprocess, 'check_output', return_value='a' * 40 + '\n'):
            staging.stage('rust')
        output = self.root / 'target/publication-rust'
        receipt = json.loads((output / 'receipt.json').read_text())
        self.assertEqual(receipt['revision'], 'a' * 40)
        self.assertEqual(receipt['packages'][0]['sha256'], staging.sha(archive))
        self.assertEqual((output / 'SHA256SUMS').read_text(), staging.sha(archive) + '  ' + archive.name + '\n')
        self.assertEqual((output / 'publication-order.txt').read_text(), archive.name + '\n')
        with self.assertRaises(FileExistsError):
            staging.stage('rust')


if __name__ == '__main__':
    unittest.main()
