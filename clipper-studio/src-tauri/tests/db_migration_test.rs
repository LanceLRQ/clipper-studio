use clipper_studio_lib::db::Database;

/// Helper: create an in-memory SQLite database with migrations applied
async fn setup_test_db() -> Database {
    let db = Database::connect(std::path::Path::new(":memory:"))
        .await
        .expect("failed to connect to in-memory SQLite");
    db.run_migrations().await.expect("failed to run migrations");
    db
}

#[tokio::test]
async fn test_migration_creates_all_tables() {
    let db = setup_test_db().await;
    let conn = db.conn();

    let expected_tables = [
        "workspaces",
        "streamers",
        "recording_sessions",
        "videos",
        "audio_envelopes",
        "clip_tasks",
        "clip_outputs",
        "encoding_presets",
        "app_settings",
        "settings_kv",
        "tags",
        "video_tags",
        "analytics_events",
    ];

    for table in &expected_tables {
        let result = sea_orm::ConnectionTrait::query_one(
            conn,
            sea_orm::Statement::from_string(
                sea_orm::DatabaseBackend::Sqlite,
                format!(
                    "SELECT name FROM sqlite_master WHERE type='table' AND name='{}'",
                    table
                ),
            ),
        )
        .await
        .expect("query failed");

        assert!(
            result.is_some(),
            "table '{}' should exist after migration",
            table
        );
    }
}

#[tokio::test]
async fn test_migration_creates_indexes() {
    let db = setup_test_db().await;
    let conn = db.conn();

    let result = sea_orm::ConnectionTrait::query_all(
        conn,
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            "SELECT name FROM sqlite_master WHERE type='index' AND name LIKE 'idx_%'".to_string(),
        ),
    )
    .await
    .expect("query failed");

    let index_names: Vec<String> = result
        .iter()
        .filter_map(|row| row.try_get::<String>("", "name").ok())
        .collect();

    assert!(index_names.contains(&"idx_videos_workspace".to_string()));
    assert!(index_names.contains(&"idx_videos_streamer".to_string()));
    assert!(index_names.contains(&"idx_videos_session".to_string()));
    assert!(index_names.contains(&"idx_videos_recorded".to_string()));
    assert!(index_names.contains(&"idx_analytics_event".to_string()));
}

#[tokio::test]
async fn test_migration_inserts_default_settings() {
    let db = setup_test_db().await;
    let conn = db.conn();

    let row = sea_orm::ConnectionTrait::query_one(
        conn,
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            "SELECT theme, language FROM app_settings WHERE id = 1".to_string(),
        ),
    )
    .await
    .expect("query failed")
    .expect("default settings row should exist");

    let theme: String = row.try_get("", "theme").unwrap_or_default();
    let language: String = row.try_get("", "language").unwrap_or_default();

    assert_eq!(theme, "system");
    assert_eq!(language, "zh-CN");
}

#[tokio::test]
async fn test_migration_inserts_builtin_presets() {
    let db = setup_test_db().await;
    let conn = db.conn();

    let rows = sea_orm::ConnectionTrait::query_all(
        conn,
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            "SELECT name, is_builtin FROM encoding_presets WHERE is_builtin = 1 ORDER BY sort_order"
                .to_string(),
        ),
    )
    .await
    .expect("query failed");

    assert_eq!(rows.len(), 5, "should have 5 builtin presets");

    let names: Vec<String> = rows
        .iter()
        .filter_map(|r| r.try_get::<String>("", "name").ok())
        .collect();

    assert!(names
        .contains(&"\u{6781}\u{901f}\u{ff08}\u{65e0}\u{91cd}\u{7f16}\u{7801}\u{ff09}".to_string()));
    assert!(names.contains(&"\u{6807}\u{51c6}\u{8d28}\u{91cf}".to_string()));
}

#[tokio::test]
async fn test_migration_is_idempotent() {
    // Running migrations twice should not fail
    let db = setup_test_db().await;
    db.run_migrations()
        .await
        .expect("running migrations twice should not fail");
}

#[tokio::test]
async fn test_workspaces_table_schema() {
    let db = setup_test_db().await;
    let conn = db.conn();

    // Insert a workspace and verify all columns work
    sea_orm::ConnectionTrait::execute_unprepared(
        conn,
        "INSERT INTO workspaces (name, path, adapter_id) VALUES ('test', '/test/path', 'generic')",
    )
    .await
    .expect("insert should succeed");

    let row = sea_orm::ConnectionTrait::query_one(
        conn,
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            "SELECT id, name, path, adapter_id, auto_scan, created_at, updated_at FROM workspaces WHERE path = '/test/path'".to_string(),
        ),
    )
    .await
    .expect("query failed")
    .expect("row should exist");

    let id: i64 = row.try_get("", "id").unwrap();
    assert!(id > 0);

    let auto_scan: bool = row.try_get("", "auto_scan").unwrap();
    assert!(auto_scan, "auto_scan should default to true");

    let created_at: String = row.try_get("", "created_at").unwrap();
    assert!(!created_at.is_empty(), "created_at should have a default");
}

#[tokio::test]
async fn test_settings_kv_basic_operations() {
    let db = setup_test_db().await;
    let conn = db.conn();

    // Insert
    sea_orm::ConnectionTrait::execute_unprepared(
        conn,
        "INSERT INTO settings_kv (key, value) VALUES ('test_key', 'test_value')",
    )
    .await
    .expect("insert should succeed");

    // Read
    let row = sea_orm::ConnectionTrait::query_one(
        conn,
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            "SELECT value FROM settings_kv WHERE key = 'test_key'".to_string(),
        ),
    )
    .await
    .expect("query failed")
    .expect("row should exist");

    let value: String = row.try_get("", "value").unwrap();
    assert_eq!(value, "test_value");

    // Upsert (INSERT OR REPLACE)
    sea_orm::ConnectionTrait::execute_unprepared(
        conn,
        "INSERT OR REPLACE INTO settings_kv (key, value) VALUES ('test_key', 'updated_value')",
    )
    .await
    .expect("upsert should succeed");

    let row = sea_orm::ConnectionTrait::query_one(
        conn,
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            "SELECT value FROM settings_kv WHERE key = 'test_key'".to_string(),
        ),
    )
    .await
    .expect("query failed")
    .expect("row should exist");

    let value: String = row.try_get("", "value").unwrap();
    assert_eq!(value, "updated_value");
}

// ==================== Extended migration coverage ====================

#[tokio::test]
async fn test_migration_creates_asr_subtitle_tables() {
    let db = setup_test_db().await;
    let conn = db.conn();

    for table in &[
        "asr_tasks",
        "subtitle_segments",
        "subtitle_fts",
        "media_tasks",
    ] {
        let result = sea_orm::ConnectionTrait::query_one(
            conn,
            sea_orm::Statement::from_string(
                sea_orm::DatabaseBackend::Sqlite,
                format!(
                    "SELECT name FROM sqlite_master \
                     WHERE (type='table' OR type='view') AND name='{}'",
                    table
                ),
            ),
        )
        .await
        .expect("query failed");
        assert!(result.is_some(), "table '{}' missing", table);
    }
}

#[tokio::test]
async fn test_migration_creates_fts_triggers() {
    let db = setup_test_db().await;
    let conn = db.conn();

    let triggers = sea_orm::ConnectionTrait::query_all(
        conn,
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            "SELECT name FROM sqlite_master WHERE type='trigger' AND name LIKE 'subtitle_fts_%'"
                .to_string(),
        ),
    )
    .await
    .expect("query failed");
    let names: Vec<String> = triggers
        .iter()
        .filter_map(|r| r.try_get::<String>("", "name").ok())
        .collect();
    assert!(names.contains(&"subtitle_fts_insert".to_string()));
    assert!(names.contains(&"subtitle_fts_delete".to_string()));
    assert!(names.contains(&"subtitle_fts_update".to_string()));
}

#[tokio::test]
async fn test_clip_tasks_burn_flag_columns_present() {
    // m20260417_000008_clip_task_burn_flags adds 4 columns
    let db = setup_test_db().await;
    let conn = db.conn();

    let rows = sea_orm::ConnectionTrait::query_all(
        conn,
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            "PRAGMA table_info(clip_tasks)".to_string(),
        ),
    )
    .await
    .expect("query failed");
    let cols: Vec<String> = rows
        .iter()
        .filter_map(|r| r.try_get::<String>("", "name").ok())
        .collect();
    for c in [
        "include_danmaku",
        "include_subtitle",
        "export_subtitle",
        "export_danmaku",
        "batch_id",
        "batch_title",
    ] {
        assert!(
            cols.contains(&c.to_string()),
            "clip_tasks missing column {}",
            c
        );
    }
}

#[tokio::test]
async fn test_videos_file_missing_column_present() {
    let db = setup_test_db().await;
    let conn = db.conn();
    let rows = sea_orm::ConnectionTrait::query_all(
        conn,
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            "PRAGMA table_info(videos)".to_string(),
        ),
    )
    .await
    .expect("query failed");
    let cols: Vec<String> = rows
        .iter()
        .filter_map(|r| r.try_get::<String>("", "name").ok())
        .collect();
    assert!(cols.contains(&"file_missing".to_string()));
}

#[tokio::test]
async fn test_workspace_clip_output_dir_column_present() {
    let db = setup_test_db().await;
    let conn = db.conn();
    let rows = sea_orm::ConnectionTrait::query_all(
        conn,
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            "PRAGMA table_info(workspaces)".to_string(),
        ),
    )
    .await
    .expect("query failed");
    let cols: Vec<String> = rows
        .iter()
        .filter_map(|r| r.try_get::<String>("", "name").ok())
        .collect();
    assert!(cols.contains(&"clip_output_dir".to_string()));
}

#[tokio::test]
async fn test_added_missing_indexes_present() {
    // m20260416_000006_add_missing_indexes
    let db = setup_test_db().await;
    let conn = db.conn();
    let rows = sea_orm::ConnectionTrait::query_all(
        conn,
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            "SELECT name FROM sqlite_master WHERE type='index' AND name LIKE 'idx_%'".to_string(),
        ),
    )
    .await
    .expect("query failed");
    let names: Vec<String> = rows
        .iter()
        .filter_map(|r| r.try_get::<String>("", "name").ok())
        .collect();
    // At least these performance-critical indexes should exist
    let must_have = [
        "idx_clip_tasks_status",
        "idx_clip_outputs_clip_task_id",
        "idx_video_tags_tag_id",
        "idx_recording_sessions_workspace",
    ];
    for idx in must_have {
        assert!(
            names.iter().any(|n| n == idx),
            "missing required index {}, have: {:?}",
            idx,
            names
        );
    }
}

#[tokio::test]
async fn test_settings_kv_unique_key_constraint() {
    let db = setup_test_db().await;
    let conn = db.conn();

    sea_orm::ConnectionTrait::execute_unprepared(
        conn,
        "INSERT INTO settings_kv (key, value) VALUES ('k1', 'v1')",
    )
    .await
    .expect("first insert");

    // Second insert with same key should fail (PRIMARY KEY conflict)
    let result = sea_orm::ConnectionTrait::execute_unprepared(
        conn,
        "INSERT INTO settings_kv (key, value) VALUES ('k1', 'v2')",
    )
    .await;
    assert!(result.is_err(), "duplicate key insert should fail");
}
