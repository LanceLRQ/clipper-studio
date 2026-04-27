use std::collections::HashMap;
use std::sync::Arc;
use tauri::{Emitter, Runtime};
use tokio::sync::{mpsc, Mutex};
use tokio_util::sync::CancellationToken;

/// Unique task identifier
pub type TaskId = i64;

/// Task status
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Pending,
    Processing,
    Completed,
    Failed,
    Cancelled,
}

/// Task progress event, emitted via Tauri Event
#[derive(Debug, Clone, serde::Serialize)]
pub struct TaskProgressEvent {
    pub task_id: TaskId,
    pub status: TaskStatus,
    pub progress: f64,
    pub message: String,
}

/// Task handler closure: receives cancel token + progress sender, returns spawned join handle
pub type TaskHandler = Box<
    dyn FnOnce(CancellationToken, TaskProgressSender) -> tokio::task::JoinHandle<Result<(), String>>
        + Send,
>;

/// A runnable task definition
pub struct TaskDefinition {
    pub task_id: TaskId,
    pub cancel_token: CancellationToken,
    pub handler: TaskHandler,
}

/// Sender for progress updates from within a task
///
/// 使用有界 channel（容量 [`PROGRESS_CHANNEL_CAPACITY`]）+ `try_send` 策略：
/// 当前端消费慢时新事件被丢弃而不是无限堆积，防止长时间运行任务耗尽内存。
pub type TaskProgressSender = mpsc::Sender<TaskProgressEvent>;

/// 进度事件 channel 容量：足够容纳若干节流后的进度批次，满时 try_send 丢弃最新事件
pub const PROGRESS_CHANNEL_CAPACITY: usize = 100;

/// Task queue that manages async task execution with progress reporting.
///
/// Design:
/// - Tasks are submitted and executed in order
/// - Each task gets a CancellationToken for cancellation support
/// - Progress events are forwarded to the Tauri event system
/// - Max concurrent tasks is configurable (default: 2)
pub struct TaskQueue<R: Runtime = tauri::Wry> {
    app_handle: tauri::AppHandle<R>,
    cancel_tokens: Arc<Mutex<HashMap<TaskId, CancellationToken>>>,
    semaphore: Arc<tokio::sync::Semaphore>,
}

impl<R: Runtime> TaskQueue<R> {
    pub fn new(app_handle: tauri::AppHandle<R>, max_concurrent: usize) -> Self {
        Self {
            app_handle,
            cancel_tokens: Arc::new(Mutex::new(HashMap::new())),
            semaphore: Arc::new(tokio::sync::Semaphore::new(max_concurrent)),
        }
    }

    /// Submit a task for execution.
    /// The handler receives a CancellationToken and a progress sender.
    pub async fn submit<F, Fut>(&self, task_id: TaskId, handler: F)
    where
        F: FnOnce(CancellationToken, TaskProgressSender) -> Fut + Send + 'static,
        Fut: std::future::Future<Output = Result<(), String>> + Send + 'static,
    {
        let cancel_token = CancellationToken::new();

        // Store cancel token
        {
            let mut tokens = self.cancel_tokens.lock().await;
            tokens.insert(task_id, cancel_token.clone());
        }

        let (progress_tx, mut progress_rx) =
            mpsc::channel::<TaskProgressEvent>(PROGRESS_CHANNEL_CAPACITY);
        let app_handle = self.app_handle.clone();
        let cancel_tokens = self.cancel_tokens.clone();
        let semaphore = self.semaphore.clone();

        // Spawn progress forwarder (Tauri Event)
        let app_handle_progress = app_handle.clone();
        tokio::spawn(async move {
            while let Some(event) = progress_rx.recv().await {
                let _ = app_handle_progress.emit("task-progress", &event);
            }
        });

        // Spawn task executor
        tokio::spawn(async move {
            // Wait for semaphore permit (concurrency limit)
            let _permit = match semaphore.acquire().await {
                Ok(p) => p,
                Err(_) => {
                    tracing::warn!("Task {} abandoned: task queue is closing", task_id);
                    return;
                }
            };

            // Emit processing status
            let _ = app_handle.emit(
                "task-progress",
                &TaskProgressEvent {
                    task_id,
                    status: TaskStatus::Processing,
                    progress: 0.0,
                    message: "处理中...".to_string(),
                },
            );

            let result = handler(cancel_token.clone(), progress_tx).await;

            // Emit final status
            let final_event = match &result {
                Ok(()) => TaskProgressEvent {
                    task_id,
                    status: TaskStatus::Completed,
                    progress: 1.0,
                    message: "完成".to_string(),
                },
                Err(e) if e == "Task cancelled" => TaskProgressEvent {
                    task_id,
                    status: TaskStatus::Cancelled,
                    progress: 0.0,
                    message: "已取消".to_string(),
                },
                Err(e) => TaskProgressEvent {
                    task_id,
                    status: TaskStatus::Failed,
                    progress: 0.0,
                    message: e.clone(),
                },
            };

            let _ = app_handle.emit("task-progress", &final_event);

            // Cleanup cancel token
            let mut tokens = cancel_tokens.lock().await;
            tokens.remove(&task_id);
        });
    }

    /// Check if there are any active (pending/processing) tasks (async version)
    pub async fn has_active_tasks(&self) -> bool {
        !self.cancel_tokens.lock().await.is_empty()
    }

    /// Check if there are any active tasks (sync, non-blocking, for use in window event handlers)
    pub fn has_active_tasks_sync(&self) -> bool {
        self.cancel_tokens
            .try_lock()
            .map(|tokens| !tokens.is_empty())
            .unwrap_or(false)
    }

    /// Cancel a running task
    pub async fn cancel(&self, task_id: TaskId) -> bool {
        let tokens = self.cancel_tokens.lock().await;
        if let Some(token) = tokens.get(&task_id) {
            token.cancel();
            tracing::info!("Task {} cancelled", task_id);
            true
        } else {
            false
        }
    }

    /// Cancel all running tasks (used during application shutdown)
    pub async fn cancel_all(&self) {
        let tokens = self.cancel_tokens.lock().await;
        let count = tokens.len();
        for (task_id, token) in tokens.iter() {
            token.cancel();
            tracing::info!("Task {} cancelled (shutdown)", task_id);
        }
        if count > 0 {
            tracing::info!("Cancelled {} active tasks during shutdown", count);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    // ==================== TaskStatus / TaskProgressEvent serialization ====================

    #[test]
    fn test_task_status_serialize_snake_case() {
        assert_eq!(
            serde_json::to_string(&TaskStatus::Pending).unwrap(),
            "\"pending\""
        );
        assert_eq!(
            serde_json::to_string(&TaskStatus::Processing).unwrap(),
            "\"processing\""
        );
        assert_eq!(
            serde_json::to_string(&TaskStatus::Completed).unwrap(),
            "\"completed\""
        );
        assert_eq!(
            serde_json::to_string(&TaskStatus::Failed).unwrap(),
            "\"failed\""
        );
        assert_eq!(
            serde_json::to_string(&TaskStatus::Cancelled).unwrap(),
            "\"cancelled\""
        );
    }

    #[test]
    fn test_task_progress_event_serialize() {
        let evt = TaskProgressEvent {
            task_id: 42,
            status: TaskStatus::Processing,
            progress: 0.55,
            message: "正在处理".to_string(),
        };
        let json = serde_json::to_string(&evt).unwrap();
        assert!(json.contains("\"task_id\":42"));
        assert!(json.contains("\"status\":\"processing\""));
        assert!(json.contains("\"progress\":0.55"));
        assert!(json.contains("正在处理"));
    }

    // 编译期断言：PROGRESS_CHANNEL_CAPACITY 必须 >= 32，否则编译失败
    const _: () = assert!(PROGRESS_CHANNEL_CAPACITY >= 32);

    // ==================== TaskQueue behavior (with mock app) ====================

    fn make_queue(max_concurrent: usize) -> TaskQueue<tauri::test::MockRuntime> {
        let app = tauri::test::mock_app();
        TaskQueue::new(app.handle().clone(), max_concurrent)
    }

    #[tokio::test]
    async fn test_submit_runs_handler_to_completion() {
        let queue = make_queue(2);
        let counter = Arc::new(AtomicUsize::new(0));
        let counter_clone = counter.clone();

        queue
            .submit(1, move |_cancel, _tx| {
                let c = counter_clone.clone();
                async move {
                    c.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                }
            })
            .await;

        // Give the spawned task a moment to run and clean up
        tokio::time::sleep(Duration::from_millis(150)).await;
        assert_eq!(counter.load(Ordering::SeqCst), 1);
        assert!(
            !queue.has_active_tasks().await,
            "queue should be empty after task finishes"
        );
    }

    #[tokio::test]
    async fn test_has_active_tasks_during_run() {
        let queue = make_queue(2);
        let started = Arc::new(tokio::sync::Notify::new());
        let started_clone = started.clone();

        queue
            .submit(10, move |cancel, _tx| {
                let n = started_clone.clone();
                async move {
                    n.notify_one();
                    // Wait until cancelled or 2s timeout
                    tokio::select! {
                        _ = cancel.cancelled() => {}
                        _ = tokio::time::sleep(Duration::from_secs(2)) => {}
                    }
                    Ok(())
                }
            })
            .await;

        started.notified().await;
        assert!(
            queue.has_active_tasks().await,
            "task should be active while running"
        );

        let cancelled = queue.cancel(10).await;
        assert!(cancelled, "cancel should return true for known task");
        // Wait for cleanup
        tokio::time::sleep(Duration::from_millis(150)).await;
        assert!(
            !queue.has_active_tasks().await,
            "task should be cleared after cancel"
        );
    }

    #[tokio::test]
    async fn test_cancel_unknown_task_returns_false() {
        let queue = make_queue(2);
        let result = queue.cancel(9999).await;
        assert!(!result);
    }

    #[tokio::test]
    async fn test_concurrency_limit_serializes_excess_tasks() {
        // Concurrency = 1: tasks must run one at a time.
        let queue = make_queue(1);
        let active = Arc::new(AtomicUsize::new(0));
        let max_seen = Arc::new(AtomicUsize::new(0));

        for id in 0..3 {
            let active_clone = active.clone();
            let max_clone = max_seen.clone();
            queue
                .submit(id, move |_cancel, _tx| async move {
                    let now = active_clone.fetch_add(1, Ordering::SeqCst) + 1;
                    let prev_max = max_clone.load(Ordering::SeqCst);
                    if now > prev_max {
                        max_clone.store(now, Ordering::SeqCst);
                    }
                    tokio::time::sleep(Duration::from_millis(80)).await;
                    active_clone.fetch_sub(1, Ordering::SeqCst);
                    Ok(())
                })
                .await;
        }

        // Wait for all 3 tasks to finish (3 * 80ms = ~240ms + buffer)
        tokio::time::sleep(Duration::from_millis(600)).await;
        assert_eq!(
            max_seen.load(Ordering::SeqCst),
            1,
            "with concurrency=1, no two tasks should run simultaneously"
        );
        assert!(!queue.has_active_tasks().await);
    }

    #[tokio::test]
    async fn test_concurrency_limit_two_tasks_run_in_parallel() {
        let queue = make_queue(2);
        let active = Arc::new(AtomicUsize::new(0));
        let max_seen = Arc::new(AtomicUsize::new(0));

        for id in 0..2 {
            let active_clone = active.clone();
            let max_clone = max_seen.clone();
            queue
                .submit(id, move |_cancel, _tx| async move {
                    let now = active_clone.fetch_add(1, Ordering::SeqCst) + 1;
                    let prev_max = max_clone.load(Ordering::SeqCst);
                    if now > prev_max {
                        max_clone.store(now, Ordering::SeqCst);
                    }
                    tokio::time::sleep(Duration::from_millis(150)).await;
                    active_clone.fetch_sub(1, Ordering::SeqCst);
                    Ok(())
                })
                .await;
        }

        tokio::time::sleep(Duration::from_millis(500)).await;
        assert_eq!(
            max_seen.load(Ordering::SeqCst),
            2,
            "with concurrency=2, both tasks should run together"
        );
    }

    #[tokio::test]
    async fn test_cancel_all_cancels_active_tasks() {
        let queue = make_queue(4);
        let cancelled_count = Arc::new(AtomicUsize::new(0));

        for id in 0..3 {
            let cnt = cancelled_count.clone();
            queue
                .submit(id, move |cancel, _tx| async move {
                    tokio::select! {
                        _ = cancel.cancelled() => {
                            cnt.fetch_add(1, Ordering::SeqCst);
                        }
                        _ = tokio::time::sleep(Duration::from_secs(5)) => {}
                    }
                    Err::<(), _>("Task cancelled".to_string())
                })
                .await;
        }

        // Let all tasks start
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert!(queue.has_active_tasks().await);

        queue.cancel_all().await;

        // Wait for cancellation to propagate
        tokio::time::sleep(Duration::from_millis(200)).await;
        assert_eq!(
            cancelled_count.load(Ordering::SeqCst),
            3,
            "all 3 tasks should observe cancellation"
        );
        assert!(!queue.has_active_tasks().await);
    }

    #[tokio::test]
    async fn test_failed_task_clears_from_active() {
        let queue = make_queue(2);
        queue
            .submit(99, |_cancel, _tx| async move {
                Err::<(), _>("simulated failure".to_string())
            })
            .await;

        tokio::time::sleep(Duration::from_millis(150)).await;
        assert!(
            !queue.has_active_tasks().await,
            "failed task should be cleared from registry"
        );
    }

    #[tokio::test]
    async fn test_progress_sender_is_passed() {
        let queue = make_queue(2);
        let received = Arc::new(AtomicUsize::new(0));
        let received_clone = received.clone();

        queue
            .submit(1, move |_cancel, tx| {
                let cnt = received_clone.clone();
                async move {
                    let evt = TaskProgressEvent {
                        task_id: 1,
                        status: TaskStatus::Processing,
                        progress: 0.5,
                        message: "halfway".to_string(),
                    };
                    if tx.try_send(evt).is_ok() {
                        cnt.fetch_add(1, Ordering::SeqCst);
                    }
                    Ok(())
                }
            })
            .await;

        tokio::time::sleep(Duration::from_millis(150)).await;
        assert_eq!(
            received.load(Ordering::SeqCst),
            1,
            "handler should be able to push to progress channel"
        );
    }

    #[test]
    fn test_has_active_tasks_sync_when_empty() {
        let queue = make_queue(2);
        assert!(!queue.has_active_tasks_sync());
    }
}
