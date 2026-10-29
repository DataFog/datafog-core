//! Verify coverage without changing selection, attribution or transformation strategy.
use crate::TextRange;

/// Original ranges not completely covered by the supplied disjoint source-ordered ranges.
pub(crate) fn uncovered(required: &[TextRange], selected: &[TextRange]) -> Vec<usize> {
    let mut union: Vec<TextRange> = Vec::new();
    for range in selected {
        if let Some(last) = union.last_mut().filter(|last| range.start <= last.end) {
            last.end = last.end.max(range.end);
        } else {
            union.push(*range);
        }
    }
    required
        .iter()
        .enumerate()
        .filter_map(|(index, range)| {
            let first = union.partition_point(|candidate| candidate.end <= range.start);
            (!union.get(first).is_some_and(|candidate| {
                candidate.start <= range.start && candidate.end >= range.end
            }))
            .then_some(index)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn crossing_suffixes_fail_but_containment_adjacent_tiles_and_equal_ranges_cover() {
        let required = [
            TextRange { start: 0, end: 4 },
            TextRange { start: 2, end: 6 },
        ];
        assert_eq!(uncovered(&required, &[required[0]]), vec![1]);
        assert!(uncovered(&required, &[TextRange { start: 0, end: 6 }]).is_empty());
        assert!(
            uncovered(
                &required,
                &[
                    TextRange { start: 0, end: 3 },
                    TextRange { start: 3, end: 6 }
                ]
            )
            .is_empty()
        );
        assert_eq!(
            uncovered(
                &required,
                &[
                    TextRange { start: 0, end: 2 },
                    TextRange { start: 3, end: 6 }
                ]
            ),
            vec![0, 1]
        );
    }
}
