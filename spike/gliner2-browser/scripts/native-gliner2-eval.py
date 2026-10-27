#!/usr/bin/env python3
"""
Run native GLiNER2 inference on D4 fixtures for parity comparison.

Uses the same model, labels, and threshold as the browser benchmark.
"""
import json
import sys
import time
from pathlib import Path
from typing import Optional

FIXTURE_PATH = Path(__file__).parent.parent / "fixtures" / "d4-names-addresses.jsonl"
MODEL_ID = "fastino/gliner2-privacy-filter-PII-multi"
MODEL_REVISION = "1cb4166094dc58fa8d836429f060d6c95f62b495"

LABELS = ["name", "address", "email", "phone_num", "id_num", "url", "username"]
LABEL_MAPPING = {
    "name": "PERSON",
    "address": "STREET_ADDRESS",
    "phone_num": "PHONE",
}


def load_fixtures():
    fixtures = []
    with open(FIXTURE_PATH) as f:
        for line in f:
            if line.strip():
                fixtures.append(json.loads(line))
    return fixtures


def run_gliner2(texts: list[str], labels: list[str], threshold: float = 0.5):
    """Run GLiNER2 inference using the gliner2 Python library."""
    from gliner2 import GLiNER2
    
    print(f"Loading model {MODEL_ID}...", file=sys.stderr)
    start = time.time()
    model = GLiNER2.from_pretrained(MODEL_ID, map_location="cpu")
    load_time = time.time() - start
    print(f"Model loaded in {load_time:.1f}s", file=sys.stderr)
    
    results = []
    
    print(f"Running batch inference on {len(texts)} texts...", file=sys.stderr)
    start = time.time()
    batch_results = model.batch_extract_entities(
        texts, labels, 
        threshold=threshold, 
        include_spans=True, 
        include_confidence=True,
        batch_size=8
    )
    total_inference_time = time.time() - start
    
    for result in batch_results:
        spans = []
        entities_dict = result.get("entities", {})
        for label, entity_list in entities_dict.items():
            mapped_label = LABEL_MAPPING.get(label, label.upper())
            for entity in entity_list:
                spans.append({
                    "label": mapped_label,
                    "text": entity["text"],
                    "start": entity["start"],
                    "end": entity["end"],
                    "score": entity["confidence"],
                })
        results.append(spans)
    
    avg_inference = total_inference_time / len(texts) * 1000 if texts else 0
    print(f"Total: {total_inference_time:.1f}s, Avg: {avg_inference:.1f}ms per text", file=sys.stderr)
    
    return results, load_time, avg_inference


def calculate_metrics(predictions: list[list[dict]], fixtures: list[dict], threshold: float):
    """Calculate precision, recall, F1 for each entity type."""
    stats = {}
    missed_entities = []
    
    for i, (preds, fixture) in enumerate(zip(predictions, fixtures)):
        ground_truth = fixture.get("entities", [])
        
        pred_matched = [False] * len(preds)
        
        for gt in ground_truth:
            gt_label = LABEL_MAPPING.get(gt["label"], gt["label"].upper())
            gt_start = gt["start"]
            gt_end = gt["end"]
            
            if gt_label not in stats:
                stats[gt_label] = {"tp": 0, "fp": 0, "fn": 0}
            
            matched = False
            for j, pred in enumerate(preds):
                if (pred["label"] == gt_label and 
                    pred["start"] == gt_start and 
                    pred["end"] == gt_end and
                    not pred_matched[j]):
                    stats[gt_label]["tp"] += 1
                    pred_matched[j] = True
                    matched = True
                    break
            
            if not matched:
                stats[gt_label]["fn"] += 1
                missed_entities.append({
                    "fixture_id": fixture.get("id", i),
                    "label": gt_label,
                    "text": gt.get("text", fixture["text"][gt_start:gt_end]),
                    "start": gt_start,
                    "end": gt_end,
                })
        
        for j, pred in enumerate(preds):
            if not pred_matched[j]:
                label = pred["label"]
                if label not in stats:
                    stats[label] = {"tp": 0, "fp": 0, "fn": 0}
                stats[label]["fp"] += 1
    
    for label, s in stats.items():
        tp, fp, fn = s["tp"], s["fp"], s["fn"]
        s["precision"] = tp / (tp + fp) if tp + fp > 0 else 0
        s["recall"] = tp / (tp + fn) if tp + fn > 0 else 0
        s["f1"] = 2 * s["precision"] * s["recall"] / (s["precision"] + s["recall"]) if s["precision"] + s["recall"] > 0 else 0
    
    return stats, missed_entities


def main():
    import argparse
    parser = argparse.ArgumentParser()
    parser.add_argument("--threshold", type=float, default=0.5)
    parser.add_argument("--thresholds", type=str, help="Comma-separated thresholds for sweep")
    parser.add_argument("--json", action="store_true")
    parser.add_argument("--output", type=str, help="Output file for results")
    args = parser.parse_args()
    
    fixtures = load_fixtures()
    texts = [f["text"] for f in fixtures]
    print(f"Loaded {len(fixtures)} fixtures", file=sys.stderr)
    
    thresholds = [args.threshold]
    if args.thresholds:
        thresholds = [float(t) for t in args.thresholds.split(",")]
    
    all_results = {}
    
    for threshold in thresholds:
        print(f"\n{'='*60}", file=sys.stderr)
        print(f"Running with threshold={threshold}", file=sys.stderr)
        print(f"{'='*60}", file=sys.stderr)
        
        predictions, load_time, avg_inference = run_gliner2(texts, LABELS, threshold)
        metrics, missed = calculate_metrics(predictions, fixtures, threshold)
        
        result = {
            "threshold": threshold,
            "model_id": MODEL_ID,
            "model_revision": MODEL_REVISION,
            "load_time_s": load_time,
            "avg_inference_ms": avg_inference,
            "metrics": metrics,
            "missed_entities": missed if threshold == 0.5 else [],
        }
        all_results[str(threshold)] = result
        
        if not args.json:
            print(f"\nResults at threshold={threshold}:", file=sys.stderr)
            for label, m in sorted(metrics.items()):
                print(f"  {label}: P={m['precision']:.4f} R={m['recall']:.4f} F1={m['f1']:.4f} (TP={m['tp']} FP={m['fp']} FN={m['fn']})", file=sys.stderr)
            
            if threshold == 0.5 and "PERSON" in metrics:
                person_fn = metrics["PERSON"]["fn"]
                print(f"\nMissed PERSON entities ({person_fn} total):", file=sys.stderr)
                for m in missed:
                    if m["label"] == "PERSON":
                        print(f"  [{m['fixture_id']}] \"{m['text']}\" @ {m['start']}-{m['end']}", file=sys.stderr)
    
    output = {
        "runtime": "native_gliner2_python",
        "model_id": MODEL_ID,
        "model_revision": MODEL_REVISION,
        "labels": LABELS,
        "label_mapping": LABEL_MAPPING,
        "fixture_count": len(fixtures),
        "results_by_threshold": all_results,
    }
    
    if args.output:
        with open(args.output, "w") as f:
            json.dump(output, f, indent=2)
        print(f"\nResults written to {args.output}", file=sys.stderr)
    
    if args.json:
        print(json.dumps(output, indent=2))


if __name__ == "__main__":
    main()
