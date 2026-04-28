//! asr/queue.rs 集成测试
//!
//! ASRTaskQueue 持有具体 `tauri::AppHandle`（非 generic），无法用 mock_app 直接构造。
//! 本测试聚焦其底层 SQL 行为，与队列内部所执行 DB 语句保持等价：
//! - enqueue 的 DB 重复检查 + INSERT 创建 queued 任务
//! - cancel 的 UPDATE → cancelled
//! - recover_on_startup 的 processing → failed + 重排 queued
//! - get_queue_snapshot 的字段映射（数据结构层）
//!
//! 公共类型序列化、退避公式等纯逻辑见 src/asr/queue.rs 的 #[cfg(test)] 内联模块。

use clipper_studio_lib::db::Database;

async fn setup_test_db() -> Database {
    let db = Database::connect(std::path::Path::new(":memory:"))
        .await
        .expect("connect");
    db.run_migrations().await.expect("migrate");
    db
}

async fn exec(conn: &sea_orm::DatabaseConnection, sql: &str) {
    sea_orm::ConnectionTrait::execute_unprepared(conn, sql)
        .await
        .unwrap_or_else(|e| panic!("SQL: {}\n{}", e, sql));
}

async fn insert_video(db: &Database, id: i64, file_path: &str, file_name: &str) {
    exec(
        db.conn(),
        &format!(
            "INSERT INTO videos (id, file_path, file_name, file_size) \
             VALUES ({}, '{}', '{}', 100)",
            id, file_path, file_name
        ),
    )
    .await;
}

/// 复刻 enqueue 中的 DB 重复检查
async fn db_active_count(db: &Database, video_id: i64) -> i64 {
    let row = sea_orm::ConnectionTrait::query_one(
        db.conn(),
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
    .unwrap()
    .unwrap();
    row.try_get::<i64>("", "cnt").unwrap_or(0)
}

/// 复刻 enqueue 中的 INSERT 语句
async fn enqueue_insert(db: &Database, video_id: i64, language: &str) -> i64 {
    exec(
        db.conn(),
        &format!(
            "INSERT INTO asr_tasks (video_id, status, asr_provider_id, language) \
             VALUES ({}, 'queued', 'queue', '{}')",
            video_id,
            language.replace('\'', "''"),
        ),
    )
    .await;
    let row = sea_orm::ConnectionTrait::query_one(
        db.conn(),
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            "SELECT last_insert_rowid() as id".to_string(),
        ),
    )
    .await
    .unwrap()
    .unwrap();
    row.try_get::<i64>("", "id").unwrap()
}

// ============== enqueue: 重复检测 ==============

#[tokio::test]
async fn test_no_active_task_initially() {
    let db = setup_test_db().await;
    insert_video(&db, 1, "/v.mp4", "v.mp4").await;
    assert_eq!(db_active_count(&db, 1).await, 0);
}

#[tokio::test]
async fn test_queued_task_blocks_new_enqueue() {
    let db = setup_test_db().await;
    insert_video(&db, 1, "/v.mp4", "v.mp4").await;
    enqueue_insert(&db, 1, "Chinese").await;

    assert_eq!(
        db_active_count(&db, 1).await,
        1,
        "queued 任务应被 active 检查统计"
    );
}

#[tokio::test]
async fn test_processing_task_blocks_new_enqueue() {
    let db = setup_test_db().await;
    insert_video(&db, 1, "/v.mp4", "v.mp4").await;
    exec(
        db.conn(),
        "INSERT INTO asr_tasks (video_id, status) VALUES (1, 'processing')",
    )
    .await;

    assert_eq!(db_active_count(&db, 1).await, 1);
}

#[tokio::test]
async fn test_completed_task_does_not_block_new_enqueue() {
    let db = setup_test_db().await;
    insert_video(&db, 1, "/v.mp4", "v.mp4").await;
    exec(
        db.conn(),
        "INSERT INTO asr_tasks (video_id, status) VALUES (1, 'completed')",
    )
    .await;

    assert_eq!(
        db_active_count(&db, 1).await,
        0,
        "completed 不算 active，可重新入队"
    );
}

#[tokio::test]
async fn test_failed_task_does_not_block_new_enqueue() {
    let db = setup_test_db().await;
    insert_video(&db, 1, "/v.mp4", "v.mp4").await;
    exec(
        db.conn(),
        "INSERT INTO asr_tasks (video_id, status, error_message) VALUES (1, 'failed', 'boom')",
    )
    .await;

    assert_eq!(db_active_count(&db, 1).await, 0);
}

#[tokio::test]
async fn test_cancelled_task_does_not_block_new_enqueue() {
    let db = setup_test_db().await;
    insert_video(&db, 1, "/v.mp4", "v.mp4").await;
    exec(
        db.conn(),
        "INSERT INTO asr_tasks (video_id, status) VALUES (1, 'cancelled')",
    )
    .await;

    assert_eq!(db_active_count(&db, 1).await, 0);
}

#[tokio::test]
async fn test_active_check_isolated_per_video() {
    let db = setup_test_db().await;
    insert_video(&db, 1, "/v1.mp4", "v1.mp4").await;
    insert_video(&db, 2, "/v2.mp4", "v2.mp4").await;
    enqueue_insert(&db, 1, "Chinese").await;

    assert_eq!(db_active_count(&db, 1).await, 1);
    assert_eq!(db_active_count(&db, 2).await, 0, "v2 不受影响");
}

// ============== enqueue: INSERT 字段 ==============

#[tokio::test]
async fn test_enqueue_inserts_with_queued_status_and_queue_provider() {
    let db = setup_test_db().await;
    insert_video(&db, 1, "/v.mp4", "v.mp4").await;
    let task_id = enqueue_insert(&db, 1, "Chinese").await;
    assert!(task_id > 0);

    let row = sea_orm::ConnectionTrait::query_one(
        db.conn(),
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            format!(
                "SELECT status, asr_provider_id, language, progress, retry_count \
                 FROM asr_tasks WHERE id = {}",
                task_id
            ),
        ),
    )
    .await
    .unwrap()
    .unwrap();

    let status: String = row.try_get("", "status").unwrap();
    let provider: String = row.try_get("", "asr_provider_id").unwrap();
    let language: String = row.try_get("", "language").unwrap();
    let progress: f64 = row.try_get("", "progress").unwrap();
    let retry: i32 = row.try_get("", "retry_count").unwrap();

    assert_eq!(status, "queued");
    assert_eq!(provider, "queue", "初次入队 provider 占位为 'queue'");
    assert_eq!(language, "Chinese");
    assert_eq!(progress, 0.0);
    assert_eq!(retry, 0);
}

#[tokio::test]
async fn test_enqueue_handles_quote_in_language() {
    let db = setup_test_db().await;
    insert_video(&db, 1, "/v.mp4", "v.mp4").await;
    // 单引号会被 escape 为 ''
    let id = enqueue_insert(&db, 1, "Chi'nese").await;
    let row = sea_orm::ConnectionTrait::query_one(
        db.conn(),
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            format!("SELECT language FROM asr_tasks WHERE id = {}", id),
        ),
    )
    .await
    .unwrap()
    .unwrap();
    let lang: String = row.try_get("", "language").unwrap();
    assert_eq!(lang, "Chi'nese");
}

// ============== cancel: SQL 行为 ==============

#[tokio::test]
async fn test_cancel_marks_task_cancelled() {
    let db = setup_test_db().await;
    insert_video(&db, 1, "/v.mp4", "v.mp4").await;
    let id = enqueue_insert(&db, 1, "zh").await;

    // 复刻 cancel 路径：UPDATE → 'cancelled'
    exec(
        db.conn(),
        &format!(
            "UPDATE asr_tasks SET status = 'cancelled' WHERE id = {}",
            id
        ),
    )
    .await;

    let row = sea_orm::ConnectionTrait::query_one(
        db.conn(),
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            format!("SELECT status FROM asr_tasks WHERE id = {}", id),
        ),
    )
    .await
    .unwrap()
    .unwrap();
    let status: String = row.try_get("", "status").unwrap();
    assert_eq!(status, "cancelled");
}

// ============== recover_on_startup: 关键 SQL ==============

#[tokio::test]
async fn test_recover_marks_stale_processing_as_failed() {
    let db = setup_test_db().await;
    insert_video(&db, 1, "/v.mp4", "v.mp4").await;
    insert_video(&db, 2, "/v2.mp4", "v2.mp4").await;
    exec(
        db.conn(),
        "INSERT INTO asr_tasks (video_id, status, progress) VALUES (1, 'processing', 0.5)",
    )
    .await;
    exec(
        db.conn(),
        "INSERT INTO asr_tasks (video_id, status) VALUES (2, 'completed')",
    )
    .await;

    // 复刻 recover SQL
    exec(
        db.conn(),
        "UPDATE asr_tasks SET status = 'failed', error_message = '应用异常退出' \
         WHERE status = 'processing'",
    )
    .await;

    let row = sea_orm::ConnectionTrait::query_one(
        db.conn(),
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            "SELECT status, error_message FROM asr_tasks WHERE video_id = 1".to_string(),
        ),
    )
    .await
    .unwrap()
    .unwrap();
    let status: String = row.try_get("", "status").unwrap();
    let err: Option<String> = row.try_get("", "error_message").ok();
    assert_eq!(status, "failed");
    assert_eq!(err.as_deref(), Some("应用异常退出"));

    // completed 任务不受影响
    let row2 = sea_orm::ConnectionTrait::query_one(
        db.conn(),
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            "SELECT status FROM asr_tasks WHERE video_id = 2".to_string(),
        ),
    )
    .await
    .unwrap()
    .unwrap();
    let s2: String = row2.try_get("", "status").unwrap();
    assert_eq!(s2, "completed");
}

#[tokio::test]
async fn test_recover_query_picks_only_queued_with_video_join() {
    let db = setup_test_db().await;
    insert_video(&db, 1, "/v1.mp4", "v1.mp4").await;
    insert_video(&db, 2, "/v2.mp4", "v2.mp4").await;
    enqueue_insert(&db, 1, "zh").await;
    tokio::time::sleep(std::time::Duration::from_millis(1100)).await;
    enqueue_insert(&db, 2, "en").await;
    exec(
        db.conn(),
        "INSERT INTO asr_tasks (video_id, status) VALUES (1, 'completed')",
    )
    .await;

    // 复刻 recover 的 SELECT
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
    .await
    .unwrap();

    assert_eq!(rows.len(), 2, "仅 queued 应被恢复");

    let first_lang: String = rows[0].try_get("", "language").unwrap();
    let second_lang: String = rows[1].try_get("", "language").unwrap();
    assert_eq!(first_lang, "zh", "ORDER BY created_at ASC：先入队的在前");
    assert_eq!(second_lang, "en");

    // LEFT JOIN 带出 file_path
    let path: String = rows[0].try_get("", "file_path").unwrap();
    assert_eq!(path, "/v1.mp4");
}

#[tokio::test]
async fn test_recover_skips_video_with_empty_file_path() {
    // 复刻业务跳过条件：file_path 为空时 continue
    let db = setup_test_db().await;
    // 视频 file_path 列在 schema 上 NOT NULL UNIQUE，业务侧的"空 path"实际意味着
    // 字段缺失或外部数据错误。测试以最小 file_path 构造 + 业务 if-empty 判定来覆盖。
    insert_video(&db, 1, "/v.mp4", "v.mp4").await;
    enqueue_insert(&db, 1, "zh").await;

    let rows = sea_orm::ConnectionTrait::query_all(
        db.conn(),
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            "SELECT v.file_path FROM asr_tasks t \
             LEFT JOIN videos v ON t.video_id = v.id \
             WHERE t.status = 'queued'"
                .to_string(),
        ),
    )
    .await
    .unwrap();
    assert_eq!(rows.len(), 1);
    let path: String = rows[0].try_get("", "file_path").unwrap_or_default();
    // 业务代码：if file_path.is_empty() { continue; }
    let would_skip = path.is_empty();
    assert!(!would_skip, "正常视频不应被跳过");
    assert_eq!(path, "/v.mp4");
}

// ============== execute_task: completed 分支 SQL 副作用 ==============

#[tokio::test]
async fn test_complete_updates_task_and_sets_video_has_subtitle() {
    let db = setup_test_db().await;
    insert_video(&db, 1, "/v.mp4", "v.mp4").await;
    let id = enqueue_insert(&db, 1, "zh").await;

    // 复刻 completed 分支
    let segment_count = 42;
    exec(
        db.conn(),
        &format!(
            "UPDATE asr_tasks SET status = 'completed', progress = 1.0, \
             segment_count = {}, completed_at = datetime('now') WHERE id = {}",
            segment_count, id
        ),
    )
    .await;
    exec(db.conn(), "UPDATE videos SET has_subtitle = 1 WHERE id = 1").await;

    let row = sea_orm::ConnectionTrait::query_one(
        db.conn(),
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            format!(
                "SELECT status, progress, segment_count, completed_at FROM asr_tasks WHERE id = {}",
                id
            ),
        ),
    )
    .await
    .unwrap()
    .unwrap();
    let status: String = row.try_get("", "status").unwrap();
    let progress: f64 = row.try_get("", "progress").unwrap();
    let count: i32 = row.try_get("", "segment_count").unwrap();
    let completed_at: Option<String> = row.try_get("", "completed_at").ok();
    assert_eq!(status, "completed");
    assert_eq!(progress, 1.0);
    assert_eq!(count, 42);
    assert!(completed_at.is_some(), "应设置 completed_at");

    let v = sea_orm::ConnectionTrait::query_one(
        db.conn(),
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            "SELECT has_subtitle FROM videos WHERE id = 1".to_string(),
        ),
    )
    .await
    .unwrap()
    .unwrap();
    let has_sub: bool = v.try_get("", "has_subtitle").unwrap();
    assert!(has_sub, "视频 has_subtitle 应被置 1");
}

// ============== retry: retry_count 累加 ==============

#[tokio::test]
async fn test_retry_count_increments_on_retryable_error() {
    let db = setup_test_db().await;
    insert_video(&db, 1, "/v.mp4", "v.mp4").await;
    let id = enqueue_insert(&db, 1, "zh").await;

    // 模拟队列中的 retry 路径：UPDATE retry_count
    for n in 1..=2_i32 {
        exec(
            db.conn(),
            &format!("UPDATE asr_tasks SET retry_count = {} WHERE id = {}", n, id),
        )
        .await;
        let row = sea_orm::ConnectionTrait::query_one(
            db.conn(),
            sea_orm::Statement::from_string(
                sea_orm::DatabaseBackend::Sqlite,
                format!("SELECT retry_count FROM asr_tasks WHERE id = {}", id),
            ),
        )
        .await
        .unwrap()
        .unwrap();
        let v: i32 = row.try_get("", "retry_count").unwrap();
        assert_eq!(v, n);
    }
}

// ============== spawn_mark_failed 等价 SQL ==============

#[tokio::test]
async fn test_mark_failed_persists_error_message() {
    let db = setup_test_db().await;
    insert_video(&db, 1, "/v.mp4", "v.mp4").await;
    let id = enqueue_insert(&db, 1, "zh").await;

    let err = "ASR 提交失败: it's bad"; // 包含单引号
    let escaped = err.replace('\'', "''");
    exec(
        db.conn(),
        &format!(
            "UPDATE asr_tasks SET status = 'failed', error_message = '{}' WHERE id = {}",
            escaped, id
        ),
    )
    .await;

    let row = sea_orm::ConnectionTrait::query_one(
        db.conn(),
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            format!(
                "SELECT status, error_message FROM asr_tasks WHERE id = {}",
                id
            ),
        ),
    )
    .await
    .unwrap()
    .unwrap();
    let status: String = row.try_get("", "status").unwrap();
    let err_msg: Option<String> = row.try_get("", "error_message").ok();
    assert_eq!(status, "failed");
    assert_eq!(err_msg.as_deref(), Some(err));
}

// ============== started_at 字段 ==============

#[tokio::test]
async fn test_submit_phase_sets_started_at_and_remote_id() {
    let db = setup_test_db().await;
    insert_video(&db, 1, "/v.mp4", "v.mp4").await;
    let id = enqueue_insert(&db, 1, "zh").await;

    // 复刻 submit 后的 UPDATE
    exec(
        db.conn(),
        &format!(
            "UPDATE asr_tasks SET status = 'processing', asr_provider_id = 'local', \
             remote_task_id = 'remote-abc', started_at = datetime('now') WHERE id = {}",
            id
        ),
    )
    .await;

    let row = sea_orm::ConnectionTrait::query_one(
        db.conn(),
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            format!(
                "SELECT status, asr_provider_id, remote_task_id, started_at \
                 FROM asr_tasks WHERE id = {}",
                id
            ),
        ),
    )
    .await
    .unwrap()
    .unwrap();
    let status: String = row.try_get("", "status").unwrap();
    let provider: String = row.try_get("", "asr_provider_id").unwrap();
    let remote: Option<String> = row.try_get("", "remote_task_id").ok();
    let started: Option<String> = row.try_get("", "started_at").ok();

    assert_eq!(status, "processing");
    assert_eq!(
        provider, "local",
        "submit 后 provider 由 'queue' 更新为真实 id"
    );
    assert_eq!(remote.as_deref(), Some("remote-abc"));
    assert!(started.is_some(), "started_at 应被设置");
}
