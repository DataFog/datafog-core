//! Exhaustive, deterministic differential coverage check for PR #273.
use serde_json::json;
use std::collections::BTreeMap;
use std::time::Instant;

type Span = (String, usize, usize);

fn covered(spans: &[Span], entity: Option<&str>, start: usize, end: usize) -> bool {
    let mut ranges: Vec<_> = spans
        .iter()
        .filter(|(kind, _, _)| entity.is_none_or(|entity| kind == entity))
        .map(|(_, start, end)| (*start, *end))
        .collect();
    ranges.sort_unstable();
    let mut cursor = start;
    for (left, right) in ranges {
        if left > cursor {
            break;
        }
        cursor = cursor.max(right);
        if cursor >= end {
            return true;
        }
    }
    false
}

macro_rules! spans {
    ($api:ident, $text:expr) => {
        $api::scan($text)
            .into_iter()
            .map(|finding| {
                (
                    finding.entity_type,
                    finding.byte_range.start,
                    finding.byte_range.end,
                )
            })
            .collect::<Vec<Span>>()
    };
}

fn plant(text: &mut String, planted: &mut Vec<Span>, entity: &str, value: &str) {
    let start = text.len();
    text.push_str(value);
    planted.push((entity.to_owned(), start, text.len()));
}

fn main() {
    // Every combination is visited; there is no random seed or sampling cutoff.
    let contexts = [
        "",
        "contact",
        "メールは",
        "です",
        "用户",
        "电子邮件",
        "연락처",
        "почта",
        "البريد",
        "דוא״ל",
        "ईमेल",
        "ইমেইল",
        "อีเมล",
        "អ៊ីមែល",
        "ელფოსტა",
        "էլփոստ",
        "ηλεκτρονικό",
        "jörg",
        "e\u{301}",
        "😀",
    ];
    let emails = [
        "alice@example.com",
        "b@example.net",
        "first.last+tag@example.co.uk",
        "user_name@sub.example.org",
        "a-b@a-b.example",
        "123@example.com",
        "josé@example.com",
        "jörg@müller.de",
        "e\u{301}@example.com",
        "用户@例子.广告",
        "почта@пример.рф",
        "ébob@example.net",
    ];
    let separators = ["", ".", "-", "/", " ", "\n", ",", "です"];
    let started = Instant::now();
    let mut cases = 0usize;
    let mut values = 0usize;
    let mut same_entity_losses = BTreeMap::<String, usize>::new();
    let mut incidental_losses = BTreeMap::<String, usize>::new();
    let mut examples = BTreeMap::new();
    for prefix in contexts {
        for suffix in contexts {
            for first in emails {
                for second in emails {
                    for separator in separators {
                        for format in ["text", "env", "sql"] {
                            let mut text = match format {
                                "env" => "CONTACT=\"".to_owned(),
                                "sql" => "INSERT INTO contacts VALUES ('".to_owned(),
                                _ => String::new(),
                            };
                            let mut planted = Vec::new();
                            text.push_str(prefix);
                            plant(
                                &mut text,
                                &mut planted,
                                "CREDIT_CARD",
                                "4111 1111 1111 1111",
                            );
                            text.push_str(separator);
                            plant(&mut text, &mut planted, "PHONE", "212-555-0100");
                            text.push_str(suffix);
                            plant(&mut text, &mut planted, "SSN", "123-45-6789");
                            text.push_str(prefix);
                            plant(&mut text, &mut planted, "EMAIL", first);
                            text.push_str(separator);
                            plant(&mut text, &mut planted, "EMAIL", second);
                            text.push_str(suffix);
                            match format {
                                "env" => text.push('"'),
                                "sql" => text.push_str("');"),
                                _ => {}
                            }
                            let candidate = spans!(candidate, &text);
                            let baselines = [
                                ("0.4.4", spans!(before_unicode, &text)),
                                ("1.7.0", spans!(before_fix, &text)),
                            ];
                            for (baseline, findings) in baselines {
                                for (entity, start, end) in &planted {
                                    if covered(&candidate, Some(entity), *start, *end) {
                                        continue;
                                    }
                                    let same_entity =
                                        covered(&findings, Some(entity), *start, *end);
                                    let incidental = covered(&findings, None, *start, *end)
                                        && !covered(&candidate, None, *start, *end);
                                    if !same_entity && !incidental {
                                        continue;
                                    }
                                    let category = if same_entity {
                                        "same_entity"
                                    } else {
                                        "incidental"
                                    };
                                    let key = format!("{baseline}/{entity}/{category}");
                                    let counts = if same_entity {
                                        &mut same_entity_losses
                                    } else {
                                        &mut incidental_losses
                                    };
                                    *counts.entry(key.clone()).or_default() += 1;
                                    examples.entry(key).or_insert_with(|| json!({
                                        "text": text,
                                        "planted": {"entity":entity, "start_byte":start,"end_byte":end},
                                        "baseline_findings": findings,
                                        "candidate_findings": candidate,
                                    }));
                                }
                            }
                            cases += 1;
                            values += planted.len();
                        }
                    }
                }
            }
        }
    }
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "matrix": {"contexts":contexts.len(), "email_shapes":emails.len(),
                "separators":separators.len(), "source_wrappers":3},
            "cases":cases, "planted_values":values,
            "elapsed_seconds":started.elapsed().as_secs_f64(),
            "same_entity_coverage_losses":same_entity_losses,
            "incidental_cross_entity_coverage_losses":incidental_losses,
            "first_example_per_category":examples,
        }))
        .unwrap()
    );
    if !same_entity_losses.is_empty() {
        std::process::exit(1);
    }
}
