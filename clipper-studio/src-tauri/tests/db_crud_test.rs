//! 数据持久层 CRUD 集成测试 (T-30)
//!
//! 验证 14 张表的核心 CRUD 操作 + 批量事务一致性。
//! 已有 `video_commands_test.rs` / `workspace_commands_test.rs` 覆盖 video/workspace CRUD，
//! 此处补充其它表 + 批量事务 + 跨表关联。

use clipper_studio_lib::db::Database;
use sea_orm::{ConnectionTrait, DatabaseBackend, Statement, TransactionTrait};

async fn setup_db() -> Database {
    let db = Database::connect(std::path::Path::new(":memory:"))
        .await
        .expect("connect");
    db.run_migrations().await.expect("migrations");
    db
}

async fn seed_video(db: &Database, video_id: i64) {
    let conn = db.conn();
    conn.execute_unprepared(
        "INSERT OR IGNORE INTO workspaces (id, name, path, adapter_id) \
         VALUES (1, 'ws', '/tmp/ws', 'generic')",
    )
    .await
    .unwrap();
    conn.execute_unprepared(&format!(
        "INSERT INTO videos (id, workspace_id, file_path, file_name, file_size) \
         VALUES ({}, 1, '/tmp/v{}.mp4', 'v{}.mp4', 0)",
        video_id, video_id, video_id
    ))
    .await
    .unwrap();
}

// ==================== streamers ====================

#[tokio::test]
async fn test_streamers_crud() {
    let db = setup_db().await;
    let conn = db.conn();

    conn.execute_unprepared(
        "INSERT INTO streamers (platform, room_id, name) VALUES ('bilibili', '12345', '测试主播')",
    )
    .await
    .unwrap();

    let row = conn
        .query_one(Statement::from_string(
            DatabaseBackend::Sqlite,
            "SELECT id, name FROM streamers WHERE room_id = '12345'".to_string(),
        ))
        .await
        .unwrap()
        .expect("row exists");
    let name: String = row.try_get("", "name").unwrap();
    assert_eq!(name, "测试主播");

    // Update
    conn.execute_unprepared("UPDATE streamers SET name = '更新后' WHERE room_id = '12345'")
        .await
        .unwrap();
    let row = conn
        .query_one(Statement::from_string(
            DatabaseBackend::Sqlite,
            "SELECT name FROM streamers WHERE room_id = '12345'".to_string(),
        ))
        .await
        .unwrap()
        .unwrap();
    let name: String = row.try_get("", "name").unwrap();
    assert_eq!(name, "更新后");

    // Delete
    conn.execute_unprepared("DELETE FROM streamers WHERE room_id = '12345'")
        .await
        .unwrap();
    let row = conn
        .query_one(Statement::from_string(
            DatabaseBackend::Sqlite,
            "SELECT id FROM streamers WHERE room_id = '12345'".to_string(),
        ))
        .await
        .unwrap();
    assert!(row.is_none());
}

#[tokio::test]
async fn test_streamers_unique_platform_room() {
    let db = setup_db().await;
    let conn = db.conn();

    conn.execute_unprepared(
        "INSERT INTO streamers (platform, room_id, name) VALUES ('bilibili', '999', 'a')",
    )
    .await
    .unwrap();

    let dup = conn
        .execute_unprepared(
            "INSERT INTO streamers (platform, room_id, name) VALUES ('bilibili', '999', 'b')",
        )
        .await;
    assert!(dup.is_err(), "duplicate platform+room_id should fail");
}

// ==================== recording_sessions ====================

#[tokio::test]
async fn test_recording_sessions_crud() {
    let db = setup_db().await;
    let conn = db.conn();
    conn.execute_unprepared(
        "INSERT INTO workspaces (name, path, adapter_id) VALUES ('w', '/p', 'generic')",
    )
    .await
    .unwrap();

    conn.execute_unprepared(
        "INSERT INTO recording_sessions (workspace_id, title, started_at, ended_at, file_count) \
         VALUES (1, '直播标题', '2026-04-25 10:00:00', '2026-04-25 12:00:00', 5)",
    )
    .await
    .unwrap();

    let row = conn
        .query_one(Statement::from_string(
            DatabaseBackend::Sqlite,
            "SELECT title, file_count FROM recording_sessions WHERE workspace_id = 1".to_string(),
        ))
        .await
        .unwrap()
        .unwrap();
    let title: String = row.try_get("", "title").unwrap();
    let file_count: i64 = row.try_get("", "file_count").unwrap();
    assert_eq!(title, "直播标题");
    assert_eq!(file_count, 5);
}

// ==================== audio_envelopes (BLOB) ====================

#[tokio::test]
async fn test_audio_envelopes_blob_crud() {
    let db = setup_db().await;
    seed_video(&db, 1).await;
    let conn = db.conn();

    // f32le bytes: 4 floats = 16 bytes
    let blob: Vec<u8> = vec![
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x80, 0x3F, 0x00, 0x00, 0x00, 0x40, 0x00, 0x00, 0x40,
        0x40,
    ];
    conn.execute(Statement::from_sql_and_values(
        DatabaseBackend::Sqlite,
        "INSERT INTO audio_envelopes (video_id, window_ms, data) VALUES (?, ?, ?)",
        [1.into(), 500.into(), blob.clone().into()],
    ))
    .await
    .unwrap();

    let row = conn
        .query_one(Statement::from_string(
            DatabaseBackend::Sqlite,
            "SELECT window_ms, length(data) AS len FROM audio_envelopes WHERE video_id = 1"
                .to_string(),
        ))
        .await
        .unwrap()
        .unwrap();
    let window_ms: i64 = row.try_get("", "window_ms").unwrap();
    let len: i64 = row.try_get("", "len").unwrap();
    assert_eq!(window_ms, 500);
    assert_eq!(len, blob.len() as i64);
}

#[tokio::test]
async fn test_audio_envelopes_pk_uniqueness() {
    // PK is video_id, so a second insert for same video should fail
    let db = setup_db().await;
    seed_video(&db, 1).await;
    let conn = db.conn();
    let blob: Vec<u8> = vec![0u8; 16];
    conn.execute(Statement::from_sql_and_values(
        DatabaseBackend::Sqlite,
        "INSERT INTO audio_envelopes (video_id, window_ms, data) VALUES (?, ?, ?)",
        [1.into(), 500.into(), blob.clone().into()],
    ))
    .await
    .unwrap();

    let result = conn
        .execute(Statement::from_sql_and_values(
            DatabaseBackend::Sqlite,
            "INSERT INTO audio_envelopes (video_id, window_ms, data) VALUES (?, ?, ?)",
            [1.into(), 500.into(), blob.into()],
        ))
        .await;
    assert!(result.is_err(), "duplicate PK should fail");
}

// ==================== clip_tasks + clip_outputs ====================

#[tokio::test]
async fn test_clip_tasks_full_lifecycle() {
    let db = setup_db().await;
    seed_video(&db, 1).await;
    let conn = db.conn();

    conn.execute_unprepared(
        "INSERT INTO clip_tasks (video_id, start_time_ms, end_time_ms, title) \
         VALUES (1, 0, 30000, '片段A')",
    )
    .await
    .unwrap();

    // Verify default status
    let row = conn
        .query_one(Statement::from_string(
            DatabaseBackend::Sqlite,
            "SELECT status, progress FROM clip_tasks WHERE video_id = 1".to_string(),
        ))
        .await
        .unwrap()
        .unwrap();
    let status: String = row.try_get("", "status").unwrap();
    let progress: f64 = row.try_get("", "progress").unwrap();
    assert_eq!(status, "pending");
    assert_eq!(progress, 0.0);

    // Update progress
    conn.execute_unprepared(
        "UPDATE clip_tasks SET status = 'processing', progress = 0.5 WHERE video_id = 1",
    )
    .await
    .unwrap();
    let row = conn
        .query_one(Statement::from_string(
            DatabaseBackend::Sqlite,
            "SELECT status, progress FROM clip_tasks WHERE video_id = 1".to_string(),
        ))
        .await
        .unwrap()
        .unwrap();
    let progress: f64 = row.try_get("", "progress").unwrap();
    assert!((progress - 0.5).abs() < 1e-6);

    // Mark complete with output
    conn.execute_unprepared(
        "UPDATE clip_tasks SET status = 'completed', progress = 1.0 WHERE id = 1",
    )
    .await
    .unwrap();
    conn.execute_unprepared(
        "INSERT INTO clip_outputs (clip_task_id, video_id, output_path, format, variant) \
         VALUES (1, 1, '/tmp/out.mp4', 'mp4', 'main')",
    )
    .await
    .unwrap();

    let count_row = conn
        .query_one(Statement::from_string(
            DatabaseBackend::Sqlite,
            "SELECT COUNT(*) AS c FROM clip_outputs WHERE clip_task_id = 1".to_string(),
        ))
        .await
        .unwrap()
        .unwrap();
    let count: i64 = count_row.try_get("", "c").unwrap();
    assert_eq!(count, 1);
}

// ==================== encoding_presets ====================

#[tokio::test]
async fn test_encoding_presets_builtin_seeded() {
    let db = setup_db().await;
    let conn = db.conn();
    let row = conn
        .query_one(Statement::from_string(
            DatabaseBackend::Sqlite,
            "SELECT COUNT(*) AS c FROM encoding_presets WHERE is_builtin = 1".to_string(),
        ))
        .await
        .unwrap()
        .unwrap();
    let count: i64 = row.try_get("", "c").unwrap();
    assert_eq!(count, 5, "5 builtin presets should be seeded");
}

#[tokio::test]
async fn test_encoding_presets_user_can_add() {
    let db = setup_db().await;
    let conn = db.conn();
    conn.execute_unprepared(
        r#"INSERT INTO encoding_presets (name, options, is_builtin) VALUES ('我的预设', '{"codec":"h264","crf":20}', 0)"#,
    )
    .await
    .unwrap();

    let row = conn
        .query_one(Statement::from_string(
            DatabaseBackend::Sqlite,
            "SELECT options FROM encoding_presets WHERE name = '我的预设'".to_string(),
        ))
        .await
        .unwrap()
        .unwrap();
    let opts: String = row.try_get("", "options").unwrap();
    assert!(opts.contains("\"codec\":\"h264\""));
    assert!(opts.contains("\"crf\":20"));
}

// ==================== tags + video_tags ====================

#[tokio::test]
async fn test_tags_unique_name() {
    let db = setup_db().await;
    let conn = db.conn();
    conn.execute_unprepared("INSERT INTO tags (name, color) VALUES ('精彩', '#ff0000')")
        .await
        .unwrap();

    let dup = conn
        .execute_unprepared("INSERT INTO tags (name) VALUES ('精彩')")
        .await;
    assert!(dup.is_err(), "duplicate tag name should fail");
}

#[tokio::test]
async fn test_video_tags_composite_pk() {
    let db = setup_db().await;
    seed_video(&db, 1).await;
    let conn = db.conn();

    conn.execute_unprepared("INSERT INTO tags (id, name) VALUES (10, '搞笑')")
        .await
        .unwrap();
    conn.execute_unprepared("INSERT INTO tags (id, name) VALUES (11, '高能')")
        .await
        .unwrap();

    conn.execute_unprepared("INSERT INTO video_tags (video_id, tag_id) VALUES (1, 10)")
        .await
        .unwrap();
    conn.execute_unprepared("INSERT INTO video_tags (video_id, tag_id) VALUES (1, 11)")
        .await
        .unwrap();

    // Duplicate composite key fails
    let dup = conn
        .execute_unprepared("INSERT INTO video_tags (video_id, tag_id) VALUES (1, 10)")
        .await;
    assert!(dup.is_err(), "composite PK conflict should fail");

    // Count tags for video 1
    let row = conn
        .query_one(Statement::from_string(
            DatabaseBackend::Sqlite,
            "SELECT COUNT(*) AS c FROM video_tags WHERE video_id = 1".to_string(),
        ))
        .await
        .unwrap()
        .unwrap();
    let c: i64 = row.try_get("", "c").unwrap();
    assert_eq!(c, 2);
}

// ==================== analytics_events ====================

#[tokio::test]
async fn test_analytics_events_insert_and_filter() {
    let db = setup_db().await;
    let conn = db.conn();

    for event in [
        "video_imported",
        "clip_created",
        "video_imported",
        "heatmap_clicked",
    ] {
        conn.execute(Statement::from_sql_and_values(
            DatabaseBackend::Sqlite,
            "INSERT INTO analytics_events (event, properties) VALUES (?, '{}')",
            [event.into()],
        ))
        .await
        .unwrap();
    }

    let row = conn
        .query_one(Statement::from_string(
            DatabaseBackend::Sqlite,
            "SELECT COUNT(*) AS c FROM analytics_events WHERE event = 'video_imported'".to_string(),
        ))
        .await
        .unwrap()
        .unwrap();
    let c: i64 = row.try_get("", "c").unwrap();
    assert_eq!(c, 2);
}

// ==================== Foreign keys (PRAGMA foreign_keys = ON required) ====================

#[tokio::test]
async fn test_foreign_keys_enforced_when_pragma_on() {
    let db = setup_db().await;
    let conn = db.conn();
    conn.execute_unprepared("PRAGMA foreign_keys = ON")
        .await
        .unwrap();

    // Inserting clip_task with nonexistent video should fail
    let result = conn
        .execute_unprepared(
            "INSERT INTO clip_tasks (video_id, start_time_ms, end_time_ms) VALUES (9999, 0, 1000)",
        )
        .await;
    assert!(
        result.is_err(),
        "FK should be enforced when PRAGMA foreign_keys=ON"
    );
}

// ==================== Batch transaction ====================

#[tokio::test]
async fn test_batch_insert_in_transaction_commits_atomically() {
    let db = setup_db().await;
    seed_video(&db, 1).await;
    let conn = db.conn();

    let txn = conn.begin().await.unwrap();
    for i in 0..50 {
        txn.execute(Statement::from_sql_and_values(
            DatabaseBackend::Sqlite,
            "INSERT INTO subtitle_segments (video_id, start_ms, end_ms, text) VALUES (?, ?, ?, ?)",
            [
                1.into(),
                (i * 1000_i64).into(),
                ((i + 1) * 1000_i64).into(),
                format!("片段 {}", i).into(),
            ],
        ))
        .await
        .unwrap();
    }
    txn.commit().await.unwrap();

    let row = conn
        .query_one(Statement::from_string(
            DatabaseBackend::Sqlite,
            "SELECT COUNT(*) AS c FROM subtitle_segments WHERE video_id = 1".to_string(),
        ))
        .await
        .unwrap()
        .unwrap();
    let c: i64 = row.try_get("", "c").unwrap();
    assert_eq!(c, 50);
}

#[tokio::test]
async fn test_batch_rollback_on_failure_keeps_db_clean() {
    let db = setup_db().await;
    seed_video(&db, 1).await;
    let conn = db.conn();

    let txn = conn.begin().await.unwrap();
    txn.execute(Statement::from_sql_and_values(
        DatabaseBackend::Sqlite,
        "INSERT INTO subtitle_segments (video_id, start_ms, end_ms, text) VALUES (?, ?, ?, ?)",
        [1.into(), 0_i64.into(), 1000_i64.into(), "first".into()],
    ))
    .await
    .unwrap();
    // Simulate explicit rollback (we don't commit)
    txn.rollback().await.unwrap();

    let row = conn
        .query_one(Statement::from_string(
            DatabaseBackend::Sqlite,
            "SELECT COUNT(*) AS c FROM subtitle_segments WHERE video_id = 1".to_string(),
        ))
        .await
        .unwrap()
        .unwrap();
    let c: i64 = row.try_get("", "c").unwrap();
    assert_eq!(c, 0, "rolled-back transaction should leave no rows");
}

// ==================== settings_kv plugin namespace ====================

#[tokio::test]
async fn test_settings_kv_supports_plugin_namespace() {
    let db = setup_db().await;
    let conn = db.conn();

    // Plugin configs use "plugin:{id}:{key}" namespace convention
    for (k, v) in [
        ("plugin:asr.qwen3:device", "auto"),
        ("plugin:asr.qwen3:lang", "zh"),
        ("plugin:recorder.bililive:port", "2356"),
    ] {
        conn.execute(Statement::from_sql_and_values(
            DatabaseBackend::Sqlite,
            "INSERT INTO settings_kv (key, value) VALUES (?, ?)",
            [k.into(), v.into()],
        ))
        .await
        .unwrap();
    }

    let rows = conn
        .query_all(Statement::from_sql_and_values(
            DatabaseBackend::Sqlite,
            "SELECT key, value FROM settings_kv WHERE key LIKE ? ORDER BY key",
            ["plugin:asr.qwen3:%".into()],
        ))
        .await
        .unwrap();
    assert_eq!(rows.len(), 2);
}
