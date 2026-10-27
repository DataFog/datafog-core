/**
 * Minimal tokenizer for GLiNER2's DeBERTa-v3 vocabulary.
 * 
 * GLiNER2 uses microsoft/mdeberta-v3-base tokenization. This implements
 * enough of the WordPiece/SentencePiece logic to match the Python tokenizer
 * for ASCII text. Unicode support is partial.
 */

export class Gliner2Tokenizer {
  constructor(tokenizerJson) {
    // Handle SentencePiece format: vocab is array of [token, score] or object {token: id}
    const rawVocab = tokenizerJson.model.vocab;
    this.vocab = new Map();
    this.idToToken = new Map();
    
    if (Array.isArray(rawVocab)) {
      // SentencePiece format: [[token, score], ...]
      for (let id = 0; id < rawVocab.length; id++) {
        const entry = rawVocab[id];
        const token = Array.isArray(entry) ? entry[0] : entry;
        this.vocab.set(token, id);
        this.idToToken.set(id, token);
      }
    } else {
      // Object format: {token: id}
      for (const [token, id] of Object.entries(rawVocab)) {
        this.vocab.set(token, id);
        this.idToToken.set(id, token);
      }
    }
    
    this.addedTokens = new Map();
    for (const token of tokenizerJson.added_tokens || []) {
      this.addedTokens.set(token.content, token.id);
      this.idToToken.set(token.id, token.content);
    }
    
    this.unkTokenId = this.vocab.get("[UNK]") ?? this.addedTokens.get("[UNK]") ?? 3;
    this.padTokenId = this.vocab.get("[PAD]") ?? this.addedTokens.get("[PAD]") ?? 0;
    this.clsTokenId = this.vocab.get("[CLS]") ?? this.addedTokens.get("[CLS]") ?? 1;
    this.sepTokenId = this.vocab.get("[SEP]") ?? this.addedTokens.get("[SEP]") ?? 2;
    
    // GLiNER2 special tokens (these may not exist in standard DeBERTa)
    this.sepTextId = this.addedTokens.get("[SEP_TEXT]") ?? null;
    this.pId = this.addedTokens.get("[P]") ?? null;
    this.eId = this.addedTokens.get("[E]") ?? null;
  }
  
  static async fromUrl(url) {
    const response = await fetch(url);
    const json = await response.json();
    return new Gliner2Tokenizer(json);
  }
  
  /**
   * Tokenize a single word into subword tokens.
   * 
   * DeBERTa uses SentencePiece with "▁" as word boundary marker.
   * WordPiece uses "##" for continuation tokens.
   */
  tokenizeWord(word, isFirstWord = true) {
    const tokens = [];
    
    // Try SentencePiece style first (with ▁ prefix for word start)
    const spPrefix = isFirstWord ? "▁" : "";
    const fullWord = spPrefix + word;
    
    // Check if full word is in vocabulary
    if (this.vocab.has(fullWord)) {
      tokens.push(this.vocab.get(fullWord));
      return tokens;
    }
    
    // Fallback to character-by-character
    let start = 0;
    while (start < word.length) {
      let end = word.length;
      let found = null;
      
      while (start < end) {
        // Try SentencePiece style first
        const prefix = (start === 0 && isFirstWord) ? "▁" : "";
        let substr = prefix + word.slice(start, end);
        
        if (this.vocab.has(substr)) {
          found = substr;
          break;
        }
        
        // Try without prefix
        substr = word.slice(start, end);
        if (this.vocab.has(substr)) {
          found = substr;
          break;
        }
        
        // Try WordPiece style (##)
        if (start > 0) {
          substr = "##" + word.slice(start, end);
          if (this.vocab.has(substr)) {
            found = substr;
            break;
          }
        }
        
        end--;
      }
      
      if (found === null) {
        // Single character fallback
        const ch = word[start];
        if (this.vocab.has(ch)) {
          tokens.push(this.vocab.get(ch));
        } else if (this.vocab.has("▁" + ch)) {
          tokens.push(this.vocab.get("▁" + ch));
        } else {
          tokens.push(this.unkTokenId);
        }
        start++;
      } else {
        tokens.push(this.vocab.get(found));
        start = end;
      }
    }
    
    return tokens;
  }
  
  /**
   * Simple whitespace + punctuation word splitting.
   * Returns array of { word, start, end } with character positions.
   */
  splitIntoWords(text) {
    const words = [];
    const pattern = /[^\s]+/g;
    let match;
    
    while ((match = pattern.exec(text)) !== null) {
      const word = match[0];
      const start = match.index;
      
      // Split off leading/trailing punctuation
      const parts = this._splitPunctuation(word, start);
      words.push(...parts);
    }
    
    return words;
  }
  
  _splitPunctuation(word, baseStart) {
    const results = [];
    let current = "";
    let currentStart = baseStart;
    
    for (let i = 0; i < word.length; i++) {
      const ch = word[i];
      const isPunct = /[.,!?;:'"()\[\]{}]/.test(ch);
      
      if (isPunct) {
        if (current) {
          results.push({
            word: current,
            start: currentStart,
            end: currentStart + current.length,
          });
        }
        results.push({
          word: ch,
          start: baseStart + i,
          end: baseStart + i + 1,
        });
        current = "";
        currentStart = baseStart + i + 1;
      } else {
        current += ch;
      }
    }
    
    if (current) {
      results.push({
        word: current,
        start: currentStart,
        end: currentStart + current.length,
      });
    }
    
    return results;
  }
  
  /**
   * Build GLiNER2 input prompt.
   * 
   * Format: [P] task [E] label1 [E] label2 ... [SEP_TEXT] word1 word2 ...
   */
  encodeForGliner2(text, labels) {
    const words = this.splitIntoWords(text);
    
    // Build schema part: [P] entities [E] label1 [E] label2 ...
    const schemaTokens = [this.pId];
    const schemaPositions = [0]; // Position of [P]
    
    const taskTokens = this.tokenizeWord("entities");
    schemaTokens.push(...taskTokens);
    
    for (const label of labels) {
      schemaPositions.push(schemaTokens.length); // Position of each [E]
      schemaTokens.push(this.eId);
      const labelTokens = this.tokenizeWord(label.toLowerCase());
      schemaTokens.push(...labelTokens);
    }
    
    // Add [SEP_TEXT]
    schemaTokens.push(this.sepTextId);
    
    // Build text part
    const textTokens = [];
    const wordPositions = [];
    
    for (const { word } of words) {
      wordPositions.push(schemaTokens.length + textTokens.length);
      const wordTokens = this.tokenizeWord(word.toLowerCase());
      textTokens.push(...wordTokens);
    }
    
    // Combine all tokens
    const inputIds = [...schemaTokens, ...textTokens];
    const attentionMask = new Array(inputIds.length).fill(1);
    
    return {
      inputIds,
      attentionMask,
      words,
      wordPositions,
      schemaPositions,
      labels,
    };
  }
  
  decode(tokenIds) {
    return tokenIds
      .map((id) => this.idToToken.get(id) || "[UNK]")
      .join(" ")
      .replace(/ ##/g, "");
  }
}
