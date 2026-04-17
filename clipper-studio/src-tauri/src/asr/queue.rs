use crate::utils::locks::{MutexExt, RwLockExt};
use std::collections::{HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::Serialize;
use tauri::{AppHandle, Emitter};

use crate::db::Database;

use super::local::LocalASRProvider;
use super::provider::{ASRProvider, ASRTaskStatus};
use super::remote::RemoteASRProvider;
use super::service;
use super::splitter;

/// RAII guard that ensures a temporary file is deleted when dropped.
struct TempFileGuard(PathBuf);

impl Drop for TempFileGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// Maximum automatic retry count
const MAX_AUTO_RETRIES: u32 = 2;
/// Initial retry delay in seconds
const INITIAL_RETRY_DELAY_SECS: u64 = 5;
/// Poll interval in seconds
const POLL_INTERVAL_SECS: u64 = 3;

// ==================== Data Structures ====================

/// Entry in the pending queue
#[derive(Debug, Clone)]
struct ASRQueueEntry {
    asr_task_id: i64,
    video_id: i64,
    video_file_name: String,
    video_file_path: String,
    language: String,
}

/// Progress event emitted to frontend via Tauri events
#[derive(Debug, Clone, Serialize)]
pub struct ASRTaskProgressEvent {
    pub task_id: i64,
    pub video_id: i64,
    pub status: String,
    pub progress: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
    pub video_file_name: String,
}

/// Queue snapshot item returned to frontend
#[derive(Debug, Clone, Serialize)]
pub struct ASRQueueItem {
    pub task_id: i64,
    pub video_id: i64,
    pub video_file_name: String,
    pub status: String,
    pub progress: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
}

/// Internal shared state protected by a single std Mutex (never held across await)
struct QueueInner {
    pending: VecDeque<ASRQueueEntry>,
    /// Currently running task info (id, video_id, file_name, status, progress)
    running: Option<(i64, i64, String, String, f64)>,
    cancelled_ids: HashSet<i64>,
}

// ==================== ASRTaskQueue ====================

/// ASR task queue with single-threaded serial execution and backend-autonomous polling.
///
/// Uses a single `std::sync::Mutex` for shared state (never held across `.await`).
/// The worker communicates via `tokio::sync::Notify`.
pub struct ASRTaskQueue {
    inner: Arc<std::sync::Mutex<QueueInner>>,
    queue_notify: Arc<tokio::sync::Notify>,
    db: Database,
    ffmpeg_path: Arc<std::sync::RwLock<String>>,
    app_handle: AppHandle,
}

impl ASRTaskQueue {
    /// Create a new ASR task queue. Call `start()` after AppState is fully managed.
    pub fn new(
        app_handle: AppHandle,
        db: Database,
        ffmpeg_path: Arc<std::sync::RwLock<String>>,
    ) -> Self {
        Self {
            inner: Arc::new(std::sync::Mutex::new(QueueInner {
                pending: VecDeque::new(),
                running: None,
                cancelled_ids: HashSet::new(),
            })),
            queue_notify: Arc::new(tokio::sync::Notify::new()),
            db,
            ffmpeg_path,
            app_handle,
        }
    }

    /// Start the background worker and recovery. Call AFTER Tauri setup is complete.
    pub fn start(&self) {
        let db = self.db.clone();
        let inner = self.inner.clone();
        let notify = self.queue_notify.clone();
        let app = self.app_handle.clone();
        let ffmpeg = self.ffmpeg_path.clone();

        tauri::async_runtime::spawn(async move {
            // Phase 1: Recovery
            Self::recover_on_startup(&db, &inner, &app).await;
            if !inner.lock_safe().pending.is_empty() {
                notify.notify_one();
            }

            // Phase 2: Worker loop (runs forever)
            Self::worker_loop(db, app, ffmpeg, inner, notify).await;
        });
    }

    /// Enqueue an ASR task for a video. Returns the asr_task_id.
    pub async fn enqueue(
        &self,
        video_id: i64,
        language: String,
    ) -> Result<i64, String> {
        // Duplicate check (quick, no await while holding lock)
        {
            let inner = self.inner.lock_safe();
            if inner.pending.iter().any(|e| e.video_id == video_id) {
                return Err("该视频已有排队中的 ASR 任务".to_string());
            }
            if let Some((_, vid, _, _, _)) = &inner.running {
                if *vid == video_id {
                    return Err("该视频已有正在执行的 ASR 任务".to_string());
                }
            }
        }

        // DB duplicate check
        let db_active = sea_orm::ConnectionTrait::query_one(
            self.db.conn(),
            sea_orm::Statement::from_string(
                sea_orm::DatabaseBackend::Sqlite,
                format!(
                    "SELECT COUNT(*) as cnt FROM asr_tasks WHERE video_id = {} \
                     AND status IN ('queued', 'processing', 'pending')",
                    video_id
                ),
            ),
        )
        .await
        .ok()
        .flatten()
        .and_then(|r| r.try_get::<i64>("", "cnt").ok())
        .unwrap_or(0);

        if db_active > 0 {
            return Err("该视频已有进行中的 ASR 任务".to_string());
        }

        // Get video info
        let video_row = sea_orm::ConnectionTrait::query_one(
            self.db.conn(),
            sea_orm::Statement::from_string(
                sea_orm::DatabaseBackend::Sqlite,
                format!(
                    "SELECT file_path, file_name FROM videos WHERE id = {}",
                    video_id
                ),
            ),
        )
        .await
        .map_err(|e| e.to_string())?
        .ok_or("视频不存在".to_string())?;

        let file_path: String = video_row.try_get("", "file_path").unwrap_or_default();
        let file_name: String = video_row.try_get("", "file_name").unwrap_or_default();

        if file_path.is_empty() {
            return Err("视频文件路径为空".to_string());
        }

        // Create DB row
        sea_orm::ConnectionTrait::execute_unprepared(
            self.db.conn(),
            &format!(
                "INSERT INTO asr_tasks (video_id, status, asr_provider_id, language) \
                 VALUES ({}, 'queued', 'queue', '{}')",
                video_id,
                language.replace('\'', "''"),
            ),
        )
        .await
        .map_err(|e| format!("创建 ASR 任务失败: {}", e))?;

        let task_id: i64 = sea_orm::ConnectionTrait::query_one(
            self.db.conn(),
            sea_orm::Statement::from_string(
                sea_orm::DatabaseBackend::Sqlite,
                "SELECT last_insert_rowid() as id".to_string(),
            ),
        )
        .await
        .ok()
        .flatten()
        .and_then(|r| r.try_get("", "id").ok())
        .unwrap_or(0);

        tracing::info!("ASR task {} enqueued for video {} ({})", task_id, video_id, file_name);

        // Push to queue (quick lock, no await)
        {
            let mut inner = self.inner.lock_safe();
            inner.pending.push_back(ASRQueueEntry {
                asr_task_id: task_id,
                video_id,
                video_file_name: file_name.clone(),
                video_file_path: file_path,
                language,
            });
        }

        Self::emit_progress(&self.app_handle, task_id, video_id, "queued", 0.0, Some("排队中"), None, &file_name);
        self.queue_notify.notify_one();
        Ok(task_id)
    }

    /// Cancel an ASR task (queued or running).
    pub fn cancel(&self, asr_task_id: i64) -> Result<bool, String> {
        let mut inner = self.inner.lock_safe();

        // Check pending queue
        if let Some(pos) = inner.pending.iter().position(|e| e.asr_task_id == asr_task_id) {
            let entry = inner.pending.remove(pos).unwrap();
            drop(inner); // release lock before DB/emit

            // Update DB asynchronously
            let db = self.db.clone();
            let app = self.app_handle.clone();
            tauri::async_runtime::spawn(async move {
                let _ = sea_orm::ConnectionTrait::execute_unprepared(
                    db.conn(),
                    &format!("UPDATE asr_tasks SET status = 'cancelled' WHERE id = {}", asr_task_id),
                ).await;
                Self::emit_progress(&app, asr_task_id, entry.video_id, "cancelled", 0.0, Some("已取消"), None, &entry.video_file_name);
            });

            tracing::info!("ASR task {} cancelled (was queued)", asr_task_id);
            return Ok(true);
        }

        // Check running task
        if let Some((tid, _, _, _, _)) = &inner.running {
            if *tid == asr_task_id {
                inner.cancelled_ids.insert(asr_task_id);
                tracing::info!("ASR task {} marked for cancellation (running)", asr_task_id);
                return Ok(true);
            }
        }

        Err("任务不在队列中".to_string())
    }

    /// Get a snapshot of the current queue state.
    /// Check if there are any active ASR tasks (running or queued). Synchronous.
    pub fn has_active_tasks(&self) -> bool {
        let inner = self.inner.lock_safe();
        inner.running.is_some() || !inner.pending.is_empty()
    }

    pub fn get_queue_snapshot(&self) -> Vec<ASRQueueItem> {
        let inner = self.inner.lock_safe();
        let mut items = Vec::new();

        // Running task
        if let Some((tid, vid, ref fname, ref status, progress)) = inner.running {
            items.push(ASRQueueItem {
                task_id: tid,
                video_id: vid,
                video_file_name: fname.clone(),
                status: status.clone(),
                progress,
                message: None,
                error_message: None,
            });
        }

        // Pending tasks
        for entry in &inner.pending {
            items.push(ASRQueueItem {
                task_id: entry.asr_task_id,
                video_id: entry.video_id,
                video_file_name: entry.video_file_name.clone(),
                status: "queued".to_string(),
                progress: 0.0,
                message: Some("排队中".to_string()),
                error_message: None,
            });
        }

        items
    }

    // ==================== Internal: Startup Recovery ====================

    async fn recover_on_startup(
        db: &Database,
        inner: &std::sync::Mutex<QueueInner>,
        app_handle: &AppHandle,
    ) {
        // Mark stale 'processing' tasks as failed
        let _ = sea_orm::ConnectionTrait::execute_unprepared(
            db.conn(),
            "UPDATE asr_tasks SET status = 'failed', error_message = '应用异常退出' \
             WHERE status = 'processing'",
        )
        .await;

        // Re-enqueue 'queued' tasks
        let rows = sea_orm::ConnectionTrait::query_all(
            db.conn(),
            sea_orm::Statement::from_string(
                sea_orm::DatabaseBackend::Sqlite,
                "SELECT t.id, t.video_id, t.language, v.file_path, v.file_name \
                 FROM asr_tasks t \
                 LEFT JOIN videos v ON t.video_id = v.id \
                 WHERE t.status = 'queued' \
                 ORDER BY t.created_at ASC"
                    .to_string(),
            ),
        )
        .await;

        if let Ok(rows) = rows {
            let mut q = inner.lock_safe();
            for row in &rows {
                let task_id: i64 = row.try_get("", "id").unwrap_or(0);
                let video_id: i64 = row.try_get("", "video_id").unwrap_or(0);
                let language: String = row.try_get("", "language").unwrap_or("Chinese".to_string());
                let file_path: String = row.try_get("", "file_path").unwrap_or_default();
                let file_name: String = row.try_get("", "file_name").unwrap_or_default();

                if file_path.is_empty() {
                    continue;
                }

                q.pending.push_back(ASRQueueEntry {
                    asr_task_id: task_id,
                    video_id,
                    video_file_name: file_name.clone(),
                    video_file_path: file_path,
                    language,
                });

                tracing::info!("ASR task {} recovered from DB (was queued)", task_id);
            }
            // Drop lock before emitting events
            let recovered: Vec<_> = q.pending.iter().map(|e| (e.asr_task_id, e.video_id, e.video_file_name.clone())).collect();
            drop(q);

            for (tid, vid, fname) in recovered {
                Self::emit_progress(app_handle, tid, vid, "queued", 0.0, Some("排队中（已恢复）"), None, &fname);
            }
        }
    }

    // ==================== Internal: Worker Loop ====================

    async fn worker_loop(
        db: Database,
        app_handle: AppHandle,
        ffmpeg_path: Arc<std::sync::RwLock<String>>,
        inner: Arc<std::sync::Mutex<QueueInner>>,
        queue_notify: Arc<tokio::sync::Notify>,
    ) {
        tracing::info!("ASR task queue worker started");

        loop {
            // Wait for notification or timeout
            tokio::select! {
                _ = queue_notify.notified() => {},
                _ = tokio::time::sleep(tokio::time::Duration::from_secs(5)) => {},
            }

            // Take next entry (quick lock, no await)
            let entry = {
                let mut q = inner.lock_safe();
                q.pending.pop_front()
            };

            let entry = match entry {
                Some(e) => e,
                None => continue,
            };

            let task_id = entry.asr_task_id;
            let video_id = entry.video_id;
            let file_name = entry.video_file_name.clone();

            // Check cancellation before starting (quick lock, then release)
            let was_cancelled = {
                let mut q = inner.lock_safe();
                if q.cancelled_ids.remove(&task_id) {
                    true
                } else {
                    q.running = Some((task_id, video_id, file_name.clone(), "converting".to_string(), 0.0));
                    false
                }
            };
            if was_cancelled {
                Self::do_mark_cancelled(&db, &app_handle, task_id, video_id, &file_name).await;
                continue;
            }

            // Execute the task
            let result = Self::execute_task(
                &db, &app_handle, &ffmpeg_path, &inner, inner.clone(), &entry,
            ).await;

            if let Err(e) = result {
                tracing::error!("ASR task {} failed: {}", task_id, e);
            }

            // Clear running
            {
                let mut q = inner.lock_safe();
                q.running = None;
                q.cancelled_ids.remove(&task_id);
            }
        }
    }

    /// Execute a single ASR task: convert → submit → poll → import
    async fn execute_task(
        db: &Database,
        app_handle: &AppHandle,
        ffmpeg_path: &Arc<std::sync::RwLock<String>>,
        inner: &std::sync::Mutex<QueueInner>,
        inner_arc: Arc<std::sync::Mutex<QueueInner>>,
        entry: &ASRQueueEntry,
    ) -> Result<(), String> {
        let task_id = entry.asr_task_id;
        let video_id = entry.video_id;
        let file_name = &entry.video_file_name;

        // --- Phase 1: Convert to WAV ---
        Self::update_running(inner, "converting", 0.0);
        Self::emit_progress(app_handle, task_id, video_id, "converting", 0.0, Some("音频转换中..."), None, file_name);

        let ffmpeg = ffmpeg_path.read_safe().clone();
        // Build an atomic cancel flag so WAV conversion can be interrupted
        let cancel_flag = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let flag_clone = cancel_flag.clone();
        let inner_for_watcher = inner_arc.clone();
        let watcher = tokio::spawn(async move {
            loop {
                tokio::time::sleep(std::time::Duration::from_millis(250)).await;
                let cancelled = inner_for_watcher
                    .lock()
                    .map(|q| q.cancelled_ids.contains(&task_id))
                    .unwrap_or(false);
                if cancelled {
                    flag_clone.store(true, std::sync::atomic::Ordering::Relaxed);
                    return;
                }
            }
        });
        let wav_path = service::convert_to_asr_wav(
            &ffmpeg,
            Path::new(&entry.video_file_path),
            Some(cancel_flag),
        )
        .await
        .map_err(|e| {
            watcher.abort();
            let msg = format!("音频转换失败: {}", e);
            Self::emit_progress(app_handle, task_id, video_id, "failed", 0.0, None, Some(&msg), file_name);
            Self::spawn_mark_failed(db.clone(), task_id, &msg);
            msg
        })?;
        watcher.abort(); // WAV conversion done, stop the cancel watcher

        // RAII guard ensures WAV cleanup on any exit path (panic, early return, etc.)
        let _wav_guard = TempFileGuard(wav_path.clone());

        // Check cancellation
        if Self::check_cancelled(inner, task_id) {
            Self::do_mark_cancelled(db, app_handle, task_id, video_id, file_name).await;
            return Ok(());
        }

        // --- Phase 2: Build provider and submit ---
        Self::update_running(inner, "submitting", 0.0);
        Self::emit_progress(app_handle, task_id, video_id, "submitting", 0.0, Some("提交识别任务..."), None, file_name);

        let provider = match Self::build_provider(db).await {
            Ok(p) => p,
            Err(e) => {
                Self::emit_progress(app_handle, task_id, video_id, "failed", 0.0, None, Some(&e), file_name);
                Self::spawn_mark_failed(db.clone(), task_id, &e);
                return Err(e);
            }
        };

        let remote_task_id = match provider.submit(&wav_path, Some(&entry.language)).await {
            Ok(id) => id,
            Err(e) => {
                let msg = format!("ASR 提交失败: {}", e);
                Self::emit_progress(app_handle, task_id, video_id, "failed", 0.0, None, Some(&msg), file_name);
                Self::spawn_mark_failed(db.clone(), task_id, &msg);
                return Err(msg);
            }
        };

        // Update DB with remote_task_id
        let _ = sea_orm::ConnectionTrait::execute_unprepared(
            db.conn(),
            &format!(
                "UPDATE asr_tasks SET status = 'processing', asr_provider_id = '{}', \
                 remote_task_id = '{}', started_at = datetime('now') WHERE id = {}",
                provider.provider_id(),
                remote_task_id.replace('\'', "''"),
                task_id
            ),
        )
        .await;

        // --- Phase 3: Poll loop ---
        Self::update_running(inner, "processing", 0.0);
        let mut retry_count: u32 = 0;

        loop {
            if Self::check_cancelled(inner, task_id) {
                // Notify remote ASR service to cancel the task (best-effort)
                if let Err(e) = provider.cancel(&remote_task_id).await {
                    tracing::warn!("Failed to cancel remote ASR task {}: {}", remote_task_id, e);
                }
                Self::do_mark_cancelled(db, app_handle, task_id, video_id, file_name).await;
                return Ok(());
            }

            tokio::time::sleep(tokio::time::Duration::from_secs(POLL_INTERVAL_SECS)).await;

            if Self::check_cancelled(inner, task_id) {
                if let Err(e) = provider.cancel(&remote_task_id).await {
                    tracing::warn!("Failed to cancel remote ASR task {}: {}", remote_task_id, e);
                }
                Self::do_mark_cancelled(db, app_handle, task_id, video_id, file_name).await;
                return Ok(());
            }

            let status = match provider.query(&remote_task_id).await {
                Ok(s) => s,
                Err(e) => {
                    tracing::warn!("ASR task {} poll error: {}", task_id, e);
                    retry_count += 1;
                    if retry_count > MAX_AUTO_RETRIES {
                        let msg = format!("轮询失败: {}", e);
                        Self::emit_progress(app_handle, task_id, video_id, "failed", 0.0, None, Some(&msg), file_name);
                        Self::spawn_mark_failed(db.clone(), task_id, &msg);
                        return Err(msg);
                    }
                    continue;
                }
            };

            match status {
                ASRTaskStatus::Pending => {
                    Self::emit_progress(app_handle, task_id, video_id, "processing", 0.0, Some("等待识别引擎处理..."), None, file_name);
                }
                ASRTaskStatus::Processing { progress } => {
                    Self::update_running(inner, "processing", progress);
                    let _ = sea_orm::ConnectionTrait::execute_unprepared(
                        db.conn(),
                        &format!(
                            "UPDATE asr_tasks SET status = 'processing', progress = {} WHERE id = {}",
                            progress, task_id
                        ),
                    )
                    .await;
                    Self::emit_progress(app_handle, task_id, video_id, "processing", progress, None, None, file_name);
                }
                ASRTaskStatus::Completed { segments: raw_segments } => {
                    let max_chars = service::read_setting_from_db(db, "asr_max_chars")
                        .await
                        .and_then(|v| v.parse::<usize>().ok())
                        .unwrap_or(15);

                    let segments = splitter::split_segments(&raw_segments, max_chars);
                    let count = service::import_segments(db, video_id, &entry.language, &segments)
                        .await
                        .map_err(|e| {
                            let msg = format!("字幕导入失败: {}", e);
                            Self::emit_progress(app_handle, task_id, video_id, "failed", 0.0, None, Some(&msg), file_name);
                            Self::spawn_mark_failed(db.clone(), task_id, &msg);
                            msg
                        })?;

                    let _ = sea_orm::ConnectionTrait::execute_unprepared(
                        db.conn(),
                        &format!(
                            "UPDATE asr_tasks SET status = 'completed', progress = 1.0, \
                             segment_count = {}, completed_at = datetime('now') WHERE id = {}",
                            count, task_id
                        ),
                    )
                    .await;

                    let _ = sea_orm::ConnectionTrait::execute_unprepared(
                        db.conn(),
                        &format!("UPDATE videos SET has_subtitle = 1 WHERE id = {}", video_id),
                    )
                    .await;

                    Self::update_running(inner, "completed", 1.0);
                    Self::emit_progress(app_handle, task_id, video_id, "completed", 1.0, Some(&format!("完成，共 {} 条字幕", count)), None, file_name);
                    tracing::info!("ASR task {} completed: {} segments imported", task_id, count);
                    return Ok(());
                }
                ASRTaskStatus::RetryableError { error, .. } => {
                    retry_count += 1;
                    if retry_count > MAX_AUTO_RETRIES {
                        let msg = format!("重试次数已用尽: {}", error);
                        Self::emit_progress(app_handle, task_id, video_id, "failed", 0.0, None, Some(&msg), file_name);
                        Self::spawn_mark_failed(db.clone(), task_id, &msg);
                        return Err(msg);
                    }

                    let delay = INITIAL_RETRY_DELAY_SECS * 2u64.pow(retry_count - 1);
                    tracing::warn!("ASR task {} retryable error (attempt {}): {}. Retry in {}s", task_id, retry_count, error, delay);

                    let _ = sea_orm::ConnectionTrait::execute_unprepared(
                        db.conn(),
                        &format!("UPDATE asr_tasks SET retry_count = {} WHERE id = {}", retry_count, task_id),
                    )
                    .await;

                    let cur_progress = inner.lock_safe().running.as_ref().map(|r| r.4).unwrap_or(0.0);
                    Self::emit_progress(app_handle, task_id, video_id, "processing", cur_progress,
                        Some(&format!("重试中（第 {} 次）...", retry_count)), None, file_name);

                    tokio::time::sleep(tokio::time::Duration::from_secs(delay)).await;
                }
                ASRTaskStatus::PermanentError { error } => {
                    Self::emit_progress(app_handle, task_id, video_id, "failed", 0.0, None, Some(&error), file_name);
                    Self::spawn_mark_failed(db.clone(), task_id, &error);
                    return Err(error);
                }
            }
        }
    }

    // ==================== Internal: Helpers ====================

    /// Update running task status/progress (quick lock, no await)
    fn update_running(inner: &std::sync::Mutex<QueueInner>, status: &str, progress: f64) {
        if let Some(ref mut r) = inner.lock_safe().running {
            r.3 = status.to_string();
            r.4 = progress;
        }
    }

    /// Check if a task is cancelled (quick lock, no await)
    fn check_cancelled(inner: &std::sync::Mutex<QueueInner>, task_id: i64) -> bool {
        inner.lock_safe().cancelled_ids.contains(&task_id)
    }

    /// Spawn a fire-and-forget DB update to mark a task as failed
    fn spawn_mark_failed(db: Database, task_id: i64, error: &str) {
        let error = error.replace('\'', "''");
        tauri::async_runtime::spawn(async move {
            let _ = sea_orm::ConnectionTrait::execute_unprepared(
                db.conn(),
                &format!(
                    "UPDATE asr_tasks SET status = 'failed', error_message = '{}' WHERE id = {}",
                    error, task_id
                ),
            )
            .await;
        });
    }

    async fn do_mark_cancelled(
        db: &Database,
        app_handle: &AppHandle,
        task_id: i64,
        video_id: i64,
        file_name: &str,
    ) {
        let _ = sea_orm::ConnectionTrait::execute_unprepared(
            db.conn(),
            &format!("UPDATE asr_tasks SET status = 'cancelled' WHERE id = {}", task_id),
        )
        .await;
        Self::emit_progress(app_handle, task_id, video_id, "cancelled", 0.0, Some("已取消"), None, file_name);
        tracing::info!("ASR task {} cancelled", task_id);
    }

    async fn build_provider(db: &Database) -> Result<Arc<dyn ASRProvider>, String> {
        let mode = service::read_setting_from_db(db, "asr_mode")
            .await
            .unwrap_or("local".to_string());

        match mode.as_str() {
            "disabled" => Err("ASR 功能已禁用".to_string()),
            "remote" => {
                let url = service::read_setting_from_db(db, "asr_url")
                    .await
                    .ok_or("请先配置远程 ASR 地址")?;
                let api_key = service::read_setting_from_db(db, "asr_api_key").await;
                Ok(Arc::new(RemoteASRProvider::new(&url, api_key)))
            }
            _ => {
                let port: u16 = service::read_setting_from_db(db, "asr_port")
                    .await
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(8765);
                Ok(Arc::new(LocalASRProvider::new(port)))
            }
        }
    }

    fn emit_progress(
        app_handle: &AppHandle,
        task_id: i64,
        video_id: i64,
        status: &str,
        progress: f64,
        message: Option<&str>,
        error_message: Option<&str>,
        video_file_name: &str,
    ) {
        let _ = app_handle.emit(
            "asr-task-progress",
            ASRTaskProgressEvent {
                task_id,
                video_id,
                status: status.to_string(),
                progress,
                message: message.map(String::from),
                error_message: error_message.map(String::from),
                video_file_name: video_file_name.to_string(),
            },
        );
    }
}
