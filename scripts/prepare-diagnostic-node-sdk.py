"""Fetch only official checksum-verified Node inputs for the Win32 N-API shim."""
import hashlib
import json
from pathlib import Path
import tarfile
import urllib.request

version = "24.22.2"
base = f"https://nodejs.org/dist/v{version}/"
output = Path("diagnostic-sdk")
output.mkdir(exist_ok=True)
checksums = urllib.request.urlopen(base + "SHASUMS256.txt").read()
(output / "SHASUMS256.txt").write_bytes(checksums)
expected = dict(line.split()[::-1] for line in checksums.decode().splitlines() if line.strip())
records = []
for name in [f"node-v{version}-headers.tar.gz", "win-x64/node.lib"]:
    payload = urllib.request.urlopen(base + name).read()
    actual = hashlib.sha256(payload).hexdigest()
    assert actual == expected[name], (name, actual)
    path = output / Path(name).name
    path.write_bytes(payload)
    records.append({"url": base + name, "sha256": actual, "bytes": len(payload)})
with tarfile.open(output / f"node-v{version}-headers.tar.gz") as archive:
    archive.extractall(output, filter="data")
Path("shutdown-diagnosis").mkdir(exist_ok=True)
Path("shutdown-diagnosis/error-mode-build-inputs.json").write_text(json.dumps({"node": version, "inputs": records}, indent=2))
