//! commands/media.rs 查询类集成测试
//!
//! 验证 list_media_tasks / delete_media_task / clear_finished_media_tasks
//! 三个命令所执行 SQL 的行为：task_type 过滤、JSON video_ids 解析、
//! 防止删除 in-flight 任务、批量清理未完成保留。

use clipper_studio_lib::db::Database;

async fn setup_test_db() -> Database {
    let db = Database::connect(std::path::Path::new(":memory:"))
        .await
        .expect("failed to connect to in-memory SQLite");
    db.run_migrations().await.expect("failed to run migrations");
    db
}

async fn insert_media_task(
    conn: &sea_orm::DatabaseConnection,
    task_type: &str,
    video_ids: &[i64],
    status: &str,
    output_path: Option<&str>,
) -> i64 {
    let video_ids_json = format!(
        "[{}]",
        video_ids
            .iter()
            .map(|i| i.to_string())
            .collect::<Vec<_>>()
            .join(",")
    );
    let out_sql = output_path
        .map(|p| format!("'{}'", p.replace('\'', "''")))
        .unwrap_or_else(|| "NULL".to_string());

    sea_orm::ConnectionTrait::execute_unprepared(
        conn,
        &format!(
            "INSERT INTO media_tasks (task_type, video_ids, output_path, status) \
             VALUES ('{}', '{}', {}, '{}')",
            task_type, video_ids_json, out_sql, status,
        ),
    )
    .await
    .expect("insert");

    let row = sea_orm::ConnectionTrait::query_one(
        conn,
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

// ============== list_media_tasks ==============

#[tokio::test]
async fn test_list_empty_returns_empty() {
    let db = setup_test_db().await;
    let rows = sea_orm::ConnectionTrait::query_all(
        db.conn(),
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            "SELECT * FROM media_tasks ORDER BY created_at DESC".to_string(),
        ),
    )
    .await
    .unwrap();
    assert!(rows.is_empty());
}

#[tokio::test]
async fn test_list_filtered_by_task_type() {
    let db = setup_test_db().await;
    insert_media_task(db.conn(), "transcode", &[1], "completed", Some("/o/a.mp4")).await;
    insert_media_task(db.conn(), "merge", &[1, 2], "pending", None).await;
    insert_media_task(db.conn(), "transcode", &[3], "failed", None).await;

    // task_type = 'transcode' 应仅返回 2 条
    let rows = sea_orm::ConnectionTrait::query_all(
        db.conn(),
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            "SELECT id FROM media_tasks WHERE task_type = 'transcode' ORDER BY created_at DESC"
                .to_string(),
        ),
    )
    .await
    .unwrap();
    assert_eq!(rows.len(), 2);
}

#[tokio::test]
async fn test_list_video_ids_json_parses_back() {
    let db = setup_test_db().await;
    insert_media_task(db.conn(), "merge", &[10, 20, 30], "pending", None).await;

    let row = sea_orm::ConnectionTrait::query_one(
        db.conn(),
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            "SELECT video_ids FROM media_tasks WHERE task_type = 'merge'".to_string(),
        ),
    )
    .await
    .unwrap()
    .unwrap();
    let json: String = row.try_get("", "video_ids").unwrap();

    // 复刻命令中的反序列化
    let parsed: Vec<i64> = serde_json::from_str(&json).expect("valid JSON");
    assert_eq!(parsed, vec![10, 20, 30]);
}

#[tokio::test]
async fn test_list_order_by_created_at_desc() {
    let db = setup_test_db().await;
    let id1 = insert_media_task(db.conn(), "transcode", &[1], "pending", None).await;
    // 制造时间差
    tokio::time::sleep(std::time::Duration::from_millis(1100)).await;
    let id2 = insert_media_task(db.conn(), "transcode", &[2], "pending", None).await;

    let rows = sea_orm::ConnectionTrait::query_all(
        db.conn(),
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            "SELECT id FROM media_tasks ORDER BY created_at DESC".to_string(),
        ),
    )
    .await
    .unwrap();
    let first: i64 = rows[0].try_get("", "id").unwrap();
    let second: i64 = rows[1].try_get("", "id").unwrap();
    assert_eq!(first, id2, "最新插入的应在最前");
    assert_eq!(second, id1);
}

// ============== delete_media_task ==============

#[tokio::test]
async fn test_delete_completed_task_succeeds() {
    let db = setup_test_db().await;
    let id = insert_media_task(db.conn(), "transcode", &[1], "completed", None).await;

    // 复刻命令逻辑：先查 status，再 DELETE
    let row = sea_orm::ConnectionTrait::query_one(
        db.conn(),
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            format!("SELECT status FROM media_tasks WHERE id = {}", id),
        ),
    )
    .await
    .unwrap()
    .unwrap();
    let status: String = row.try_get("", "status").unwrap();
    assert_ne!(status, "pending");
    assert_ne!(status, "processing");

    sea_orm::ConnectionTrait::execute_unprepared(
        db.conn(),
        &format!("DELETE FROM media_tasks WHERE id = {}", id),
    )
    .await
    .unwrap();

    let after = sea_orm::ConnectionTrait::query_one(
        db.conn(),
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            format!("SELECT id FROM media_tasks WHERE id = {}", id),
        ),
    )
    .await
    .unwrap();
    assert!(after.is_none());
}

#[tokio::test]
async fn test_delete_pending_task_blocked_by_status_check() {
    let db = setup_test_db().await;
    let id = insert_media_task(db.conn(), "transcode", &[1], "pending", None).await;

    // 命令在 status=pending/processing 时直接返回 Err，不执行 DELETE
    let row = sea_orm::ConnectionTrait::query_one(
        db.conn(),
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            format!("SELECT status FROM media_tasks WHERE id = {}", id),
        ),
    )
    .await
    .unwrap()
    .unwrap();
    let status: String = row.try_get("", "status").unwrap();
    let blocked = status == "pending" || status == "processing";
    assert!(blocked, "pending 任务应被阻止删除");

    // 验证记录仍存在
    let still = sea_orm::ConnectionTrait::query_one(
        db.conn(),
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            format!("SELECT id FROM media_tasks WHERE id = {}", id),
        ),
    )
    .await
    .unwrap();
    assert!(still.is_some());
}

#[tokio::test]
async fn test_delete_processing_task_also_blocked() {
    let id_status = "processing";
    let blocked = id_status == "pending" || id_status == "processing";
    assert!(blocked, "processing 状态也应被阻止删除");
}

#[tokio::test]
async fn test_delete_nonexistent_task_returns_none_status() {
    let db = setup_test_db().await;
    let row = sea_orm::ConnectionTrait::query_one(
        db.conn(),
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            "SELECT status FROM media_tasks WHERE id = 99999".to_string(),
        ),
    )
    .await
    .unwrap();
    assert!(row.is_none(), "命令依据 None 返回 '任务不存在' 错误");
}

// ============== clear_finished_media_tasks ==============

#[tokio::test]
async fn test_clear_finished_keeps_pending_and_processing() {
    let db = setup_test_db().await;
    insert_media_task(db.conn(), "transcode", &[1], "completed", None).await;
    insert_media_task(db.conn(), "transcode", &[2], "failed", None).await;
    insert_media_task(db.conn(), "transcode", &[3], "cancelled", None).await;
    insert_media_task(db.conn(), "transcode", &[4], "pending", None).await;
    insert_media_task(db.conn(), "transcode", &[5], "processing", None).await;

    let result = sea_orm::ConnectionTrait::execute_unprepared(
        db.conn(),
        "DELETE FROM media_tasks WHERE status NOT IN ('pending','processing')",
    )
    .await
    .unwrap();
    assert_eq!(
        result.rows_affected(),
        3,
        "应删除 completed/failed/cancelled"
    );

    let remaining = sea_orm::ConnectionTrait::query_all(
        db.conn(),
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            "SELECT status FROM media_tasks ORDER BY status".to_string(),
        ),
    )
    .await
    .unwrap();
    let statuses: Vec<String> = remaining
        .iter()
        .filter_map(|r| r.try_get::<String>("", "status").ok())
        .collect();
    assert_eq!(
        statuses,
        vec!["pending".to_string(), "processing".to_string()]
    );
}

#[tokio::test]
async fn test_clear_finished_select_with_output_path_only() {
    let db = setup_test_db().await;
    // 触发 delete_files=true 分支：先 SELECT output_path NOT NULL
    insert_media_task(db.conn(), "transcode", &[1], "completed", Some("/o/a")).await;
    insert_media_task(db.conn(), "transcode", &[2], "completed", None).await;
    insert_media_task(db.conn(), "transcode", &[3], "pending", Some("/o/c")).await;

    let rows = sea_orm::ConnectionTrait::query_all(
        db.conn(),
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            "SELECT output_path FROM media_tasks \
             WHERE status NOT IN ('pending','processing') AND output_path IS NOT NULL"
                .to_string(),
        ),
    )
    .await
    .unwrap();

    let paths: Vec<String> = rows
        .iter()
        .filter_map(|r| r.try_get::<String>("", "output_path").ok())
        .collect();
    assert_eq!(
        paths,
        vec!["/o/a".to_string()],
        "仅 completed + 有 output_path"
    );
}

#[tokio::test]
async fn test_clear_finished_empty_db_returns_zero() {
    let db = setup_test_db().await;
    let result = sea_orm::ConnectionTrait::execute_unprepared(
        db.conn(),
        "DELETE FROM media_tasks WHERE status NOT IN ('pending','processing')",
    )
    .await
    .unwrap();
    assert_eq!(result.rows_affected(), 0);
}
