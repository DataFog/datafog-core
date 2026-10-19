"""Actual installed-wheel model qualification and measured resource evidence.

Requires explicit local assets, an installed Core wheel and psutil. Synthetic
fixtures measure integration and report fixture quality, not general PII recall.
"""
from __future__ import annotations
import argparse
from concurrent.futures import ThreadPoolExecutor
import hashlib
import json
import os
from pathlib import Path
import platform
import statistics
import subprocess
import sys
import threading
import time

import psutil
from datafog_core import ModelSession, ModelCancellation, DataFogModelError, transform, transform_structured

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("config", type=Path)
parser.add_argument("report", type=Path)
parser.add_argument("--warm-runs", type=int, required=True, help="#55 requires at least 100 for each representative size")
args = parser.parse_args()
if args.warm_runs < 100:
    raise SystemExit("qualification requires the issue's 100 warm runs per size")
config = json.loads(args.config.read_text())
fixture_path = Path(__file__).with_name("model-fixtures.json")
fixtures = json.loads(fixture_path.read_text())
texts = [case["text"] for group in fixtures.values() for case in group]
texts += ["", "👋 e\u0301 John Smith lives at 123 Main Street. John Smith lives at 123 Main Street.",
          "Ordinary words without private details. " * 160 + "John Smith lives at 123 Main Street."]
text = texts[0]
memory_stop = threading.Event()
peak_rss = [0]
def sample_memory():
    process = psutil.Process()
    while not memory_stop.wait(0.01):
        try:
            peak_rss[0] = max(peak_rss[0], sum(p.memory_info().rss for p in [process, *process.children(recursive=True)]))
        except psutil.NoSuchProcess:
            pass
sampler = threading.Thread(target=sample_memory)
sampler.start()
started = time.perf_counter()
try:
    session = ModelSession.open(config)
except BaseException:
    memory_stop.set(); sampler.join()
    raise
cold_load_ms = (time.perf_counter() - started) * 1000
report = {"host": platform.platform(), "machine": platform.machine(), "python": platform.python_version(),
          "cold_load_ms": cold_load_ms, "fixture_sha256": hashlib.sha256(fixture_path.read_bytes()).hexdigest(),
          "fixture_kind": "synthetic integration fixtures; no population accuracy claim", "cases": [], "benchmarks": []}
try:
    report["session"] = session.inspect()
    started = time.perf_counter()
    session.scan(text, "model_only")
    report["first_scan_ms"] = (time.perf_counter() - started) * 1000
    session.warm_up()
    for value in texts:
        result = session.scan(value, "model_only")
        for finding, evidence in zip(result.findings, result.evidence, strict=True):
            assert value[finding.codepoint_range.start:finding.codepoint_range.end] == finding.matched_text
            assert value.encode()[finding.byte_range.start:finding.byte_range.end].decode() == finding.matched_text
            assert evidence["finding_index"] == result.findings.index(finding)
            score = evidence["sources"][0]["raw_score"]["value"]
            assert 0 <= score <= 1
            if "gliner2" in config:
                assert finding.confidence is None
        report["cases"].append({"text": value, "findings": [
            {"entity": f.entity_type, "text": f.matched_text, "start": f.codepoint_range.start,
             "end": f.codepoint_range.end, "byte_start": f.byte_range.start, "byte_end": f.byte_range.end}
            for f in result.findings], "evidence": result.evidence})
    assert any(f["entity"] == "STREET_ADDRESS" and f["text"] == "123 Main Street" for f in report["cases"][0]["findings"])
    assert report["cases"][1]["findings"] == []
    # Keep development and held-out denominators separate; do not tune thresholds on held-out output.
    offset = 0
    report["quality"] = {}
    for group, cases in fixtures.items():
        missed = false_positive = expected = returned = 0
        for case, actual in zip(cases, report["cases"][offset:offset + len(cases)], strict=True):
            observed = {f["text"] for f in actual["findings"] if f["entity"] == "STREET_ADDRESS"}
            truth = set(case["street_addresses"])
            missed += len(truth - observed); false_positive += len(observed - truth)
            expected += len(truth); returned += len(observed)
        report["quality"][group] = {"texts": len(cases), "expected_spans": expected, "returned_spans": returned,
                                   "misses": missed, "false_positives": false_positive}
        offset += len(cases)
    combined = session.scan(text, "combined")
    for strategy in ["redact", "mask", "remove"]:
        protected = transform(text, combined.findings, {"default": {"strategy": strategy}})
        assert "123 Main Street" not in protected.text
    document = {"first_name": "John", "note": text, "keep": 12, "negative": "The weather is clear."}
    structured = session.scan_structured(document, "combined")
    assert any(f.path == "/first_name" and f.finding.entity_type == "PERSON" for f in structured.findings)
    assert any(f.path == "/note" and f.finding.entity_type == "STREET_ADDRESS" for f in structured.findings)
    protected = transform_structured(document, structured.findings, {"default": {"strategy": "redact"}})
    assert protected.data["keep"] == 12 and protected.data["negative"] == document["negative"]
    cancellation = ModelCancellation(); cancellation.cancel()
    try:
        session.scan(text, "combined", cancellation=cancellation)
    except DataFogModelError as error:
        assert error.code == "model_cancelled"
    else:
        raise AssertionError("pre-cancelled call returned findings")
    for name, value in [("short", text), ("long", texts[-1])]:
        for mode in ("model_only", "combined"):
            durations = []
            for _ in range(args.warm_runs):
                started = time.perf_counter()
                session.scan(value, mode)
                durations.append((time.perf_counter() - started) * 1000)
            report["benchmarks"].append({"size": name, "mode": mode, "utf8_bytes": len(value.encode()),
                "runs": len(durations), "p50_ms": statistics.median(durations),
                "p95_ms": sorted(durations)[(95 * len(durations) + 99)//100 - 1],
                "throughput_requests_per_second": 1000 / statistics.mean(durations)})
    session.health()
finally:
    session.close()
    memory_stop.set(); sampler.join()
report["sampled_peak_parent_and_worker_rss_bytes"] = peak_rss[0]
report["memory_sampling_interval_ms"] = 10
report["state_after_close"] = session.inspect()["state"]
assert report["state_after_close"] == "closed"
args.report.parent.mkdir(parents=True, exist_ok=True)
args.report.write_text(json.dumps(report, indent=2))
print(json.dumps({"quality": report["quality"], "benchmarks": report["benchmarks"], "cold_load_ms": cold_load_ms,
                  "sampled_peak_rss_bytes": peak_rss[0]}))
