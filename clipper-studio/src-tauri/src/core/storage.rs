use std::collections::HashMap;
use std::path::{Path, PathBuf};
use serde::Serialize;
use regex::Regex;

/// Metadata extracted from a recording file name
#[derive(Debug, Clone, Serialize)]
pub struct RecordingFileMeta {
    pub room_id: Option<String>,
    pub streamer_name: Option<String>,
    pub stream_title: Option<String>,
    pub recorded_at: Option<String>,
    /// Duration in milliseconds (from FFprobe, filled after scan)
    pub duration_ms: Option<i64>,
    pub file_path: PathBuf,
    pub file_name: String,
    pub extension: String,
    /// Associated files (danmaku xml, log txt, etc.)
    pub associated_files: Vec<PathBuf>,
}

/// Result of scanning a workspace directory
#[derive(Debug, Clone, Serialize)]
pub struct WorkspaceScanResult {
    pub adapter_id: String,
    pub root_dir: PathBuf,
    pub files: Vec<RecordingFileMeta>,
    pub streamer_dirs: Vec<StreamerDir>,
}

#[derive(Debug, Clone, Serialize)]
pub struct StreamerDir {
    pub room_id: String,
    pub name: String,
    pub dir_path: PathBuf,
}

/// A grouped recording session (multiple files from one live stream)
#[derive(Debug, Clone, Serialize)]
pub struct RecordingSession {
    pub room_id: String,
    pub streamer_name: String,
    pub title: String,
    pub started_at: String,
    pub files: Vec<RecordingFileMeta>,
}

// ======================== BililiveRecorder Adapter ========================

/// Default file name regex for BililiveRecorder:
/// `录制-{room_id}-{yyyyMMdd}-{HHmmss}-{ms}-{title}.{ext}`
const BILIREC_FILE_REGEX: &str =
    r"^录制-(\d+)-(\d{8})-(\d{6})-(\d{3})-(.+)\.(flv|mp4|ts)$";

/// Default directory regex: `{room_id}-{name}`
const BILIREC_DIR_REGEX: &str = r"^(\d+)-(.+)$";

/// Video file extensions to scan
const VIDEO_EXTENSIONS: &[&str] = &["flv", "mp4", "ts", "mkv", "avi", "mov", "webm"];

/// Detect if a directory is a BililiveRecorder workspace
pub fn detect_bililive_recorder(dir: &Path) -> bool {
    // Check 1: config.json with BililiveRecorder signature
    let config_path = dir.join("config.json");
    if config_path.exists() {
        if let Ok(content) = std::fs::read_to_string(&config_path) {
            if content.contains("roomId") || content.contains("RecordMode") {
                return true;
            }
        }
    }

    // Check 2: subdirectories matching {room_id}-{name} pattern with recording files
    let dir_re = Regex::new(BILIREC_DIR_REGEX).unwrap();
    let file_re = Regex::new(BILIREC_FILE_REGEX).unwrap();

    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                let dir_name = entry.file_name().to_string_lossy().to_string();
                if dir_re.is_match(&dir_name) {
                    // Check if any file inside matches recording pattern
                    if let Ok(sub_entries) = std::fs::read_dir(entry.path()) {
                        for sub in sub_entries.flatten() {
                            let fname = sub.file_name().to_string_lossy().to_string();
                            if file_re.is_match(&fname) {
                                return true;
                            }
                        }
                    }
                }
            }
        }
    }

    false
}

/// Scan a BililiveRecorder workspace directory
pub fn scan_bililive_recorder(dir: &Path) -> WorkspaceScanResult {
    let dir_re = Regex::new(BILIREC_DIR_REGEX).unwrap();
    let file_re = Regex::new(BILIREC_FILE_REGEX).unwrap();

    let mut files: Vec<RecordingFileMeta> = Vec::new();
    let mut streamer_dirs: Vec<StreamerDir> = Vec::new();

    // Scan subdirectories
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let entry_path = entry.path();
            if !entry_path.is_dir() {
                // Also scan root-level video files (flat layout)
                scan_file(&entry_path, &file_re, None, None, &mut files);
                continue;
            }

            let dir_name = entry.file_name().to_string_lossy().to_string();

            // Parse directory name for streamer info
            let (room_id, streamer_name) = if let Some(caps) = dir_re.captures(&dir_name) {
                (
                    Some(caps[1].to_string()),
                    Some(caps[2].to_string()),
                )
            } else {
                (None, None)
            };

            if let (Some(ref rid), Some(ref name)) = (&room_id, &streamer_name) {
                streamer_dirs.push(StreamerDir {
                    room_id: rid.clone(),
                    name: name.clone(),
                    dir_path: entry_path.clone(),
                });
            }

            // Scan files in subdirectory
            if let Ok(sub_entries) = std::fs::read_dir(&entry_path) {
                for sub in sub_entries.flatten() {
                    scan_file(
                        &sub.path(),
                        &file_re,
                        room_id.as_deref(),
                        streamer_name.as_deref(),
                        &mut files,
                    );
                }
            }
        }
    }

    WorkspaceScanResult {
        adapter_id: "bililive-recorder".to_string(),
        root_dir: dir.to_path_buf(),
        files,
        streamer_dirs,
    }
}

/// Parse a single file and add to results if it's a video
fn scan_file(
    path: &Path,
    file_re: &Regex,
    dir_room_id: Option<&str>,
    dir_streamer_name: Option<&str>,
    results: &mut Vec<RecordingFileMeta>,
) {
    if !path.is_file() {
        return;
    }

    let file_name = match path.file_name() {
        Some(n) => n.to_string_lossy().to_string(),
        None => return,
    };

    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    if !VIDEO_EXTENSIONS.contains(&ext.as_str()) {
        return;
    }

    // Try to parse BililiveRecorder filename pattern
    let (room_id, recorded_at, title) = if let Some(caps) = file_re.captures(&file_name) {
        let rid = caps[1].to_string();
        let date = &caps[2]; // yyyyMMdd
        let time = &caps[3]; // HHmmss
        let _ms = &caps[4];
        let title = caps[5].to_string();

        // Format: yyyy-MM-dd HH:mm:ss
        let recorded_at = format!(
            "{}-{}-{} {}:{}:{}",
            &date[0..4], &date[4..6], &date[6..8],
            &time[0..2], &time[2..4], &time[4..6],
        );

        (Some(rid), Some(recorded_at), Some(title))
    } else {
        (None, None, None)
    };

    // Find associated files (same stem, different extension)
    let stem = path.file_stem().unwrap_or_default().to_string_lossy();
    let parent = path.parent().unwrap_or(Path::new("."));
    let mut associated = Vec::new();
    for assoc_ext in &["xml", "txt", "srt", "ass"] {
        let assoc_path = parent.join(format!("{}.{}", stem, assoc_ext));
        if assoc_path.exists() {
            associated.push(assoc_path);
        }
    }

    results.push(RecordingFileMeta {
        room_id: room_id.or_else(|| dir_room_id.map(|s| s.to_string())),
        streamer_name: dir_streamer_name.map(|s| s.to_string()),
        stream_title: title,
        recorded_at,
        duration_ms: None, // Filled later by FFprobe
        file_path: path.to_path_buf(),
        file_name,
        extension: ext,
        associated_files: associated,
    });
}

/// Group scanned files into recording sessions.
///
/// Grouping rule: same room_id + adjacent files within gap_threshold (default 1 hour)
/// → same session.
pub fn group_into_sessions(
    files: &[RecordingFileMeta],
    gap_threshold_secs: i64,
) -> Vec<RecordingSession> {
    // Group by room_id first
    let mut by_room: HashMap<String, Vec<&RecordingFileMeta>> = HashMap::new();
    for f in files {
        let key = f.room_id.clone().unwrap_or_else(|| "unknown".to_string());
        by_room.entry(key).or_default().push(f);
    }

    let mut sessions = Vec::new();

    for (room_id, mut room_files) in by_room {
        // Sort by recorded_at
        room_files.sort_by(|a, b| {
            a.recorded_at
                .as_deref()
                .unwrap_or("")
                .cmp(b.recorded_at.as_deref().unwrap_or(""))
        });

        let mut current_session: Vec<&RecordingFileMeta> = Vec::new();
        // End time of the last file in the current session (start + duration)
        let mut last_end_secs: Option<i64> = None;

        for file in &room_files {
            let curr_start_secs = file
                .recorded_at
                .as_deref()
                .and_then(parse_timestamp_secs);

            let should_split = if let (Some(prev_end), Some(curr_start)) =
                (last_end_secs, curr_start_secs)
            {
                // Gap = next file start - previous file end
                (curr_start - prev_end) > gap_threshold_secs
            } else {
                !current_session.is_empty() && curr_start_secs.is_some()
            };

            if should_split && !current_session.is_empty() {
                sessions.push(build_session(&room_id, &current_session));
                current_session.clear();
            }

            current_session.push(file);

            // Calculate end time: start + duration
            last_end_secs = match (curr_start_secs, file.duration_ms) {
                (Some(start), Some(dur)) => Some(start + dur / 1000),
                (Some(start), None) => Some(start), // No duration info, use start as fallback
                _ => last_end_secs,
            };
        }

        if !current_session.is_empty() {
            sessions.push(build_session(&room_id, &current_session));
        }
    }

    // Sort sessions by start time (most recent first)
    sessions.sort_by(|a, b| b.started_at.cmp(&a.started_at));
    sessions
}

fn build_session(room_id: &str, files: &[&RecordingFileMeta]) -> RecordingSession {
    let first = files[0];
    RecordingSession {
        room_id: room_id.to_string(),
        streamer_name: first.streamer_name.clone().unwrap_or_default(),
        title: first.stream_title.clone().unwrap_or_default(),
        started_at: first.recorded_at.clone().unwrap_or_default(),
        files: files.iter().map(|f| (*f).clone()).collect(),
    }
}

/// Parse "yyyy-MM-dd HH:mm:ss" to approximate seconds (for gap comparison)
fn parse_timestamp_secs(s: &str) -> Option<i64> {
    let parts: Vec<&str> = s.split(&['-', ' ', ':'][..]).collect();
    if parts.len() < 6 {
        return None;
    }
    let y: i64 = parts[0].parse().ok()?;
    let mo: i64 = parts[1].parse().ok()?;
    let d: i64 = parts[2].parse().ok()?;
    let h: i64 = parts[3].parse().ok()?;
    let mi: i64 = parts[4].parse().ok()?;
    let sec: i64 = parts[5].parse().ok()?;
    // Rough estimate (good enough for gap calculation, not calendar-accurate)
    Some(((y * 365 + mo * 30 + d) * 86400) + h * 3600 + mi * 60 + sec)
}

/// Detect adapter type for a directory
pub fn detect_adapter(dir: &Path) -> &'static str {
    if detect_bililive_recorder(dir) {
        "bililive-recorder"
    } else {
        "generic"
    }
}

/// Scan a directory with auto-detected adapter
pub fn scan_workspace(dir: &Path) -> WorkspaceScanResult {
    let adapter = detect_adapter(dir);
    match adapter {
        "bililive-recorder" => scan_bililive_recorder(dir),
        _ => scan_generic(dir),
    }
}

/// Generic scan: just find all video files recursively
fn scan_generic(dir: &Path) -> WorkspaceScanResult {
    let mut files = Vec::new();
    scan_dir_recursive(dir, &mut files);

    WorkspaceScanResult {
        adapter_id: "generic".to_string(),
        root_dir: dir.to_path_buf(),
        files,
        streamer_dirs: Vec::new(),
    }
}

fn scan_dir_recursive(dir: &Path, results: &mut Vec<RecordingFileMeta>) {
    let file_re = Regex::new(BILIREC_FILE_REGEX).unwrap();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                scan_dir_recursive(&path, results);
            } else {
                scan_file(&path, &file_re, None, None, results);
            }
        }
    }
}
