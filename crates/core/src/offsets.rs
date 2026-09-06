use crate::{TextRange, Utf16RangeError, validate_utf8_range};

const CHECKPOINT_INTERVAL: usize = 256;

#[derive(Clone, Copy, Default)]
struct Position {
    byte: usize,
    codepoint: usize,
    utf16: usize,
}

#[derive(Clone, Copy)]
enum Coordinate {
    Byte,
    Codepoint,
}

impl Coordinate {
    fn offset(self, position: Position) -> usize {
        match self {
            Self::Byte => position.byte,
            Self::Codepoint => position.codepoint,
        }
    }
}

/// Reusable offset conversion for one immutable string.
///
/// The index walks text lazily and keeps one checkpoint per 256 code points.
/// Reuse it when converting many ranges, including overlapping or unordered
/// ranges. Short strings require no checkpoint allocation. No text is copied.
pub struct TextIndex<'a> {
    text: &'a str,
    frontier: Position,
    checkpoints: Vec<Position>,
}

impl<'a> TextIndex<'a> {
    /// Create an empty index borrowing the exact source string.
    pub fn new(text: &'a str) -> Self {
        Self {
            text,
            frontier: Position::default(),
            checkpoints: Vec::new(),
        }
    }

    /// Validate UTF-8 byte boundaries and convert to UTF-16 code-unit offsets.
    /// Empty ranges are accepted; malformed ranges return `Utf16RangeError`.
    pub fn utf16_range(&mut self, range: TextRange) -> Result<TextRange, Utf16RangeError> {
        validate_utf8_range(self.text, range)?;
        Ok(TextRange {
            start: self.locate(range.start, Coordinate::Byte).utf16,
            end: self.locate(range.end, Coordinate::Byte).utf16,
        })
    }

    pub(crate) fn byte_offset_at_codepoint(&mut self, offset: usize) -> Option<usize> {
        let position = self.locate(offset, Coordinate::Codepoint);
        (position.codepoint == offset).then_some(position.byte)
    }

    // Core detector/token spans already identify valid UTF-8 boundaries.
    pub(crate) fn codepoint_offset(&mut self, byte: usize) -> usize {
        debug_assert!(self.text.is_char_boundary(byte));
        self.locate(byte, Coordinate::Byte).codepoint
    }

    fn locate(&mut self, offset: usize, coordinate: Coordinate) -> Position {
        let frontier_offset = coordinate.offset(self.frontier);
        if frontier_offset == offset {
            return self.frontier;
        }
        let mut position = if offset > frontier_offset {
            self.frontier
        } else {
            let end = self
                .checkpoints
                .partition_point(|&position| coordinate.offset(position) <= offset);
            end.checked_sub(1)
                .map(|index| self.checkpoints[index])
                .unwrap_or_default()
        };
        for character in self.text[position.byte..].chars() {
            if coordinate.offset(position) >= offset {
                break;
            }
            position.byte += character.len_utf8();
            position.codepoint += 1;
            position.utf16 += character.len_utf16();
            if position.byte > self.frontier.byte {
                self.frontier = position;
                if position.codepoint % CHECKPOINT_INTERVAL == 0 {
                    self.checkpoints.push(position);
                }
            }
        }
        position
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unordered_offsets_match_direct_unicode_counts_across_checkpoints() {
        for text in ["", "ascii", "a👋e\u{301}中"].map(|part| part.repeat(300)) {
            let boundaries: Vec<_> = text
                .char_indices()
                .map(|(byte, _)| byte)
                .chain(std::iter::once(text.len()))
                .collect();
            let mut index = TextIndex::new(&text);
            assert!(index.checkpoints.is_empty());
            for codepoint in (0..boundaries.len()).rev().chain(0..boundaries.len()) {
                let byte = boundaries[codepoint];
                assert_eq!(index.byte_offset_at_codepoint(codepoint), Some(byte));
                assert_eq!(index.codepoint_offset(byte), codepoint);
                let range = TextRange {
                    start: byte,
                    end: text.len(),
                };
                assert_eq!(index.utf16_range(range), crate::utf16_range(&text, range));
            }
            assert_eq!(index.byte_offset_at_codepoint(usize::MAX), None);
            assert_eq!(index.byte_offset_at_codepoint(boundaries.len()), None);
            assert_eq!(index.byte_offset_at_codepoint(0), Some(0));
            assert_eq!(
                index.checkpoints.len(),
                (boundaries.len() - 1) / CHECKPOINT_INTERVAL
            );
        }
    }

    #[test]
    fn invalid_and_empty_ranges_match_the_scalar_converter() {
        let text = "a👋中";
        let mut index = TextIndex::new(text);
        for start in (0..=text.len() + 1).chain(std::iter::once(usize::MAX)) {
            for end in (0..=text.len() + 1).chain(std::iter::once(usize::MAX)) {
                let range = TextRange { start, end };
                assert_eq!(index.utf16_range(range), crate::utf16_range(text, range));
            }
        }
        assert!(index.checkpoints.is_empty());
    }

    #[test]
    fn cached_validation_preserves_first_error_and_original_finding_index() {
        use crate::{
            Finding, FindingValidationError, PrivacyError, TransformationConfig,
            TransformationStrategy,
        };
        let text = format!("{}may@example.test", "👋e\u{301} ".repeat(300));
        let late = crate::scan(&text).pop().unwrap();
        let early = Finding {
            entity_type: "CUSTOM".to_owned(),
            matched_text: "👋".to_owned(),
            byte_range: TextRange { start: 0, end: 4 },
            codepoint_range: TextRange { start: 0, end: 1 },
            confidence: None,
            detector_name: "test".to_owned(),
            detector_version: None,
        };
        let config = TransformationConfig::new(TransformationStrategy::Redact)
            .with_entities(vec!["UNSELECTED".to_owned()])
            .unwrap();
        for (byte_range, codepoint_range, expected) in [
            (
                TextRange { start: 0, end: 4 },
                TextRange { start: 1, end: 2 },
                FindingValidationError::InconsistentRanges,
            ),
            (
                TextRange { start: 0, end: 4 },
                TextRange {
                    start: 0,
                    end: usize::MAX,
                },
                FindingValidationError::CodepointRangeOutOfBounds,
            ),
            (
                TextRange { start: 1, end: 4 },
                TextRange {
                    start: 0,
                    end: usize::MAX,
                },
                FindingValidationError::InvalidUtf8Boundary,
            ),
        ] {
            let invalid = Finding {
                byte_range,
                codepoint_range,
                ..early.clone()
            };
            assert_eq!(
                crate::select_findings(&text, &[late.clone(), early.clone(), invalid], &config),
                Err(PrivacyError::invalid_finding(2, expected)),
            );
        }
    }
}
