use std::path::{Path, PathBuf};
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio_util::sync::CancellationToken;

use crate::utils::ffmpeg;

/// FFmpeg progress info parsed from stderr
#[derive(Debug, Clone, serde::Serialize)]
pub struct ClipProgress {
    /// Estimated progress 0.0 ~ 1.0
    pub progress: f64,
    /// Current processing time in seconds
    pub time_secs: f64,
    /// Speed multiplier (e.g. 2.5x)
    pub speed: Option<f64>,
}

/// Encoding preset options
#[derive(Debug, Clone, serde::Deserialize)]
pub struct PresetOptions {
    pub codec: String,
    #[serde(default)]
    pub crf: Option<u32>,
    #[serde(default)]
    pub audio_only: Option<bool>,
}

/// Execute a clip operation using FFmpeg.
///
/// Supports:
/// - Stream copy (codec = "copy"): fastest, no re-encoding
/// - Re-encode (codec = "auto" / "h264" / "h265"): with CRF quality control
/// - Audio extract (audio_only = true): extract audio track only
///
/// Reports progress via the callback. Respects cancellation token.
pub async fn execute_clip(
    ffmpeg_path: &str,
    input: &Path,
    output: &Path,
    start_ms: i64,
    end_ms: i64,
    preset: &PresetOptions,
    cancel: CancellationToken,
    on_progress: impl Fn(ClipProgress) + Send + 'static,
) -> Result<(), String> {
    if ffmpeg_path.is_empty() {
        return Err("FFmpeg not available".to_string());
    }

    let duration_ms = end_ms - start_ms;
    if duration_ms <= 0 {
        return Err("Invalid time range: end must be after start".to_string());
    }

    // Ensure output directory exists
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create output directory: {}", e))?;
    }

    let duration_secs = duration_ms as f64 / 1000.0;
    let start_secs = start_ms as f64 / 1000.0;

    let mut args: Vec<String> = Vec::new();

    // Input seeking (before -i for fast seek)
    args.extend(["-ss".to_string(), format!("{:.3}", start_secs)]);
    args.extend(["-i".to_string(), input.to_string_lossy().to_string()]);
    // Duration
    args.extend(["-t".to_string(), format!("{:.3}", duration_secs)]);

    if preset.audio_only.unwrap_or(false) {
        // Audio extraction
        args.extend(["-vn".to_string(), "-acodec".to_string(), "aac".to_string()]);
    } else if preset.codec == "copy" {
        // Stream copy (fastest)
        args.extend(["-c".to_string(), "copy".to_string()]);
    } else {
        // Re-encode with specified codec
        let video_codec = resolve_video_codec(ffmpeg_path, &preset.codec);
        args.extend(["-c:v".to_string(), video_codec]);
        args.extend(["-c:a".to_string(), "aac".to_string()]);

        if let Some(crf) = preset.crf {
            args.extend(["-crf".to_string(), crf.to_string()]);
        }
    }

    // Overwrite output, progress output
    args.extend([
        "-y".to_string(),
        "-progress".to_string(),
        "pipe:1".to_string(),
        output.to_string_lossy().to_string(),
    ]);

    tracing::info!("FFmpeg clip: {} {:?}", ffmpeg_path, args);

    let mut child = Command::new(ffmpeg_path)
        .args(&args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to start FFmpeg: {}", e))?;

    let stdout = child.stdout.take().unwrap();
    let mut reader = BufReader::new(stdout).lines();

    let mut current_speed: Option<f64> = None;

    // Parse FFmpeg progress output
    loop {
        tokio::select! {
            _ = cancel.cancelled() => {
                let _ = child.kill().await;
                return Err("Task cancelled".to_string());
            }
            line = reader.next_line() => {
                match line {
                    Ok(Some(line)) => {
                        if let Some(time_str) = line.strip_prefix("out_time_us=") {
                            if let Ok(us) = time_str.trim().parse::<i64>() {
                                let time_secs = us as f64 / 1_000_000.0;
                                let progress = (time_secs / duration_secs).min(1.0).max(0.0);
                                on_progress(ClipProgress {
                                    progress,
                                    time_secs,
                                    speed: current_speed,
                                });
                            }
                        } else if let Some(speed_str) = line.strip_prefix("speed=") {
                            let cleaned = speed_str.trim().trim_end_matches('x');
                            current_speed = cleaned.parse::<f64>().ok();
                        } else if line.starts_with("progress=end") {
                            on_progress(ClipProgress {
                                progress: 1.0,
                                time_secs: duration_secs,
                                speed: current_speed,
                            });
                            break;
                        }
                    }
                    Ok(None) => break, // EOF
                    Err(e) => {
                        tracing::warn!("Failed to read FFmpeg output: {}", e);
                        break;
                    }
                }
            }
        }
    }

    let status = child
        .wait()
        .await
        .map_err(|e| format!("FFmpeg process error: {}", e))?;

    if !status.success() {
        return Err(format!("FFmpeg exited with code: {}", status));
    }

    // Verify output exists
    if !output.exists() {
        return Err("FFmpeg completed but output file not found".to_string());
    }

    tracing::info!("Clip completed: {}", output.display());
    Ok(())
}

/// Detect available encoders and select the best video codec.
/// Priority: hardware (NVENC/VideoToolbox/QSV) > software (libx264)
pub fn resolve_video_codec(ffmpeg_path: &str, codec_hint: &str) -> String {
    match codec_hint {
        "h264" | "auto" => detect_best_h264(ffmpeg_path),
        "h265" | "hevc" => detect_best_h265(ffmpeg_path),
        other => other.to_string(),
    }
}

fn detect_best_h264(ffmpeg_path: &str) -> String {
    let candidates = [
        "h264_videotoolbox", // macOS
        "h264_nvenc",        // NVIDIA
        "h264_qsv",         // Intel
        "h264_amf",         // AMD
    ];
    for c in candidates {
        if encoder_available(ffmpeg_path, c) {
            tracing::info!("Using hardware H.264 encoder: {}", c);
            return c.to_string();
        }
    }
    "libx264".to_string()
}

fn detect_best_h265(ffmpeg_path: &str) -> String {
    let candidates = [
        "hevc_videotoolbox",
        "hevc_nvenc",
        "hevc_qsv",
        "hevc_amf",
    ];
    for c in candidates {
        if encoder_available(ffmpeg_path, c) {
            tracing::info!("Using hardware H.265 encoder: {}", c);
            return c.to_string();
        }
    }
    "libx265".to_string()
}

/// Check if a specific encoder is available
fn encoder_available(ffmpeg_path: &str, encoder: &str) -> bool {
    std::process::Command::new(ffmpeg_path)
        .args(["-hide_banner", "-encoders"])
        .output()
        .ok()
        .map(|output| {
            String::from_utf8_lossy(&output.stdout).contains(encoder)
        })
        .unwrap_or(false)
}

// ====== Two-pass Clip + Burn Pipeline ======

/// Options for subtitle/danmaku burning after clipping
#[derive(Debug, Clone)]
pub struct BurnOptions {
    /// Burn danmaku overlay
    pub burn_danmaku: bool,
    /// Burn subtitle overlay
    pub burn_subtitle: bool,
    /// Path to generated danmaku ASS file (clip-relative timestamps)
    pub danmaku_ass_path: Option<PathBuf>,
    /// Path to generated subtitle ASS file (clip-relative timestamps)
    pub subtitle_ass_path: Option<PathBuf>,
    /// Video codec for the burn pass (e.g. "auto", "h264", "h265")
    pub burn_codec: String,
    /// CRF quality for the burn pass
    pub burn_crf: Option<u32>,
}

impl BurnOptions {
    /// Returns true if any burning is requested and has a valid ASS file
    pub fn needs_burn(&self) -> bool {
        (self.burn_danmaku && self.danmaku_ass_path.is_some())
            || (self.burn_subtitle && self.subtitle_ass_path.is_some())
    }
}

/// Execute a clip operation with optional subtitle/danmaku burning.
///
/// Two-pass pipeline:
/// 1. **Pass 1 (Clip)**: Extract the time range from the source video (can use stream copy)
/// 2. **Pass 2 (Burn)**: If burning is requested, burn ASS overlay into the clipped video
///
/// Progress is split: Pass 1 = 0%~40%, Pass 2 = 40%~100%.
/// If no burning is needed, Pass 1 uses the full 0%~100% range.
pub async fn execute_clip_with_burn(
    ffmpeg_path: &str,
    input: &Path,
    output: &Path,
    start_ms: i64,
    end_ms: i64,
    preset: &PresetOptions,
    burn: &BurnOptions,
    cancel: CancellationToken,
    on_progress: impl Fn(ClipProgress) + Send + Clone + 'static,
) -> Result<(), String> {
    let needs_burn = burn.needs_burn();

    if !needs_burn {
        // No burning — just do a regular clip with full progress range
        return execute_clip(ffmpeg_path, input, output, start_ms, end_ms, preset, cancel, on_progress).await;
    }

    // === Pass 1: Clip to intermediate file ===
    let intermediate = output.with_extension("_tmp.mp4");
    let on_progress_p1 = on_progress.clone();

    tracing::info!("Clip+Burn Pass 1: clipping to intermediate {}", intermediate.display());

    execute_clip(
        ffmpeg_path,
        input,
        &intermediate,
        start_ms,
        end_ms,
        preset,
        cancel.clone(),
        move |p| {
            // Map pass 1 progress to 0% ~ 40%
            on_progress_p1(ClipProgress {
                progress: p.progress * 0.4,
                time_secs: p.time_secs,
                speed: p.speed,
            });
        },
    )
    .await
    .map_err(|e| {
        let _ = std::fs::remove_file(&intermediate);
        e
    })?;

    // === Merge ASS files if both danmaku and subtitle are requested ===
    let merged_ass = if burn.burn_danmaku && burn.burn_subtitle
        && burn.danmaku_ass_path.is_some()
        && burn.subtitle_ass_path.is_some()
    {
        let merged_path = std::env::temp_dir().join(format!(
            "clipper_merged_{}.ass",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis()
        ));
        merge_ass_files(
            burn.danmaku_ass_path.as_ref().unwrap(),
            burn.subtitle_ass_path.as_ref().unwrap(),
            &merged_path,
        )?;
        Some(merged_path)
    } else {
        None
    };

    // Determine which ASS file to burn
    let ass_to_burn = if let Some(ref merged) = merged_ass {
        merged.clone()
    } else if burn.burn_danmaku && burn.danmaku_ass_path.is_some() {
        burn.danmaku_ass_path.clone().unwrap()
    } else if burn.burn_subtitle && burn.subtitle_ass_path.is_some() {
        burn.subtitle_ass_path.clone().unwrap()
    } else {
        // No ASS file available to burn — skip burn pass, just rename intermediate to output
        tracing::warn!("No ASS file available for burning, skipping burn pass");
        std::fs::rename(&intermediate, output)
            .map_err(|e| format!("Failed to rename intermediate file: {}", e))?;
        return Ok(());
    };

    // === Pass 2: Burn ASS into clipped video ===
    tracing::info!("Clip+Burn Pass 2: burning {} into {}", ass_to_burn.display(), output.display());

    let burn_result = ffmpeg::burn_subtitle_with_progress(
        ffmpeg_path,
        &intermediate,
        &ass_to_burn,
        output,
        &burn.burn_codec,
        burn.burn_crf,
        cancel,
        move |p| {
            // Map pass 2 progress to 40% ~ 100%
            on_progress(ClipProgress {
                progress: 0.4 + p.progress * 0.6,
                time_secs: p.time_secs,
                speed: p.speed,
            });
        },
    )
    .await;

    // Cleanup temp files
    let _ = std::fs::remove_file(&intermediate);
    if let Some(ref merged) = merged_ass {
        let _ = std::fs::remove_file(merged);
    }

    burn_result
}

/// Merge two ASS files by appending subtitle events from the second file
/// into the first file's event section.
///
/// The danmaku ASS (from DanmakuFactory) is used as base since it has
/// proper styles for scrolling text. Subtitle events are appended with
/// the "Default" style from a separate style definition.
fn merge_ass_files(
    danmaku_ass: &Path,
    subtitle_ass: &Path,
    output: &Path,
) -> Result<(), String> {
    let danmaku_content = std::fs::read_to_string(danmaku_ass)
        .map_err(|e| format!("Failed to read danmaku ASS: {}", e))?;
    let subtitle_content = std::fs::read_to_string(subtitle_ass)
        .map_err(|e| format!("Failed to read subtitle ASS: {}", e))?;

    let mut merged = danmaku_content.clone();

    // Add a subtitle style to the danmaku ASS if not already present
    if !merged.contains("Style: Subtitle,") {
        // Insert subtitle style before [Events] section
        if let Some(pos) = merged.find("[Events]") {
            let subtitle_style = "Style: Subtitle,Arial,48,&H00FFFFFF,&H000000FF,&H00000000,&H80000000,-1,0,0,0,100,100,0,0,1,2,1,2,10,10,10,1\n";
            merged.insert_str(pos, subtitle_style);
        }
    }

    // Extract Dialogue lines from subtitle ASS and append with "Subtitle" style
    for line in subtitle_content.lines() {
        if line.starts_with("Dialogue:") {
            // Replace "Default" style with "Subtitle" style
            let modified = line.replacen(",Default,", ",Subtitle,", 1);
            merged.push_str(&modified);
            merged.push('\n');
        }
    }

    std::fs::write(output, &merged)
        .map_err(|e| format!("Failed to write merged ASS: {}", e))?;

    tracing::info!("Merged ASS files into {}", output.display());
    Ok(())
}
