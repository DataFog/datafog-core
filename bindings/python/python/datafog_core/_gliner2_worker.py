"""Private local transport around the official pinned GLiNER2 span runtime.

Core owns label mapping, threshold filtering, validation, composition and policy.
The official runtime owns tokenization, span decoding and overlapping word windows.
Never write input, native exceptions or local paths to the response protocol.
"""
from contextlib import redirect_stdout
from importlib.metadata import version
import json
import os
import platform
from pathlib import Path
import sys

__version__ = "0.4.5b4"
RUNTIME = {"gliner2": "2.1.0", "transformers": "5.8.0", "torch": "2.12.0"}
if sys.platform == "darwin" and platform.machine() == "x86_64":
    RUNTIME.update(transformers="4.57.6", torch="2.2.2")


def respond(value):
    print(json.dumps(value, ensure_ascii=True, allow_nan=False), flush=True)


def _patch_count_pred(model, torch):
    """Wrap the count_pred module to floor a zero count prediction at 1.

    GLiNER2's span runtime (`inference/runtime.py`) uses `count_pred` as a
    structural-instance budget, not a span cap:

    * `_compute_direct_span_logits_batched` argmaxes the count head and, when
      the class is 0, skips `count_embed` and span scoring for that task.
    * For entity tasks the runtime then keeps only structural instance 0
      (`struct_proj = struct_proj[:1]`) and `_find_spans` returns every span
      above threshold on that instance. A predicted count of 2+ does not
      add extra entity spans; a predicted count of 0 drops all of them.
    * Relation and JSON-structure decoders *do* iterate `range(pred_count)`.
      This worker only builds `schema.entities(...)`, so flooring does not
      change record or relation instance counts.

    Flooring a 0→1 prediction therefore unblocks entity span decoding without
    dropping a second same-type entity. Upstream 2.1.0 has no extract-time
    option to disable this gate (the boundary/record path that replaces
    `count_pred` is a different architecture and is not this checkpoint).
    `count_pred` is a `torch.nn.Module`; it must stay one so
    `Module.__setattr__` accepts the wrap.
    """
    if os.environ.get("DATAFOG_GLINER2_DISABLE_COUNT_FLOOR") == "1":
        return

    class FlooredCountPred(torch.nn.Module):
        def __init__(self, inner):
            super().__init__()
            self.inner = inner

        def forward(self, x):
            logits = self.inner(x)
            if logits.shape[-1] < 2:
                return logits
            floor_boost = torch.zeros_like(logits)
            floor_boost[..., 1] = 100.0
            zero_pred = logits.argmax(dim=-1) == 0
            return torch.where(
                zero_pred.unsqueeze(-1),
                logits + floor_boost,
                logits,
            )

    model.count_pred = FlooredCountPred(model.count_pred)


def main():
    # Private JSONL is UTF-8 on every host, independent of Windows' console locale.
    sys.stdin.reconfigure(encoding="utf-8")
    sys.stdout.reconfigure(encoding="utf-8")
    loaded = False
    try:
        actual = {name: version(name).split("+")[0] for name in RUNTIME}
        if actual != RUNTIME:
            respond({"error": "runtime_incompatible"})
            return 1
        bundle_name = sys.argv[1]
        # Rust canonicalization uses Windows extended paths. Some upstream
        # tokenizer/config loaders interpret that spelling as a Hub identifier.
        if sys.platform == "win32":
            if bundle_name.startswith("\\\\?\\UNC\\"):
                bundle_name = "\\\\" + bundle_name[8:]
            elif bundle_name.startswith("\\\\?\\"):
                bundle_name = bundle_name[4:]
        bundle = Path(bundle_name)
        settings = json.loads(sys.argv[2])
        # Offline loading of pinned built-in DeBERTa config needs no repository code.
        with redirect_stdout(sys.stderr):
            import torch
            from gliner2 import GLiNER2
            torch.set_num_threads(settings["intra_threads"])
            torch.set_num_interop_threads(1)
            model = GLiNER2.from_pretrained(str(bundle), local_files_only=True, map_location="cpu")
            model.eval()
            # Issue #87: Floor the count head at 1 to prevent skipping span
            # decoding when it predicts zero entities.
            _patch_count_pred(model, torch)
            # Keep the official strict mode explicit: never accept per-sample failures as empty findings.
            model.strict_extraction = True
            # The pinned processor otherwise appends punctuation, creating spans
            # outside the original leaf (e.g. "John." for input "John").
            # Disable that preprocessing hook; never trim/reconstruct returned spans.
            model.processor._normalize_text = lambda text: text
            labels = {item["label"]: {"description": item["description"]}
                      if item.get("description") else {} for item in settings["labels"]}
            schema = model.create_schema().entities(labels)
        loaded = True
        for line in sys.stdin:
            request = json.loads(line)
            text = request["text"]
            with redirect_stdout(sys.stderr), torch.inference_mode():
                output = model.extract_long(
                    text, schema, threshold=0.0,
                    chunk_size=settings["chunk_size"], chunk_overlap=settings["chunk_overlap"],
                    batch_size=1, num_workers=0, include_confidence=True, include_spans=True,
                    overlap_policy="allow",
                ) if text.strip() else {"entities": {}}
            findings = []
            for label, spans in output["entities"].items():
                for span in spans:
                    findings.append({"label": label, "start": span["start"], "end": span["end"],
                                     "text": span["text"], "raw_score": span["confidence"]})
            respond({"findings": findings, "runtime": actual, "worker_version": __version__})
        return 0
    except MemoryError:
        respond({"error": "resource_exhausted"})
    except Exception as error:
        # Only a bounded category is sent; never include exception text or tracebacks.
        category = type(error).__name__
        if category not in {"OSError", "ValueError", "ImportError", "RuntimeError", "AttributeError"}:
            category = "BackendError"
        respond({"error": "inference_failed" if loaded else "load_failed", "category": category})
    return 1


if __name__ == "__main__":
    raise SystemExit(main())
