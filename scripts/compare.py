#!/usr/bin/env python3
"""Run the deterministic DataFog Python-versus-Rust comparison."""

from __future__ import annotations

import argparse
import json
import math
import platform
import statistics
import subprocess
import sys
import tempfile
import time
from collections import Counter
from datetime import datetime, timezone
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[1]
RESULTS_DIRECTORY = ROOT / "results"
PYTHON_REPOSITORY = "https://github.com/DataFog/datafog-python.git"
PYTHON_COMMIT = "75e414b23a4c9be1938263f509354e2cb4d886e2"
LABELS = (
    "EMAIL",
    "PHONE",
    "SSN",
    "CREDIT_CARD",
    "IP_ADDRESS",
    "DATE",
    "ZIP_CODE",
)
WARMUP_RUNS = 1
MEASURED_RUNS = 5

PYTHON_RUNNER = r'''
import json
import sys
import time
from datafog.engine import scan

labels = {"EMAIL", "PHONE", "SSN", "CREDIT_CARD", "IP_ADDRESS", "DATE", "ZIP_CODE"}
warmups = int(sys.argv[1])
runs = int(sys.argv[2])
records = [json.loads(line) for line in sys.stdin if line.strip()]

for _ in range(warmups):
    for record in records:
        scan(record["text"], engine="regex")

for record in records:
    durations_ns = []
    entities = []
    for run in range(runs):
        started = time.perf_counter_ns()
        result = scan(record["text"], engine="regex")
        durations_ns.append(time.perf_counter_ns() - started)
        if run == 0:
            entities = [
                {"label": entity.type, "text": entity.text, "start": entity.start, "end": entity.end}
                for entity in result.entities
                if entity.type in labels
            ]
    print(json.dumps({"id": record["id"], "entities": entities, "durations_ns": durations_ns}))
'''

BINDING_RUNNER = r'''
import json
import sys
import time
from datafog_rs import scan

warmups = int(sys.argv[1])
runs = int(sys.argv[2])
records = [json.loads(line) for line in sys.stdin if line.strip()]

for _ in range(warmups):
    for record in records:
        scan(record["text"])

for record in records:
    durations_ns = []
    entities = []
    for run in range(runs):
        started = time.perf_counter_ns()
        result = scan(record["text"])
        durations_ns.append(time.perf_counter_ns() - started)
        if run == 0:
            entities = [
                {"label": entity.label, "text": entity.text, "start": entity.start, "end": entity.end}
                for entity in result
            ]
    print(json.dumps({"id": record["id"], "entities": entities, "durations_ns": durations_ns}))
'''


def run(command: list[str], **kwargs: Any) -> subprocess.CompletedProcess[str]:
    return subprocess.run(command, check=True, text=True, **kwargs)


def git_revision(path: Path) -> str:
    return run(["git", "-C", str(path), "rev-parse", "HEAD"], capture_output=True).stdout.strip()


def load_records(path: Path) -> list[dict[str, Any]]:
    return [json.loads(line) for line in path.read_text().splitlines() if line.strip()]


def entity_key(entity: dict[str, Any]) -> tuple[str, str, int, int]:
    return (entity["label"], entity["text"], entity["start"], entity["end"])


def normalize(entities: list[dict[str, Any]]) -> list[dict[str, Any]]:
    return sorted(entities, key=lambda entity: (entity["start"], entity["end"], entity["label"]))


def percentile(values: list[int], percent: float) -> float:
    ordered = sorted(values)
    position = (len(ordered) - 1) * percent
    lower = math.floor(position)
    upper = math.ceil(position)
    if lower == upper:
        return float(ordered[lower])
    return ordered[lower] + (ordered[upper] - ordered[lower]) * (position - lower)


def quality(expected: list[dict[str, Any]], actual: list[dict[str, Any]]) -> dict[str, Any]:
    counts: dict[str, Counter[str]] = {label: Counter() for label in (*LABELS, "overall")}

    for expected_record, actual_record in zip(expected, actual, strict=True):
        expected_entities = {entity_key(entity) for entity in expected_record["entities"]}
        actual_entities = {entity_key(entity) for entity in actual_record["entities"]}

        for key in expected_entities & actual_entities:
            counts[key[0]]["tp"] += 1
            counts["overall"]["tp"] += 1
        for key in actual_entities - expected_entities:
            counts[key[0]]["fp"] += 1
            counts["overall"]["fp"] += 1
        for key in expected_entities - actual_entities:
            counts[key[0]]["fn"] += 1
            counts["overall"]["fn"] += 1

    def summarize(count: Counter[str]) -> dict[str, float | int]:
        tp, fp, fn = count["tp"], count["fp"], count["fn"]
        precision = tp / (tp + fp) if tp + fp else 0.0
        recall = tp / (tp + fn) if tp + fn else 0.0
        f1 = 2 * precision * recall / (precision + recall) if precision + recall else 0.0
        return {"tp": tp, "fp": fp, "fn": fn, "precision": precision, "recall": recall, "f1": f1}

    return {label: summarize(counts[label]) for label in (*LABELS, "overall")}


def compare_outputs(
    records: list[dict[str, Any]],
    left_output: list[dict[str, Any]],
    right_output: list[dict[str, Any]],
    left_name: str = "python",
    right_name: str = "rust",
) -> dict[str, Any]:
    differences = []
    for record, left_record, right_record in zip(records, left_output, right_output, strict=True):
        left_entities = normalize(left_record["entities"])
        right_entities = normalize(right_record["entities"])
        if left_entities != right_entities:
            differences.append(
                {
                    "id": record["id"],
                    left_name: left_entities,
                    right_name: right_entities,
                }
            )
    return {
        "different_sentences": len(differences),
        "difference_rate": len(differences) / len(records),
        "differences": differences,
    }


def performance(output: list[dict[str, Any]]) -> dict[str, float | int]:
    durations = [duration for record in output for duration in record["durations_ns"]]
    total_ns = sum(durations)
    return {
        "scan_calls": len(durations),
        "total_runtime_ms": total_ns / 1_000_000,
        "p50_latency_us": percentile(durations, 0.50) / 1_000,
        "p95_latency_us": percentile(durations, 0.95) / 1_000,
        "sentences_per_second": len(durations) / (total_ns / 1_000_000_000),
    }


def batch_performance(output: list[dict[str, Any]]) -> dict[str, float | int]:
    batch_durations = [
        sum(record["durations_ns"][run] for record in output)
        for run in range(MEASURED_RUNS)
    ]
    median_ns = statistics.median(batch_durations)
    return {
        "measured_runs": MEASURED_RUNS,
        "median_batch_ms": median_ns / 1_000_000,
        "p95_batch_ms": percentile(batch_durations, 0.95) / 1_000_000,
        "sentences_per_second": len(output) / (median_ns / 1_000_000_000),
    }


def parse_jsonl(stdout: str) -> list[dict[str, Any]]:
    return [json.loads(line) for line in stdout.splitlines() if line.strip()]


def run_rust(binary: Path, fixture: Path) -> list[dict[str, Any]]:
    completed = run(
        [str(binary), "--warmups", str(WARMUP_RUNS), "--runs", str(MEASURED_RUNS)],
        input=fixture.read_text(),
        capture_output=True,
    )
    return parse_jsonl(completed.stdout)


def run_python(interpreter: Path, fixture: Path) -> list[dict[str, Any]]:
    return run_python_runner(interpreter, PYTHON_RUNNER, fixture)


def run_python_binding(interpreter: Path, fixture: Path) -> list[dict[str, Any]]:
    return run_python_runner(interpreter, BINDING_RUNNER, fixture)


def run_python_runner(interpreter: Path, runner: str, fixture: Path) -> list[dict[str, Any]]:
    completed = run(
        [str(interpreter), "-c", runner, str(WARMUP_RUNS), str(MEASURED_RUNS)],
        input=fixture.read_text(),
        capture_output=True,
    )
    return parse_jsonl(completed.stdout)


def startup_time(command: list[str]) -> float:
    samples = []
    for _ in range(MEASURED_RUNS):
        started = time.perf_counter_ns()
        run(command, input="", capture_output=True)
        samples.append(time.perf_counter_ns() - started)
    return statistics.median(samples) / 1_000_000


def peak_memory_bytes(command: list[str], fixture: Path) -> int | None:
    time_command = Path("/usr/bin/time")
    if not time_command.exists():
        return None
    completed = run(
        [str(time_command), "-l", *command],
        input=fixture.read_text(),
        capture_output=True,
    )
    for line in completed.stderr.splitlines():
        if "maximum resident set size" in line:
            return int(line.split()[0])
    return None


def build_rust_runner() -> Path:
    run(["cargo", "build", "--release", "--bin", "scan_jsonl"], cwd=ROOT)
    return ROOT / "target" / "release" / "scan_jsonl"


def metadata(python: Path) -> dict[str, str | int]:
    return {
        "generated_at_utc": datetime.now(timezone.utc).isoformat(),
        "python_baseline_repository": PYTHON_REPOSITORY,
        "python_baseline_commit": PYTHON_COMMIT,
        "rust_core_commit": git_revision(ROOT),
        "python_baseline_version": run([str(python), "--version"], capture_output=True).stdout.strip(),
        "rust_version": run(["rustc", "--version"], capture_output=True).stdout.strip(),
        "platform": platform.platform(),
        "machine": platform.machine(),
        "warmup_runs": WARMUP_RUNS,
        "measured_runs": MEASURED_RUNS,
    }


def write_report(kind: str, report: dict[str, Any]) -> None:
    RESULTS_DIRECTORY.mkdir(exist_ok=True)
    timestamp = datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%SZ")
    output = RESULTS_DIRECTORY / f"{kind}-{timestamp}.json"
    output.write_text(json.dumps(report, indent=2) + "\n")
    print(f"Wrote {output.relative_to(ROOT)}")


def install_baseline(temporary_directory: str) -> Path:
    venv = Path(temporary_directory) / "venv"
    run([sys.executable, "-m", "venv", str(venv)])
    python = venv / "bin" / "python"
    run([str(python), "-m", "pip", "install", "--quiet", f"git+{PYTHON_REPOSITORY}@{PYTHON_COMMIT}"])
    return python


def install_wheel(temporary_directory: str, wheel: Path) -> Path:
    venv = Path(temporary_directory) / "binding-venv"
    run([sys.executable, "-m", "venv", str(venv)])
    python = venv / "bin" / "python"
    run([str(python), "-m", "pip", "install", "--quiet", str(wheel)])
    return python


def compare_fixture(fixture: Path, wheel: Path | None) -> None:
    if not fixture.is_file():
        raise SystemExit(f"Fixture file not found: {fixture}")
    if wheel is not None and not wheel.is_file():
        raise SystemExit(f"Wheel file not found: {wheel}")

    rust_binary = build_rust_runner()

    with tempfile.TemporaryDirectory(prefix="datafog-rust-poc-") as temporary_directory:
        python = install_baseline(temporary_directory)
        binding_python = install_wheel(temporary_directory, wheel) if wheel else None

        records = load_records(fixture)
        python_output = run_python(python, fixture)
        rust_output = run_rust(rust_binary, fixture)
        if len(python_output) != len(records) or len(rust_output) != len(records):
            raise RuntimeError(f"Scanner output count did not match {fixture.name}.")

        dataset = {
            "fixture": str(fixture),
            "sentences": len(records),
            "quality": {
                "python_baseline": quality(records, python_output),
                "rust_core": quality(records, rust_output),
            },
            "output_differences": {
                "python_baseline_vs_rust_core": compare_outputs(
                    records, python_output, rust_output, "python_baseline", "rust_core"
                ),
            },
            "performance": {
                "python_baseline": performance(python_output),
                "rust_core": performance(rust_output),
            },
        }
        if binding_python is not None:
            binding_output = run_python_binding(binding_python, fixture)
            if len(binding_output) != len(records):
                raise RuntimeError(f"Python binding output count did not match {fixture.name}.")
            dataset["quality"]["datafog_rs_python_binding"] = quality(records, binding_output)
            dataset["performance"]["datafog_rs_python_binding"] = performance(binding_output)
            dataset["output_differences"]["python_baseline_vs_datafog_rs_python_binding"] = compare_outputs(
                records, python_output, binding_output, "python_baseline", "datafog_rs_python_binding"
            )
        dataset["startup_time_ms_median"] = {
            "python_baseline": startup_time([str(python), "-c", "from datafog.engine import scan"]),
            "rust_core": startup_time([str(rust_binary)]),
        }
        dataset["peak_memory_bytes"] = {
            "python_baseline": peak_memory_bytes(
                [str(python), "-c", PYTHON_RUNNER, "0", "1"], fixture
            ),
            "rust_core": peak_memory_bytes([str(rust_binary), "--warmups", "0", "--runs", "1"], fixture),
        }
        if binding_python is not None:
            dataset["startup_time_ms_median"]["datafog_rs_python_binding"] = startup_time(
                [str(binding_python), "-c", "from datafog_rs import scan"]
            )
            dataset["peak_memory_bytes"]["datafog_rs_python_binding"] = peak_memory_bytes(
                [str(binding_python), "-c", BINDING_RUNNER, "0", "1"], fixture
            )

        report = {
            "kind": "comparison",
            "metadata": {
                **metadata(python),
                "datafog_rs_python_binding_wheel": wheel.name if wheel else None,
            },
            "datasets": {fixture.stem: dataset},
        }

    write_report("comparison", report)


def scale_fixtures(fixtures: list[Path], wheel: Path | None) -> None:
    if any(not fixture.is_file() for fixture in fixtures):
        missing = next(fixture for fixture in fixtures if not fixture.is_file())
        raise SystemExit(f"Fixture file not found: {missing}")
    if wheel is not None and not wheel.is_file():
        raise SystemExit(f"Wheel file not found: {wheel}")

    rust_binary = build_rust_runner()
    with tempfile.TemporaryDirectory(prefix="datafog-rust-poc-") as temporary_directory:
        python = install_baseline(temporary_directory)
        binding_python = install_wheel(temporary_directory, wheel) if wheel else None
        workloads = []

        for fixture in fixtures:
            records = load_records(fixture)
            python_output = run_python(python, fixture)
            rust_output = run_rust(rust_binary, fixture)
            if len(python_output) != len(records) or len(rust_output) != len(records):
                raise RuntimeError(f"Scanner output count did not match {fixture.name}.")

            workload = {
                "fixture": str(fixture),
                "sentences": len(records),
                "rust_core": batch_performance(rust_output),
                "python_baseline": batch_performance(python_output),
            }
            if binding_python is not None:
                binding_output = run_python_binding(binding_python, fixture)
                if len(binding_output) != len(records):
                    raise RuntimeError(f"Python binding output count did not match {fixture.name}.")
                workload["datafog_rs_python_binding"] = batch_performance(binding_output)
            workloads.append(workload)

        report = {
            "kind": "scaling",
            "metadata": {**metadata(python), "datafog_rs_python_binding_wheel": wheel.name if wheel else None},
            "workloads": sorted(workloads, key=lambda workload: workload["sentences"]),
        }

    write_report("scaling", report)


def main() -> None:
    if len(sys.argv) > 1 and sys.argv[1] == "scale":
        parser = argparse.ArgumentParser(
            description="Measure Rust and Python batch throughput for one or more JSONL fixtures."
        )
        parser.add_argument("mode")
        parser.add_argument("fixtures", nargs="+", type=Path, help="JSONL fixture paths to benchmark.")
        parser.add_argument("--wheel", type=Path, help="Path to a datafog-rs wheel to include.")
        arguments = parser.parse_args()
        scale_fixtures(
            [fixture.resolve() for fixture in arguments.fixtures],
            arguments.wheel.resolve() if arguments.wheel else None,
        )
        return

    parser = argparse.ArgumentParser(
        description="Compare the pinned Python baseline with the Rust core for one JSONL fixture."
    )
    parser.add_argument("fixture", type=Path, help="Path to the JSONL fixture to compare.")
    parser.add_argument("--wheel", type=Path, help="Path to a datafog-rs wheel to include.")
    arguments = parser.parse_args()
    compare_fixture(arguments.fixture.resolve(), arguments.wheel.resolve() if arguments.wheel else None)


if __name__ == "__main__":
    main()
