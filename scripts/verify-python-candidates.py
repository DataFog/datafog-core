"""Reject a partial PyPI release whose existing filenames differ from qualified bytes."""
from email.parser import BytesParser
import hashlib
import json
from pathlib import Path
import sys
import tarfile
from urllib.error import HTTPError
from urllib.request import urlopen
import zipfile

for path in sorted(Path(sys.argv[1]).iterdir()):
    if path.suffix == ".whl":
        with zipfile.ZipFile(path) as archive:
            metadata = archive.read(next(n for n in archive.namelist() if n.endswith(".dist-info/METADATA")))
    elif path.name.endswith(".tar.gz"):
        with tarfile.open(path) as archive:
            member = next(m for m in archive.getmembers() if m.name.count("/") == 1 and m.name.endswith("/PKG-INFO"))
            stream = archive.extractfile(member)
            if stream is None:
                raise SystemExit("missing source distribution metadata")
            metadata = stream.read()
    else:
        continue
    message = BytesParser().parsebytes(metadata)
    name, version = message["Name"], message["Version"]
    try:
        with urlopen(f"https://pypi.org/pypi/{name}/{version}/json") as response:
            published = json.load(response)["urls"]
    except HTTPError as error:
        if error.code != 404:
            raise
        print(f"New qualified distribution: {name} {version} {path.name}")
        continue
    existing = next((f for f in published if f["filename"] == path.name), None)
    if existing is not None:
        with path.open("rb") as stream:
            digest = hashlib.file_digest(stream,"sha256").hexdigest()
        if existing["digests"]["sha256"] != digest:
            raise SystemExit(f"Published distribution differs from qualified bytes: {path.name}")
        print(f"Already published with matching integrity: {path.name}")
