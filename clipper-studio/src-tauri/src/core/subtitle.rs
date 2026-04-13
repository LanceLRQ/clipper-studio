//! Subtitle ASS generation utilities
//!
//! Extracted from commands/asr.rs for reuse in the clip burning pipeline.

use std::path::Path;

use crate::asr::service::SubtitleSegment;
use crate::db::Database;

/// Format milliseconds to ASS time format: H:MM:SS.CC
pub fn format_ass_time(ms: i64) -> String {
    let total_secs = ms / 1000;
    let h = total_secs / 3600;
    let m = (total_secs % 3600) / 60;
    let s = total_secs % 60;
    let cs = (ms % 1000) / 10;
    format!("{}:{:02}:{:02}.{:02}", h, m, s, cs)
}

/// Generate ASS content from subtitle segments.
///
/// `base_ms` is the recorded_at Unix-approx milliseconds used to convert
/// absolute timestamps to video-relative timestamps.
pub fn generate_ass(segments: &[SubtitleSegment], base_ms: i64) -> String {
    let mut out = String::from(
        "[Script Info]\nTitle: ClipperStudio Export\nScriptType: v4.00+\nPlayResX: 1920\nPlayResY: 1080\n\n\
         [V4+ Styles]\nFormat: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding\n\
         Style: Default,Arial,48,&H00FFFFFF,&H000000FF,&H00000000,&H80000000,-1,0,0,0,100,100,0,0,1,2,1,2,10,10,10,1\n\n\
         [Events]\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\n",
    );
    for seg in segments {
        let start = seg.start_ms - base_ms;
        let end = seg.end_ms - base_ms;
        out.push_str(&format!(
            "Dialogue: 0,{},{},Default,,0,0,0,,{}\n",
            format_ass_time(start.max(0)),
            format_ass_time(end.max(0)),
            seg.text,
        ));
    }
    out
}

/// Get the base time (recorded_at as approx ms) for absolute→relative conversion.
pub async fn get_base_ms(db: &Database, video_id: i64) -> i64 {
    sea_orm::ConnectionTrait::query_one(
        db.conn(),
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            format!("SELECT recorded_at FROM videos WHERE id = {}", video_id),
        ),
    )
    .await
    .ok()
    .flatten()
    .and_then(|r| r.try_get::<String>("", "recorded_at").ok())
    .and_then(|ts| crate::asr::service::parse_recorded_at_to_unix_ms(&ts))
    .unwrap_or(0)
}

/// Format milliseconds to SRT time format: HH:MM:SS,mmm
fn format_srt_time(ms: i64) -> String {
    let total_secs = ms / 1000;
    let h = total_secs / 3600;
    let m = (total_secs % 3600) / 60;
    let s = total_secs % 60;
    let ms_part = ms % 1000;
    format!("{:02}:{:02}:{:02},{:03}", h, m, s, ms_part)
}

/// Export subtitle segments as SRT file for a specific clip time range.
///
/// Uses the same overlap/clamp logic as `export_ass_for_clip`:
/// segments partially overlapping with the clip range are included with
/// their start/end clamped to the clip boundaries.
/// Returns `Ok(true)` if segments were found and written.
pub async fn export_srt_for_clip(
    db: &Database,
    video_id: i64,
    clip_start_ms: i64,
    clip_end_ms: i64,
    output_path: &Path,
) -> Result<bool, String> {
    let segments = query_clip_segments(db, video_id, clip_start_ms, clip_end_ms).await?;

    if segments.is_empty() {
        return Ok(false);
    }

    let clip_duration = clip_end_ms - clip_start_ms;
    let mut out = String::new();
    for (i, seg) in segments.iter().enumerate() {
        let start = seg.start_ms.max(0);
        let end = seg.end_ms.min(clip_duration);
        out.push_str(&format!(
            "{}\n{} --> {}\n{}\n\n",
            i + 1,
            format_srt_time(start),
            format_srt_time(end),
            seg.text,
        ));
    }

    std::fs::write(output_path, &out)
        .map_err(|e| format!("Failed to write SRT file: {}", e))?;

    tracing::info!(
        "Exported {} subtitle segments as SRT for clip [{}-{}ms] to {}",
        segments.len(),
        clip_start_ms,
        clip_end_ms,
        output_path.display(),
    );

    Ok(true)
}

/// Query subtitle segments overlapping with a clip range, returning clip-relative timestamps.
async fn query_clip_segments(
    db: &Database,
    video_id: i64,
    clip_start_ms: i64,
    clip_end_ms: i64,
) -> Result<Vec<SubtitleSegment>, String> {
    let base_ms = get_base_ms(db, video_id).await;
    let abs_start = base_ms + clip_start_ms;
    let abs_end = base_ms + clip_end_ms;

    let rows = sea_orm::ConnectionTrait::query_all(
        db.conn(),
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            format!(
                "SELECT * FROM subtitle_segments \
                 WHERE video_id = {} AND end_ms > {} AND start_ms < {} \
                 ORDER BY start_ms ASC",
                video_id, abs_start, abs_end,
            ),
        ),
    )
    .await
    .map_err(|e| format!("Failed to query subtitles: {}", e))?;

    Ok(rows
        .iter()
        .map(|row| {
            let start_ms: i64 = row.try_get("", "start_ms").unwrap_or(0);
            let end_ms: i64 = row.try_get("", "end_ms").unwrap_or(0);
            SubtitleSegment {
                id: row.try_get("", "id").unwrap_or(0),
                video_id,
                language: row.try_get("", "language").unwrap_or_default(),
                start_ms: (start_ms - abs_start).max(0),
                end_ms: (end_ms - abs_start).min(clip_end_ms - clip_start_ms),
                text: row.try_get("", "text").unwrap_or_default(),
                source: row.try_get("", "source").unwrap_or_default(),
            }
        })
        .collect())
}

/// Export subtitle segments as ASS content for a specific clip time range.
///
/// - `clip_start_ms` / `clip_end_ms`: video-relative milliseconds (not absolute)
/// - The output ASS has timestamps starting at 0 (clip-relative).
/// - Returns `None` if no subtitle segments found in the range.
pub async fn export_ass_for_clip(
    db: &Database,
    video_id: i64,
    clip_start_ms: i64,
    clip_end_ms: i64,
    output_path: &Path,
) -> Result<bool, String> {
    let segments = query_clip_segments(db, video_id, clip_start_ms, clip_end_ms).await?;

    if segments.is_empty() {
        return Ok(false);
    }

    // Generate ASS with base_ms=0 since timestamps are already clip-relative
    let ass_content = generate_ass(&segments, 0);

    std::fs::write(output_path, &ass_content)
        .map_err(|e| format!("Failed to write ASS file: {}", e))?;

    tracing::info!(
        "Exported {} subtitle segments as ASS for clip [{}-{}ms] to {}",
        segments.len(),
        clip_start_ms,
        clip_end_ms,
        output_path.display(),
    );

    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ===== format_ass_time =====

    #[test]
    fn test_format_ass_time_zero() {
        assert_eq!(format_ass_time(0), "0:00:00.00");
    }

    #[test]
    fn test_format_ass_time_seconds_only() {
        assert_eq!(format_ass_time(1500), "0:00:01.50");
        assert_eq!(format_ass_time(10), "0:00:00.01"); // 1 centisecond
    }

    #[test]
    fn test_format_ass_time_minutes() {
        assert_eq!(format_ass_time(90000), "0:01:30.00");
    }

    #[test]
    fn test_format_ass_time_hours() {
        assert_eq!(format_ass_time(3661500), "1:01:01.50");
        assert_eq!(format_ass_time(3600000), "1:00:00.00");
    }

    #[test]
    fn test_format_ass_time_large_value() {
        // 100 hours
        assert_eq!(format_ass_time(360_000_000), "100:00:00.00");
    }

    // ===== generate_ass =====

    fn make_segment(start_ms: i64, end_ms: i64, text: &str) -> SubtitleSegment {
        SubtitleSegment {
            id: 0,
            video_id: 1,
            language: "zh".into(),
            start_ms,
            end_ms,
            text: text.into(),
            source: "asr".into(),
        }
    }

    #[test]
    fn test_generate_ass_empty_segments() {
        let segments: Vec<SubtitleSegment> = vec![];
        let result = generate_ass(&segments, 0);
        assert!(result.contains("[Script Info]"));
        assert!(result.contains("[Events]"));
        assert!(!result.contains("Dialogue:"));
    }

    #[test]
    fn test_generate_ass_single_segment() {
        let segments = vec![make_segment(1000, 3000, "Hello")];
        let result = generate_ass(&segments, 0);
        assert!(result.contains("Dialogue: 0,0:00:01.00,0:00:03.00,Default,,0,0,0,,Hello\n"));
    }

    #[test]
    fn test_generate_ass_with_base_offset() {
        // Absolute start=1h, base=1h → relative start=0
        let segments = vec![make_segment(3_600_000, 3_601_000, "offset test")];
        let result = generate_ass(&segments, 3_600_000);
        assert!(result.contains("Dialogue: 0,0:00:00.00,0:00:01.00,Default,,0,0,0,,offset test\n"));
    }

    #[test]
    fn test_generate_ass_clamps_negative_start() {
        // start 0.5s before base → clamped to 0
        let segments = vec![make_segment(500, 2000, "clamp test")];
        let result = generate_ass(&segments, 1000);
        assert!(result.contains("Dialogue: 0,0:00:00.00,0:00:01.00,Default,,0,0,0,,clamp test\n"));
    }

    #[test]
    fn test_generate_ass_multiple_segments() {
        let segments = vec![
            make_segment(1000, 3000, "first"),
            make_segment(4000, 6000, "second"),
        ];
        let result = generate_ass(&segments, 0);
        assert!(result.contains("Dialogue: 0,0:00:01.00,0:00:03.00,Default,,0,0,0,,first\n"));
        assert!(result.contains("Dialogue: 0,0:00:04.00,0:00:06.00,Default,,0,0,0,,second\n"));
    }
}
