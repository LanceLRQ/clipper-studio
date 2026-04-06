use std::path::Path;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio_util::sync::CancellationToken;

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
fn resolve_video_codec(ffmpeg_path: &str, codec_hint: &str) -> String {
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
