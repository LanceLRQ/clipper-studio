use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Mutex, OnceLock};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio_util::sync::CancellationToken;

use crate::utils::ffmpeg;

/// FFmpeg 进度事件节流间隔（ms）——避免 30fps 进度 spam 导致前端重渲染卡顿
pub(crate) const PROGRESS_EMIT_INTERVAL_MS: u64 = 200;

/// Graceful FFmpeg shutdown: send SIGTERM first, wait 3s, then SIGKILL.
///
/// On Unix, tokio's `kill()` sends SIGKILL immediately which prevents
/// FFmpeg from finalizing output (potentially leaving corrupt files).
/// Sending SIGTERM first gives FFmpeg a chance to write MP4 moov atom.
pub(crate) async fn graceful_kill_child(child: &mut tokio::process::Child) {
    #[cfg(unix)]
    {
        // Send SIGTERM via PID
        if let Some(id) = child.id() {
            unsafe {
                libc::kill(id as i32, libc::SIGTERM);
            }
        }
        // Wait up to 3s for graceful exit
        if let Ok(Ok(_)) =
            tokio::time::timeout(std::time::Duration::from_secs(3), child.wait()).await
        {
            return;
        }
        // Timed out or error, fall through to SIGKILL
    }
    let _ = child.kill().await;
    let _ = child.wait().await;
}

/// 按 ffmpeg 可执行路径缓存 `-encoders` 输出，避免 clip 时重复启动子进程
/// key: ffmpeg_path, value: encoder 名称集合（stdout 文本）
fn encoders_cache() -> &'static Mutex<HashMap<String, String>> {
    static CACHE: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

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
#[allow(clippy::too_many_arguments)]
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

    // Concurrently consume stderr to prevent pipe buffer deadlock
    // 使用 read_stderr_capped 避免长时间编码累积数十 MB 日志
    let stderr = child.stderr.take();
    let stderr_task = tokio::spawn(async move {
        if let Some(stderr) = stderr {
            ffmpeg::read_stderr_capped(stderr).await
        } else {
            Vec::new()
        }
    });

    let mut current_speed: Option<f64> = None;
    // 进度节流：FFmpeg stdout 每帧输出一次（~30/s），每 200ms 最多发送一次事件
    let mut last_emit = std::time::Instant::now()
        .checked_sub(std::time::Duration::from_millis(PROGRESS_EMIT_INTERVAL_MS))
        .unwrap_or_else(std::time::Instant::now);

    // Parse FFmpeg progress output
    loop {
        tokio::select! {
            _ = cancel.cancelled() => {
                graceful_kill_child(&mut child).await;
                return Err("Task cancelled".to_string());
            }
            line = reader.next_line() => {
                match line {
                    Ok(Some(line)) => {
                        if let Some(time_str) = line.strip_prefix("out_time_us=") {
                            if let Ok(us) = time_str.trim().parse::<i64>() {
                                let time_secs = us as f64 / 1_000_000.0;
                                let progress = (time_secs / duration_secs).clamp(0.0, 1.0);
                                if last_emit.elapsed()
                                    >= std::time::Duration::from_millis(PROGRESS_EMIT_INTERVAL_MS)
                                {
                                    on_progress(ClipProgress {
                                        progress,
                                        time_secs,
                                        speed: current_speed,
                                    });
                                    last_emit = std::time::Instant::now();
                                }
                            }
                        } else if let Some(speed_str) = line.strip_prefix("speed=") {
                            let cleaned = speed_str.trim().trim_end_matches('x');
                            current_speed = cleaned.parse::<f64>().ok();
                        } else if line.starts_with("progress=end") {
                            // 结束事件必发，忽略节流窗口
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

    let stderr_output = stderr_task.await.unwrap_or_default();

    let status = child
        .wait()
        .await
        .map_err(|e| format!("FFmpeg process error: {}", e))?;

    if !status.success() {
        let stderr_str = String::from_utf8_lossy(&stderr_output);
        return Err(format!(
            "FFmpeg exited with code: {} (stderr: {})",
            status,
            stderr_str.chars().take(500).collect::<String>()
        ));
    }

    // Verify output exists
    if !output.exists() {
        return Err("FFmpeg completed but output file not found".to_string());
    }

    tracing::info!("Clip completed: {}", output.display());
    Ok(())
}

/// 根据视频编码器类型，将 CRF 质量值转换为对应编码器的正确参数并追加到 `args`。
///
/// 背景（P4-COMPAT-18）：硬件编码器（VideoToolbox / NVENC / QSV / AMF）不支持
/// `-crf`，各自使用不同的质量控制参数。若所有编码器都无条件使用 `-crf`，
/// 硬件编码路径下参数会被忽略甚至导致报错，产出视频质量不可控。
///
/// - `videotoolbox` — 无等价参数，跳过（使用默认质量）
/// - `nvenc`        — `-cq <val> -b:v 0`（0 比特率强制恒定质量模式）
/// - `qsv`          — `-global_quality <val>`
/// - `amf`          — `-q:v <val>`
/// - 软件编码器    — `-crf <val>`
pub fn apply_quality_args(video_codec: &str, crf: Option<u32>, args: &mut Vec<String>) {
    let Some(crf_val) = crf else { return };
    if video_codec.contains("videotoolbox") {
        tracing::debug!("VideoToolbox encoder: skipping quality param, using default");
    } else if video_codec.contains("nvenc") {
        args.extend(["-cq".to_string(), crf_val.to_string()]);
        args.extend(["-b:v".to_string(), "0".to_string()]);
    } else if video_codec.contains("qsv") {
        args.extend(["-global_quality".to_string(), crf_val.to_string()]);
    } else if video_codec.contains("amf") {
        args.extend(["-q:v".to_string(), crf_val.to_string()]);
    } else {
        args.extend(["-crf".to_string(), crf_val.to_string()]);
    }
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
        "h264_qsv",          // Intel
        "h264_amf",          // AMD
    ];
    for c in candidates {
        if encoder_available(ffmpeg_path, c) && encoder_really_available(ffmpeg_path, c) {
            tracing::info!("Using hardware H.264 encoder: {}", c);
            return c.to_string();
        }
    }
    tracing::info!("No hardware H.264 encoder available, falling back to libx264");
    "libx264".to_string()
}

fn detect_best_h265(ffmpeg_path: &str) -> String {
    let candidates = ["hevc_videotoolbox", "hevc_nvenc", "hevc_qsv", "hevc_amf"];
    for c in candidates {
        if encoder_available(ffmpeg_path, c) && encoder_really_available(ffmpeg_path, c) {
            tracing::info!("Using hardware H.265 encoder: {}", c);
            return c.to_string();
        }
    }
    tracing::info!("No hardware H.265 encoder available, falling back to libx265");
    "libx265".to_string()
}

/// Check if a specific encoder is available.
///
/// 首次调用时执行 `ffmpeg -encoders` 并按 `ffmpeg_path` 缓存输出，
/// 后续调用直接查缓存，避免每次 clip 启动 4+ 次 ffmpeg 子进程。
fn encoder_available(ffmpeg_path: &str, encoder: &str) -> bool {
    let cache = encoders_cache();

    if let Ok(guard) = cache.lock() {
        if let Some(text) = guard.get(ffmpeg_path) {
            return text.contains(encoder);
        }
    }

    let output = match std::process::Command::new(ffmpeg_path)
        .args(["-hide_banner", "-encoders"])
        .output()
    {
        Ok(o) => o,
        Err(e) => {
            tracing::warn!("ffmpeg -encoders failed ({}): {}", ffmpeg_path, e);
            return false;
        }
    };

    let text = String::from_utf8_lossy(&output.stdout).into_owned();
    let contains = text.contains(encoder);

    if let Ok(mut guard) = cache.lock() {
        guard.insert(ffmpeg_path.to_string(), text);
    }

    contains
}

/// Cache for hardware encoder test-encode results (key: "ffmpeg_path:encoder")
fn encoders_verified() -> &'static std::sync::Mutex<std::collections::HashMap<String, bool>> {
    static MAP: OnceLock<std::sync::Mutex<std::collections::HashMap<String, bool>>> =
        OnceLock::new();
    MAP.get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()))
}

/// Verify a hardware encoder can actually produce output.
///
/// FFmpeg may list hardware encoders in `-encoders` even when the required
/// hardware/driver is absent (e.g., NVENC without NVIDIA GPU, VideoToolbox
/// in headless environments). This runs a minimal test encode and caches
/// the result to avoid re-checking on every clip.
fn encoder_really_available(ffmpeg_path: &str, encoder: &str) -> bool {
    let key = format!("{}:{}", ffmpeg_path, encoder);

    if let Ok(guard) = encoders_verified().lock() {
        if let Some(&result) = guard.get(&key) {
            return result;
        }
    }

    let result = std::process::Command::new(ffmpeg_path)
        .args([
            "-hide_banner",
            "-f",
            "lavfi",
            "-i",
            "color=c=black:s=64x64:d=0.1",
            "-c:v",
            encoder,
            "-frames:v",
            "1",
            "-f",
            "null",
            "-",
        ])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    if !result {
        tracing::info!(
            "Encoder {} listed but hardware unavailable, falling back",
            encoder
        );
    }

    if let Ok(mut guard) = encoders_verified().lock() {
        guard.insert(key, result);
    }

    result
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
#[allow(clippy::too_many_arguments)]
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
        return execute_clip(
            ffmpeg_path,
            input,
            output,
            start_ms,
            end_ms,
            preset,
            cancel,
            on_progress,
        )
        .await;
    }

    // === Pass 1: Clip to intermediate file ===
    let intermediate = output.with_extension("_tmp.mp4");
    let on_progress_p1 = on_progress.clone();

    tracing::info!(
        "Clip+Burn Pass 1: clipping to intermediate {}",
        intermediate.display()
    );

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
    .inspect_err(|_e| {
        let _ = std::fs::remove_file(&intermediate);
    })?;

    // === Merge ASS files if both danmaku and subtitle are requested ===
    let merged_ass = match (
        burn.burn_danmaku && burn.burn_subtitle,
        burn.danmaku_ass_path.as_ref(),
        burn.subtitle_ass_path.as_ref(),
    ) {
        (true, Some(danmaku_path), Some(subtitle_path)) => {
            let merged_path = std::env::temp_dir().join(format!(
                "clipper_merged_{}_{}.ass",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_nanos()
            ));
            merge_ass_files(danmaku_path, subtitle_path, &merged_path).await?;
            Some(merged_path)
        }
        _ => None,
    };

    // Determine which ASS file to burn
    let ass_to_burn = if let Some(ref merged) = merged_ass {
        merged.clone()
    } else if let (true, Some(path)) = (burn.burn_danmaku, burn.danmaku_ass_path.as_ref()) {
        path.clone()
    } else if let (true, Some(path)) = (burn.burn_subtitle, burn.subtitle_ass_path.as_ref()) {
        path.clone()
    } else {
        // No ASS file available to burn — skip burn pass, just rename intermediate to output
        tracing::warn!("No ASS file available for burning, skipping burn pass");
        std::fs::rename(&intermediate, output)
            .map_err(|e| format!("Failed to rename intermediate file: {}", e))?;
        return Ok(());
    };

    // === Pass 2: Burn ASS into clipped video ===
    tracing::info!(
        "Clip+Burn Pass 2: burning {} into {}",
        ass_to_burn.display(),
        output.display()
    );

    // Use clip duration as fallback when ffprobe fails on the intermediate file
    let clip_duration_hint = (end_ms - start_ms) as f64 / 1000.0;
    let burn_result = ffmpeg::burn_subtitle_with_progress(
        ffmpeg_path,
        &intermediate,
        &ass_to_burn,
        output,
        &burn.burn_codec,
        burn.burn_crf,
        Some(clip_duration_hint),
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
async fn merge_ass_files(
    danmaku_ass: &Path,
    subtitle_ass: &Path,
    output: &Path,
) -> Result<(), String> {
    let danmaku_content = tokio::fs::read_to_string(danmaku_ass).await.map_err(|e| {
        format!(
            "Failed to read danmaku ASS {}: {}",
            danmaku_ass.display(),
            e
        )
    })?;
    let subtitle_content = tokio::fs::read_to_string(subtitle_ass).await.map_err(|e| {
        format!(
            "Failed to read subtitle ASS {}: {}",
            subtitle_ass.display(),
            e
        )
    })?;

    let mut merged = danmaku_content;

    // Add a subtitle style to the danmaku ASS if not already present
    if !merged.contains("Style: Subtitle,") {
        // Insert subtitle style before [Events] section
        if let Some(pos) = merged.find("[Events]") {
            let font = crate::core::subtitle::default_cjk_font();
            let subtitle_style = format!("Style: Subtitle,{font},48,&H00FFFFFF,&H000000FF,&H00000000,&H80000000,-1,0,0,0,100,100,0,0,1,2,1,2,10,10,10,1\n");
            merged.insert_str(pos, &subtitle_style);
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

    tokio::fs::write(output, &merged)
        .await
        .map_err(|e| format!("Failed to write merged ASS: {}", e))?;

    tracing::info!("Merged ASS files into {}", output.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // ==================== apply_quality_args ====================

    #[test]
    fn test_apply_quality_args_videotoolbox_skipped() {
        let mut args: Vec<String> = Vec::new();
        apply_quality_args("h264_videotoolbox", Some(23), &mut args);
        assert!(
            args.is_empty(),
            "VideoToolbox should not append any quality args"
        );
    }

    #[test]
    fn test_apply_quality_args_nvenc() {
        let mut args: Vec<String> = Vec::new();
        apply_quality_args("h264_nvenc", Some(20), &mut args);
        assert_eq!(
            args,
            vec![
                "-cq".to_string(),
                "20".to_string(),
                "-b:v".to_string(),
                "0".to_string()
            ]
        );
    }

    #[test]
    fn test_apply_quality_args_qsv() {
        let mut args: Vec<String> = Vec::new();
        apply_quality_args("hevc_qsv", Some(24), &mut args);
        assert_eq!(args, vec!["-global_quality".to_string(), "24".to_string()]);
    }

    #[test]
    fn test_apply_quality_args_amf() {
        let mut args: Vec<String> = Vec::new();
        apply_quality_args("h264_amf", Some(28), &mut args);
        assert_eq!(args, vec!["-q:v".to_string(), "28".to_string()]);
    }

    #[test]
    fn test_apply_quality_args_software_libx264() {
        let mut args: Vec<String> = Vec::new();
        apply_quality_args("libx264", Some(23), &mut args);
        assert_eq!(args, vec!["-crf".to_string(), "23".to_string()]);
    }

    #[test]
    fn test_apply_quality_args_software_libx265() {
        let mut args: Vec<String> = Vec::new();
        apply_quality_args("libx265", Some(28), &mut args);
        assert_eq!(args, vec!["-crf".to_string(), "28".to_string()]);
    }

    #[test]
    fn test_apply_quality_args_none_crf_no_args() {
        let mut args: Vec<String> = Vec::new();
        apply_quality_args("libx264", None, &mut args);
        assert!(args.is_empty(), "None crf should not append any args");
    }

    #[test]
    fn test_apply_quality_args_preserves_existing_args() {
        let mut args: Vec<String> = vec!["-i".to_string(), "input.mp4".to_string()];
        apply_quality_args("libx264", Some(20), &mut args);
        assert_eq!(
            args,
            vec![
                "-i".to_string(),
                "input.mp4".to_string(),
                "-crf".to_string(),
                "20".to_string()
            ]
        );
    }

    // ==================== resolve_video_codec (passthrough branch) ====================

    #[test]
    fn test_resolve_video_codec_passthrough_unknown() {
        // Unknown codec hint should pass through unchanged (no FFmpeg call)
        let result = resolve_video_codec("/nonexistent/ffmpeg", "libsvtav1");
        assert_eq!(result, "libsvtav1");
    }

    #[test]
    fn test_resolve_video_codec_passthrough_copy() {
        let result = resolve_video_codec("/nonexistent/ffmpeg", "copy");
        assert_eq!(result, "copy");
    }

    #[test]
    fn test_resolve_video_codec_h264_falls_back_when_ffmpeg_missing() {
        // With nonexistent ffmpeg, hardware probe fails, expect software fallback
        let result = resolve_video_codec("/nonexistent/ffmpeg", "h264");
        assert_eq!(result, "libx264");
    }

    #[test]
    fn test_resolve_video_codec_h265_falls_back_when_ffmpeg_missing() {
        let result = resolve_video_codec("/nonexistent/ffmpeg", "h265");
        assert_eq!(result, "libx265");
    }

    #[test]
    fn test_resolve_video_codec_hevc_alias_falls_back() {
        let result = resolve_video_codec("/nonexistent/ffmpeg", "hevc");
        assert_eq!(result, "libx265");
    }

    // ==================== BurnOptions::needs_burn ====================

    fn make_burn_options(
        burn_danmaku: bool,
        burn_subtitle: bool,
        danmaku_path: Option<PathBuf>,
        subtitle_path: Option<PathBuf>,
    ) -> BurnOptions {
        BurnOptions {
            burn_danmaku,
            burn_subtitle,
            danmaku_ass_path: danmaku_path,
            subtitle_ass_path: subtitle_path,
            burn_codec: "auto".to_string(),
            burn_crf: Some(23),
        }
    }

    #[test]
    fn test_needs_burn_both_disabled() {
        let opts = make_burn_options(false, false, None, None);
        assert!(!opts.needs_burn());
    }

    #[test]
    fn test_needs_burn_danmaku_with_path() {
        let opts = make_burn_options(true, false, Some(PathBuf::from("/tmp/d.ass")), None);
        assert!(opts.needs_burn());
    }

    #[test]
    fn test_needs_burn_danmaku_without_path() {
        let opts = make_burn_options(true, false, None, None);
        assert!(
            !opts.needs_burn(),
            "burn flag without ASS path should not trigger burn"
        );
    }

    #[test]
    fn test_needs_burn_subtitle_with_path() {
        let opts = make_burn_options(false, true, None, Some(PathBuf::from("/tmp/s.ass")));
        assert!(opts.needs_burn());
    }

    #[test]
    fn test_needs_burn_subtitle_without_path() {
        let opts = make_burn_options(false, true, None, None);
        assert!(!opts.needs_burn());
    }

    #[test]
    fn test_needs_burn_both_enabled_both_paths() {
        let opts = make_burn_options(
            true,
            true,
            Some(PathBuf::from("/tmp/d.ass")),
            Some(PathBuf::from("/tmp/s.ass")),
        );
        assert!(opts.needs_burn());
    }

    #[test]
    fn test_needs_burn_path_present_but_flag_off() {
        // ASS path present but burn flag off — should not burn
        let opts = make_burn_options(false, false, Some(PathBuf::from("/tmp/d.ass")), None);
        assert!(!opts.needs_burn());
    }

    // ==================== PresetOptions deserialization ====================

    #[test]
    fn test_preset_options_deserialize_minimal() {
        let json = r#"{"codec":"copy"}"#;
        let opts: PresetOptions = serde_json::from_str(json).unwrap();
        assert_eq!(opts.codec, "copy");
        assert!(opts.crf.is_none());
        assert!(opts.audio_only.is_none());
    }

    #[test]
    fn test_preset_options_deserialize_full() {
        let json = r#"{"codec":"h264","crf":23,"audio_only":false}"#;
        let opts: PresetOptions = serde_json::from_str(json).unwrap();
        assert_eq!(opts.codec, "h264");
        assert_eq!(opts.crf, Some(23));
        assert_eq!(opts.audio_only, Some(false));
    }

    #[test]
    fn test_preset_options_deserialize_audio_only() {
        let json = r#"{"codec":"copy","audio_only":true}"#;
        let opts: PresetOptions = serde_json::from_str(json).unwrap();
        assert_eq!(opts.audio_only, Some(true));
    }

    // ==================== ClipProgress serialization ====================

    #[test]
    fn test_clip_progress_serialize_with_speed() {
        let p = ClipProgress {
            progress: 0.5,
            time_secs: 12.34,
            speed: Some(2.5),
        };
        let json = serde_json::to_string(&p).unwrap();
        assert!(json.contains("\"progress\":0.5"));
        assert!(json.contains("\"time_secs\":12.34"));
        assert!(json.contains("\"speed\":2.5"));
    }

    #[test]
    fn test_clip_progress_serialize_no_speed() {
        let p = ClipProgress {
            progress: 1.0,
            time_secs: 60.0,
            speed: None,
        };
        let json = serde_json::to_string(&p).unwrap();
        assert!(json.contains("\"speed\":null"));
    }

    // ==================== execute_clip input validation ====================

    #[tokio::test]
    async fn test_execute_clip_empty_ffmpeg_path() {
        let preset = PresetOptions {
            codec: "copy".to_string(),
            crf: None,
            audio_only: None,
        };
        let result = execute_clip(
            "",
            Path::new("/tmp/in.mp4"),
            Path::new("/tmp/out.mp4"),
            0,
            1000,
            &preset,
            CancellationToken::new(),
            |_| {},
        )
        .await;
        assert_eq!(result, Err("FFmpeg not available".to_string()));
    }

    #[tokio::test]
    async fn test_execute_clip_invalid_range_zero_duration() {
        let preset = PresetOptions {
            codec: "copy".to_string(),
            crf: None,
            audio_only: None,
        };
        let result = execute_clip(
            "/usr/bin/ffmpeg",
            Path::new("/tmp/in.mp4"),
            Path::new("/tmp/out.mp4"),
            1000,
            1000,
            &preset,
            CancellationToken::new(),
            |_| {},
        )
        .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid time range"));
    }

    #[tokio::test]
    async fn test_execute_clip_invalid_range_negative() {
        let preset = PresetOptions {
            codec: "copy".to_string(),
            crf: None,
            audio_only: None,
        };
        let result = execute_clip(
            "/usr/bin/ffmpeg",
            Path::new("/tmp/in.mp4"),
            Path::new("/tmp/out.mp4"),
            2000,
            1000,
            &preset,
            CancellationToken::new(),
            |_| {},
        )
        .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid time range"));
    }
}
