"""Explicit installed-wheel lifecycle/failure qualification with actual local workers."""
from concurrent.futures import ThreadPoolExecutor
import json
from pathlib import Path
import sys
import time

import psutil
from datafog_core import ModelSession, ModelCancellation, DataFogModelError, DataFogConfigurationError

config = json.loads(Path(sys.argv[1]).read_text(encoding="utf-8"))
limits = config["limits"]
wait_seconds = (limits["timeout_ms"] + limits["cleanup_timeout_ms"]) / 1000
prefix, suffix = "Ordinary words without private details. ", "John Smith lives at 123 Main Street."
long_text = prefix * ((limits["max_input_bytes"] - len(suffix)) // len(prefix)) + suffix


def open_worker():
    parent = psutil.Process()
    previous = {p.pid for p in parent.children()}
    session = ModelSession.open(config)
    workers = [p for p in parent.children() if p.pid not in previous]
    assert len(workers) == 1
    return session, workers[0]


def await_inference(worker, baseline, future):
    deadline = time.monotonic() + limits["timeout_ms"] / 1000
    while time.monotonic() < deadline:
        times = worker.cpu_times()
        if times.user + times.system > baseline:
            return
        if future.done():
            raise AssertionError("fixture completed before active cancellation could be exercised")
        time.sleep(0.001)
    raise AssertionError("worker did not begin inference within its configured deadline")


def reject(call, code):
    try:
        call()
    except (DataFogModelError, DataFogConfigurationError) as error:
        assert error.code == code, (error.code, code)
        assert config["bundle"] not in str(error) and "private@example.test" not in str(error)
    else:
        raise AssertionError(f"expected {code}; operation returned successful protection")


with ThreadPoolExecutor(max_workers=1) as executor:
    session, worker = open_worker()
    try:
        reject(lambda: session.scan("x" * (limits["max_input_bytes"] + 1), "combined"), "invalid_configuration")
        assert session.inspect()["state"] == "ready"
        cancellation = ModelCancellation()
        cpu = worker.cpu_times(); baseline = cpu.user + cpu.system
        pending = executor.submit(session.scan, long_text, "combined", cancellation=cancellation)
        await_inference(worker, baseline, pending)
        reject(lambda: session.scan("private@example.test", "combined"), "model_busy")
        cancellation.cancel()
        reject(lambda: pending.result(timeout=wait_seconds), "model_cancelled")
        assert session.inspect()["state"] == "failed"
        assert not worker.is_running()
        reject(lambda: session.scan("private@example.test", "combined"), "model_session_failed")
    finally:
        session.close()
    assert session.inspect()["state"] == "closed"

    session, worker = open_worker()
    try:
        worker.kill()
        # Core, the process owner, performs the reap; psutil must not consume its exit status.
        reject(lambda: session.scan("private@example.test", "combined"), "model_inference_failed")
        assert session.inspect()["state"] == "failed"
        assert not worker.is_running()
    finally:
        session.close()

    session, worker = open_worker()
    cpu = worker.cpu_times(); baseline = cpu.user + cpu.system
    pending = executor.submit(session.scan, long_text, "combined")
    await_inference(worker, baseline, pending)
    session.close()
    reject(lambda: pending.result(timeout=wait_seconds), "model_cancelled")
    assert not worker.is_running() and session.inspect()["state"] == "closed"
    session.close()

bad = dict(config, manifest_sha256="0" * 64)
reject(lambda: ModelSession.open(bad), "model_load_failed")
print("Actual worker active cancellation, busy admission, abrupt death, close interruption, oversize and sanitized atomic failures passed.")
