// Boundary candidate decoding adapted from Pastel-Cloud OÜ's MIT-licensed
// gliner2.5-onnx-webgpu (9f8173223d84bb65e6952d070137a990f93ef298).
// The full notice is in THIRD_PARTY_NOTICES.md.

function sigmoid(value) {
  return value >= 0
    ? 1 / (1 + Math.exp(-value))
    : Math.exp(value) / (1 + Math.exp(value));
}

function nonOverlapping(spans) {
  spans.sort((a, b) => a.end - b.end || a.start - b.start);
  const previous = new Int32Array(spans.length).fill(-1);
  const best = new Float64Array(spans.length);
  for (let index = 0; index < spans.length; index++) {
    let low = 0,
      high = index - 1;
    while (low <= high) {
      const middle = (low + high) >>> 1;
      if (spans[middle].end <= spans[index].start) {
        previous[index] = middle;
        low = middle + 1;
      } else high = middle - 1;
    }
    const withSpan =
      spans[index].score + (previous[index] >= 0 ? best[previous[index]] : 0);
    best[index] = Math.max(index ? best[index - 1] : 0, withSpan);
  }
  const kept = [];
  for (let index = spans.length - 1; index >= 0; ) {
    const withSpan =
      spans[index].score + (previous[index] >= 0 ? best[previous[index]] : 0);
    if (index === 0 || withSpan >= best[index - 1]) {
      kept.push(spans[index]);
      index = previous[index];
    } else index--;
  }
  return kept;
}

export function decodeEntities(outputs, { words }, text, labels, threshold) {
  const {
    pair_indices: indices,
    pair_logits: logits,
    pair_valid: valid,
  } = outputs;
  const candidates = logits?.dims[2];
  if (
    !indices ||
    !logits ||
    !valid ||
    logits.dims.length !== 3 ||
    logits.dims[0] !== 1 ||
    logits.dims[1] !== labels.length ||
    indices.dims.length !== 4 ||
    indices.dims[0] !== 1 ||
    indices.dims[1] !== labels.length ||
    indices.dims[2] !== candidates ||
    indices.dims[3] !== 2 ||
    valid.dims.length !== 3 ||
    valid.dims[0] !== 1 ||
    valid.dims[1] !== labels.length ||
    valid.dims[2] !== candidates
  ) {
    throw new Error("Model returned an unexpected boundary candidate shape");
  }
  const entities = [];
  const encoder = new TextEncoder();
  for (const [query, label] of labels.entries()) {
    const unique = new Map();
    for (let slot = 0; slot < candidates; slot++) {
      const offset = query * candidates + slot;
      if (!valid.data[offset]) continue;
      const start = Number(indices.data[offset * 2]);
      const end = Number(indices.data[offset * 2 + 1]);
      // The export emits padded and duplicate candidates, sometimes with high logits.
      if (
        !Number.isInteger(start) ||
        !Number.isInteger(end) ||
        start < 0 ||
        start >= end ||
        end > words.length
      )
        continue;
      const logit = logits.data[offset];
      if (!Number.isFinite(logit))
        throw new Error("Model returned a nonfinite candidate score");
      const score = sigmoid(logit);
      if (score < threshold) continue;
      const key = `${start},${end}`;
      if (!unique.has(key) || unique.get(key).score < score)
        unique.set(key, { start, end, score });
    }
    for (const span of nonOverlapping([...unique.values()])) {
      const start = words[span.start].start;
      // Terminal punctuation added as model context is never part of the source span.
      const end = Math.min(words[span.end - 1].end, text.length);
      if (start >= end) continue;
      const matched = text.slice(start, end);
      entities.push({
        label,
        text: matched,
        score: span.score,
        byteRange: {
          start: encoder.encode(text.slice(0, start)).length,
          end: encoder.encode(text.slice(0, end)).length,
        },
        codepointRange: {
          start: Array.from(text.slice(0, start)).length,
          end: Array.from(text.slice(0, end)).length,
        },
        utf16Range: { start, end },
      });
    }
  }
  return entities.sort(
    (a, b) =>
      a.utf16Range.start - b.utf16Range.start ||
      a.utf16Range.end - b.utf16Range.end ||
      a.label.localeCompare(b.label),
  );
}
