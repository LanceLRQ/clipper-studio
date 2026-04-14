use std::collections::HashMap;
use std::sync::Arc;
use tauri::Emitter;
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

/// A runnable task definition
pub struct TaskDefinition {
    pub task_id: TaskId,
    pub cancel_token: CancellationToken,
    pub handler: Box<dyn FnOnce(CancellationToken, TaskProgressSender) -> tokio::task::JoinHandle<Result<(), String>> + Send>,
}

/// Sender for progress updates from within a task
pub type TaskProgressSender = mpsc::UnboundedSender<TaskProgressEvent>;

/// Task queue that manages async task execution with progress reporting.
///
/// Design:
/// - Tasks are submitted and executed in order
/// - Each task gets a CancellationToken for cancellation support
/// - Progress events are forwarded to the Tauri event system
/// - Max concurrent tasks is configurable (default: 2)
pub struct TaskQueue {
    app_handle: tauri::AppHandle,
    cancel_tokens: Arc<Mutex<HashMap<TaskId, CancellationToken>>>,
    semaphore: Arc<tokio::sync::Semaphore>,
}

impl TaskQueue {
    pub fn new(app_handle: tauri::AppHandle, max_concurrent: usize) -> Self {
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

        let (progress_tx, mut progress_rx) = mpsc::unbounded_channel::<TaskProgressEvent>();
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
            let _permit = semaphore.acquire().await.unwrap();

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
}
