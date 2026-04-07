//! ASR subtitle segment splitting by punctuation and character count.
//!
//! Splits long ASR segments into shorter subtitles suitable for display,
//! using word-level timestamps when available for accurate timing.

use super::provider::{ASRSegment, RawASRSegment};

/// Punctuation characters used as sentence boundaries
const PUNCTUATION: &[char] = &[
    '，', '。', '！', '？', '；', '：', '、', '…',
    ',', '.', '!', '?', ';', ':',
];

/// A sentence span within a segment's text
struct Sentence {
    text: String,
    char_start: usize,
    char_end: usize,
}

/// A group of merged sentences forming one subtitle
struct Group {
    char_start: usize,
    char_end: usize,
}

/// Character-to-time mapping entry
struct CharTime {
    start: f64,
    end: f64,
}

/// Split raw ASR segments into shorter subtitles by punctuation and character count.
///
/// - `max_chars == 0`: no splitting, return segments as-is
/// - Each subtitle contains at least one sentence (never splits mid-sentence)
/// - Uses word-level timestamps when available, falls back to linear interpolation
pub fn split_segments(raw: &[RawASRSegment], max_chars: usize) -> Vec<ASRSegment> {
    let mut result = Vec::new();
    for seg in raw {
        split_one_segment(seg, max_chars, &mut result);
    }
    result
}

fn split_one_segment(seg: &RawASRSegment, max_chars: usize, out: &mut Vec<ASRSegment>) {
    let text = seg.text.trim();
    if text.is_empty() {
        return;
    }

    let char_count = text.chars().count();

    // No splitting needed
    if max_chars == 0 || char_count <= max_chars {
        out.push(ASRSegment {
            start: seg.start,
            end: seg.end,
            text: text.to_string(),
        });
        return;
    }

    // Phase 1: Split text into sentences by punctuation
    let sentences = split_by_punctuation(text);
    if sentences.is_empty() {
        out.push(ASRSegment {
            start: seg.start,
            end: seg.end,
            text: text.to_string(),
        });
        return;
    }

    // Phase 2: Merge sentences into groups respecting max_chars
    let groups = merge_sentences(&sentences, max_chars);

    // Phase 3: Compute timestamps for each group
    let char_map = build_char_time_map(seg, char_count);

    for group in &groups {
        let group_text: String = text
            .chars()
            .skip(group.char_start)
            .take(group.char_end - group.char_start)
            .collect();

        let (start, end) = compute_group_time(seg, &char_map, group, char_count);

        if !group_text.trim().is_empty() {
            out.push(ASRSegment {
                start,
                end,
                text: group_text.trim().to_string(),
            });
        }
    }
}

/// Split text into sentences at punctuation boundaries.
/// Trailing punctuation is included in the current sentence.
/// Consecutive punctuation characters are grouped together.
fn split_by_punctuation(text: &str) -> Vec<Sentence> {
    let chars: Vec<char> = text.chars().collect();
    let mut sentences = Vec::new();
    let mut start = 0;
    let mut i = 0;

    while i < chars.len() {
        if PUNCTUATION.contains(&chars[i]) {
            // Consume all consecutive punctuation
            while i + 1 < chars.len() && PUNCTUATION.contains(&chars[i + 1]) {
                i += 1;
            }
            let end = i + 1;
            let sentence_text: String = chars[start..end].iter().collect();
            if !sentence_text.trim().is_empty() {
                sentences.push(Sentence {
                    text: sentence_text,
                    char_start: start,
                    char_end: end,
                });
            }
            start = end;
        }
        i += 1;
    }

    // Remaining text after last punctuation
    if start < chars.len() {
        let sentence_text: String = chars[start..].iter().collect();
        if !sentence_text.trim().is_empty() {
            sentences.push(Sentence {
                text: sentence_text,
                char_start: start,
                char_end: chars.len(),
            });
        }
    }

    sentences
}

/// Greedily merge sentences into groups respecting max_chars.
/// Each group contains at least one sentence, even if it exceeds max_chars.
fn merge_sentences(sentences: &[Sentence], max_chars: usize) -> Vec<Group> {
    let mut groups = Vec::new();
    let mut current_start = sentences[0].char_start;
    let mut current_len: usize = 0;

    for (i, sentence) in sentences.iter().enumerate() {
        let sentence_len = sentence.text.chars().count();

        if current_len > 0 && current_len + sentence_len > max_chars {
            // Close current group (up to previous sentence end)
            let prev_end = sentences[i - 1].char_end;
            groups.push(Group {
                char_start: current_start,
                char_end: prev_end,
            });
            current_start = sentence.char_start;
            current_len = sentence_len;
        } else {
            current_len += sentence_len;
        }
    }

    // Close last group
    if let Some(last) = sentences.last() {
        groups.push(Group {
            char_start: current_start,
            char_end: last.char_end,
        });
    }

    groups
}

/// Build a per-character time mapping from word-level timestamps.
/// Returns empty vec if words are not available.
fn build_char_time_map(seg: &RawASRSegment, total_chars: usize) -> Vec<CharTime> {
    let words = match &seg.words {
        Some(w) if !w.is_empty() => w,
        _ => return Vec::new(),
    };

    let mut char_map = Vec::with_capacity(total_chars);
    for word in words {
        let word_chars = word.text.chars().count();
        for _ in 0..word_chars {
            char_map.push(CharTime {
                start: word.start,
                end: word.end,
            });
        }
    }

    char_map
}

/// Compute start/end time for a group using char_map or linear interpolation.
fn compute_group_time(
    seg: &RawASRSegment,
    char_map: &[CharTime],
    group: &Group,
    total_chars: usize,
) -> (f64, f64) {
    if !char_map.is_empty() && group.char_end <= char_map.len() {
        // Use word-level timestamps
        let start = char_map[group.char_start].start;
        let end = char_map[group.char_end - 1].end;
        (start, end)
    } else {
        // Linear interpolation fallback
        let duration = seg.end - seg.start;
        let total = total_chars as f64;
        let start = seg.start + duration * (group.char_start as f64) / total;
        let end = seg.start + duration * (group.char_end as f64) / total;
        (start, end)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::asr::provider::ASRWord;

    fn make_raw(start: f64, end: f64, text: &str, words: Option<Vec<ASRWord>>) -> RawASRSegment {
        RawASRSegment {
            start,
            end,
            text: text.to_string(),
            words,
        }
    }

    #[test]
    fn test_no_split_when_disabled() {
        let raw = vec![make_raw(0.0, 5.0, "你好世界，这是一个测试", None)];
        let result = split_segments(&raw, 0);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].text, "你好世界，这是一个测试");
    }

    #[test]
    fn test_no_split_when_short() {
        let raw = vec![make_raw(0.0, 2.0, "你好", None)];
        let result = split_segments(&raw, 15);
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_split_by_punctuation() {
        let text = "效率，兄弟们，这把图的就是一个呃两个字，效率，效率还是效，哎，快快快快！";
        let raw = vec![make_raw(0.0, 10.0, text, None)];
        let result = split_segments(&raw, 15);
        assert!(result.len() >= 2, "Expected at least 2 segments, got {}", result.len());
        // All text should be preserved
        let combined: String = result.iter().map(|s| s.text.as_str()).collect::<Vec<_>>().join("");
        let original_no_space: String = text.chars().collect();
        assert_eq!(combined, original_no_space);
    }

    #[test]
    fn test_split_with_word_timestamps() {
        let words = vec![
            ASRWord { text: "你".into(), start: 0.0, end: 0.2 },
            ASRWord { text: "好".into(), start: 0.2, end: 0.4 },
            ASRWord { text: "，".into(), start: 0.4, end: 0.5 },
            ASRWord { text: "世".into(), start: 0.5, end: 0.7 },
            ASRWord { text: "界".into(), start: 0.7, end: 0.9 },
            ASRWord { text: "！".into(), start: 0.9, end: 1.0 },
        ];
        let raw = vec![make_raw(0.0, 1.0, "你好，世界！", Some(words))];
        let result = split_segments(&raw, 3);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].text, "你好，");
        assert_eq!(result[1].text, "世界！");
        // Check timestamps from word-level data
        assert!((result[0].start - 0.0).abs() < 0.001);
        assert!((result[0].end - 0.5).abs() < 0.001);
        assert!((result[1].start - 0.5).abs() < 0.001);
        assert!((result[1].end - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_single_sentence_exceeds_max() {
        // Single sentence without punctuation should not be split
        let raw = vec![make_raw(0.0, 5.0, "这是一个没有标点符号的很长的句子", None)];
        let result = split_segments(&raw, 5);
        assert_eq!(result.len(), 1, "Single sentence without punctuation should stay intact");
    }

    #[test]
    fn test_linear_interpolation_without_words() {
        let raw = vec![make_raw(0.0, 10.0, "你好，世界！", None)];
        let result = split_segments(&raw, 3);
        assert_eq!(result.len(), 2);
        // Linear interpolation: "你好，" is chars 0-3 out of 6
        assert!((result[0].start - 0.0).abs() < 0.001);
        assert!((result[0].end - 5.0).abs() < 0.001);
        assert!((result[1].start - 5.0).abs() < 0.001);
        assert!((result[1].end - 10.0).abs() < 0.001);
    }
}
