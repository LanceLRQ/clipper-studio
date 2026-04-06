use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;
use std::time::Duration;

use notify::RecursiveMode;
use notify_debouncer_full::{new_debouncer, DebounceEventResult, Debouncer, RecommendedCache};
use serde::Serialize;
use tauri::{AppHandle, Emitter};

use crate::core::storage::VIDEO_EXTENSIONS;

/// Event payload emitted to the frontend when new files are detected
#[derive(Debug, Clone, Serialize)]
pub struct WorkspaceFileChangeEvent {
    pub workspace_id: i64,
    pub workspace_path: String,
    pub new_files: Vec<String>,
}

type WatcherInstance = Debouncer<notify::RecommendedWatcher, RecommendedCache>;

/// Manages file system watchers for all active workspaces
pub struct WorkspaceWatcher {
    watchers: Mutex<HashMap<i64, WatcherInstance>>,
    app_handle: AppHandle,
}

impl WorkspaceWatcher {
    pub fn new(app_handle: AppHandle) -> Self {
        Self {
            watchers: Mutex::new(HashMap::new()),
            app_handle,
        }
    }

    /// Start watching a workspace directory
    pub fn watch(&self, workspace_id: i64, path: &Path) -> Result<(), String> {
        let mut watchers = self.watchers.lock().map_err(|e| e.to_string())?;

        // Stop existing watcher for this workspace if any
        watchers.remove(&workspace_id);

        if !path.is_dir() {
            return Err(format!("Path is not a directory: {}", path.display()));
        }

        let app_handle = self.app_handle.clone();
        let ws_id = workspace_id;
        let ws_path = path.to_string_lossy().to_string();

        let (tx, rx) = std::sync::mpsc::channel();

        let mut debouncer = new_debouncer(
            Duration::from_secs(3),
            None,
            tx,
        )
        .map_err(|e| format!("Failed to create watcher: {}", e))?;

        debouncer
            .watch(path, RecursiveMode::Recursive)
            .map_err(|e| format!("Failed to watch path: {}", e))?;

        // Spawn a thread to process debounced events
        let ws_path_clone = ws_path.clone();
        std::thread::spawn(move || {
            process_events(rx, app_handle, ws_id, &ws_path_clone);
        });

        watchers.insert(workspace_id, debouncer);
        tracing::info!(
            "Started watching workspace {} at {}",
            workspace_id,
            ws_path
        );

        Ok(())
    }

    /// Stop watching a workspace directory
    pub fn unwatch(&self, workspace_id: i64) {
        if let Ok(mut watchers) = self.watchers.lock() {
            if watchers.remove(&workspace_id).is_some() {
                tracing::info!("Stopped watching workspace {}", workspace_id);
            }
        }
    }

    /// Stop all watchers
    pub fn unwatch_all(&self) {
        if let Ok(mut watchers) = self.watchers.lock() {
            let count = watchers.len();
            watchers.clear();
            tracing::info!("Stopped all {} workspace watchers", count);
        }
    }

    /// Check if a workspace is being watched
    pub fn is_watching(&self, workspace_id: i64) -> bool {
        self.watchers
            .lock()
            .map(|w| w.contains_key(&workspace_id))
            .unwrap_or(false)
    }
}

/// Process debounced file events from the watcher channel
fn process_events(
    rx: std::sync::mpsc::Receiver<DebounceEventResult>,
    app_handle: AppHandle,
    workspace_id: i64,
    workspace_path: &str,
) {
    for result in rx {
        match result {
            Ok(events) => {
                let mut new_video_files: Vec<String> = Vec::new();

                for event in events {
                    // Only handle file creation events
                    if !event.event.kind.is_create() {
                        continue;
                    }

                    for path in &event.event.paths {
                        if is_video_file(path) {
                            new_video_files
                                .push(path.to_string_lossy().to_string());
                        }
                    }
                }

                if !new_video_files.is_empty() {
                    tracing::info!(
                        "Workspace {} detected {} new video file(s)",
                        workspace_id,
                        new_video_files.len()
                    );

                    let _ = app_handle.emit(
                        "workspace-file-change",
                        WorkspaceFileChangeEvent {
                            workspace_id,
                            workspace_path: workspace_path.to_string(),
                            new_files: new_video_files,
                        },
                    );
                }
            }
            Err(errors) => {
                for error in errors {
                    tracing::warn!("Watcher error for workspace {}: {:?}", workspace_id, error);
                }
            }
        }
    }

    tracing::debug!("Watcher event loop ended for workspace {}", workspace_id);
}

/// Check if a file path has a video extension
fn is_video_file(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|ext| VIDEO_EXTENSIONS.contains(&ext.to_lowercase().as_str()))
        .unwrap_or(false)
}
