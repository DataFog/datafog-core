use super::*;
use std::hint::black_box;
use std::time::{Duration, Instant};

// Frozen selection algorithm from 4275002, before Slice Ten. Keep the linear
// duplicate search and repeated overlap scans as an independent reference.
// The preference comparators themselves are unchanged by this optimization.
fn reference_selection(findings: &[Finding], config: &TransformationConfig) -> Vec<Finding> {
    let mut remaining: Vec<Finding> = Vec::with_capacity(findings.len());
    for finding in findings
        .iter()
        .filter(|finding| config.includes(finding) && !config.allows(finding))
    {
        if let Some(existing) = remaining.iter_mut().find(|existing| {
            existing.entity_type == finding.entity_type
                && existing.matched_text == finding.matched_text
                && existing.byte_range == finding.byte_range
                && existing.codepoint_range == finding.codepoint_range
        }) {
            if duplicate_preference(finding, existing).is_lt() {
                *existing = finding.clone();
            }
        } else {
            remaining.push(finding.clone());
        }
    }
    remaining.sort_by_key(|finding| {
        (
            finding.codepoint_range.start,
            finding.codepoint_range.end,
            finding.entity_type.clone(),
        )
    });
    let mut selected = Vec::with_capacity(remaining.len());
    while !remaining.is_empty() {
        let mut preferred_index = 0;
        for candidate_index in 1..remaining.len() {
            if overlap_preference(&remaining[candidate_index], &remaining[preferred_index]).is_lt()
            {
                preferred_index = candidate_index;
            }
        }
        let preferred = remaining.remove(preferred_index);
        remaining.retain(|finding| !findings_overlap(&preferred, finding));
        selected.push(preferred);
    }
    selected.sort_by(|left, right| {
        left.byte_range
            .start
            .cmp(&right.byte_range.start)
            .then_with(|| left.byte_range.end.cmp(&right.byte_range.end))
            .then_with(|| left.entity_type.cmp(&right.entity_type))
    });
    selected
}

fn supplied_finding(text: &str, start: usize, end: usize, confidence: Option<f32>) -> Finding {
    let byte_start = byte_offset_at_codepoint(text, start).unwrap();
    let byte_end = byte_offset_at_codepoint(text, end).unwrap();
    Finding {
        entity_type: "PERSON".to_owned(),
        matched_text: text[byte_start..byte_end].to_owned(),
        byte_range: TextRange {
            start: byte_start,
            end: byte_end,
        },
        codepoint_range: TextRange { start, end },
        confidence,
        detector_name: "test".to_owned(),
        detector_version: None,
    }
}

#[test]
fn cyclic_overlap_preferences_preserve_pairwise_winner() {
    let text = "abcdef";
    let findings = vec![
        supplied_finding(text, 0, 4, Some(0.2)),
        supplied_finding(text, 1, 5, None),
        supplied_finding(text, 2, 6, Some(0.8)),
    ];
    assert!(overlap_preference(&findings[0], &findings[1]).is_lt());
    assert!(overlap_preference(&findings[1], &findings[2]).is_lt());
    assert!(overlap_preference(&findings[2], &findings[0]).is_lt());
    let config = TransformationConfig::new(TransformationStrategy::Redact);
    assert_eq!(
        select_findings(text, &findings, &config).unwrap(),
        vec![findings[2].clone()]
    );
    assert_eq!(
        transform(text, &findings, &config).unwrap().text,
        "ab[PERSON]"
    );
}

#[test]
fn duplicate_confidence_and_provenance_preserve_encounter_order() {
    let text = "José";
    let mut findings = vec![
        supplied_finding(text, 0, 4, Some(0.2)),
        supplied_finding(text, 0, 4, None),
        supplied_finding(text, 0, 4, Some(0.8)),
    ];
    for (finding, detector) in findings.iter_mut().zip(["a", "b", "c"]) {
        finding.detector_name = detector.to_owned();
    }
    let config = TransformationConfig::new(TransformationStrategy::Redact);
    assert_eq!(
        select_findings(text, &findings, &config).unwrap(),
        vec![findings[2].clone()]
    );
    findings.rotate_left(1);
    assert_eq!(
        select_findings(text, &findings, &config).unwrap(),
        vec![findings[2].clone()]
    );
}

#[test]
fn validation_still_precedes_filtering_and_duplicate_collapse() {
    let text = "José";
    let valid = supplied_finding(text, 0, 4, None);
    let mut invalid = valid.clone();
    invalid.matched_text = "wrong".to_owned();
    let mut later_invalid = valid.clone();
    later_invalid.byte_range.end = text.len() + 1;
    for config in [
        TransformationConfig::new(TransformationStrategy::Redact),
        TransformationConfig::new(TransformationStrategy::Redact)
            .with_entities(vec!["EMAIL".to_owned()])
            .unwrap(),
        TransformationConfig::new(TransformationStrategy::Redact)
            .with_exact_allowlist("PERSON", vec![text.to_owned(), "wrong".to_owned()])
            .unwrap(),
    ] {
        assert_eq!(
            select_findings(
                text,
                &[valid.clone(), invalid.clone(), later_invalid.clone()],
                &config
            ),
            Err(PrivacyError::invalid_finding(
                1,
                FindingValidationError::MatchedTextMismatch
            )),
        );
    }
}

struct Random(u64);

impl Random {
    fn below(&mut self, limit: usize) -> usize {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1);
        ((self.0 >> 32) as usize) % limit
    }

    fn shuffle(&mut self, findings: &mut [Finding]) {
        for index in (1..findings.len()).rev() {
            findings.swap(index, self.below(index + 1));
        }
    }
}

#[test]
fn selection_matches_reference_across_unicode_ranges_policies_and_input_orders() {
    let text = "a👋é中e\u{301} z".repeat(16);
    let codepoints = text.chars().count();
    let mut random = Random(0x5eed);
    let configs = [
        TransformationConfig::new(TransformationStrategy::Redact),
        TransformationConfig::new(TransformationStrategy::Remove)
            .with_entities(vec!["PERSON".to_owned(), "EMAIL".to_owned()])
            .unwrap()
            .with_exact_allowlist("PERSON", vec!["a👋".to_owned()])
            .unwrap(),
        TransformationConfig::new(TransformationStrategy::Redact)
            .with_regex_allowlist("PERSON", vec![RegexAllowRule::new("a.*", true)])
            .unwrap(),
    ];
    for trial in 0..3000 {
        let mut findings: Vec<Finding> = Vec::new();
        for _ in 0..random.below(96) {
            let start = random.below(codepoints - 1);
            let length = 1 + random.below((codepoints - start).min(12));
            let score = [0.0, -0.0, 0.2, 0.8, 1.0][random.below(5)];
            let confidence = match trial % 4 {
                0 => None,
                1 => Some(score),
                2 => (length % 2 == 0).then_some(score),
                _ => (random.below(2) == 0).then_some(score),
            };
            let mut finding = supplied_finding(&text, start, start + length, confidence);
            if !findings.is_empty() && random.below(4) == 0 {
                finding = findings[random.below(findings.len())].clone();
                if trial % 4 == 3 {
                    finding.confidence = confidence;
                }
            } else {
                finding.entity_type = ["PERSON", "EMAIL", "CUSTOM"][random.below(3)].to_owned();
            }
            finding.detector_name = ["a", "b", "c"][random.below(3)].to_owned();
            finding.detector_version =
                [None, Some("1"), Some("2")][random.below(3)].map(str::to_owned);
            findings.push(finding);
        }
        for permutation in 0..3 {
            random.shuffle(&mut findings);
            let config = &configs[trial % configs.len()];
            assert_eq!(
                select_findings(&text, &findings, config).unwrap(),
                reference_selection(&findings, config),
                "trial {trial}, permutation {permutation}",
            );
        }
    }
}

#[test]
fn ordered_interval_selection_handles_touching_nested_and_partial_spans() {
    let text = "a👋é中e\u{301} z".repeat(16);
    let config = TransformationConfig::new(TransformationStrategy::Redact);
    let mut random = Random(42);
    for confidence in [None, Some(0.5)] {
        for ranges in [
            vec![(0, 4), (4, 8), (8, 12), (20, 22)],
            vec![(0, 12), (1, 11), (2, 10), (3, 9)],
            vec![(0, 5), (4, 9), (8, 13), (12, 17)],
            vec![(0, 2), (10, 20), (19, 29), (5, 15), (30, 32)],
        ] {
            let mut findings: Vec<_> = ranges
                .into_iter()
                .map(|(start, end)| supplied_finding(&text, start, end, confidence))
                .collect();
            for _ in 0..20 {
                random.shuffle(&mut findings);
                assert_eq!(
                    select_findings(&text, &findings, &config).unwrap(),
                    reference_selection(&findings, &config)
                );
            }
        }
    }
}

fn benchmark_findings(count: usize, workload: &str) -> Vec<Finding> {
    (0..count)
        .map(|index| {
            let (start, length) = match workload {
                "disjoint" => (index * 8, 4),
                "overlap_clusters" => ((index / 4) * 16 + (index % 4) * 2, 4),
                "duplicates" => ((index / 4) * 8, 4),
                _ => unreachable!("unknown test workload"),
            };
            Finding {
                entity_type: "PERSON".to_owned(),
                matched_text: "x".repeat(length),
                byte_range: TextRange {
                    start,
                    end: start + length,
                },
                codepoint_range: TextRange {
                    start,
                    end: start + length,
                },
                confidence: None,
                detector_name: format!("detector-{}", index % 4),
                detector_version: None,
            }
        })
        .collect()
}

fn measure(selection: impl FnOnce() -> Vec<Finding>) -> Duration {
    let started = Instant::now();
    black_box(selection());
    started.elapsed()
}

#[test]
#[ignore = "manual release-mode selection benchmark; no wall-clock assertions"]
fn finding_selection_benchmark() {
    let config = TransformationConfig::new(TransformationStrategy::Redact);
    println!("workload,findings,selected,reference_us,optimized_us,speedup");
    for workload in ["disjoint", "overlap_clusters", "duplicates"] {
        for count in [256, 512, 1024, 2048, 4096] {
            let findings = benchmark_findings(count, workload);
            let text = "x".repeat(count * 8 + 16);
            for finding in &findings {
                validate_finding(&text, finding).unwrap();
            }
            let expected = reference_selection(&findings, &config);
            assert_eq!(select_validated_findings(&findings, &config), expected);
            let reference = || reference_selection(black_box(&findings), black_box(&config));
            let optimized = || select_validated_findings(black_box(&findings), black_box(&config));
            black_box(reference());
            black_box(optimized());
            let mut before = Vec::new();
            let mut after = Vec::new();
            for round in 0..7 {
                if round % 2 == 0 {
                    before.push(measure(reference));
                    after.push(measure(optimized));
                } else {
                    after.push(measure(optimized));
                    before.push(measure(reference));
                }
            }
            before.sort();
            after.sort();
            let before = before[3].as_secs_f64() * 1_000_000.0;
            let after = after[3].as_secs_f64() * 1_000_000.0;
            println!(
                "{workload},{count},{},{before:.3},{after:.3},{:.2}",
                expected.len(),
                before / after
            );
        }
    }
}
