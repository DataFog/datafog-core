#!/usr/bin/env python3
"""Provision pinned browser SQLite3MC assets; never used implicitly by checks."""
import hashlib
import io
import json
from pathlib import Path
import urllib.request
import zipfile

root = Path(__file__).resolve().parents[1] / 'integrations/sqlite-browser'
lock = json.loads((root / 'sqlite3mc-lock.json').read_text())
with urllib.request.urlopen(lock['url'], timeout=120) as response:
    archive = response.read(16 * 1024 * 1024)
if hashlib.sha256(archive).hexdigest() != lock['sha256']:
    raise SystemExit('SQLite3MC archive digest mismatch')
with zipfile.ZipFile(io.BytesIO(archive)) as source:
    assets = {}
    for name, digest in lock['files'].items():
        matches = [entry for entry in source.namelist() if entry.endswith('/jswasm/' + name)]
        if len(matches) != 1:
            raise SystemExit('SQLite3MC asset missing or ambiguous')
        data = source.read(matches[0])
        if hashlib.sha256(data).hexdigest() != digest:
            raise SystemExit('SQLite3MC asset digest mismatch')
        assets[name] = data
    (root / 'vendor').mkdir(exist_ok=True)
    for name, data in assets.items():
        (root / 'vendor' / name).write_bytes(data)
print('Verified SQLite3MC ' + lock['version'])
