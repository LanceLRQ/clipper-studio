//! Audio auto-segmentation
//!
//! Detects silence gaps in audio envelope data to automatically
//! split a video into segments for clip creation.

use crate::utils::ffmpeg::AudioEnvelope;

/// Parameters for audio auto-segmentation
#[derive(Debug, Clone, serde::Deserialize)]
pub struct SegmentParams {
    /// Volume threshold (0.0-1.0): values below this are considered silence
    #[serde(default = "default_threshold")]
    pub silence_threshold: f32,
    /// Minimum silence duration (ms) to trigger a split
    #[serde(default = "default_min_silence")]
    pub min_silence_ms: i64,
    /// Minimum segment duration (ms) — shorter segments are merged with neighbors
    #[serde(default = "default_min_segment")]
    pub min_segment_ms: i64,
}

fn default_threshold() -> f32 {
    0.05
}
fn default_min_silence() -> i64 {
    3000
}
fn default_min_segment() -> i64 {
    10000
}

impl Default for SegmentParams {
    fn default() -> Self {
        Self {
            silence_threshold: default_threshold(),
            min_silence_ms: default_min_silence(),
            min_segment_ms: default_min_segment(),
        }
    }
}

/// A detected segment (start_ms, end_ms) — video-relative
#[derive(Debug, Clone, serde::Serialize)]
pub struct DetectedSegment {
    pub start_ms: i64,
    pub end_ms: i64,
}

/// Detect segments by finding silence gaps in the audio envelope.
///
/// Algorithm:
/// 1. Mark windows below threshold as silent
/// 2. Group consecutive silent windows into silence regions
/// 3. Filter silence regions shorter than min_silence_ms
/// 4. Split at silence region midpoints
/// 5. Filter segments shorter than min_segment_ms (merge with neighbor)
pub fn detect_segments(envelope: &AudioEnvelope, params: &SegmentParams) -> Vec<DetectedSegment> {
    if envelope.values.is_empty() {
        return Vec::new();
    }

    let window_ms = envelope.window_ms as i64;
    let total_ms = envelope.values.len() as i64 * window_ms;

    // Step 1+2: Find silence regions (consecutive windows below threshold)
    let mut silence_regions: Vec<(i64, i64)> = Vec::new();
    let mut silence_start: Option<i64> = None;

    for (i, &val) in envelope.values.iter().enumerate() {
        let t = i as i64 * window_ms;
        if val < params.silence_threshold {
            if silence_start.is_none() {
                silence_start = Some(t);
            }
        } else if let Some(start) = silence_start.take() {
            silence_regions.push((start, t));
        }
    }
    // Close trailing silence
    if let Some(start) = silence_start {
        silence_regions.push((start, total_ms));
    }

    // Step 3: Filter short silence regions
    let silence_regions: Vec<(i64, i64)> = silence_regions
        .into_iter()
        .filter(|(s, e)| (e - s) >= params.min_silence_ms)
        .collect();

    if silence_regions.is_empty() {
        // No silence found — return the whole video as one segment
        return vec![DetectedSegment {
            start_ms: 0,
            end_ms: total_ms,
        }];
    }

    // Step 4: Split at silence midpoints
    let mut segments: Vec<DetectedSegment> = Vec::new();
    let mut seg_start: i64 = 0;

    for (sil_start, sil_end) in &silence_regions {
        let midpoint = (sil_start + sil_end) / 2;
        if midpoint > seg_start {
            segments.push(DetectedSegment {
                start_ms: seg_start,
                end_ms: midpoint,
            });
        }
        seg_start = midpoint;
    }
    // Last segment to end of video
    if seg_start < total_ms {
        segments.push(DetectedSegment {
            start_ms: seg_start,
            end_ms: total_ms,
        });
    }

    // Step 5: Merge short segments with neighbors
    let mut merged: Vec<DetectedSegment> = Vec::new();
    for seg in segments {
        if let Some(last) = merged.last_mut() {
            if (seg.end_ms - seg.start_ms) < params.min_segment_ms {
                // Merge into previous segment
                last.end_ms = seg.end_ms;
                continue;
            }
            // Also merge if previous segment is too short
            if (last.end_ms - last.start_ms) < params.min_segment_ms {
                last.end_ms = seg.end_ms;
                continue;
            }
        }
        merged.push(seg);
    }

    merged
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_envelope(values: Vec<f32>, window_ms: u32) -> AudioEnvelope {
        AudioEnvelope { window_ms, values }
    }

    #[test]
    fn test_no_silence() {
        let env = make_envelope(vec![0.5, 0.6, 0.7, 0.8, 0.9], 1000);
        let params = SegmentParams {
            silence_threshold: 0.05,
            min_silence_ms: 2000,
            min_segment_ms: 1000,
        };
        let segs = detect_segments(&env, &params);
        assert_eq!(segs.len(), 1);
        assert_eq!(segs[0].start_ms, 0);
        assert_eq!(segs[0].end_ms, 5000);
    }

    #[test]
    fn test_one_silence_gap() {
        // 3 loud, 3 silent, 3 loud  (each 1000ms)
        let env = make_envelope(vec![0.5, 0.6, 0.5, 0.01, 0.01, 0.01, 0.5, 0.6, 0.5], 1000);
        let params = SegmentParams {
            silence_threshold: 0.05,
            min_silence_ms: 2000,
            min_segment_ms: 1000,
        };
        let segs = detect_segments(&env, &params);
        assert_eq!(segs.len(), 2);
        assert_eq!(segs[0].start_ms, 0);
        // Midpoint of silence [3000, 6000] = 4500
        assert_eq!(segs[0].end_ms, 4500);
        assert_eq!(segs[1].start_ms, 4500);
        assert_eq!(segs[1].end_ms, 9000);
    }

    #[test]
    fn test_short_silence_ignored() {
        // 1 silent window (1000ms) — shorter than min_silence_ms (2000ms)
        let env = make_envelope(vec![0.5, 0.01, 0.5], 1000);
        let params = SegmentParams {
            silence_threshold: 0.05,
            min_silence_ms: 2000,
            min_segment_ms: 1000,
        };
        let segs = detect_segments(&env, &params);
        assert_eq!(segs.len(), 1);
    }

    #[test]
    fn test_empty_envelope() {
        let env = make_envelope(vec![], 1000);
        let segs = detect_segments(&env, &SegmentParams::default());
        assert!(segs.is_empty());
    }
}
