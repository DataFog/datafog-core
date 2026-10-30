"""A successful cargo exit alone cannot prove required consumer tests ran."""
import importlib.util
from concurrent.futures import ThreadPoolExecutor
import io
import json
from pathlib import Path
import tempfile
import threading
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location("native_registry", Path(__file__).with_name("qualify-native-registry.py"))
qualifier = importlib.util.module_from_spec(spec)
spec.loader.exec_module(qualifier)


class TestExecutionProof(unittest.TestCase):
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
