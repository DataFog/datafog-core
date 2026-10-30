"""A successful cargo exit alone cannot prove required consumer tests ran."""
import importlib.util
from concurrent.futures import ThreadPoolExecutor
import io
import json
import hashlib
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import os
from pathlib import Path
import subprocess
import tarfile
import tempfile
import threading
import tomllib
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location("native_registry", Path(__file__).with_name("qualify-native-registry.py"))
qualifier = importlib.util.module_from_spec(spec)
spec.loader.exec_module(qualifier)


class TestExecutionProof(unittest.TestCase):
    def test_cold_cargo_metadata_provisions_only_admitted_registry_identities(self):
        # Real Cargo and an empty home reproduce CI's missing beta.3 index entry.
        # A tiny localhost registry avoids relying on any public network/cache.
        archive = io.BytesIO()
        with tarfile.open(fileobj=archive, mode="w:gz") as bundle:
            for name, data in {
                "Cargo.toml": b'[package]\nname="datafog-core"\nversion="0.5.0-beta.3"\nedition="2021"\n',
                "src/lib.rs": b"pub fn synthetic_only() {}\n",
            }.items():
                entry = tarfile.TarInfo("datafog-core-0.5.0-beta.3/" + name)
                entry.size = len(data)
                bundle.addfile(entry, io.BytesIO(data))
        checksum = hashlib.sha256(archive.getvalue()).hexdigest()
        index = {"name": "datafog-core", "vers": "0.5.0-beta.3", "deps": [],
                 "cksum": checksum, "features": {}, "yanked": False}
        responses = {}
        class Handler(BaseHTTPRequestHandler):
            def do_GET(self):
                body = responses.get(self.path)
                self.send_response(200 if body is not None else 404)
                self.end_headers()
                if body is not None: self.wfile.write(body)
            def log_message(self, *_):
                pass
        server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        base = "http://127.0.0.1:" + str(server.server_port)
        responses.update({"/index/config.json": json.dumps({"dl": base + "/crates"}).encode(),
                          "/index/da/ta/datafog-core": (json.dumps(index) + "\n").encode(),
                          "/crates/datafog-core/0.5.0-beta.3/download": archive.getvalue()})
        thread = threading.Thread(target=server.serve_forever, daemon=True)
        thread.start()
        try:
            with tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                (root / "src").mkdir()
                (root / "src/lib.rs").write_text("// Synthetic metadata consumer.\n")
                manifest = root / "Cargo.toml"
                manifest.write_text('[package]\nname="cold-index-fixture"\nversion="0.0.0"\nedition="2021"\n[workspace]\n[dependencies]\npublished-beta3={package="datafog-core",version="=0.5.0-beta.3"}\n')
                (root / ".cargo").mkdir()
                (root / ".cargo/config.toml").write_text('[source.crates-io]\nreplace-with="synthetic"\n[source.synthetic]\nregistry=' + json.dumps("sparse+" + base + "/index/") + "\n")
                env = dict(os.environ, CARGO_HOME=str(root / "empty-cargo-home"), CARGO_NET_RETRY="0", CARGO_HTTP_TIMEOUT="10")
                command = qualifier.metadata_command(manifest)
                cold = subprocess.run(command + ["--offline"], cwd=root, env=env, capture_output=True, text=True, timeout=30)
                self.assertNotEqual(cold.returncode, 0)
                self.assertIn("datafog-core", cold.stderr)
                provisioned = subprocess.run(command, cwd=root, env=env, capture_output=True, text=True, timeout=30)
                self.assertEqual(provisioned.returncode, 0, provisioned.stderr)
                lock = root / "Cargo.lock"
                self.assertEqual(next(row["source"] for row in tomllib.loads(lock.read_text())["package"] if row["name"] == "datafog-core"), qualifier.REGISTRY)
                approved = {("datafog-core", "0.5.0-beta.3"): {"checksum": checksum}}
                self.assertEqual(len(qualifier.admit_registry_graph(lock, approved, {})), 1)
                with self.assertRaises(ValueError):
                    qualifier.admit_registry_graph(lock, {}, {})
                with self.assertRaises(ValueError):
                    qualifier.admit_registry_graph(lock, {("datafog-core", "0.5.0-beta.3"): {"checksum": "0" * 64}}, {})
        finally:
            server.shutdown()
            thread.join(timeout=5)
            server.server_close()

    def test_two_versions_wait_for_complete_cold_index(self):
        rows = [{"name": "block-buffer", "vers": version, "cksum": checksum}
                for version, checksum in (("0.10.4", "a" * 64), ("0.12.1", "b" * 64))]
        payload = ("\n".join(json.dumps(row) for row in rows) + "\n").encode()
        partial, second_attempt, release = threading.Event(), threading.Event(), threading.Event()
        class ObservedLock:
            def __init__(self):
                self.lock, self.count_lock, self.attempts = threading.Lock(), threading.Lock(), 0
            def __enter__(self):
                with self.count_lock:
                    self.attempts += 1
                    if self.attempts == 2: second_attempt.set()
                self.lock.acquire()
            def __exit__(self, *args):
                self.lock.release()
        lock = ObservedLock()
        real_write = Path.write_bytes
        def expose_partial_index(path, data):
            real_write(path, payload.splitlines(keepends=True)[0])
            partial.set()
            if not release.wait(5): raise AssertionError("cold-cache writer was never released")
            return real_write(path, data)
        with tempfile.TemporaryDirectory() as directory, \
             patch.object(qualifier.urllib.request, "urlopen", return_value=io.BytesIO(payload)) as fetch, \
             patch.object(Path, "write_bytes", expose_partial_index), \
             ThreadPoolExecutor(max_workers=2) as pool:
            cache = Path(directory)
            first = pool.submit(qualifier.read_index_entry, cache, "block-buffer", "0.10.4", "a" * 64, lock)
            try:
                self.assertTrue(partial.wait(5), "first index write did not expose its partial state")
                second = pool.submit(qualifier.read_index_entry, cache, "block-buffer", "0.12.1", "b" * 64, lock)
                self.assertTrue(second_attempt.wait(5), "second version never attempted the shared name lock")
                self.assertFalse(second.done(), "second version read an incomplete index")
            finally:
                release.set()
            self.assertEqual(first.result(timeout=5), rows[0])
            self.assertEqual(second.result(timeout=5), rows[1])
            self.assertEqual(fetch.call_count, 1)
            with self.assertRaises(ValueError):
                qualifier.read_index_entry(cache, "block-buffer", "0.12.1", "0" * 64, lock)

    def output(self):
        return "\n".join("test " + name + " ... ok" for name in sorted(qualifier.REQUIRED_CONSUMER_TESTS)) + "\ntest result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.71s\n"

    def test_complete_named_suite_passes(self):
        self.assertEqual(len(qualifier.admit_consumer_test_output(self.output())), 10)

    def test_missing_duplicate_ignored_or_failure_cannot_pass(self):
        text = self.output()
        first = text.splitlines()[0]
        for value in (text.replace(first + "\n", ""), text + first + "\n",
                      text.replace(first, first.replace("ok", "ignored")),
                      text.replace(first, first.replace("ok", "FAILED")),
                      text.replace("10 passed", "0 passed"), "test result: ok. 0 passed; 0 failed;\n"):
            with self.subTest(output=value), self.assertRaises(ValueError):
                qualifier.admit_consumer_test_output(value)


if __name__ == "__main__":
    unittest.main()
