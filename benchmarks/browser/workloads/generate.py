#!/usr/bin/env python3
"""Generate reproducible, synthetic DataFog Workspace benchmark fixtures."""

from __future__ import annotations

import argparse
import hashlib
import json
import sys
from pathlib import Path
from typing import Any

TARGETS = (10, 100, 1_000, 10_000)
MODES = ("storage_workspace", "complete_extension")
HISTORIES = ("fresh", "long_history")
REUSE = ("repeated_existing", "mostly_new")
SHAPES = ("chat", "statement", "batch", "unicode", "table")
GATES = (
    "exact_restore_after_browser_restart",
    "no_handoff_before_confirmed_save_and_activation",
    "wrong_credentials_rejected",
    "conflicting_save_rejected_without_overwrite",
    "lost_save_reply_recovers_without_duplicate_activation",
    "lock_during_save_prevents_handoff",
    "stale_review_rejected",
    "conversation_scope_enforced",
    "backup_recovery_preserves_mappings_and_authority",
)


def canonical_json(value: Any) -> bytes:
    return json.dumps(value, ensure_ascii=False, sort_keys=True, separators=(",", ":")).encode("utf-8")


def _synthetic_value(index: int) -> str:
    return f"Synthetic Person {index:06d}"


def _render(shape: str, row: int, value: str) -> tuple[str, str]:
    if shape == "chat":
        return f"Message {row:05d}: please verify {value} before continuing.", value
    if shape == "statement":
        return f"SYNTHETIC STATEMENT {row:05d}\nAccount holder: {value}\nStatus: sample only", value
    if shape == "unicode":
        return f"🧪 sample {row:05d} — 名字: {value} · résumé test · 끝", value
    if shape == "table":
        return f"|{row:05d}|{value}|sample|", value
    raise ValueError(f"shape {shape!r} does not render as a single-input text")


def _split_counts(count: int, parts: int) -> list[int]:
    base, extra = divmod(count, parts)
    return [base + (index < extra) for index in range(parts)]


def _inputs(shape: str, count: int, values: list[str], prefix: str) -> list[dict[str, Any]]:
    parts = min(32, max(1, count)) if shape == "batch" else 1
    counts = _split_counts(count, parts)
    result: list[dict[str, Any]] = []
    cursor = 0
    for part, part_count in enumerate(counts):
        lines: list[str] = []
        findings: list[dict[str, Any]] = []
        byte_cursor = 0
        for row in range(part_count):
            value = values[cursor]
            cursor += 1
            if shape == "batch":
                text = f"Batch {part:02d} row {row:04d}: {value}"
                matched = value
            else:
                text, matched = _render(shape, cursor - 1, value)
            start_char = text.index(matched)
            start_byte = len(text[:start_char].encode("utf-8"))
            end_byte = start_byte + len(matched.encode("utf-8"))
            if lines:
                byte_cursor += 1  # newline joining this row to the previous row
            start_byte += byte_cursor
            end_byte += byte_cursor
            findings.append({
                "entityType": "PERSON",
                "matchedText": matched,
                "startByte": start_byte,
                "endByte": end_byte,
            })
            lines.append(text)
            byte_cursor += len(text.encode("utf-8"))
        result.append({"id": f"{prefix}-{part:02d}", "text": "\n".join(lines), "findings": findings})
    return result


def make_scenario(mode: str, seed: int, target: int, history: str, reuse: str, shape: str) -> dict[str, Any]:
    if mode not in MODES or target not in TARGETS or history not in HISTORIES or reuse not in REUSE or shape not in SHAPES:
        raise ValueError("unsupported scenario dimension")
    scenario_id = f"{mode}-m{target}-{history}-{reuse}-{shape}-s{seed}"
    scenario = {
        "scenarioVersion": 1,
        "scenarioId": scenario_id,
        "mode": mode,
        "seed": seed,
        "mappingTarget": target,
        "historyProfile": history,
        "valueReuse": reuse,
        "inputShape": shape,
        "fixtureRef": f"synthetic://browser-pseudonymization/v1/{scenario_id}",
    }

    seed_values = [_synthetic_value(i + 1) for i in range(target)] if reuse == "repeated_existing" else []
    trial_count = target
    if reuse == "repeated_existing":
        pool = seed_values[: min(32, target)]
        trial_values = [pool[(i + seed) % len(pool)] for i in range(trial_count)]
    else:
        trial_values = [_synthetic_value(target + i + 1) for i in range(trial_count)]

    seed_inputs = _inputs(shape, target, seed_values, "seed") if seed_values else []
    trial_inputs = _inputs(shape, trial_count, trial_values, "trial")
    # Core limits include aggregate UTF-8 source bytes, findings, and the 32-member batch cap.
    seed_source_bytes = sum(len(item["text"].encode("utf-8")) for item in seed_inputs)
    trial_source_bytes = sum(len(item["text"].encode("utf-8")) for item in trial_inputs)
    if len(seed_inputs) + len(trial_inputs) > 64:
        raise AssertionError("each seed and trial batch must independently use at most 32 inputs")
    if trial_count > 10_000 or any(len(item["findings"]) > 10_000 for item in seed_inputs + trial_inputs):
        raise AssertionError("fixture exceeds Core finding limits")
    if seed_source_bytes > 1_048_576 or trial_source_bytes > 1_048_576:
        raise AssertionError("fixture seed or trial source exceeds Core's per-prepare byte budget")

    expected = {
        "initialMappingCount": target if reuse == "repeated_existing" else 0,
        "finalMappingCount": target,
        "trialFindingCount": trial_count,
        "seedFindingCount": target if seed_inputs else 0,
        "seedDistinctValues": len(set(seed_values)),
        "trialDistinctValues": len(set(trial_values)),
        "seedInputCount": len(seed_inputs),
        "trialInputCount": len(trial_inputs),
        "seedSourceBytes": seed_source_bytes,
        "trialSourceBytes": trial_source_bytes,
        "restoreTexts": [{"inputId": item["id"], "text": item["text"]} for item in trial_inputs],
        "history": {
            "operations": 0 if history == "fresh" else 1_000,
            "conversations": 0 if history == "fresh" else 100,
            "messagesPerConversation": 0 if history == "fresh" else 10,
            "recipe": None if history == "fresh" else {
                "operationIds": "For every step, allocate a unique 16-byte ID with crypto.getRandomValues; never reuse an ID within this Workspace.",
                "timestamps": {"baseMs": 1_704_067_200_000, "stepMs": 1, "source": "synthetic, distinct, deterministic, valid Core timestamps; no wall-clock or provider time"},
                "operationOrder": "Number committed history events from 1. User-cancelled outcome i (1..1000) uses baseMs+i. For conversation j (0..999), prepare uses baseMs+1001+2*j and observed-send resolution uses baseMs+1002+2*j. Generate unique CSPRNG operation IDs in this same order.",
                "operationOutcomes": {
                    "count": 1_000,
                    "kind": "user_cancelled",
                    "issueCount": 0,
                    "action": "Core prepareOutcome({kind:'user_cancelled', issueCount:0}, operation)",
                    "persistence": "Export candidate bundle, persist it through the selected adapter, wait for confirmed save, then activateSaved before the next event.",
                },
                "conversations": 100,
                "messagesPerConversation": 10,
                "conversationRecipe": {
                    "conversationCount": 100,
                    "messagesPerConversation": 10,
                    "surface": "synthetic-browser-benchmark-v1",
                    "siteIdByteBudget": 64,
                    "messageText": "Synthetic history message <conversation-index>-<message-index>; no findings.",
                    "prepare": "prepareConversationSend one no-finding input with the stable logical ID and the original context {surface, siteId:null, siteIdByteBudget:64}; persist confirmed candidate and activate; assert intent status pending.",
                    "observe": "prepareObservedSend for that intent and logical ID with site ID synthetic-conversation-<zero-padded-index>; persist confirmed candidate and activate; assert intent status observed_sent.",
                    "providerCalls": 0,
                    "handoff": "Do not release or send the synthetic text; observed-send records are local synthetic history only.",
                },
                "expectedBeforeTrial": {
                    "operations": 3_001 + (1 if seed_inputs else 0),
                    "pendingIntents": 0,
                    "conversationContexts": 100,
                    "confirmedSends": 1_000,
                    "activeMappings": target if reuse == "repeated_existing" else 0,
                },
                "maxOrdinaryOperationRows": 99_744,
                "maxTotalOperationRows": 100_000,
                "bundlePersistence": "Every population step persists the complete encrypted Workspace bundle; do not use incremental writes.",
            },
        },
    }
    assertions = {
        "requiredGates": list(GATES),
        "admission": {
            "coreMaxSourceBytesPerPrepare": 1_048_576,
            "coreMaxFindingsPerPrepare": 10_000,
            "coreMaxBatchInputs": 32,
            "recordResultEvenWhenRejected": True,
        },
    }
    payload = {
        "scenario": scenario,
        "seedInputs": seed_inputs,
        "inputs": trial_inputs,
        "expected": expected,
        "assertions": assertions,
    }
    payload["fixtureSha256"] = hashlib.sha256(canonical_json(payload)).hexdigest()
    return payload


def catalog(seed: int = 1) -> dict[str, Any]:
    scenarios = []
    for mode in MODES:
        for target in TARGETS:
            for history in HISTORIES:
                for reuse in REUSE:
                    for shape in SHAPES:
                        scenario_id = f"{mode}-m{target}-{history}-{reuse}-{shape}-s{seed}"
                        scenarios.append({
                            "scenarioVersion": 1,
                            "scenarioId": scenario_id,
                            "mode": mode,
                            "seed": seed,
                            "mappingTarget": target,
                            "historyProfile": history,
                            "valueReuse": reuse,
                            "inputShape": shape,
                            "fixtureRef": f"synthetic://browser-pseudonymization/v1/{scenario_id}",
                        })
    return {"scenarioVersion": 1, "scenarioCount": len(scenarios), "scenarios": scenarios}


def _parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--catalog", action="store_true", help="emit the full scenario matrix")
    parser.add_argument("--mode", choices=MODES)
    parser.add_argument("--seed", type=int, default=1)
    parser.add_argument("--target", type=int, choices=TARGETS)
    parser.add_argument("--history", choices=HISTORIES)
    parser.add_argument("--reuse", choices=REUSE)
    parser.add_argument("--shape", choices=SHAPES)
    parser.add_argument("--output", type=Path, help="write JSON to this path instead of stdout")
    return parser


def main(argv: list[str] | None = None) -> int:
    args = _parser().parse_args(argv)
    if args.catalog:
        output = catalog(args.seed)
    elif all((args.mode, args.target, args.history, args.reuse, args.shape)):
        output = make_scenario(args.mode, args.seed, args.target, args.history, args.reuse, args.shape)
    else:
        _parser().error("provide --catalog or all of --mode, --target, --history, --reuse, and --shape")
    encoded = json.dumps(output, ensure_ascii=False, indent=2) + "\n"
    if args.output:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(encoded, encoding="utf-8")
    else:
        sys.stdout.write(encoded)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
