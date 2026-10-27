/**
 * GLiNER2 inference pipeline for Node.js (reference implementation).
 *
 * Uses onnxruntime-node for ONNX inference with the preprocessing
 * that matches the Python GLiNER2 pipeline.
 */

import * as ort from "onnxruntime-node";
import { readFileSync, existsSync } from "node:fs";
import { join } from "node:path";
import { Gliner2Preprocessor } from "./gliner2-preprocessor.mjs";

export class Gliner2InferenceSession {
  constructor(session, preprocessor, entityLabels, metadata) {
    this.session = session;
    this.preprocessor = preprocessor;
    this.entityLabels = entityLabels;
    this.metadata = metadata;
  }

  static async create(options) {
    const {
      modelPath,
      dataPath,
      tokenizerPath,
      entityLabelsPath,
      metadataPath,
      executionProviders = ["cpu"],
    } = options;

    const sessionOptions = {
      executionProviders,
      graphOptimizationLevel: "all",
    };

    if (dataPath && existsSync(dataPath)) {
      const dataFileName = dataPath.split("/").pop();
      sessionOptions.externalData = [
        { path: dataFileName, data: dataPath },
      ];
    }

    const session = await ort.InferenceSession.create(modelPath, sessionOptions);

    const preprocessor = await Gliner2Preprocessor.fromPretrained(tokenizerPath, {
      localOnly: true,
    });

    let entityLabels = null;
    if (entityLabelsPath && existsSync(entityLabelsPath)) {
      const labelsData = JSON.parse(readFileSync(entityLabelsPath, "utf-8"));
      entityLabels = labelsData.entity_labels || labelsData;
    }

    let metadata = null;
    if (metadataPath && existsSync(metadataPath)) {
      metadata = JSON.parse(readFileSync(metadataPath, "utf-8"));
    }

    return new Gliner2InferenceSession(session, preprocessor, entityLabels, metadata);
  }

  async extract(text, options = {}) {
    const { threshold = 0.5, labels = null } = options;

    const activeLabels = labels || this.entityLabels;
    const prepared = this.preprocessor.prepareFeed(text, activeLabels);

    const feed = {
      input_ids: this._tensor2D(prepared.feed.input_ids, "int64"),
      attention_mask: this._tensor2D(prepared.feed.attention_mask, "int64"),
      text_word_indices: this._tensor2D(prepared.feed.text_word_indices, "int64"),
      text_word_counts: this._tensor1D(prepared.feed.text_word_counts, "int64"),
    };

    const startTime = performance.now();
    const outputs = await this.session.run(feed);
    const inferenceTimeMs = performance.now() - startTime;

    const entities = this._decodeSpans(
      outputs,
      prepared,
      text,
      activeLabels,
      threshold
    );

    return {
      entities,
      inferenceTimeMs,
      tokenCount: prepared.feed.input_ids[0].length,
      wordCount: prepared.words.length,
    };
  }

  _tensor2D(values, dtype) {
    const rows = values.length;
    const cols = values[0]?.length ?? 0;
    const flat = values.flat();
    if (dtype === "int64") {
      return new ort.Tensor("int64", BigInt64Array.from(flat.map(BigInt)), [rows, cols]);
    }
    return new ort.Tensor("float32", Float32Array.from(flat), [rows, cols]);
  }

  _tensor1D(values, dtype) {
    const arr = Array.isArray(values) ? values : [values];
    if (dtype === "int64") {
      return new ort.Tensor("int64", BigInt64Array.from(arr.map(BigInt)), [arr.length]);
    }
    return new ort.Tensor("float32", Float32Array.from(arr), [arr.length]);
  }

  _decodeSpans(outputs, prepared, text, labels, threshold) {
    const spanScores = outputs.span_scores?.data;
    const countLogits = outputs.count_logits?.data;

    if (!spanScores) {
      return [];
    }

    let countPred = 0;
    if (countLogits) {
      const logitsArr = Array.from(countLogits);
      countPred = logitsArr.indexOf(Math.max(...logitsArr));
    }

    if (countPred === 0) {
      countPred = 1;
    }

    const dims = outputs.span_scores.dims;
    const [, numLabels, numWords, maxSpanLen] = dims;

    const entities = [];
    const startMapping = prepared.start_mapping;
    const endMapping = prepared.end_mapping;

    for (let labelIdx = 0; labelIdx < numLabels; labelIdx++) {
      const label = labels[labelIdx];

      for (let wordIdx = 0; wordIdx < numWords && wordIdx < prepared.words.length; wordIdx++) {
        for (let spanLen = 0; spanLen < maxSpanLen; spanLen++) {
          const idx = (labelIdx * numWords * maxSpanLen) + (wordIdx * maxSpanLen) + spanLen;
          const score = spanScores[idx];

          if (score >= threshold) {
            const endWordIdx = wordIdx + spanLen;
            if (endWordIdx < prepared.words.length) {
              const charStart = startMapping[wordIdx];
              const charEnd = endMapping[endWordIdx];
              const matchedText = text.slice(charStart, charEnd);

              if (matchedText.trim()) {
                entities.push({
                  label,
                  text: matchedText,
                  start: charStart,
                  end: charEnd,
                  score,
                });
              }
            }
          }
        }
      }
    }

    entities.sort((a, b) => b.score - a.score);
    const kept = [];
    for (const entity of entities) {
      const overlaps = kept.some(
        (e) => !(entity.end <= e.start || entity.start >= e.end)
      );
      if (!overlaps) {
        kept.push(entity);
      }
    }

    kept.sort((a, b) => a.start - b.start);
    return kept;
  }

  async dispose() {
    if (this.session) {
      await this.session.release();
      this.session = null;
    }
  }
}
