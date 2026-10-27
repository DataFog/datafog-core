export function findCharSpans(scores, threshold, textLen, text, startMap, endMap) {
  const spans = [];
  for (let start = 0; start < textLen; start += 1) {
    for (let width = 0; width < scores[start].length; width += 1) {
      if (scores[start][width] < threshold) {
        continue;
      }
      const end = start + width + 1;
      if (start < 0 || end > textLen) {
        continue;
      }
      try {
        const charStart = startMap[start];
        const charEnd = endMap[end - 1];
        const textSpan = text.slice(charStart, charEnd).trim();
        if (!textSpan) {
          continue;
        }
        spans.push({
          text: textSpan,
          score: scores[start][width],
          start: charStart,
          end: charEnd,
        });
      } catch {
        continue;
      }
    }
  }
  return spans;
}

export function decodeEntitySpanScores({
  text,
  labels,
  countLogits,
  spanScores,
  startMapping,
  endMapping,
  threshold,
}) {
  const countArgmax = countLogits.reduce(
    (bestIndex, value, index) => (value > countLogits[bestIndex] ? index : bestIndex),
    0,
  );
  if (countArgmax <= 0) {
    return [];
  }

  const textLen = startMapping.length;
  const entities = [];

  for (let labelIndex = 0; labelIndex < labels.length; labelIndex += 1) {
    const labelScores = spanScores[labelIndex].slice(0, textLen);
    const spans = findCharSpans(
      labelScores,
      threshold,
      textLen,
      text,
      startMapping,
      endMapping,
    );
    if (!spans.length) {
      continue;
    }

    spans.sort((left, right) => right.score - left.score);
    const selected = [];
    for (const candidate of spans) {
      const overlaps = selected.some(
        (item) => !(candidate.end <= item.start || candidate.start >= item.end),
      );
      if (!overlaps) {
        selected.push(candidate);
      }
    }

    for (const candidate of selected) {
      entities.push({
        start: candidate.start,
        end: candidate.end,
        text: candidate.text,
        label: labels[labelIndex],
        score: candidate.score,
      });
    }
  }

  entities.sort((left, right) => left.start - right.start || left.end - right.end);
  return entities;
}
