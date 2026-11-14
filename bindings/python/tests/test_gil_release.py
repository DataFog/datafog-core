"""Prove the synchronous scan paths release the GIL for their Rust work.

Python strings are immutable, so every entry point converts its arguments to
owned Rust data and then detaches. A counter thread can only make progress at
sleep granularity during the call when the GIL is released, and a Rust panic
must surface as a `DataFogInternalError` (a `RuntimeError`) instead of
aborting the interpreter.
"""

import sys
import threading
import time
from contextlib import contextmanager

import pytest

from datafog_core import (
    DataFogInternalError,
    StructuredFinding,
    panic_probe,
    discover_fields,
    scan,
    scan_and_transform,
    scan_and_transform_structured,
    scan_structured,
    transform,
    transform_structured,
)

REDACT_EMAIL = {"transform": {"default": {"strategy": "redact"}, "entities": ["EMAIL"]}}
REDACT_EMAIL_PLAIN = {"default": {"strategy": "redact"}, "entities": ["EMAIL"]}
TEXT_UNIT = "jane@example.com and 123-45-6789\n"
MINIMUM_MS = 200
TICK_INTERVAL = 0.001
# The threshold sits about 30x below the tick rate measured while detached, and
# above the ticks a GIL-holding call still leaks at bytecode boundaries.
MS_PER_TICK = 50


@contextmanager
def long_switch_interval():
    """Keep bytecode-level GIL handoffs from imitating a released GIL."""
    previous = sys.getswitchinterval()
    sys.setswitchinterval(0.1)
    try:
        yield
    finally:
        sys.setswitchinterval(previous)


def measure_worker_progress(work, maximum_repeats=1_000_000):
    """Run `work` on a worker thread and count counter-thread ticks meanwhile.

    The counter thread waits until the worker has started, so ticks recorded
    before the call cannot be mistaken for progress. Repeats double until the
    call is long enough to judge, so the test also holds on a fast release
    build. The ceiling is high enough for the cheapest workload on fast
    hardware; at the ceiling the largest workloads peak at a few gigabytes, so
    the repeats count is the only thing that grows.
    """
    repeats = 2000
    while True:
        prepared = work(repeats)
        ticks = []
        errors = []
        marks = []
        stop = threading.Event()
        started = threading.Event()

        def count_ticks():
            started.wait()
            while not stop.is_set():
                ticks.append(time.perf_counter())
                time.sleep(TICK_INTERVAL)

        def run():
            marks.append(time.perf_counter())
            started.set()
            try:
                prepared()
            except BaseException as error:
                errors.append(error)

        counter = threading.Thread(target=count_ticks, daemon=True)
        worker = threading.Thread(target=run)
        with long_switch_interval():
            counter.start()
            worker.start()
            worker.join()
            finished = time.perf_counter()
            stop.set()
            counter.join()
        assert not errors, errors
        elapsed_ms = (finished - marks[0]) * 1000
        progress = sum(1 for tick in ticks if marks[0] < tick < finished)
        if elapsed_ms >= MINIMUM_MS or repeats >= maximum_repeats:
            return progress, elapsed_ms
        repeats *= 2


def assert_progress(progress, elapsed_ms, call):
    assert elapsed_ms >= MINIMUM_MS, f"workload too small to judge: {elapsed_ms:.0f}ms"
    required = max(3, elapsed_ms // MS_PER_TICK)
    assert progress >= required, (
        f"counter thread made {progress} ticks during the {elapsed_ms:.0f}ms "
        f"{call} call, expected at least {required}"
    )


def text_work(call):
    def work(repeats):
        text = TEXT_UNIT * repeats
        return lambda: call(text)

    return work


def structured_work(call):
    def work(repeats):
        data = [{"email": "jane@example.com"} for _ in range(repeats)]
        return lambda: call(data)

    return work


def transform_work(repeats):
    text = TEXT_UNIT * repeats
    findings = scan(text)
    return lambda: transform(text, findings, REDACT_EMAIL_PLAIN)


def transform_structured_work(repeats):
    data = [{"email": "jane@example.com"} for _ in range(repeats)]
    findings = [
        StructuredFinding(item.path, item.finding)
        for item in scan_structured(data).findings
    ]
    return lambda: transform_structured(data, findings, REDACT_EMAIL_PLAIN)


WORKLOADS = [
    ("scan_and_transform", text_work(lambda text: scan_and_transform(text, REDACT_EMAIL))),
    ("transform", transform_work),
    ("scan_structured", structured_work(scan_structured)),
    ("discover_fields", structured_work(discover_fields)),
    ("transform_structured", transform_structured_work),
    (
        "scan_and_transform_structured",
        structured_work(
            lambda data: scan_and_transform_structured(data, REDACT_EMAIL)
        ),
    ),
]


@pytest.mark.parametrize("call, work", WORKLOADS, ids=[label for label, _ in WORKLOADS])
def test_synchronous_calls_release_the_gil(call, work):
    assert_progress(*measure_worker_progress(work), call)


def test_rust_panic_becomes_internal_error():
    with pytest.raises(DataFogInternalError) as caught:
        panic_probe()
    assert isinstance(caught.value, RuntimeError)
    assert caught.value.code == "internal_error"
    assert str(caught.value) == (
        "unexpected Rust panic probe failure: forced panic probe"
    )


def main():
    for call, work in WORKLOADS:
        assert_progress(*measure_worker_progress(work), call)
    test_rust_panic_becomes_internal_error()


if __name__ == "__main__":
    main()
