//! Statements-v1 preprocessing port; source and licenses are in assets/PROVENANCE.md.
use regex::Regex;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::sync::LazyLock;

const UNICODE: &[u8] = include_bytes!("assets/unicode.bin");
fn flags(c: char) -> u8 {
    let n = c as u32;
    let mut lo = 0;
    let mut hi = UNICODE.len() / 9;
    while lo < hi {
        let mid = (lo + hi) / 2;
        let p = mid * 9;
        let end = u32::from_le_bytes(
            UNICODE[p + 4..p + 8]
                .try_into()
                .expect("fixed Unicode record"),
        );
        if end < n {
            lo = mid + 1;
        } else {
            hi = mid;
        }
    }
    UNICODE[lo * 9 + 8]
}
fn has(c: char, mask: u8) -> bool {
    flags(c) & mask != 0
}
fn digit(c: char) -> bool {
    has(c, 2)
}
fn py_word(c: char) -> bool {
    has(c, 64)
}
fn trim(s: &str) -> &str {
    s.trim_matches(|c| has(c, 128))
}

#[derive(Debug)]
pub(super) struct Token {
    pub start: usize,
    pub end: usize,
    pub word: usize,
    pub chars: Vec<usize>,
    pub features: [f32; 22],
}

#[derive(Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
struct Rules {
    re_us_pc: String,
    re_uk_pc: String,
    re_ca_pc: String,
    re_num_start: String,
    re_pobox: String,
    re_rr: String,
    company_words: String,
    label_line: String,
    pos_cues: String,
    neg_cues: String,
    stub_cues: String,
    street_types: Vec<String>,
    states: Vec<String>,
    provs: Vec<String>,
}
struct Compiled {
    regex: BTreeMap<&'static str, Regex>,
    streets: Vec<String>,
    states: Vec<String>,
    provinces: Regex,
}
static FIVE_DIGITS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\d{5}").expect("five digits"));
static DECIMAL: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\p{Nd}$").expect("decimal"));
static RULES: LazyLock<Compiled> = LazyLock::new(|| {
    let r: Rules =
        serde_json::from_str(include_str!("assets/rules.json")).expect("embedded rule config");
    let regex = [
        ("us", r.re_us_pc, false),
        ("uk", r.re_uk_pc, true),
        ("ca", r.re_ca_pc, true),
        ("number", r.re_num_start, false),
        ("box", r.re_pobox, true),
        ("rr", r.re_rr, true),
        ("company", r.company_words, true),
        ("label", r.label_line, true),
        ("positive", r.pos_cues, true),
        ("negative", r.neg_cues, true),
        ("stub", r.stub_cues, true),
    ]
    .into_iter()
    .map(|(k, p, ic)| {
        (
            k,
            regex::RegexBuilder::new(&p)
                .case_insensitive(ic)
                .build()
                .expect("embedded regex"),
        )
    })
    .collect();
    Compiled {
        regex,
        streets: r.street_types,
        states: r.states,
        provinces: Regex::new(&format!(r"\b({})\b", r.provs.join("|"))).expect("province regex"),
    }
});
// Python's Unicode \w excludes marks/join controls but includes nondecimal
// numbers. These regexes use word boundaries and ASCII literals, so project only
// those differing classes to equivalent non-ASCII word/nonword representatives.
// This affects rule features only; tokenizer/hash text and all offsets stay original.
fn regex_text(s: &str, insensitive: bool) -> String {
    static WORD: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\w$").expect("word category"));
    s.chars()
        .map(|c| {
            if insensitive && matches!(c, 'İ' | 'ı') {
                return 'I';
            }
            if has(c, 128) && matches!(c, '\u{1c}'..='\u{1f}') {
                return ' ';
            }
            let mut bytes = [0; 4];
            let rust_word = WORD.is_match(c.encode_utf8(&mut bytes));
            if py_word(c) != rust_word {
                if py_word(c) { '字' } else { '!' }
            } else {
                c
            }
        })
        .collect()
}
fn matches(key: &str, s: &str) -> bool {
    RULES.regex[key].is_match(&regex_text(s, !matches!(key, "us" | "number")))
}
fn postcode(s: &str) -> bool {
    let t = trim(s);
    let upper = regex_text(&t.to_uppercase(), false);
    if let Some(c) = RULES.regex["us"].captures(&upper) {
        if RULES.states.iter().any(|x| x == &c[1]) {
            return true;
        }
    }
    if matches("ca", t) && RULES.provinces.is_match(&upper) {
        return true;
    }
    if matches("uk", t) && !FIVE_DIGITS.is_match(t) {
        return true;
    }
    RULES.regex["uk"]
        .find(&regex_text(t, true))
        .is_some_and(|m| m.start() == 0 && m.end() == t.len())
}
fn street(s: &str) -> bool {
    let t = trim(s);
    if matches("box", t) || matches("rr", t) {
        return true;
    }
    if !matches("number", t) {
        return false;
    }
    let normalized: String = t
        .to_uppercase()
        .chars()
        .map(|c| if py_word(c) || has(c, 128) { c } else { ' ' })
        .collect();
    let words: Vec<_> = normalized
        .split(|c| has(c, 128))
        .filter(|s| !s.is_empty())
        .collect();
    words
        .iter()
        .skip(1)
        .any(|w| RULES.streets.iter().any(|s| s == w))
        || (words.len() <= 5 && words.iter().skip(1).all(|w| !w.chars().any(digit)))
}
fn name(s: &str) -> bool {
    let t = trim(s);
    !t.is_empty()
        && t.chars().count() <= 60
        && !t.chars().any(digit)
        && !t.contains(':')
        && !matches("label", t)
        && !matches("positive", t)
        && !matches("negative", t)
        && !matches("stub", t)
}
fn title(s: &str) -> bool {
    let mut previous = false;
    let mut cased = false;
    for c in s.chars() {
        if has(c, 16 | 32) {
            if previous {
                return false;
            }
            previous = true;
            cased = true;
        } else if has(c, 8) {
            if !previous {
                return false;
            }
            previous = true;
            cased = true;
        } else {
            previous = false;
        }
    }
    cased
}
fn split_at(chars: &[(usize, char)], i: usize) -> bool {
    let a = chars[i - 1].1;
    let b = chars[i].1;
    // Python's \d is decimal, while isdigit also includes superscript digits.
    let decimal = |c: char| DECIMAL.is_match(&c.to_string());
    let letter = |c: char| py_word(c) && !decimal(c) && c != '_';
    (decimal(a) && i + 2 < chars.len() && chars[i..i + 3].iter().all(|(_, c)| letter(*c)))
        || (decimal(b) && i >= 3 && chars[i - 3..i].iter().all(|(_, c)| letter(*c)))
        || ((a.is_ascii_lowercase() || ('à'..='ÿ').contains(&a))
            && (b.is_ascii_uppercase() || ('À'..='Þ').contains(&b)))
}
pub(super) fn encode(text: &str) -> Result<Vec<Token>, crate::model_scan::ModelError> {
    let mut base = Vec::new();
    let mut start = None;
    for (i, c) in text.char_indices() {
        if has(c, 1) {
            if start.is_none() {
                start = Some(i);
            }
        } else {
            if let Some(s) = start.take() {
                base.push((s, i));
            }
            if !has(c, 128) {
                base.push((i, i + c.len_utf8()));
            }
        }
    }
    if let Some(s) = start {
        base.push((s, text.len()));
    }
    let mut spans = Vec::new();
    for (s, e) in base {
        let all: Vec<_> = text[s..e].char_indices().collect();
        for chunk in all.chunks(32) {
            let begin = s + chunk[0].0;
            let end = s
                + chunk.last().expect("nonempty chunk").0
                + chunk.last().expect("nonempty chunk").1.len_utf8();
            let chars: Vec<_> = text[begin..end].char_indices().collect();
            let mut prev = begin;
            for i in 1..chars.len() {
                if split_at(&chars, i) {
                    spans.push((prev, begin + chars[i].0));
                    prev = begin + chars[i].0;
                }
            }
            spans.push((prev, end));
        }
    }
    if spans.len() > super::MAX_TOKENS {
        return Err(crate::model_scan::ModelError::new(
            "model_resource_exhausted",
            "limit_exceeded",
            "/data",
        ));
    }
    let lexicon: serde_json::Value =
        serde_json::from_str(include_str!("assets/config.json")).expect("embedded config");
    let lexicon = lexicon["lexicon"].as_array().expect("lexicon");
    let lines: Vec<_> = text.split('\n').collect();
    let mut starts = Vec::new();
    let mut offset = 0;
    for line in &lines {
        starts.push(offset);
        offset += line.len() + 1;
    }
    let b = |x: bool| if x { 1.0 } else { 0.0 };
    let line_features: Vec<_> = lines
        .iter()
        .enumerate()
        .map(|(i, line)| {
            let above = lines[i.saturating_sub(4)..i].join("\n");
            let stub = lines[i.saturating_sub(8)..i].join("\n");
            [
                b(postcode(line)),
                b(street(line)),
                b(name(line)),
                b(matches("company", line)),
                b(matches("positive", line)),
                b(matches("positive", &above)),
                b(matches("negative", &above)),
                b(matches("stub", &stub)),
            ]
        })
        .collect();
    let mut li = 0;
    let mut previous = None;
    Ok(spans
        .into_iter()
        .map(|(start, end)| {
            while li + 1 < lines.len() && starts[li + 1] <= start {
                li += 1;
            }
            let w = &text[start..end];
            let lower = w.to_lowercase();
            let chars: Vec<_> = w.chars().collect();
            let case = chars.iter().any(|c| has(*c, 8 | 16 | 32));
            let canadian = chars.len() == 3
                && chars[0].is_ascii_alphabetic()
                && DECIMAL.is_match(&chars[1].to_string())
                && chars[2].is_ascii_alphabetic();
            static UK: LazyLock<Regex> =
                LazyLock::new(|| Regex::new(r"^[A-Za-z]{1,2}\d[A-Za-z\d]?$").expect("uk token"));
            let mut features = [
                b(case
                    && !chars
                        .iter()
                        .any(|c| has(*c, 16) || (has(*c, 32) && !has(*c, 8)))),
                b(case
                    && !chars
                        .iter()
                        .any(|c| has(*c, 8) || (has(*c, 32) && !has(*c, 16)))),
                b(title(w)),
                b(chars.iter().all(|c| digit(*c))),
                b(chars.iter().any(|c| digit(*c))),
                b(chars.iter().all(|c| has(*c, 4))),
                b(!w.is_ascii()),
                chars.len().min(32) as f32 / 32.0,
                b(lexicon.iter().any(|v| v.as_str() == Some(&lower))),
                0.0,
                b(canadian),
                b(UK.is_match(w)),
                line_features[li][0],
                line_features[li][1],
                line_features[li][2],
                line_features[li][3],
                line_features[li][4],
                line_features[li][5],
                line_features[li][6],
                line_features[li][7],
                b(previous != Some(li)),
                li as f32 / lines.len().max(1) as f32,
            ];
            // page_items stores features as fp16 before the inference batch is made.
            for f in &mut features {
                *f = super::half_round(*f);
            }
            previous = Some(li);
            let hash = Sha256::digest([b"42\0".as_slice(), lower.as_bytes()].concat());
            Token {
                start,
                end,
                word: 1
                    + (u64::from_be_bytes(hash[..8].try_into().expect("digest")) % 32767) as usize,
                chars: chars.into_iter().map(|c| 1 + c as usize % 1023).collect(),
                features,
            }
        })
        .collect())
}
