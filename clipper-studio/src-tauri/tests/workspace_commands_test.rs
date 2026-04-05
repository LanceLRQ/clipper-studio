use clipper_studio_lib::db::Database;

/// Helper: create an in-memory SQLite database with migrations applied
async fn setup_test_db() -> Database {
    let db = Database::connect(std::path::Path::new(":memory:"))
        .await
        .expect("failed to connect to in-memory SQLite");
    db.run_migrations()
        .await
        .expect("failed to run migrations");
    db
}

/// Helper: insert a workspace directly
async fn insert_workspace(conn: &sea_orm::DatabaseConnection, name: &str, path: &str, adapter_id: &str) -> i64 {
    sea_orm::ConnectionTrait::execute_unprepared(
        conn,
        &format!(
            "INSERT INTO workspaces (name, path, adapter_id) VALUES ('{}', '{}', '{}')",
            name.replace('\'', "''"),
            path.replace('\'', "''"),
            adapter_id.replace('\'', "''"),
        ),
    )
    .await
    .expect("insert should succeed");

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
    .expect("row should exist");

    row.try_get::<i64>("", "id").unwrap()
}

#[tokio::test]
async fn test_list_workspaces_empty() {
    let db = setup_test_db().await;
    let conn = db.conn();

    let rows = sea_orm::ConnectionTrait::query_all(
        conn,
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            "SELECT id, name, path, adapter_id, auto_scan, created_at FROM workspaces ORDER BY created_at DESC".to_string(),
        ),
    )
    .await
    .expect("query failed");

    assert!(rows.is_empty(), "should have no workspaces initially");
}

#[tokio::test]
async fn test_list_workspaces_ordered_by_created_at() {
    let db = setup_test_db().await;
    let conn = db.conn();

    // Insert with explicit timestamps to ensure ordering
    sea_orm::ConnectionTrait::execute_unprepared(
        conn,
        "INSERT INTO workspaces (name, path, adapter_id, created_at) VALUES ('first', '/path/first', 'generic', '2026-01-01 00:00:00')",
    )
    .await
    .expect("insert should succeed");

    sea_orm::ConnectionTrait::execute_unprepared(
        conn,
        "INSERT INTO workspaces (name, path, adapter_id, created_at) VALUES ('second', '/path/second', 'generic', '2026-02-01 00:00:00')",
    )
    .await
    .expect("insert should succeed");

    sea_orm::ConnectionTrait::execute_unprepared(
        conn,
        "INSERT INTO workspaces (name, path, adapter_id, created_at) VALUES ('third', '/path/third', 'bililive-recorder', '2026-03-01 00:00:00')",
    )
    .await
    .expect("insert should succeed");

    let rows = sea_orm::ConnectionTrait::query_all(
        conn,
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            "SELECT name FROM workspaces ORDER BY created_at DESC".to_string(),
        ),
    )
    .await
    .expect("query failed");

    let names: Vec<String> = rows
        .iter()
        .filter_map(|r| r.try_get::<String>("", "name").ok())
        .collect();

    assert_eq!(names, vec!["third", "second", "first"]);
}

#[tokio::test]
async fn test_create_workspace_sql_injection_protection() {
    let db = setup_test_db().await;
    let conn = db.conn();

    // Simulate the escaping logic from create_workspace command
    let malicious_name = "test'; DROP TABLE workspaces; --";
    let escaped_name = malicious_name.replace('\'', "''");

    sea_orm::ConnectionTrait::execute_unprepared(
        conn,
        &format!(
            "INSERT INTO workspaces (name, path, adapter_id) VALUES ('{}', '/safe/path', 'generic')",
            escaped_name,
        ),
    )
    .await
    .expect("insert should succeed with escaped name");

    // Verify workspaces table still exists and has the row
    let row = sea_orm::ConnectionTrait::query_one(
        conn,
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            "SELECT name FROM workspaces WHERE path = '/safe/path'".to_string(),
        ),
    )
    .await
    .expect("query failed")
    .expect("row should exist");

    let name: String = row.try_get("", "name").unwrap();
    assert_eq!(name, malicious_name);
}

#[tokio::test]
async fn test_delete_workspace_cascades() {
    let db = setup_test_db().await;
    let conn = db.conn();

    let ws_id = insert_workspace(conn, "to-delete", "/delete/path", "generic").await;

    // Insert a recording session linked to workspace
    sea_orm::ConnectionTrait::execute_unprepared(
        conn,
        &format!(
            "INSERT INTO recording_sessions (workspace_id) VALUES ({})",
            ws_id
        ),
    )
    .await
    .expect("insert session should succeed");

    // Insert a video linked to workspace
    sea_orm::ConnectionTrait::execute_unprepared(
        conn,
        &format!(
            "INSERT INTO videos (file_path, file_name, file_size, workspace_id) VALUES ('/test.mp4', 'test.mp4', 1024, {})",
            ws_id
        ),
    )
    .await
    .expect("insert video should succeed");

    // Simulate delete_workspace: delete videos, sessions, then workspace
    sea_orm::ConnectionTrait::execute_unprepared(
        conn,
        &format!("DELETE FROM videos WHERE workspace_id = {}", ws_id),
    )
    .await
    .expect("delete videos should succeed");

    sea_orm::ConnectionTrait::execute_unprepared(
        conn,
        &format!("DELETE FROM recording_sessions WHERE workspace_id = {}", ws_id),
    )
    .await
    .expect("delete sessions should succeed");

    sea_orm::ConnectionTrait::execute_unprepared(
        conn,
        &format!("DELETE FROM workspaces WHERE id = {}", ws_id),
    )
    .await
    .expect("delete workspace should succeed");

    // Verify workspace is gone
    let row = sea_orm::ConnectionTrait::query_one(
        conn,
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            format!("SELECT id FROM workspaces WHERE id = {}", ws_id),
        ),
    )
    .await
    .expect("query failed");

    assert!(row.is_none(), "workspace should be deleted");
}

#[tokio::test]
async fn test_get_and_set_active_workspace() {
    let db = setup_test_db().await;
    let conn = db.conn();

    // Initially no active workspace
    let row = sea_orm::ConnectionTrait::query_one(
        conn,
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            "SELECT value FROM settings_kv WHERE key = 'active_workspace_id'".to_string(),
        ),
    )
    .await
    .expect("query failed");

    assert!(row.is_none(), "should have no active workspace initially");

    // Set active workspace
    sea_orm::ConnectionTrait::execute_unprepared(
        conn,
        "INSERT OR REPLACE INTO settings_kv (key, value) VALUES ('active_workspace_id', '42')",
    )
    .await
    .expect("set active workspace should succeed");

    let row = sea_orm::ConnectionTrait::query_one(
        conn,
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            "SELECT value FROM settings_kv WHERE key = 'active_workspace_id'".to_string(),
        ),
    )
    .await
    .expect("query failed")
    .expect("row should exist");

    let value: String = row.try_get("", "value").unwrap();
    assert_eq!(value.parse::<i64>().unwrap(), 42);

    // Update active workspace
    sea_orm::ConnectionTrait::execute_unprepared(
        conn,
        "INSERT OR REPLACE INTO settings_kv (key, value) VALUES ('active_workspace_id', '99')",
    )
    .await
    .expect("update active workspace should succeed");

    let row = sea_orm::ConnectionTrait::query_one(
        conn,
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            "SELECT value FROM settings_kv WHERE key = 'active_workspace_id'".to_string(),
        ),
    )
    .await
    .expect("query failed")
    .expect("row should exist");

    let value: String = row.try_get("", "value").unwrap();
    assert_eq!(value.parse::<i64>().unwrap(), 99);

    // Clear active workspace (set to None)
    sea_orm::ConnectionTrait::execute_unprepared(
        conn,
        "DELETE FROM settings_kv WHERE key = 'active_workspace_id'",
    )
    .await
    .expect("clear active workspace should succeed");

    let row = sea_orm::ConnectionTrait::query_one(
        conn,
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            "SELECT value FROM settings_kv WHERE key = 'active_workspace_id'".to_string(),
        ),
    )
    .await
    .expect("query failed");

    assert!(row.is_none(), "active workspace should be cleared");
}

#[tokio::test]
async fn test_workspace_unique_path_constraint() {
    let db = setup_test_db().await;
    let conn = db.conn();

    insert_workspace(conn, "ws1", "/unique/path", "generic").await;

    // Second insert with same path should fail
    let result = sea_orm::ConnectionTrait::execute_unprepared(
        conn,
        "INSERT INTO workspaces (name, path, adapter_id) VALUES ('ws2', '/unique/path', 'generic')",
    )
    .await;

    assert!(result.is_err(), "duplicate path should be rejected");
}

#[tokio::test]
async fn test_delete_nonexistent_workspace() {
    let db = setup_test_db().await;
    let conn = db.conn();

    // Deleting a non-existent workspace should not panic
    sea_orm::ConnectionTrait::execute_unprepared(
        conn,
        "DELETE FROM workspaces WHERE id = 99999",
    )
    .await
    .expect("delete nonexistent workspace should not error");
}
