"""Freeze base SDK upload inputs and retain hashes; this command never publishes."""
import argparse
import hashlib
import json
from pathlib import Path
import re
import shutil
import subprocess
import tarfile
import tomllib

ROOT = Path(__file__).resolve().parents[1]
PLATFORMS = ('darwin-arm64', 'darwin-x64', 'linux-x64-gnu', 'linux-arm64-gnu', 'win32-x64-msvc')


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def stage_rust(output):
    package = tomllib.loads((ROOT / 'crates/core/Cargo.toml').read_text())['package']
    filename = package['name'] + '-' + package['version'] + '.crate'
    candidates = [path for path in (ROOT / 'target/package').rglob(filename) if path.is_file()]
    # Cargo versions may retain identical tmp-registry and tmp-crate copies.
    if not candidates or len({sha(path) for path in candidates}) != 1:
        raise ValueError('Missing or ambiguous Cargo publication archive')
    candidate = candidates[0]
    with tarfile.open(candidate, 'r:gz') as archive:
        metadata = tomllib.loads(archive.extractfile(filename[:-6] + '/Cargo.toml').read().decode())['package']
    if (metadata['name'], metadata['version']) != (package['name'], package['version']):
        raise ValueError('Wrong Cargo publication identity')
    shutil.copyfile(candidate, output / filename)
    return [{'registry': 'crates', 'name': package['name'], 'version': package['version'], 'file': filename}]


def pack(directory, output, name, version):
    metadata = json.loads((directory / 'package.json').read_text())
    if (metadata['name'], metadata['version']) != (name, version):
        raise ValueError('Unexpected npm publication package identity')
    result = json.loads(subprocess.check_output(
        ['npm', 'pack', '--json', '--ignore-scripts', '--pack-destination', str(output)],
        cwd=directory, text=True))
    if len(result) != 1 or not re.fullmatch(r'[A-Za-z0-9_.-]+\.tgz', result[0]['filename']):
        raise ValueError('Unsafe or ambiguous npm packed archive')
    filename = result[0]['filename']
    path = output / filename
    if not path.is_file() or path.is_symlink():
        raise ValueError('npm archive missing or linked')
    with tarfile.open(path, 'r:gz') as archive:
        actual = json.load(archive.extractfile('package/package.json'))
    if (actual['name'], actual['version']) != (name, version):
        raise ValueError('Packed npm archive has a different identity')
    return {'registry': 'npm', 'name': name, 'version': version, 'file': filename}


def stage_npm(kind, output):
    directory = ROOT / 'bindings' / kind
    metadata = json.loads((directory / 'package.json').read_text())
    packages = []
    if kind == 'node':
        actual = {path.name for path in (directory / 'npm').iterdir() if path.is_dir()}
        if actual != set(PLATFORMS):
            raise ValueError('Missing or extra native npm platform package')
        expected = {metadata['name'] + '-' + target: metadata['version'] for target in PLATFORMS}
        if metadata.get('optionalDependencies') != expected:
            raise ValueError('Root npm package does not require the exact five native packages')
        for target in PLATFORMS:
            packages.append(pack(directory / 'npm' / target, output,
                                 metadata['name'] + '-' + target, metadata['version']))
    packages.append(pack(directory, output, metadata['name'], metadata['version']))
    (output / 'npm-tag.txt').write_text(metadata.get('publishConfig', {}).get('tag', 'latest') + '\n')
    return packages


def stage(kind):
    output = ROOT / 'target' / ('publication-' + kind)
    output.mkdir(parents=True, exist_ok=False)
    packages = stage_rust(output) if kind == 'rust' else stage_npm(kind, output)
    filenames = [row['file'] for row in packages]
    if len(set(filenames)) != len(filenames):
        raise ValueError('Duplicate publication archive filenames')
    for row in packages:
        path = output / row['file']
        row.update(sha256=sha(path), bytes=path.stat().st_size)
    revision = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip()
    receipt = {'schema': 'datafog-base-publication-inputs/v1', 'revision': revision,
               'kind': kind, 'packages': packages}
    (output / 'receipt.json').write_text(json.dumps(receipt, indent=2) + '\n')
    (output / 'SHA256SUMS').write_text(''.join(row['sha256'] + '  ' + row['file'] + '\n' for row in packages))
    (output / 'publication-order.txt').write_text(''.join(name + '\n' for name in filenames))
    print(json.dumps({'staged': str(output), 'packages': len(packages), 'published': False}))


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('kind', choices=('rust', 'node', 'wasm'))
    stage(parser.parse_args().kind)
