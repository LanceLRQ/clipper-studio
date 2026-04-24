//! Video merge operations
//!
//! Two modes:
//! - **Virtual merge** (concat demuxer): No re-encoding, requires compatible codecs/resolution
//! - **Physical merge** (filter_complex concat): Re-encodes, works with any inputs

use std::path::{Path, PathBuf};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio_util::sync::CancellationToken;

use crate::core::clipper::ClipProgress;

/// Merge multiple videos using FFmpeg concat demuxer (stream copy, no re-encoding).
///
/// Fast but requires all inputs to have the same codec, resolution, and timebase.
pub async fn merge_virtual(
    ffmpeg_path: &str,
    inputs: &[PathBuf],
    output: &Path,
    cancel: CancellationToken,
    on_progress: impl Fn(ClipProgress) + Send + 'static,
) -> Result<(), String> {
    if ffmpeg_path.is_empty() {
        return Err("FFmpeg not available".to_string());
    }
    if inputs.len() < 2 {
        return Err("至少需要 2 个视频才能合并".to_string());
    }

    // Ensure output directory exists
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create output directory: {}", e))?;
    }

    // Estimate total duration for progress
    let total_duration_secs = estimate_total_duration(ffmpeg_path, inputs);

    // Write concat list file
    let list_path = std::env::temp_dir().join(format!(
        "clipper_concat_{}_{}.txt",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));

    {
        use std::io::Write;
        let mut f = std::fs::File::create(&list_path)
            .map_err(|e| format!("Failed to create concat list: {}", e))?;
        for input in inputs {
            // Escape single quotes in file paths
            let escaped = input.to_string_lossy().replace('\'', "'\\''");
            writeln!(f, "file '{}'", escaped).map_err(|e| e.to_string())?;
        }
    }

    let args = vec![
        "-f".to_string(),
        "concat".to_string(),
        "-safe".to_string(),
        "0".to_string(),
        "-i".to_string(),
        list_path.to_string_lossy().to_string(),
        "-c".to_string(),
        "copy".to_string(),
        "-y".to_string(),
        "-progress".to_string(),
        "pipe:1".to_string(),
        output.to_string_lossy().to_string(),
    ];

    tracing::info!("FFmpeg virtual merge: {} {:?}", ffmpeg_path, args);

    let result =
        run_ffmpeg_with_progress(ffmpeg_path, &args, total_duration_secs, cancel, on_progress)
            .await;

    // Cleanup
    let _ = std::fs::remove_file(&list_path);

    result?;

    if !output.exists() {
        return Err("FFmpeg merge completed but output file not found".to_string());
    }

    tracing::info!("Virtual merge completed: {}", output.display());
    Ok(())
}

/// Merge multiple videos using filter_complex concat (re-encodes all inputs).
///
/// Works with any combination of codecs/resolutions but is slower.
pub async fn merge_physical(
    ffmpeg_path: &str,
    inputs: &[PathBuf],
    output: &Path,
    codec_hint: &str,
    crf: Option<u32>,
    cancel: CancellationToken,
    on_progress: impl Fn(ClipProgress) + Send + 'static,
) -> Result<(), String> {
    if ffmpeg_path.is_empty() {
        return Err("FFmpeg not available".to_string());
    }
    if inputs.len() < 2 {
        return Err("至少需要 2 个视频才能合并".to_string());
    }

    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create output directory: {}", e))?;
    }

    let total_duration_secs = estimate_total_duration(ffmpeg_path, inputs);
    let n = inputs.len();

    let mut args: Vec<String> = Vec::new();

    // Add all inputs
    for input in inputs {
        args.extend(["-i".to_string(), input.to_string_lossy().to_string()]);
    }

    // Build filter_complex
    let mut filter = String::new();
    for i in 0..n {
        filter.push_str(&format!("[{}:v:0][{}:a:0]", i, i));
    }
    filter.push_str(&format!("concat=n={}:v=1:a=1[outv][outa]", n));

    args.extend(["-filter_complex".to_string(), filter]);
    args.extend(["-map".to_string(), "[outv]".to_string()]);
    args.extend(["-map".to_string(), "[outa]".to_string()]);

    // Video codec
    let video_codec = crate::core::clipper::resolve_video_codec(ffmpeg_path, codec_hint);
    args.extend(["-c:v".to_string(), video_codec.clone()]);

    // Quality setting：通过 apply_quality_args 统一硬件/软件编码器质量参数
    // （P4-COMPAT-18：原先无条件 -crf 在硬件编码器下会被忽略或报错）
    crate::core::clipper::apply_quality_args(&video_codec, crf, &mut args);

    args.extend(["-c:a".to_string(), "aac".to_string()]);
    args.extend([
        "-y".to_string(),
        "-progress".to_string(),
        "pipe:1".to_string(),
        output.to_string_lossy().to_string(),
    ]);

    tracing::info!("FFmpeg physical merge: {} {:?}", ffmpeg_path, args);

    run_ffmpeg_with_progress(ffmpeg_path, &args, total_duration_secs, cancel, on_progress).await?;

    if !output.exists() {
        return Err("FFmpeg merge completed but output file not found".to_string());
    }

    tracing::info!("Physical merge completed: {}", output.display());
    Ok(())
}

/// Check if all input videos are compatible for virtual merge
pub fn check_merge_compatibility(ffprobe_path: &str, inputs: &[PathBuf]) -> Result<bool, String> {
    if inputs.len() < 2 {
        return Ok(true);
    }

    let first = crate::utils::ffmpeg::probe(ffprobe_path, &inputs[0])?;
    for input in &inputs[1..] {
        let info = crate::utils::ffmpeg::probe(ffprobe_path, input)?;
        if info.video_codec != first.video_codec
            || info.width != first.width
            || info.height != first.height
        {
            return Ok(false);
        }
    }

    Ok(true)
}

// ====== Internal helpers ======

fn estimate_total_duration(ffmpeg_path: &str, inputs: &[PathBuf]) -> f64 {
    let ffprobe_path = crate::utils::ffmpeg::derive_ffprobe_path(ffmpeg_path);
    let mut total = 0.0;
    for input in inputs {
        if let Ok(probe) = crate::utils::ffmpeg::probe(&ffprobe_path, input) {
            if let Some(ms) = probe.duration_ms {
                total += ms as f64 / 1000.0;
            }
        }
    }
    total
}

async fn run_ffmpeg_with_progress(
    ffmpeg_path: &str,
    args: &[String],
    total_duration_secs: f64,
    cancel: CancellationToken,
    on_progress: impl Fn(ClipProgress) + Send + 'static,
) -> Result<(), String> {
    let mut child = Command::new(ffmpeg_path)
        .args(args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to start FFmpeg: {}", e))?;

    let stdout = child.stdout.take().unwrap();
    let mut reader = BufReader::new(stdout).lines();
    let mut current_speed: Option<f64> = None;
    // 进度节流：同步 clipper.rs 的 200ms 窗口
    let interval_ms = crate::core::clipper::PROGRESS_EMIT_INTERVAL_MS;
    let mut last_emit = std::time::Instant::now()
        .checked_sub(std::time::Duration::from_millis(interval_ms))
        .unwrap_or_else(std::time::Instant::now);

    // Concurrently consume stderr to prevent pipe buffer deadlock
    // 使用 read_stderr_capped 避免长时间编码累积数十 MB 日志
    let stderr = child.stderr.take();
    let stderr_task = tokio::spawn(async move {
        if let Some(stderr) = stderr {
            crate::utils::ffmpeg::read_stderr_capped(stderr).await
        } else {
            Vec::new()
        }
    });

    loop {
        tokio::select! {
            _ = cancel.cancelled() => {
                crate::core::clipper::graceful_kill_child(&mut child).await;
                return Err("Task cancelled".to_string());
            }
            line = reader.next_line() => {
                match line {
                    Ok(Some(line)) => {
                        if let Some(time_str) = line.strip_prefix("out_time_us=") {
                            if let Ok(us) = time_str.trim().parse::<i64>() {
                                let time_secs = us as f64 / 1_000_000.0;
                                let progress = if total_duration_secs > 0.0 {
                                    (time_secs / total_duration_secs).min(1.0).max(0.0)
                                } else {
                                    0.0
                                };
                                if last_emit.elapsed()
                                    >= std::time::Duration::from_millis(interval_ms)
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
                            // 结束事件必发
                            on_progress(ClipProgress {
                                progress: 1.0,
                                time_secs: total_duration_secs,
                                speed: current_speed,
                            });
                            break;
                        }
                    }
                    Ok(None) => break,
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

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_check_merge_compatibility_single_file() {
        // Single file should always be "compatible"
        let result = check_merge_compatibility("", &[]);
        assert!(result.is_ok());
        assert!(result.unwrap());
    }

    #[test]
    fn test_check_merge_compatibility_empty_inputs() {
        let result = check_merge_compatibility("", &[]);
        assert!(result.is_ok());
        assert!(result.unwrap());
    }

    #[test]
    fn test_check_merge_compatibility_invalid_probe() {
        // Two files with invalid ffprobe path → should return error
        let inputs = vec![
            PathBuf::from("/nonexistent/file1.mp4"),
            PathBuf::from("/nonexistent/file2.mp4"),
        ];
        let result = check_merge_compatibility("/nonexistent/ffprobe", &inputs);
        assert!(result.is_err());
    }
}
