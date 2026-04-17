use clipper_studio_lib::db::Database;

/// Helper: create an in-memory SQLite database with migrations applied
async fn setup_test_db() -> Database {
    let db = Database::connect(std::path::Path::new(":memory:"))
        .await
        .expect("failed to connect to in-memory SQLite");
    db.run_migrations().await.expect("failed to run migrations");
    db
}

/// Helper: insert a workspace and return its ID
async fn insert_workspace(conn: &sea_orm::DatabaseConnection, name: &str, path: &str) -> i64 {
    sea_orm::ConnectionTrait::execute_unprepared(
        conn,
        &format!(
            "INSERT INTO workspaces (name, path, adapter_id) VALUES ('{}', '{}', 'generic')",
            name.replace('\'', "''"),
            path.replace('\'', "''"),
        ),
    )
    .await
    .expect("insert workspace should succeed");

    let row = sea_orm::ConnectionTrait::query_one(
        conn,
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            format!(
                "SELECT id FROM workspaces WHERE path = '{}'",
                path.replace('\'', "''")
            ),
        ),
    )
    .await
    .expect("query failed")
    .expect("workspace should exist");

    row.try_get::<i64>("", "id").unwrap()
}

/// Helper: insert a video directly
async fn insert_video(
    conn: &sea_orm::DatabaseConnection,
    file_path: &str,
    file_name: &str,
    file_size: i64,
    workspace_id: Option<i64>,
) -> i64 {
    let ws_sql = workspace_id
        .map(|id| id.to_string())
        .unwrap_or("NULL".to_string());

    sea_orm::ConnectionTrait::execute_unprepared(
        conn,
        &format!(
            "INSERT INTO videos (file_path, file_name, file_size, workspace_id) VALUES ('{}', '{}', {}, {})",
            file_path.replace('\'', "''"),
            file_name.replace('\'', "''"),
            file_size,
            ws_sql,
        ),
    )
    .await
    .expect("insert video should succeed");

    let row = sea_orm::ConnectionTrait::query_one(
        conn,
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            format!(
                "SELECT id FROM videos WHERE file_path = '{}'",
                file_path.replace('\'', "''")
            ),
        ),
    )
    .await
    .expect("query failed")
    .expect("video should exist");

    row.try_get::<i64>("", "id").unwrap()
}

#[tokio::test]
async fn test_list_videos_empty() {
    let db = setup_test_db().await;
    let conn = db.conn();

    let rows = sea_orm::ConnectionTrait::query_all(
        conn,
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            "SELECT * FROM videos".to_string(),
        ),
    )
    .await
    .expect("query failed");

    assert!(rows.is_empty(), "should have no videos initially");
}

#[tokio::test]
async fn test_insert_and_get_video() {
    let db = setup_test_db().await;
    let conn = db.conn();
    let ws_id = insert_workspace(conn, "test-ws", "/test/ws").await;

    let video_id = insert_video(conn, "/test/video.mp4", "video.mp4", 1024000, Some(ws_id)).await;

    let row = sea_orm::ConnectionTrait::query_one(
        conn,
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            format!("SELECT * FROM videos WHERE id = {}", video_id),
        ),
    )
    .await
    .expect("query failed")
    .expect("video should exist");

    let file_path: String = row.try_get("", "file_path").unwrap();
    let file_name: String = row.try_get("", "file_name").unwrap();
    let file_size: i64 = row.try_get("", "file_size").unwrap();
    let workspace_id: Option<i64> = row.try_get("", "workspace_id").ok();

    assert_eq!(file_path, "/test/video.mp4");
    assert_eq!(file_name, "video.mp4");
    assert_eq!(file_size, 1024000);
    assert_eq!(workspace_id, Some(ws_id));
}

#[tokio::test]
async fn test_video_with_metadata() {
    let db = setup_test_db().await;
    let conn = db.conn();

    sea_orm::ConnectionTrait::execute_unprepared(
        conn,
        "INSERT INTO videos (file_path, file_name, file_size, duration_ms, width, height, file_hash) \
         VALUES ('/test/hd.mp4', 'hd.mp4', 5242880, 3600000, 1920, 1080, 'abc123')",
    )
    .await
    .expect("insert should succeed");

    let row = sea_orm::ConnectionTrait::query_one(
        conn,
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            "SELECT duration_ms, width, height, file_hash FROM videos WHERE file_path = '/test/hd.mp4'".to_string(),
        ),
    )
    .await
    .expect("query failed")
    .expect("row should exist");

    let duration: Option<i64> = row.try_get("", "duration_ms").ok();
    let width: Option<i32> = row.try_get("", "width").ok();
    let height: Option<i32> = row.try_get("", "height").ok();
    let hash: Option<String> = row.try_get("", "file_hash").ok();

    assert_eq!(duration, Some(3600000));
    assert_eq!(width, Some(1920));
    assert_eq!(height, Some(1080));
    assert_eq!(hash, Some("abc123".to_string()));
}

#[tokio::test]
async fn test_video_unique_path_constraint() {
    let db = setup_test_db().await;
    let conn = db.conn();

    sea_orm::ConnectionTrait::execute_unprepared(
        conn,
        "INSERT INTO videos (file_path, file_name, file_size) VALUES ('/unique/path.mp4', 'path.mp4', 100)",
    )
    .await
    .expect("first insert should succeed");

    let result = sea_orm::ConnectionTrait::execute_unprepared(
        conn,
        "INSERT INTO videos (file_path, file_name, file_size) VALUES ('/unique/path.mp4', 'path.mp4', 200)",
    )
    .await;

    assert!(result.is_err(), "duplicate file_path should be rejected");
}

#[tokio::test]
async fn test_list_videos_by_workspace() {
    let db = setup_test_db().await;
    let conn = db.conn();

    let ws1 = insert_workspace(conn, "ws1", "/ws1").await;
    let ws2 = insert_workspace(conn, "ws2", "/ws2").await;

    insert_video(conn, "/ws1/a.mp4", "a.mp4", 100, Some(ws1)).await;
    insert_video(conn, "/ws1/b.mp4", "b.mp4", 200, Some(ws1)).await;
    insert_video(conn, "/ws2/c.mp4", "c.mp4", 300, Some(ws2)).await;

    let rows = sea_orm::ConnectionTrait::query_all(
        conn,
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            format!(
                "SELECT file_name FROM videos WHERE workspace_id = {} ORDER BY file_name",
                ws1
            ),
        ),
    )
    .await
    .expect("query failed");

    let names: Vec<String> = rows
        .iter()
        .filter_map(|r| r.try_get::<String>("", "file_name").ok())
        .collect();

    assert_eq!(names, vec!["a.mp4", "b.mp4"]);
}

#[tokio::test]
async fn test_list_videos_pagination() {
    let db = setup_test_db().await;
    let conn = db.conn();

    let ws_id = insert_workspace(conn, "ws", "/ws").await;

    for i in 0..10 {
        insert_video(
            conn,
            &format!("/ws/vid{}.mp4", i),
            &format!("vid{}.mp4", i),
            (i + 1) * 100,
            Some(ws_id),
        )
        .await;
    }

    // Page 1, page_size 3
    let rows = sea_orm::ConnectionTrait::query_all(
        conn,
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            format!(
                "SELECT * FROM videos WHERE workspace_id = {} ORDER BY created_at DESC LIMIT 3 OFFSET 0",
                ws_id
            ),
        ),
    )
    .await
    .expect("query failed");

    assert_eq!(rows.len(), 3);

    // Count total
    let count_row = sea_orm::ConnectionTrait::query_one(
        conn,
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            format!(
                "SELECT COUNT(*) as cnt FROM videos WHERE workspace_id = {}",
                ws_id
            ),
        ),
    )
    .await
    .expect("query failed")
    .expect("should have count");

    let total: i64 = count_row.try_get("", "cnt").unwrap();
    assert_eq!(total, 10);
}

#[tokio::test]
async fn test_delete_video() {
    let db = setup_test_db().await;
    let conn = db.conn();

    let video_id = insert_video(conn, "/delete/me.mp4", "me.mp4", 500, None).await;

    // Verify exists
    let row = sea_orm::ConnectionTrait::query_one(
        conn,
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            format!("SELECT id FROM videos WHERE id = {}", video_id),
        ),
    )
    .await
    .expect("query failed");
    assert!(row.is_some());

    // Delete
    sea_orm::ConnectionTrait::execute_unprepared(
        conn,
        &format!("DELETE FROM videos WHERE id = {}", video_id),
    )
    .await
    .expect("delete should succeed");

    // Verify gone
    let row = sea_orm::ConnectionTrait::query_one(
        conn,
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            format!("SELECT id FROM videos WHERE id = {}", video_id),
        ),
    )
    .await
    .expect("query failed");
    assert!(row.is_none(), "video should be deleted");
}

#[tokio::test]
async fn test_delete_nonexistent_video() {
    let db = setup_test_db().await;
    let conn = db.conn();

    // Deleting a non-existent video should not error
    sea_orm::ConnectionTrait::execute_unprepared(conn, "DELETE FROM videos WHERE id = 99999")
        .await
        .expect("delete nonexistent video should not error");
}

#[tokio::test]
async fn test_video_default_values() {
    let db = setup_test_db().await;
    let conn = db.conn();

    sea_orm::ConnectionTrait::execute_unprepared(
        conn,
        "INSERT INTO videos (file_path, file_name, file_size) VALUES ('/defaults/v.mp4', 'v.mp4', 1000)",
    )
    .await
    .expect("insert should succeed");

    let row = sea_orm::ConnectionTrait::query_one(
        conn,
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            "SELECT has_subtitle, has_danmaku, has_envelope, created_at, updated_at FROM videos WHERE file_path = '/defaults/v.mp4'".to_string(),
        ),
    )
    .await
    .expect("query failed")
    .expect("row should exist");

    let has_subtitle: bool = row.try_get("", "has_subtitle").unwrap();
    let has_danmaku: bool = row.try_get("", "has_danmaku").unwrap();
    let has_envelope: bool = row.try_get("", "has_envelope").unwrap();
    let created_at: String = row.try_get("", "created_at").unwrap();
    let updated_at: String = row.try_get("", "updated_at").unwrap();

    assert!(!has_subtitle, "has_subtitle should default to false");
    assert!(!has_danmaku, "has_danmaku should default to false");
    assert!(!has_envelope, "has_envelope should default to false");
    assert!(!created_at.is_empty(), "created_at should have default");
    assert!(!updated_at.is_empty(), "updated_at should have default");
}

#[tokio::test]
async fn test_video_duplicate_hash_detection() {
    let db = setup_test_db().await;
    let conn = db.conn();

    // Insert first video with hash
    sea_orm::ConnectionTrait::execute_unprepared(
        conn,
        "INSERT INTO videos (file_path, file_name, file_size, file_hash) VALUES ('/a.mp4', 'a.mp4', 100, 'hash123')",
    )
    .await
    .expect("insert should succeed");

    // Check for duplicate by hash (simulating import_video logic)
    let dup = sea_orm::ConnectionTrait::query_one(
        conn,
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            "SELECT id, file_path FROM videos WHERE file_hash = 'hash123'".to_string(),
        ),
    )
    .await
    .expect("query failed");

    assert!(dup.is_some(), "should find video with same hash");

    let dup_path: String = dup.unwrap().try_get("", "file_path").unwrap();
    assert_eq!(dup_path, "/a.mp4");
}
