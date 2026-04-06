use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Command;

/// Detect a binary (ffmpeg/ffprobe) by checking:
/// 1. Application's bin directory (bundled with installer)
/// 2. System PATH
///
/// Returns the full path string if found, None otherwise.
pub fn detect_binary(name: &str, bin_dir: &Path) -> Option<String> {
    // Check app bin directory first
    let bin_path = get_bin_path(name, bin_dir);
    if let Some(path) = bin_path {
        if path.exists() {
            tracing::debug!("{} found in bin dir: {}", name, path.display());
            return Some(path.to_string_lossy().to_string());
        }
    }

    // Fallback: check system PATH
    if let Some(path) = find_in_path(name) {
        tracing::debug!("{} found in system PATH: {}", name, path);
        return Some(path);
    }

    None
}

/// Get platform-specific binary path in the bin directory
fn get_bin_path(name: &str, bin_dir: &Path) -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        Some(bin_dir.join(format!("{}.exe", name)))
    }
    #[cfg(not(target_os = "windows"))]
    {
        Some(bin_dir.join(name))
    }
}

/// Try to find a binary in the system PATH by running `which`/`where`
fn find_in_path(name: &str) -> Option<String> {
    #[cfg(target_os = "windows")]
    let cmd = "where";
    #[cfg(not(target_os = "windows"))]
    let cmd = "which";

    Command::new(cmd)
        .arg(name)
        .output()
        .ok()
        .and_then(|output| {
            if output.status.success() {
                String::from_utf8(output.stdout)
                    .ok()
                    .map(|s| s.trim().lines().next().unwrap_or("").to_string())
                    .filter(|s| !s.is_empty())
            } else {
                None
            }
        })
}

/// Video probe result from FFprobe
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ProbeResult {
    pub duration_ms: Option<i64>,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub video_codec: Option<String>,
    pub audio_codec: Option<String>,
    pub format_name: Option<String>,
    pub file_size: i64,
}

/// Run FFprobe on a file and return structured metadata
pub fn probe(ffprobe_path: &str, file_path: &std::path::Path) -> Result<ProbeResult, String> {
    if ffprobe_path.is_empty() {
        return Err("FFprobe not available".to_string());
    }

    let output = Command::new(ffprobe_path)
        .args([
            "-v", "quiet",
            "-print_format", "json",
            "-show_format",
            "-show_streams",
        ])
        .arg(file_path)
        .output()
        .map_err(|e| format!("Failed to run ffprobe: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("FFprobe failed: {}", stderr));
    }

    let json_str = String::from_utf8(output.stdout)
        .map_err(|e| format!("FFprobe output not UTF-8: {}", e))?;

    let json: serde_json::Value = serde_json::from_str(&json_str)
        .map_err(|e| format!("FFprobe JSON parse error: {}", e))?;

    // Extract format info
    let format = json.get("format");
    let duration_ms = format
        .and_then(|f| f.get("duration"))
        .and_then(|d| d.as_str())
        .and_then(|s| s.parse::<f64>().ok())
        .map(|d| (d * 1000.0) as i64);

    let file_size = format
        .and_then(|f| f.get("size"))
        .and_then(|s| s.as_str())
        .and_then(|s| s.parse::<i64>().ok())
        .unwrap_or(0);

    let format_name = format
        .and_then(|f| f.get("format_name"))
        .and_then(|s| s.as_str())
        .map(|s| s.to_string());

    // Extract stream info
    let streams = json.get("streams").and_then(|s| s.as_array());

    let mut width = None;
    let mut height = None;
    let mut video_codec = None;
    let mut audio_codec = None;

    if let Some(streams) = streams {
        for stream in streams {
            let codec_type = stream.get("codec_type").and_then(|t| t.as_str());
            match codec_type {
                Some("video") if video_codec.is_none() => {
                    video_codec = stream
                        .get("codec_name")
                        .and_then(|c| c.as_str())
                        .map(|s| s.to_string());
                    width = stream.get("width").and_then(|w| w.as_i64()).map(|w| w as i32);
                    height = stream.get("height").and_then(|h| h.as_i64()).map(|h| h as i32);
                }
                Some("audio") if audio_codec.is_none() => {
                    audio_codec = stream
                        .get("codec_name")
                        .and_then(|c| c.as_str())
                        .map(|s| s.to_string());
                }
                _ => {}
            }
        }
    }

    Ok(ProbeResult {
        duration_ms,
        width,
        height,
        video_codec,
        audio_codec,
        format_name,
        file_size,
    })
}

/// Audio envelope data (volume heatmap)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioEnvelope {
    pub window_ms: u32,
    /// Normalized volume values (0.0 ~ 1.0) per time window
    pub values: Vec<f32>,
}

/// Extract audio volume envelope from a video file.
///
/// Uses FFmpeg to decode audio to PCM, then computes RMS per time window.
/// For a 3-hour video at 500ms windows, this produces ~21,600 values (~86KB).
pub async fn extract_audio_envelope(
    ffmpeg_path: &str,
    file_path: &std::path::Path,
    window_ms: u32,
) -> Result<AudioEnvelope, String> {
    use tokio::io::AsyncReadExt;
    use tokio::process::Command as AsyncCommand;

    if ffmpeg_path.is_empty() {
        return Err("FFmpeg not available".to_string());
    }

    // FFmpeg: decode audio to raw f32le PCM at 8000 Hz mono
    let sample_rate: u32 = 8000;
    let mut child = AsyncCommand::new(ffmpeg_path)
        .args([
            "-i",
            &file_path.to_string_lossy(),
            "-ac", "1",           // mono
            "-ar", &sample_rate.to_string(),
            "-f", "f32le",        // raw float32 little-endian
            "-v", "quiet",
            "pipe:1",
        ])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|e| format!("Failed to start FFmpeg: {}", e))?;

    let mut stdout = child.stdout.take().unwrap();

    // Read all PCM data
    let mut pcm_data = Vec::new();
    let mut buf = vec![0u8; 65536];
    loop {
        let n = stdout.read(&mut buf).await.map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        pcm_data.extend_from_slice(&buf[..n]);
    }

    let _ = child.wait().await;

    // Convert bytes to f32 samples
    let samples: Vec<f32> = pcm_data
        .chunks_exact(4)
        .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
        .collect();

    if samples.is_empty() {
        return Err("No audio data extracted".to_string());
    }

    // Compute RMS per window
    let samples_per_window = (sample_rate * window_ms / 1000) as usize;
    let mut rms_values: Vec<f32> = Vec::new();

    for chunk in samples.chunks(samples_per_window) {
        let sum_sq: f64 = chunk.iter().map(|&s| (s as f64) * (s as f64)).sum();
        let rms = (sum_sq / chunk.len() as f64).sqrt() as f32;
        rms_values.push(rms);
    }

    // Normalize to 0.0 ~ 1.0
    let max_rms = rms_values.iter().cloned().fold(f32::MIN, f32::max);
    if max_rms > 0.0 {
        for v in &mut rms_values {
            *v /= max_rms;
        }
    }

    Ok(AudioEnvelope {
        window_ms,
        values: rms_values,
    })
}

/// Get FFmpeg version string
pub fn get_version(ffmpeg_path: &str) -> Option<String> {
    if ffmpeg_path.is_empty() {
        return None;
    }
    Command::new(ffmpeg_path)
        .arg("-version")
        .output()
        .ok()
        .and_then(|output| {
            if output.status.success() {
                String::from_utf8(output.stdout)
                    .ok()
                    .and_then(|s| s.lines().next().map(|l| l.to_string()))
            } else {
                None
            }
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_detect_binary_nonexistent_dir() {
        let result = detect_binary("ffmpeg", &PathBuf::from("/nonexistent/path"));
        // May or may not find in PATH depending on system
        // Just ensure it doesn't panic
        let _ = result;
    }

    #[test]
    fn test_detect_binary_empty_name() {
        let result = detect_binary("", &PathBuf::from("/nonexistent/path"));
        // Empty name should not panic, result depends on system
        let _ = result;
    }

    #[test]
    fn test_get_version_empty_path() {
        let result = get_version("");
        assert!(result.is_none(), "empty path should return None");
    }

    #[test]
    fn test_get_version_nonexistent_path() {
        let result = get_version("/nonexistent/binary/that/does/not/exist");
        assert!(result.is_none(), "nonexistent path should return None");
    }

    #[test]
    fn test_get_bin_path_returns_path() {
        let bin_dir = PathBuf::from("/opt/app/bin");
        // get_bin_path is private, test via detect_binary behavior
        // We just verify detect_binary with a non-existent dir doesn't panic
        let _ = detect_binary("nonexistent_tool_xyz", &bin_dir);
    }

    #[test]
    fn test_detect_binary_typical_names() {
        // Test with common names, just ensure no panic
        let dir = PathBuf::from("/tmp/clipper_test_bin");
        let _ = detect_binary("ffmpeg", &dir);
        let _ = detect_binary("ffprobe", &dir);
    }

    // ==================== probe ====================

    #[test]
    fn test_probe_empty_path() {
        let result = probe("", std::path::Path::new("/some/file.mp4"));
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "FFprobe not available");
    }

    #[test]
    fn test_probe_nonexistent_ffprobe() {
        let result = probe(
            "/nonexistent/ffprobe",
            std::path::Path::new("/some/file.mp4"),
        );
        assert!(result.is_err());
        assert!(
            result.unwrap_err().contains("Failed to run ffprobe"),
            "should report ffprobe failure"
        );
    }

    #[test]
    fn test_probe_nonexistent_file() {
        // Only test if ffprobe is available on the system
        let ffprobe = detect_binary("ffprobe", &PathBuf::from("/nonexistent/bin"));
        if let Some(ffprobe_path) = ffprobe {
            let result = probe(&ffprobe_path, std::path::Path::new("/nonexistent/file.mp4"));
            assert!(result.is_err(), "probing nonexistent file should fail");
        }
    }

    #[test]
    fn test_probe_result_serialization() {
        let result = ProbeResult {
            duration_ms: Some(123456),
            width: Some(1920),
            height: Some(1080),
            video_codec: Some("h264".to_string()),
            audio_codec: Some("aac".to_string()),
            format_name: Some("mov,mp4,m4a,3gp,3g2,mj2".to_string()),
            file_size: 1048576,
        };

        // Verify serialization roundtrip
        let json = serde_json::to_string(&result).unwrap();
        let parsed: ProbeResult = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.duration_ms, Some(123456));
        assert_eq!(parsed.width, Some(1920));
        assert_eq!(parsed.height, Some(1080));
        assert_eq!(parsed.video_codec, Some("h264".to_string()));
        assert_eq!(parsed.audio_codec, Some("aac".to_string()));
        assert_eq!(parsed.format_name, Some("mov,mp4,m4a,3gp,3g2,mj2".to_string()));
        assert_eq!(parsed.file_size, 1048576);
    }

    #[test]
    fn test_probe_result_optional_fields() {
        let result = ProbeResult {
            duration_ms: None,
            width: None,
            height: None,
            video_codec: None,
            audio_codec: None,
            format_name: None,
            file_size: 0,
        };

        let json = serde_json::to_string(&result).unwrap();
        let parsed: ProbeResult = serde_json::from_str(&json).unwrap();
        assert!(parsed.duration_ms.is_none());
        assert!(parsed.width.is_none());
        assert!(parsed.video_codec.is_none());
        assert_eq!(parsed.file_size, 0);
    }
}
