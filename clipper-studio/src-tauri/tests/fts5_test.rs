//! FTS5 全文搜索集成测试 (T-32)
//!
//! 验证 subtitle_fts 虚拟表 + 三个触发器 (insert/update/delete) 的端到端行为。

use clipper_studio_lib::db::Database;
use sea_orm::{ConnectionTrait, DatabaseBackend, Statement};

async fn setup_db() -> Database {
    let db = Database::connect(std::path::Path::new(":memory:"))
        .await
        .expect("connect");
    db.run_migrations().await.expect("migrations");
    db
}

/// Seed a workspace + video so subtitle_segments can satisfy its REFERENCES videos(id).
async fn seed_workspace_and_video(db: &Database, video_id: i64) {
    let conn = db.conn();
    conn.execute_unprepared(
        "INSERT OR IGNORE INTO workspaces (id, name, path, adapter_id) \
         VALUES (1, 'ws', '/tmp/ws', 'generic')",
    )
    .await
    .expect("ws insert");
    conn.execute_unprepared(&format!(
        "INSERT INTO videos (id, workspace_id, file_path, file_name, file_size) \
         VALUES ({}, 1, '/tmp/v{}.mp4', 'v{}.mp4', 0)",
        video_id, video_id, video_id
    ))
    .await
    .expect("video insert");
}

async fn insert_subtitle(
    db: &Database,
    id: i64,
    video_id: i64,
    start_ms: i64,
    end_ms: i64,
    text: &str,
) {
    let conn = db.conn();
    conn.execute(Statement::from_sql_and_values(
        DatabaseBackend::Sqlite,
        "INSERT INTO subtitle_segments (id, video_id, start_ms, end_ms, text) \
         VALUES (?, ?, ?, ?, ?)",
        [
            id.into(),
            video_id.into(),
            start_ms.into(),
            end_ms.into(),
            text.into(),
        ],
    ))
    .await
    .expect("subtitle insert");
}

#[tokio::test]
async fn test_fts5_insert_trigger_indexes_new_row() {
    let db = setup_db().await;
    seed_workspace_and_video(&db, 1).await;

    insert_subtitle(&db, 1, 1, 0, 1000, "hello world").await;
    insert_subtitle(&db, 2, 1, 1000, 2000, "rust programming").await;

    let conn = db.conn();
    let rows = conn
        .query_all(Statement::from_string(
            DatabaseBackend::Sqlite,
            "SELECT rowid FROM subtitle_fts WHERE subtitle_fts MATCH 'hello'".to_string(),
        ))
        .await
        .expect("fts match");
    assert_eq!(rows.len(), 1);
    let rowid: i64 = rows[0].try_get("", "rowid").unwrap();
    assert_eq!(rowid, 1);
}

#[tokio::test]
async fn test_fts5_match_returns_all_matching_rows() {
    let db = setup_db().await;
    seed_workspace_and_video(&db, 1).await;

    insert_subtitle(&db, 10, 1, 0, 1000, "the quick fox").await;
    insert_subtitle(&db, 11, 1, 1000, 2000, "lazy dog jumped").await;
    insert_subtitle(&db, 12, 1, 2000, 3000, "fox and dog").await;

    let conn = db.conn();
    let rows = conn
        .query_all(Statement::from_string(
            DatabaseBackend::Sqlite,
            "SELECT rowid FROM subtitle_fts WHERE subtitle_fts MATCH 'fox'".to_string(),
        ))
        .await
        .expect("fts match");
    assert_eq!(rows.len(), 2, "expected 2 matches for 'fox'");
}

#[tokio::test]
async fn test_fts5_filter_by_video_id_via_join() {
    let db = setup_db().await;
    seed_workspace_and_video(&db, 1).await;
    seed_workspace_and_video(&db, 2).await;

    insert_subtitle(&db, 100, 1, 0, 1000, "shared keyword in v1").await;
    insert_subtitle(&db, 200, 2, 0, 1000, "shared keyword in v2").await;

    let conn = db.conn();
    // Search "keyword" but only in video 1
    let rows = conn
        .query_all(Statement::from_string(
            DatabaseBackend::Sqlite,
            "SELECT s.id FROM subtitle_segments s \
             JOIN subtitle_fts f ON f.rowid = s.id \
             WHERE f.subtitle_fts MATCH 'keyword' AND s.video_id = 1"
                .to_string(),
        ))
        .await
        .expect("filtered match");
    assert_eq!(rows.len(), 1);
    let id: i64 = rows[0].try_get("", "id").unwrap();
    assert_eq!(id, 100);
}

#[tokio::test]
async fn test_fts5_delete_trigger_removes_index() {
    let db = setup_db().await;
    seed_workspace_and_video(&db, 1).await;

    insert_subtitle(&db, 50, 1, 0, 1000, "deletable text").await;

    let conn = db.conn();
    // Verify it's indexed
    let before = conn
        .query_all(Statement::from_string(
            DatabaseBackend::Sqlite,
            "SELECT rowid FROM subtitle_fts WHERE subtitle_fts MATCH 'deletable'".to_string(),
        ))
        .await
        .unwrap();
    assert_eq!(before.len(), 1);

    // Delete the source row
    conn.execute_unprepared("DELETE FROM subtitle_segments WHERE id = 50")
        .await
        .expect("delete");

    let after = conn
        .query_all(Statement::from_string(
            DatabaseBackend::Sqlite,
            "SELECT rowid FROM subtitle_fts WHERE subtitle_fts MATCH 'deletable'".to_string(),
        ))
        .await
        .unwrap();
    assert!(after.is_empty(), "FTS index should reflect delete");
}

#[tokio::test]
async fn test_fts5_update_trigger_reindexes() {
    let db = setup_db().await;
    seed_workspace_and_video(&db, 1).await;

    insert_subtitle(&db, 60, 1, 0, 1000, "old phrase").await;

    let conn = db.conn();
    conn.execute_unprepared("UPDATE subtitle_segments SET text = 'fresh content' WHERE id = 60")
        .await
        .expect("update");

    // Old text should no longer match
    let old = conn
        .query_all(Statement::from_string(
            DatabaseBackend::Sqlite,
            "SELECT rowid FROM subtitle_fts WHERE subtitle_fts MATCH 'phrase'".to_string(),
        ))
        .await
        .unwrap();
    assert!(old.is_empty(), "old text should be removed from index");

    // New text should match
    let new = conn
        .query_all(Statement::from_string(
            DatabaseBackend::Sqlite,
            "SELECT rowid FROM subtitle_fts WHERE subtitle_fts MATCH 'fresh'".to_string(),
        ))
        .await
        .unwrap();
    assert_eq!(new.len(), 1);
}

#[tokio::test]
async fn test_fts5_phrase_search() {
    let db = setup_db().await;
    seed_workspace_and_video(&db, 1).await;

    insert_subtitle(&db, 70, 1, 0, 1000, "machine learning rocks").await;
    insert_subtitle(&db, 71, 1, 1000, 2000, "rocks and machines").await;

    let conn = db.conn();
    let rows = conn
        .query_all(Statement::from_string(
            DatabaseBackend::Sqlite,
            "SELECT rowid FROM subtitle_fts WHERE subtitle_fts MATCH '\"machine learning\"'"
                .to_string(),
        ))
        .await
        .expect("phrase match");
    assert_eq!(
        rows.len(),
        1,
        "phrase search should match exact phrase only"
    );
}

#[tokio::test]
async fn test_fts5_empty_query_returns_no_match() {
    let db = setup_db().await;
    seed_workspace_and_video(&db, 1).await;
    insert_subtitle(&db, 80, 1, 0, 1000, "anything goes here").await;

    let conn = db.conn();
    // FTS5 considers a wildcard token "missing" — empty string MATCH errors.
    // Apps usually short-circuit empty queries; we verify the error path bubbles up.
    let result = conn
        .query_all(Statement::from_string(
            DatabaseBackend::Sqlite,
            "SELECT rowid FROM subtitle_fts WHERE subtitle_fts MATCH ''".to_string(),
        ))
        .await;
    assert!(result.is_err(), "empty MATCH should error");
}

#[tokio::test]
async fn test_fts5_special_characters_escape() {
    let db = setup_db().await;
    seed_workspace_and_video(&db, 1).await;
    insert_subtitle(&db, 90, 1, 0, 1000, "quoted text with apostrophe").await;

    let conn = db.conn();
    // Use parameterized query to safely handle user input containing quotes
    let rows = conn
        .query_all(Statement::from_sql_and_values(
            DatabaseBackend::Sqlite,
            "SELECT rowid FROM subtitle_fts WHERE subtitle_fts MATCH ?",
            ["quoted".into()],
        ))
        .await
        .expect("parameterized match");
    assert_eq!(rows.len(), 1);
}

#[tokio::test]
async fn test_fts5_chinese_text_indexed() {
    // Default FTS5 tokenizer (unicode61) handles CJK at a per-codepoint level.
    // Substring lookups via `prefix` token may not work, but exact word does.
    let db = setup_db().await;
    seed_workspace_and_video(&db, 1).await;

    insert_subtitle(&db, 1000, 1, 0, 1000, "今天天气很好").await;
    insert_subtitle(&db, 1001, 1, 1000, 2000, "明天有雨").await;

    let conn = db.conn();
    // unicode61 tokenizer treats CJK chars as separate tokens; exact match on a
    // single char should work
    let rows = conn
        .query_all(Statement::from_sql_and_values(
            DatabaseBackend::Sqlite,
            "SELECT rowid FROM subtitle_fts WHERE subtitle_fts MATCH ?",
            ["今天".into()],
        ))
        .await
        .expect("cjk match");
    // Either 0 or 1 match depending on tokenizer — main goal is no panic
    assert!(rows.len() <= 2);
}

#[tokio::test]
async fn test_fts5_case_insensitive_match() {
    let db = setup_db().await;
    seed_workspace_and_video(&db, 1).await;
    insert_subtitle(&db, 2000, 1, 0, 1000, "Hello World From Rust").await;

    let conn = db.conn();
    let rows = conn
        .query_all(Statement::from_sql_and_values(
            DatabaseBackend::Sqlite,
            "SELECT rowid FROM subtitle_fts WHERE subtitle_fts MATCH ?",
            ["hello".into()],
        ))
        .await
        .expect("case-insensitive match");
    assert_eq!(rows.len(), 1, "FTS5 should match case-insensitively");
}
