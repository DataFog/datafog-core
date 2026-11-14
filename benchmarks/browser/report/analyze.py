#!/usr/bin/env python3
"""Summarize immutable browser benchmark JSONL receipts without hiding failures."""

from __future__ import annotations

import argparse
import csv
from html import escape
import json
import math
import sys
from collections import defaultdict
from pathlib import Path
from typing import Any

GATE_STATUSES = {"pass", "fail", "unsupported"}
REQUIRED_GATE_NAMES = {
    "exact_restore_after_browser_restart",
    "no_handoff_before_confirmed_save_and_activation",
    "wrong_credentials_rejected",
    "conflicting_save_rejected_without_overwrite",
    "lost_save_reply_recovers_without_duplicate_activation",
    "lock_during_save_prevents_handoff",
    "stale_review_rejected",
    "conversation_scope_enforced",
    "recovery_preserves_mappings_and_authority",
}
ADMISSION_STATUSES = {"admitted", "rejected"}
# Predeclared reporting floor: n=100 yields five observations in the upper 5% tail.
MIN_INDEPENDENT_TRIALS_FOR_PERCENTILES = 100


class ReceiptError(ValueError):
    pass


def _require(condition: bool, message: str) -> None:
    if not condition:
        raise ReceiptError(message)


def validate_independent_profiles(receipts: list[dict[str, Any]]) -> None:
    run_ids = {receipt["runId"] for receipt in receipts}
    _require(len(run_ids) == 1, "analysis inputs contain multiple runIds; analyze immutable runs separately")
    seen_trials: set[tuple[str, str, str]] = set()
    seen_profiles: set[tuple[str, str, str]] = set()
    for receipt in receipts:
        identity = (receipt["scenarioId"], receipt["backend"], receipt["trialId"])
        _require(identity not in seen_trials, f"duplicate trial identity {identity}")
        seen_trials.add(identity)
        profile = (receipt["scenarioId"], receipt["backend"], receipt["profileId"])
        _require(profile not in seen_profiles,
                 "profileId reused across trials; correlated operations cannot count as independent samples")
        seen_profiles.add(profile)


def load_receipts(paths: list[Path], run_kind: str) -> list[dict[str, Any]]:
    receipts: list[dict[str, Any]] = []
    for path in paths:
        with path.open("r", encoding="utf-8") as stream:
            for line_number, raw in enumerate(stream, 1):
                if not raw.strip():
                    continue
                try:
                    receipt = json.loads(raw)
                except json.JSONDecodeError as exc:
                    raise ReceiptError(f"{path}:{line_number}: invalid JSON: {exc}") from exc
                _require(isinstance(receipt, dict), f"{path}:{line_number}: receipt must be an object")
                required = ("runKind", "runId", "scenarioId", "backend", "trialId", "profileId", "admission", "correctness", "timings")
                missing = [key for key in required if key not in receipt]
                _require(not missing, f"{path}:{line_number}: missing fields {', '.join(missing)}")
                if receipt["runKind"] != run_kind:
                    continue
                for field in ("runId", "scenarioId", "backend", "trialId", "profileId"):
                    _require(isinstance(receipt[field], str) and receipt[field], f"{path}:{line_number}: {field} must be a nonempty string")
                admission = receipt["admission"]
                _require(isinstance(admission, dict) and admission.get("status") in ADMISSION_STATUSES,
                         f"{path}:{line_number}: admission.status must be admitted or rejected")
                correctness = receipt["correctness"]
                _require(isinstance(correctness, dict) and isinstance(correctness.get("gates"), dict),
                         f"{path}:{line_number}: correctness.gates must be an object")
                _require(bool(correctness["gates"]), f"{path}:{line_number}: correctness.gates cannot be empty")
                missing_gates = REQUIRED_GATE_NAMES - set(correctness["gates"])
                _require(not missing_gates,
                         f"{path}:{line_number}: missing required correctness gates: {', '.join(sorted(missing_gates))}")
                for gate, result in correctness["gates"].items():
                    _require(isinstance(result, dict) and result.get("status") in GATE_STATUSES,
                             f"{path}:{line_number}: correctness.gates.{gate}.status is invalid")
                timings = receipt["timings"]
                _require(isinstance(timings, dict), f"{path}:{line_number}: timings must be an object")
                for interval, measurement in timings.items():
                    _require(isinstance(measurement, dict), f"{path}:{line_number}: timings.{interval} must be an object")
                    status = measurement.get("status", "measured" if "durationMs" in measurement else "unsupported")
                    _require(status in {"measured", "unsupported", "failed"}, f"{path}:{line_number}: timings.{interval}.status is invalid")
                    if status == "measured":
                        duration = measurement.get("durationMs")
                        _require(isinstance(duration, (int, float)) and not isinstance(duration, bool) and math.isfinite(duration) and duration >= 0,
                                 f"{path}:{line_number}: timings.{interval}.durationMs must be a finite nonnegative number")
                receipt["_source"] = f"{path}:{line_number}"
                receipts.append(receipt)
    _require(bool(receipts), f"no {run_kind!r} receipts found")
    try:
        validate_independent_profiles(receipts)
    except ReceiptError as exc:
        raise ReceiptError(f"{exc}") from exc
    return receipts


def _quantile(values: list[float], probability: float) -> float | None:
    if not values:
        return None
    ordered = sorted(values)
    position = (len(ordered) - 1) * probability
    lower = math.floor(position)
    upper = math.ceil(position)
    if lower == upper:
        return ordered[lower]
    return ordered[lower] + (ordered[upper] - ordered[lower]) * (position - lower)


def summarize(receipts: list[dict[str, Any]]) -> list[dict[str, Any]]:
    groups: dict[tuple[str, str], list[dict[str, Any]]] = defaultdict(list)
    for receipt in receipts:
        groups[(receipt["scenarioId"], receipt["backend"])].append(receipt)
    output: list[dict[str, Any]] = []
    for (scenario_id, backend), trials in sorted(groups.items()):
        failed = []
        unsupported_trials = []
        excluded = []
        eligible = []
        for trial in trials:
            gate_states = [gate["status"] for gate in trial["correctness"]["gates"].values()]
            if trial["admission"]["status"] != "admitted" or "fail" in gate_states:
                failed.append(trial)
            elif "unsupported" in gate_states:
                unsupported_trials.append(trial)
            elif trial.get("exclusionReason"):
                excluded.append(trial)
            else:
                eligible.append(trial)
        intervals = sorted({name for trial in trials for name in trial["timings"]})
        interval_rows = []
        for interval in intervals:
            values = []
            unsupported = 0
            failed_measurements = 0
            for trial in eligible:
                measurement = trial["timings"].get(interval)
                if not measurement:
                    unsupported += 1
                    continue
                status = measurement.get("status", "measured" if "durationMs" in measurement else "unsupported")
                if status == "measured":
                    values.append(float(measurement["durationMs"]))
                elif status == "failed":
                    failed_measurements += 1
                else:
                    unsupported += 1
            enough_trials = len(values) >= MIN_INDEPENDENT_TRIALS_FOR_PERCENTILES
            interval_rows.append({
                "name": interval,
                "n": len(values),
                "p50Ms": _quantile(values, 0.50) if enough_trials else None,
                "p95Ms": _quantile(values, 0.95) if enough_trials else None,
                "percentileStatus": "reported" if enough_trials else "exploratory_insufficient_independent_trials",
                "unsupportedCount": unsupported,
                "measurementFailureCount": failed_measurements,
            })
        output.append({
            "scenarioId": scenario_id,
            "backend": backend,
            "trialCount": len(trials),
            "independentProfileCount": len({trial["profileId"] for trial in trials}),
            "failedTrialCount": len(failed),
            "failureRate": len(failed) / len(trials),
            "unsupportedTrialCount": len(unsupported_trials),
            "unsupportedRate": len(unsupported_trials) / len(trials),
            "excludedTrialCount": len(excluded),
            "excludedRate": len(excluded) / len(trials),
            "eligibleTrialCount": len(eligible),
            "exclusions": [
                {"trialId": trial["trialId"], "admission": trial["admission"],
                 "gates": {name: result for name, result in trial["correctness"]["gates"].items() if result["status"] != "pass"},
                 "reason": trial.get("exclusionReason"),
                 "classification": "failed" if trial in failed else "unsupported" if trial in unsupported_trials else "excluded"}
                for trial in failed + unsupported_trials + excluded
            ],
            "intervals": interval_rows,
        })
    return output


def markdown_report(run_kind: str, summary: list[dict[str, Any]]) -> str:
    lines = [
        f"# Browser reversible pseudonymization benchmark — {run_kind}",
        "",
        f"This report is generated only from the selected immutable JSONL receipts. Pilot and final receipts are analyzed separately. Each row represents an independent profile trial; repeated operations within a trial do not add samples. Percentiles use linear interpolation over sorted independent trial durations. They are reported only at {MIN_INDEPENDENT_TRIALS_FOR_PERCENTILES} or more independent eligible profile trials per scenario/backend/interval, which provides at least five observations in the upper 5% tail. Below that floor, percentiles are blank and marked exploratory; no single observation is presented as a percentile. A blank percentile can also mean no correctness-eligible measured duration was available.",
        "",
        "## Trial summary",
        "",
        "| Scenario | Backend | Independent trials | Failures | Failure rate | Unsupported | Explicit exclusions | Eligible |",
        "|---|---|---:|---:|---:|---:|---:|---:|",
    ]
    for row in summary:
        lines.append(f"| {row['scenarioId']} | {row['backend']} | {row['independentProfileCount']} | {row['failedTrialCount']} | {row['failureRate']:.1%} | {row['unsupportedTrialCount']} ({row['unsupportedRate']:.1%}) | {row['excludedTrialCount']} ({row['excludedRate']:.1%}) | {row['eligibleTrialCount']} |")
    lines += ["", "## Timing summary", "", "| Scenario | Backend | Interval | n | Percentile status | p50 (ms) | p95 (ms) | Unsupported | Measurement failures |", "|---|---|---|---:|---|---:|---:|---:|---:|"]
    for row in summary:
        for interval in row["intervals"]:
            p50 = "" if interval["p50Ms"] is None else f"{interval['p50Ms']:.3f}"
            p95 = "" if interval["p95Ms"] is None else f"{interval['p95Ms']:.3f}"
            lines.append(f"| {row['scenarioId']} | {row['backend']} | {interval['name']} | {interval['n']} | {interval['percentileStatus']} | {p50} | {p95} | {interval['unsupportedCount']} | {interval['measurementFailureCount']} |")
    lines += ["", "## Failures, unsupported trials, and exclusions", "", "Admission failures and failed correctness gates are retained in the failure rate. Unsupported correctness gates and explicit exclusions are counted separately. All three classes are listed below and omitted from performance percentiles.", ""]
    excluded = [(row["scenarioId"], row["backend"], item) for row in summary for item in row["exclusions"]]
    if excluded:
        lines += ["| Scenario | Backend | Trial | Classification | Admission | Failed/unsupported gates | Reason |", "|---|---|---|---|---|---|---|"]
        for scenario, backend, item in excluded:
            gates = ", ".join(f"{name}:{result['status']}" for name, result in item["gates"].items()) or "none"
            reason = (item["reason"] or item["admission"].get("reason") or "")
            lines.append(f"| {scenario} | {backend} | {item['trialId']} | {item['classification']} | {item['admission']['status']} | {gates} | {reason} |")
    else:
        lines.append("No excluded trials.")
    lines += ["", "## Context to complete before interpretation", "", "- Frozen harness commit and repository commit:", "- Package/artifact identities and SHA-256 digests:", "- Browser version/channel and OS:", "- Hardware and power conditions:", "- Backend security settings:", "- Measurement limitations and instrumentation overhead:", "- Unavailable device/browser coverage:", "- Workload-specific recommendations supported by these receipts:", "- Demonstrated follow-up issues:", ""]
    lines[lines.index("## Context to complete before interpretation")] = "## Context and coverage"
    context_index = lines.index("## Context and coverage")
    lines[context_index + 2:context_index + 3] = [
        "- Browser engine: installed retail Chrome only, on a single Apple M5 Pro MacBook Pro running macOS 26.6.2; no Windows, Linux, mobile, Firefox, or Edge coverage.",
        "- Workloads included in this run are listed in the trial summary. Percentiles are reported only at the independent-trial threshold; smaller groups are exploratory.",
        "- FogVault authenticated bundle migration recovery is distinct from encrypted backup archive export/import, which is unsupported by the published adapter.",
        "- Ciphertext storage bytes, renderer/utility process RSS, reliable bytes-written counts, and instrumentation overhead calibration are unavailable.",
        "- No performance recommendation is made unless both backends have sufficient eligible independent trials for the same scenario and interval.",
    ]
    return "\n".join(lines)


def write_chart(path: Path, summary: list[dict[str, Any]], interval_name: str) -> None:
    rows = []
    for group in summary:
        interval = next((item for item in group["intervals"] if item["name"] == interval_name), None)
        if interval and interval["p50Ms"] is not None:
            rows.append((group["scenarioId"], group["backend"], interval["p50Ms"]))
    if not rows:
        path.write_text(f'<svg xmlns="http://www.w3.org/2000/svg" width="900" height="100"><text x="12" y="30">No eligible measured values for {interval_name}</text></svg>\n', encoding="utf-8")
        return
    left, right, top, row_height = 280, 860, 30, 24
    max_value = max(value for _, _, value in rows) or 1.0
    height = top + row_height * len(rows) + 24
    svg = [f'<svg xmlns="http://www.w3.org/2000/svg" width="900" height="{height}" viewBox="0 0 900 {height}">',
           '<style>text{font:12px sans-serif}.bar{fill:#4169a1}</style>',
           f'<text x="12" y="20">p50 latency (ms): {escape(interval_name)}</text>']
    for index, (scenario, backend, value) in enumerate(rows):
        y = top + index * row_height
        width = (right - left) * value / max_value
        label = escape(f"{scenario} / {backend}")
        svg.append(f'<text x="8" y="{y + 14}">{label}</text><rect class="bar" x="{left}" y="{y}" width="{width:.2f}" height="16"/><text x="{left + width + 5:.2f}" y="{y + 13}">{value:.3f}</text>')
    svg.append("</svg>")
    path.write_text("\n".join(svg) + "\n", encoding="utf-8")


def write_summary_charts(output_dir: Path, summary: list[dict[str, Any]]) -> None:
    intervals = sorted({interval["name"] for group in summary for interval in group["intervals"]})
    chart_dir = output_dir / "charts"
    chart_dir.mkdir(parents=True, exist_ok=True)
    for interval in intervals:
        safe_name = "".join(char if char.isalnum() or char in "-_" else "_" for char in interval)
        write_chart(chart_dir / f"{safe_name}.svg", summary, interval)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--run-kind", choices=("pilot", "final"), required=True)
    parser.add_argument("--output-dir", type=Path, required=True)
    parser.add_argument("--chart-interval", default="approvalToSavedActivationHandoff")
    parser.add_argument("receipts", nargs="+", type=Path)
    args = parser.parse_args(argv)
    try:
        receipts = load_receipts(args.receipts, args.run_kind)
    except (ReceiptError, OSError) as exc:
        parser.error(str(exc))
    summary = summarize(receipts)
    args.output_dir.mkdir(parents=True, exist_ok=True)
    (args.output_dir / "summary.json").write_text(json.dumps({"runKind": args.run_kind, "trialCount": len(receipts), "groups": summary}, indent=2) + "\n", encoding="utf-8")
    (args.output_dir / "report.md").write_text(markdown_report(args.run_kind, summary), encoding="utf-8")
    write_chart(args.output_dir / "latency.svg", summary, args.chart_interval)
    write_summary_charts(args.output_dir, summary)
    with (args.output_dir / "timings.csv").open("w", encoding="utf-8", newline="") as stream:
        writer = csv.writer(stream)
        writer.writerow(("scenarioId", "backend", "interval", "independentTrialCount", "percentileStatus", "p50Ms", "p95Ms", "unsupportedMeasurementCount", "measurementFailureCount", "failureRate", "unsupportedTrialRate", "excludedRate"))
        for group in summary:
            for interval in group["intervals"]:
                writer.writerow((group["scenarioId"], group["backend"], interval["name"], interval["n"], interval["percentileStatus"], interval["p50Ms"], interval["p95Ms"], interval["unsupportedCount"], interval["measurementFailureCount"], group["failureRate"], group["unsupportedRate"], group["excludedRate"]))
    print(f"Analyzed {len(receipts)} independent {args.run_kind} trials into {args.output_dir}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
