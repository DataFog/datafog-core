# Embedded statements CNN provenance

Initial supplied artifact: `cnn-1mb-statements`, R6 split-tokenizer seed 43,
`runs/r6split-pretrained-s43`. DataFog's owner explicitly authorized redistribution
of these supplied quantized weights for this integration on November 4, 2026.

The upstream source revision is
`sidmohan0/cnn-NA-1mb@75f5a20e05380de5457502afefaca0383c4e8e91`.
The Rust architecture, preprocessing and decoder port derive from
`models/r3_tagger.py`, `models/r5_squeeze.py`, `models/cnn/contract.py`,
`models/cnn/features.py` and the feature functions in `baselines/rules.py`.
Those sources use Apache-2.0; retain LICENSE-APACHE-2.0.txt.
The embedded sample name lexicon is SSA-derived CC0; retain LICENSE-CC0-names.txt.
The upstream README excludes its older pretrained ONNX weights from public release;
our supplied descendant weights are included under the owner's explicit authorization,
not an inference that the upstream code license covers those older weights.

`bundle.bin` is 1,022,900 bytes. The supplied bundle total, including tensor/config
JSON, is 1,025,143 bytes. Neither number is the size of the full SDK, extension or
in-memory model. SHA-256 identities:

- bundle.bin: `47a29acb46f159fb12d33557ed6859a5157c6dd10da461e27de60d2ece4ebabd`
- bundle.json: `21440a439b944492f7f90b003275f8b48d2cb8b13147f5a10b08ebc812155b80`
- config.json: `a3c93594f1a262ef356c4aff795bf86ff842e51cf5b384ae47895de80a6ee3b7`

The layout is raw concatenated tensors; the final head.bias ends at byte 1,022,900.
The reference qualification decodes the actual serialized fp16 scales, unlike the
upstream squeeze evaluation's pre-fp16-scale simulation. Reference predictions
must therefore not be claimed identical to the upstream published dev measurement.

`rules.json` freezes upstream regexes and default street-type vocabulary without
optional local `raw/libpostal` inputs. `unicode.bin` freezes Python Unicode 16.0.0
classification (word, digit, punctuation, lower, upper, title, regex word,
whitespace flags). It consists of sorted exhaustive nine-byte records: little-endian
u32 start, u32 inclusive end, u8 flags. Unicode classification data are factual
property tables; the generating program is part of DataFog's qualification tooling.
