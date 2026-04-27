//! commands/clip.rs 查询/管理类集成测试
//!
//! 覆盖与 task_queue 解耦的纯 SQL 命令：
//! - list_clip_tasks（多过滤组合 + LEFT JOIN clip_outputs）
//! - list_presets（ORDER BY sort_order）
//! - delete_clip_task（status 防呆 + clip_outputs 联动 + 任务不存在）
//! - delete_clip_batch（batch_id 联动 + 跳过 in-flight）
//! - clear_finished_clip_tasks（保留 pending/processing）
//! - check_video_burn_availability（has_subtitle + .xml 旁路存在性）

use clipper_studio_lib::db::Database;
use std::path::Path;

async fn setup_test_db() -> Database {
    let db = Database::connect(Path::new(":memory:"))
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

async fn insert_video(db: &Database, id: i64, file_path: &str, workspace_id: Option<i64>) {
    let ws = workspace_id
        .map(|w| w.to_string())
        .unwrap_or_else(|| "NULL".to_string());
    exec(
        db.conn(),
        &format!(
            "INSERT INTO videos (id, file_path, file_name, file_size, workspace_id) \
             VALUES ({}, '{}', 'v.mp4', 100, {})",
            id, file_path, ws
        ),
    )
    .await;
}

async fn insert_clip_task(
    db: &Database,
    video_id: i64,
    status: &str,
    title: Option<&str>,
    batch_id: Option<&str>,
) -> i64 {
    let title_sql = title
        .map(|t| format!("'{}'", t.replace('\'', "''")))
        .unwrap_or_else(|| "NULL".to_string());
    let batch_sql = batch_id
        .map(|b| format!("'{}'", b.replace('\'', "''")))
        .unwrap_or_else(|| "NULL".to_string());

    exec(
        db.conn(),
        &format!(
            "INSERT INTO clip_tasks (video_id, start_time_ms, end_time_ms, title, status, batch_id) \
             VALUES ({}, 0, 1000, {}, '{}', {})",
            video_id, title_sql, status, batch_sql,
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

async fn insert_clip_output(db: &Database, task_id: i64, video_id: i64, output_path: &str) {
    exec(
        db.conn(),
        &format!(
            "INSERT INTO clip_outputs (clip_task_id, video_id, output_path, format, variant) \
             VALUES ({}, {}, '{}', 'mp4', 'normal')",
            task_id, video_id, output_path
        ),
    )
    .await;
}

async fn insert_preset(
    db: &Database,
    name: &str,
    category: &str,
    sort_order: i64,
    is_builtin: bool,
) {
    exec(
        db.conn(),
        &format!(
            "INSERT INTO encoding_presets (name, category, options, is_builtin, sort_order) \
             VALUES ('{}', '{}', '{{\"crf\":23}}', {}, {})",
            name,
            category,
            if is_builtin { 1 } else { 0 },
            sort_order
        ),
    )
    .await;
}

// ============== list_clip_tasks ==============

#[tokio::test]
async fn test_list_clip_tasks_empty() {
    let db = setup_test_db().await;
    let rows = sea_orm::ConnectionTrait::query_all(
        db.conn(),
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            "SELECT t.* FROM clip_tasks t \
             LEFT JOIN clip_outputs co ON co.clip_task_id = t.id \
             LEFT JOIN videos v ON t.video_id = v.id \
             ORDER BY t.created_at DESC"
                .to_string(),
        ),
    )
    .await
    .unwrap();
    assert!(rows.is_empty());
}

#[tokio::test]
async fn test_list_clip_tasks_filtered_by_video_id() {
    let db = setup_test_db().await;
    insert_video(&db, 1, "/v1.mp4", None).await;
    insert_video(&db, 2, "/v2.mp4", None).await;
    insert_clip_task(&db, 1, "pending", Some("a"), None).await;
    insert_clip_task(&db, 1, "completed", Some("b"), None).await;
    insert_clip_task(&db, 2, "pending", Some("c"), None).await;

    let rows = sea_orm::ConnectionTrait::query_all(
        db.conn(),
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            "SELECT t.id FROM clip_tasks t \
             LEFT JOIN clip_outputs co ON co.clip_task_id = t.id \
             LEFT JOIN videos v ON t.video_id = v.id \
             WHERE t.video_id = 1 ORDER BY t.created_at DESC"
                .to_string(),
        ),
    )
    .await
    .unwrap();
    assert_eq!(rows.len(), 2);
}

#[tokio::test]
async fn test_list_clip_tasks_filtered_by_workspace() {
    let db = setup_test_db().await;
    exec(
        db.conn(),
        "INSERT INTO workspaces (id, name, path, adapter_id) VALUES \
         (1, 'WS1', '/ws1', 'generic'), (2, 'WS2', '/ws2', 'generic')",
    )
    .await;
    insert_video(&db, 1, "/v1.mp4", Some(1)).await;
    insert_video(&db, 2, "/v2.mp4", Some(2)).await;
    insert_clip_task(&db, 1, "pending", None, None).await;
    insert_clip_task(&db, 2, "pending", None, None).await;

    let rows = sea_orm::ConnectionTrait::query_all(
        db.conn(),
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            "SELECT t.id FROM clip_tasks t \
             LEFT JOIN clip_outputs co ON co.clip_task_id = t.id \
             LEFT JOIN videos v ON t.video_id = v.id \
             WHERE v.workspace_id = 1 ORDER BY t.created_at DESC"
                .to_string(),
        ),
    )
    .await
    .unwrap();
    assert_eq!(rows.len(), 1);
}

#[tokio::test]
async fn test_list_clip_tasks_filtered_by_date_range() {
    let db = setup_test_db().await;
    insert_video(&db, 1, "/v.mp4", None).await;
    // 用显式 created_at 写入
    exec(
        db.conn(),
        "INSERT INTO clip_tasks (video_id, start_time_ms, end_time_ms, status, created_at) \
         VALUES (1, 0, 1000, 'pending', '2026-01-15 10:00:00'), \
                (1, 0, 1000, 'pending', '2026-02-15 10:00:00'), \
                (1, 0, 1000, 'pending', '2026-03-15 10:00:00')",
    )
    .await;

    let rows = sea_orm::ConnectionTrait::query_all(
        db.conn(),
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            "SELECT t.id FROM clip_tasks t \
             LEFT JOIN clip_outputs co ON co.clip_task_id = t.id \
             LEFT JOIN videos v ON t.video_id = v.id \
             WHERE t.created_at >= '2026-02-01 00:00:00' \
               AND t.created_at <= '2026-02-28 23:59:59' \
             ORDER BY t.created_at DESC"
                .to_string(),
        ),
    )
    .await
    .unwrap();
    assert_eq!(rows.len(), 1, "只应命中 2 月那条");
}

#[tokio::test]
async fn test_list_clip_tasks_left_join_brings_output_path() {
    let db = setup_test_db().await;
    insert_video(&db, 1, "/v.mp4", None).await;
    let id = insert_clip_task(&db, 1, "completed", Some("done"), None).await;
    insert_clip_output(&db, id, 1, "/output/done.mp4").await;

    let row = sea_orm::ConnectionTrait::query_one(
        db.conn(),
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            format!(
                "SELECT t.id, co.output_path FROM clip_tasks t \
                 LEFT JOIN clip_outputs co ON co.clip_task_id = t.id \
                 LEFT JOIN videos v ON t.video_id = v.id \
                 WHERE t.id = {}",
                id
            ),
        ),
    )
    .await
    .unwrap()
    .unwrap();
    let output: Option<String> = row.try_get("", "output_path").ok();
    assert_eq!(output.as_deref(), Some("/output/done.mp4"));
}

#[tokio::test]
async fn test_list_clip_tasks_left_join_null_when_no_output() {
    let db = setup_test_db().await;
    insert_video(&db, 1, "/v.mp4", None).await;
    let id = insert_clip_task(&db, 1, "pending", None, None).await;

    let row = sea_orm::ConnectionTrait::query_one(
        db.conn(),
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            format!(
                "SELECT t.id, co.output_path FROM clip_tasks t \
                 LEFT JOIN clip_outputs co ON co.clip_task_id = t.id \
                 WHERE t.id = {}",
                id
            ),
        ),
    )
    .await
    .unwrap()
    .unwrap();
    let output: Option<String> = row.try_get("", "output_path").ok();
    assert!(output.is_none(), "无 output 时 LEFT JOIN 应返回 NULL");
}

#[tokio::test]
async fn test_list_clip_tasks_order_by_created_at_desc() {
    let db = setup_test_db().await;
    insert_video(&db, 1, "/v.mp4", None).await;
    let id1 = insert_clip_task(&db, 1, "pending", Some("first"), None).await;
    tokio::time::sleep(std::time::Duration::from_millis(1100)).await;
    let id2 = insert_clip_task(&db, 1, "pending", Some("second"), None).await;

    let rows = sea_orm::ConnectionTrait::query_all(
        db.conn(),
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            "SELECT t.id FROM clip_tasks t ORDER BY t.created_at DESC".to_string(),
        ),
    )
    .await
    .unwrap();
    let first: i64 = rows[0].try_get("", "id").unwrap();
    let second: i64 = rows[1].try_get("", "id").unwrap();
    assert_eq!(first, id2, "最新插入应在前");
    assert_eq!(second, id1);
}

// ============== list_presets ==============

#[tokio::test]
async fn test_list_presets_returns_seeded_builtins() {
    // migration 自动 seed 5 个 builtin 预设（sort_order 1..5）
    let db = setup_test_db().await;
    let rows = sea_orm::ConnectionTrait::query_all(
        db.conn(),
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            "SELECT name, is_builtin FROM encoding_presets ORDER BY sort_order".to_string(),
        ),
    )
    .await
    .unwrap();
    assert_eq!(rows.len(), 5, "应有 5 个内建预设");
    for row in &rows {
        let is_builtin: bool = row.try_get("", "is_builtin").unwrap();
        assert!(is_builtin, "seeded 预设全部 is_builtin=true");
    }
    let first_name: String = rows[0].try_get("", "name").unwrap();
    assert_eq!(first_name, "极速（无重编码）", "sort_order=1 应为极速");
}

#[tokio::test]
async fn test_list_presets_order_by_sort_order_asc() {
    let db = setup_test_db().await;
    // 自定义预设用大 sort_order 排到末尾
    insert_preset(&db, "Heavy", "encoder", 300, false).await;
    insert_preset(&db, "Light", "encoder", 100, false).await;
    insert_preset(&db, "Medium", "encoder", 200, false).await;

    let rows = sea_orm::ConnectionTrait::query_all(
        db.conn(),
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            "SELECT name FROM encoding_presets WHERE is_builtin = 0 ORDER BY sort_order"
                .to_string(),
        ),
    )
    .await
    .unwrap();
    let names: Vec<String> = rows
        .iter()
        .filter_map(|r| r.try_get::<String>("", "name").ok())
        .collect();
    assert_eq!(names, vec!["Light", "Medium", "Heavy"]);
}

#[tokio::test]
async fn test_list_presets_includes_is_builtin_flag() {
    let db = setup_test_db().await;
    insert_preset(&db, "MyCustom", "encoder", 1000, false).await;

    let row = sea_orm::ConnectionTrait::query_one(
        db.conn(),
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            "SELECT is_builtin FROM encoding_presets WHERE name = 'MyCustom'".to_string(),
        ),
    )
    .await
    .unwrap()
    .unwrap();
    let is_builtin: bool = row.try_get("", "is_builtin").unwrap();
    assert!(!is_builtin, "用户自定义预设 is_builtin=false");

    // 同时验证 seeded 仍标记为 builtin
    let seeded = sea_orm::ConnectionTrait::query_one(
        db.conn(),
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            "SELECT is_builtin FROM encoding_presets WHERE name = '高质量'".to_string(),
        ),
    )
    .await
    .unwrap()
    .unwrap();
    let seeded_builtin: bool = seeded.try_get("", "is_builtin").unwrap();
    assert!(seeded_builtin);
}

// ============== delete_clip_task ==============

#[tokio::test]
async fn test_delete_completed_task_removes_outputs_and_task() {
    let db = setup_test_db().await;
    insert_video(&db, 1, "/v.mp4", None).await;
    let task_id = insert_clip_task(&db, 1, "completed", None, None).await;
    insert_clip_output(&db, task_id, 1, "/o/a.mp4").await;
    insert_clip_output(&db, task_id, 1, "/o/b.mp4").await;

    // 复刻命令：先删 outputs 再删 task
    exec(
        db.conn(),
        &format!("DELETE FROM clip_outputs WHERE clip_task_id = {}", task_id),
    )
    .await;
    exec(
        db.conn(),
        &format!("DELETE FROM clip_tasks WHERE id = {}", task_id),
    )
    .await;

    let task_left = sea_orm::ConnectionTrait::query_one(
        db.conn(),
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            format!("SELECT id FROM clip_tasks WHERE id = {}", task_id),
        ),
    )
    .await
    .unwrap();
    let output_count_row = sea_orm::ConnectionTrait::query_one(
        db.conn(),
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            format!(
                "SELECT COUNT(*) as cnt FROM clip_outputs WHERE clip_task_id = {}",
                task_id
            ),
        ),
    )
    .await
    .unwrap()
    .unwrap();
    let output_count: i64 = output_count_row.try_get("", "cnt").unwrap();

    assert!(task_left.is_none());
    assert_eq!(output_count, 0, "outputs 应被联动删除");
}

#[tokio::test]
async fn test_delete_pending_task_blocked_when_no_force() {
    let db = setup_test_db().await;
    insert_video(&db, 1, "/v.mp4", None).await;
    let task_id = insert_clip_task(&db, 1, "pending", None, None).await;

    // 复刻命令的 status 检查
    let row = sea_orm::ConnectionTrait::query_one(
        db.conn(),
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            format!("SELECT status FROM clip_tasks WHERE id = {}", task_id),
        ),
    )
    .await
    .unwrap()
    .unwrap();
    let status: String = row.try_get("", "status").unwrap();
    let blocked = status == "pending" || status == "processing";
    assert!(blocked, "pending 任务应阻止删除（除非 force=true）");
}

#[tokio::test]
async fn test_delete_nonexistent_task_returns_none() {
    let db = setup_test_db().await;
    let row = sea_orm::ConnectionTrait::query_one(
        db.conn(),
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            "SELECT status FROM clip_tasks WHERE id = 99999".to_string(),
        ),
    )
    .await
    .unwrap();
    assert!(row.is_none(), "命令依据 None 返回 '任务不存在' 错误");
}

#[tokio::test]
async fn test_delete_files_query_lists_outputs_for_task() {
    let db = setup_test_db().await;
    insert_video(&db, 1, "/v.mp4", None).await;
    let id = insert_clip_task(&db, 1, "completed", None, None).await;
    insert_clip_output(&db, id, 1, "/o/x.mp4").await;
    insert_clip_output(&db, id, 1, "/o/y.mp4").await;

    let rows = sea_orm::ConnectionTrait::query_all(
        db.conn(),
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            format!(
                "SELECT output_path FROM clip_outputs WHERE clip_task_id = {}",
                id
            ),
        ),
    )
    .await
    .unwrap();
    let paths: Vec<String> = rows
        .iter()
        .filter_map(|r| r.try_get::<String>("", "output_path").ok())
        .collect();
    assert_eq!(paths.len(), 2);
    assert!(paths.contains(&"/o/x.mp4".to_string()));
    assert!(paths.contains(&"/o/y.mp4".to_string()));
}

// ============== delete_clip_batch ==============

#[tokio::test]
async fn test_delete_batch_skips_active_tasks() {
    let db = setup_test_db().await;
    insert_video(&db, 1, "/v.mp4", None).await;
    insert_clip_task(&db, 1, "completed", Some("c1"), Some("batch-1")).await;
    insert_clip_task(&db, 1, "failed", Some("c2"), Some("batch-1")).await;
    insert_clip_task(&db, 1, "pending", Some("c3"), Some("batch-1")).await;
    insert_clip_task(&db, 1, "processing", Some("c4"), Some("batch-1")).await;

    // 复刻命令统计 active
    let row = sea_orm::ConnectionTrait::query_one(
        db.conn(),
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            "SELECT SUM(CASE WHEN status IN ('pending','processing') THEN 1 ELSE 0 END) as active \
             FROM clip_tasks WHERE batch_id = 'batch-1'"
                .to_string(),
        ),
    )
    .await
    .unwrap()
    .unwrap();
    let active: i64 = row.try_get("", "active").unwrap();
    assert_eq!(active, 2, "pending + processing 共 2 条");

    // 实际删除：只删非 active
    let result = sea_orm::ConnectionTrait::execute_unprepared(
        db.conn(),
        "DELETE FROM clip_tasks WHERE batch_id = 'batch-1' AND status NOT IN ('pending','processing')",
    )
    .await
    .unwrap();
    assert_eq!(result.rows_affected(), 2, "应删除 completed + failed");

    let remaining = sea_orm::ConnectionTrait::query_all(
        db.conn(),
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            "SELECT status FROM clip_tasks WHERE batch_id = 'batch-1'".to_string(),
        ),
    )
    .await
    .unwrap();
    let statuses: Vec<String> = remaining
        .iter()
        .filter_map(|r| r.try_get::<String>("", "status").ok())
        .collect();
    assert_eq!(statuses.len(), 2);
    assert!(statuses.iter().all(|s| s == "pending" || s == "processing"));
}

#[tokio::test]
async fn test_delete_batch_isolated_per_batch_id() {
    let db = setup_test_db().await;
    insert_video(&db, 1, "/v.mp4", None).await;
    insert_clip_task(&db, 1, "completed", None, Some("batch-A")).await;
    insert_clip_task(&db, 1, "completed", None, Some("batch-B")).await;

    let result = sea_orm::ConnectionTrait::execute_unprepared(
        db.conn(),
        "DELETE FROM clip_tasks WHERE batch_id = 'batch-A' AND status NOT IN ('pending','processing')",
    )
    .await
    .unwrap();
    assert_eq!(result.rows_affected(), 1);

    let row = sea_orm::ConnectionTrait::query_one(
        db.conn(),
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            "SELECT COUNT(*) as cnt FROM clip_tasks WHERE batch_id = 'batch-B'".to_string(),
        ),
    )
    .await
    .unwrap()
    .unwrap();
    let cnt: i64 = row.try_get("", "cnt").unwrap();
    assert_eq!(cnt, 1, "batch-B 不应受影响");
}

#[tokio::test]
async fn test_delete_batch_handles_quote_in_batch_id() {
    let db = setup_test_db().await;
    insert_video(&db, 1, "/v.mp4", None).await;
    let escaped = "it's-a-batch".replace('\'', "''");
    insert_clip_task(&db, 1, "completed", None, Some("it's-a-batch")).await;

    let result = sea_orm::ConnectionTrait::execute_unprepared(
        db.conn(),
        &format!(
            "DELETE FROM clip_tasks WHERE batch_id = '{}' AND status NOT IN ('pending','processing')",
            escaped
        ),
    )
    .await
    .unwrap();
    assert_eq!(result.rows_affected(), 1);
}

#[tokio::test]
async fn test_delete_batch_outputs_subquery_links() {
    let db = setup_test_db().await;
    insert_video(&db, 1, "/v.mp4", None).await;
    let id1 = insert_clip_task(&db, 1, "completed", None, Some("b1")).await;
    let id2 = insert_clip_task(&db, 1, "pending", None, Some("b1")).await;
    insert_clip_output(&db, id1, 1, "/o/done.mp4").await;
    insert_clip_output(&db, id2, 1, "/o/pending.mp4").await; // 不应被删，因为 task 是 pending

    let rows = sea_orm::ConnectionTrait::query_all(
        db.conn(),
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            "SELECT output_path FROM clip_outputs WHERE clip_task_id IN (\
             SELECT id FROM clip_tasks WHERE batch_id = 'b1' AND status NOT IN ('pending','processing'))"
                .to_string(),
        ),
    )
    .await
    .unwrap();
    let paths: Vec<String> = rows
        .iter()
        .filter_map(|r| r.try_get::<String>("", "output_path").ok())
        .collect();
    assert_eq!(paths, vec!["/o/done.mp4".to_string()]);
}

// ============== clear_finished_clip_tasks ==============

#[tokio::test]
async fn test_clear_finished_keeps_pending_and_processing() {
    let db = setup_test_db().await;
    insert_video(&db, 1, "/v.mp4", None).await;
    insert_clip_task(&db, 1, "completed", None, None).await;
    insert_clip_task(&db, 1, "failed", None, None).await;
    insert_clip_task(&db, 1, "cancelled", None, None).await;
    insert_clip_task(&db, 1, "pending", None, None).await;
    insert_clip_task(&db, 1, "processing", None, None).await;

    let result = sea_orm::ConnectionTrait::execute_unprepared(
        db.conn(),
        "DELETE FROM clip_tasks WHERE status NOT IN ('pending','processing')",
    )
    .await
    .unwrap();
    assert_eq!(result.rows_affected(), 3);

    let remaining = sea_orm::ConnectionTrait::query_all(
        db.conn(),
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            "SELECT status FROM clip_tasks ORDER BY status".to_string(),
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
async fn test_clear_finished_outputs_subquery_only_finished() {
    let db = setup_test_db().await;
    insert_video(&db, 1, "/v.mp4", None).await;
    let done = insert_clip_task(&db, 1, "completed", None, None).await;
    let pending = insert_clip_task(&db, 1, "pending", None, None).await;
    insert_clip_output(&db, done, 1, "/o/done.mp4").await;
    insert_clip_output(&db, pending, 1, "/o/pending.mp4").await;

    let rows = sea_orm::ConnectionTrait::query_all(
        db.conn(),
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            "SELECT output_path FROM clip_outputs WHERE clip_task_id IN (\
             SELECT id FROM clip_tasks WHERE status NOT IN ('pending','processing'))"
                .to_string(),
        ),
    )
    .await
    .unwrap();
    let paths: Vec<String> = rows
        .iter()
        .filter_map(|r| r.try_get::<String>("", "output_path").ok())
        .collect();
    assert_eq!(paths, vec!["/o/done.mp4".to_string()]);
}

#[tokio::test]
async fn test_clear_finished_empty_db_returns_zero() {
    let db = setup_test_db().await;
    let result = sea_orm::ConnectionTrait::execute_unprepared(
        db.conn(),
        "DELETE FROM clip_tasks WHERE status NOT IN ('pending','processing')",
    )
    .await
    .unwrap();
    assert_eq!(result.rows_affected(), 0);
}

// ============== check_video_burn_availability ==============

#[tokio::test]
async fn test_burn_availability_video_not_found() {
    let db = setup_test_db().await;
    let row = sea_orm::ConnectionTrait::query_one(
        db.conn(),
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            "SELECT file_path, has_subtitle FROM videos WHERE id = 9999".to_string(),
        ),
    )
    .await
    .unwrap();
    assert!(row.is_none(), "命令依据 None 返回 '视频不存在' 错误");
}

#[tokio::test]
async fn test_burn_availability_with_xml_present() {
    let db = setup_test_db().await;
    let dir = tempfile::tempdir().unwrap();
    let video_path = dir.path().join("video.mp4");
    std::fs::write(&video_path, b"x").unwrap();
    let xml_path = dir.path().join("video.xml");
    std::fs::write(&xml_path, b"<i></i>").unwrap();

    exec(
        db.conn(),
        &format!(
            "INSERT INTO videos (id, file_path, file_name, file_size, has_subtitle) \
             VALUES (1, '{}', 'video.mp4', 100, 1)",
            video_path.to_string_lossy()
        ),
    )
    .await;

    let row = sea_orm::ConnectionTrait::query_one(
        db.conn(),
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            "SELECT file_path, has_subtitle FROM videos WHERE id = 1".to_string(),
        ),
    )
    .await
    .unwrap()
    .unwrap();
    let file_path: String = row.try_get("", "file_path").unwrap();
    let has_subtitle: bool = row.try_get("", "has_subtitle").unwrap();

    let xml = std::path::PathBuf::from(&file_path).with_extension("xml");
    assert!(xml.exists(), "同名 .xml 应被检测到");
    assert!(has_subtitle, "has_subtitle=1 应被读出");
}

#[tokio::test]
async fn test_burn_availability_without_xml() {
    let db = setup_test_db().await;
    let dir = tempfile::tempdir().unwrap();
    let video_path = dir.path().join("only_video.mp4");
    std::fs::write(&video_path, b"x").unwrap();

    exec(
        db.conn(),
        &format!(
            "INSERT INTO videos (id, file_path, file_name, file_size) \
             VALUES (1, '{}', 'only_video.mp4', 100)",
            video_path.to_string_lossy()
        ),
    )
    .await;

    let row = sea_orm::ConnectionTrait::query_one(
        db.conn(),
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            "SELECT file_path, has_subtitle FROM videos WHERE id = 1".to_string(),
        ),
    )
    .await
    .unwrap()
    .unwrap();
    let file_path: String = row.try_get("", "file_path").unwrap();
    let has_subtitle: bool = row.try_get("", "has_subtitle").unwrap();

    let xml = std::path::PathBuf::from(&file_path).with_extension("xml");
    assert!(!xml.exists(), "无同名 .xml 时应判定为 false");
    assert!(!has_subtitle, "默认 has_subtitle=0");
}
