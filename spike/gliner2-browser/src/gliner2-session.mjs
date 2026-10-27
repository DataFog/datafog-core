/**
 * GLiNER2 ONNX inference session for browser.
 * 
 * This implements the core inference loop using onnxruntime-web,
 * matching the okasi/gliner2-privacy-filter-pii-multi-onnx export.
 * 
 * The okasi export simplifies the interface:
 * - Schema (entity labels) is baked into the model
 * - Input: input_ids, attention_mask, text_word_indices, text_word_counts
 * - Output: count_logits, span_scores
 */

let ort = null;

async function ensureOrt(ortPath = "/node_modules/onnxruntime-web/dist/ort.mjs") {
  if (!ort) {
    ort = await import(ortPath);
  }
  return ort;
}

import { Gliner2Tokenizer } from "./tokenizer.mjs";

export class Gliner2Session {
  constructor(session, tokenizer, metadata, entityLabels, config) {
    this.session = session;
    this.tokenizer = tokenizer;
    this.metadata = metadata;
    this.entityLabels = entityLabels;
    this.config = config;
    this.backend = null;
  }
  
  /**
   * Load a GLiNER2 session from local files.
   */
  static async create(options) {
    const {
      modelPath,
      modelDataPath, // External data file path (e.g., model_q8.onnx.data)
      tokenizerPath,
      metadataPath,
      entityLabelsPath,
      configPath,
      executionProvider = "wasm",
      wasmPaths,
      ortPath = "/node_modules/onnxruntime-web/dist/ort.mjs",
    } = options;
    
    const ortModule = await ensureOrt(ortPath);
    
    if (wasmPaths) {
      ortModule.env.wasm.wasmPaths = wasmPaths;
    }
    
    const providers = executionProvider === "webgpu" 
      ? ["webgpu", "wasm"]
      : ["wasm"];
    
    const sessionOptions = {
      executionProviders: providers,
      graphOptimizationLevel: "all",
    };
    
    // If external data file is specified, add it to session options
    if (modelDataPath) {
      // The 'path' should match what's stored in the ONNX model
      // Usually it's just the filename without directory
      const dataFileName = modelDataPath.split("/").pop();
      sessionOptions.externalData = [
        {
          path: dataFileName,
          data: modelDataPath,
        }
      ];
      console.log(`[GLiNER2] Using external data: ${dataFileName} from ${modelDataPath}`);
    }
    
    console.log(`[GLiNER2] Loading model from ${modelPath}...`);
    const loadStart = performance.now();
    
    const session = await ortModule.InferenceSession.create(modelPath, sessionOptions);
    
    const loadTime = performance.now() - loadStart;
    console.log(`[GLiNER2] Model loaded in ${loadTime.toFixed(0)}ms`);
    
    const tokenizer = await Gliner2Tokenizer.fromUrl(tokenizerPath);
    
    const metadataResp = await fetch(metadataPath);
    const metadata = await metadataResp.json();
    
    const labelsResp = await fetch(entityLabelsPath);
    const labelsData = await labelsResp.json();
    const entityLabels = labelsData.entity_labels || labelsData;
    
    const configResp = await fetch(configPath);
    const config = await configResp.json();
    
    const instance = new Gliner2Session(session, tokenizer, metadata, entityLabels, config);
    instance.backend = providers[0];
    instance.loadTimeMs = loadTime;
    
    return instance;
  }
  
  /**
   * Extract entities from text.
   * 
   * @param {string} text - Input text
   * @param {string[]} labels - Entity labels to extract (subset of entityLabels)
   * @param {number} threshold - Confidence threshold (0-1)
   * @returns {object} - { entities, inferenceTimeMs, tokenCount, wordCount }
   */
  async extract(text, labels = null, threshold = 0.5) {
    if (!text.trim()) {
      return { entities: [], inferenceTimeMs: 0, tokenCount: 0, wordCount: 0 };
    }
    
    const ortModule = await ensureOrt();
    
    // Use all entity labels if none specified
    const activeLabels = labels || this.entityLabels;
    
    // Tokenize with word tracking
    const encoded = this._tokenize(text);
    
    // Prepare tensors
    const inputIds = new ortModule.Tensor(
      "int64",
      BigInt64Array.from(encoded.inputIds.map(BigInt)),
      [1, encoded.inputIds.length]
    );
    
    const attentionMask = new ortModule.Tensor(
      "int64",
      BigInt64Array.from(encoded.attentionMask.map(BigInt)),
      [1, encoded.attentionMask.length]
    );
    
    const textWordIndices = new ortModule.Tensor(
      "int64",
      BigInt64Array.from(encoded.wordIndices.map(BigInt)),
      [1, encoded.wordIndices.length]
    );
    
    const textWordCounts = new ortModule.Tensor(
      "int64",
      BigInt64Array.from([BigInt(encoded.words.length)]),
      [1]
    );
    
    // Run inference
    const inferStart = performance.now();
    
    const outputs = await this.session.run({
      input_ids: inputIds,
      attention_mask: attentionMask,
      text_word_indices: textWordIndices,
      text_word_counts: textWordCounts,
    });
    
    const inferTime = performance.now() - inferStart;
    
    // Decode spans
    const entities = this._decodeSpans(
      outputs,
      encoded,
      text,
      activeLabels,
      threshold
    );
    
    return {
      entities,
      inferenceTimeMs: inferTime,
      tokenCount: encoded.inputIds.length,
      wordCount: encoded.words.length,
    };
  }
  
  /**
   * Tokenize text for the okasi export.
   * Returns input_ids, attention_mask, word indices, and word metadata.
   */
  _tokenize(text) {
    const words = this.tokenizer.splitIntoWords(text);
    
    const inputIds = [];
    const wordIndices = [];
    
    // Tokenize each word and track positions
    for (const { word } of words) {
      wordIndices.push(inputIds.length);
      const wordTokens = this.tokenizer.tokenizeWord(word.toLowerCase());
      inputIds.push(...wordTokens);
    }
    
    const attentionMask = new Array(inputIds.length).fill(1);
    
    return {
      inputIds,
      attentionMask,
      wordIndices,
      words,
    };
  }
  
  /**
   * Decode span scores to entity mentions.
   */
  _decodeSpans(outputs, encoded, text, labels, threshold) {
    const entities = [];
    
    // span_scores: [1, numLabels, words, maxSpanLen]
    const spanScores = outputs.span_scores?.data;
    const countLogits = outputs.count_logits?.data;
    
    if (!spanScores) {
      console.warn("[GLiNER2] No span_scores in output");
      return entities;
    }
    
    // Apply count-floor fix: if count prediction is 0, we still extract
    // The count head predicts structural instance count, not span cap
    // We floor 0 to 1 to prevent skipping all spans
    let countPrediction = 0;
    if (countLogits) {
      countPrediction = Array.from(countLogits).indexOf(Math.max(...countLogits));
      if (countPrediction === 0) {
        countPrediction = 1; // Count-floor fix
      }
    }
    
    const numLabels = this.entityLabels.length;
    const numWords = encoded.words.length;
    const maxSpanLen = 8;
    
    // Create label index map for filtering
    const labelIndices = new Map();
    this.entityLabels.forEach((label, idx) => {
      if (!labels || labels.includes(label)) {
        labelIndices.set(label, idx);
      }
    });
    
    // Decode each label
    for (let labelIdx = 0; labelIdx < numLabels; labelIdx++) {
      const label = this.entityLabels[labelIdx];
      if (labels && !labels.includes(label)) continue;
      
      for (let wordIdx = 0; wordIdx < numWords; wordIdx++) {
        for (let spanLen = 0; spanLen < maxSpanLen; spanLen++) {
          // Index into flattened array: [1, labels, words, spans]
          const idx = (labelIdx * numWords * maxSpanLen) + (wordIdx * maxSpanLen) + spanLen;
          const score = spanScores[idx];
          
          // Scores are already post-sigmoid in okasi export
          if (score >= threshold) {
            const startWord = wordIdx;
            const endWord = wordIdx + spanLen + 1;
            
            if (endWord <= numWords) {
              const startChar = encoded.words[startWord].start;
              const endChar = encoded.words[endWord - 1].end;
              const matchedText = text.slice(startChar, endChar);
              
              entities.push({
                label,
                text: matchedText,
                start: startChar,
                end: endChar,
                score,
              });
            }
          }
        }
      }
    }
    
    // Non-maximum suppression
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
