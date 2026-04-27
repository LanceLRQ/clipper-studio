//! commands/tag.rs 集成测试
//!
//! 验证 tags / video_tags 两张表所支撑的标签 CRUD 与视频-标签关联逻辑。
//! 与 video_commands_test.rs 一致：通过 in-memory SQLite + 原始 SQL 验证
//! Tauri Command 内部执行的语句行为，避免构造 `State<'_, AppState>`。

use clipper_studio_lib::db::Database;

async fn setup_test_db() -> Database {
    let db = Database::connect(std::path::Path::new(":memory:"))
        .await
        .expect("failed to connect to in-memory SQLite");
    db.run_migrations().await.expect("failed to run migrations");
    db
}

async fn insert_tag(conn: &sea_orm::DatabaseConnection, name: &str, color: Option<&str>) -> i64 {
    let color_sql = color
        .map(|c| format!("'{}'", c.replace('\'', "''")))
        .unwrap_or_else(|| "NULL".to_string());

    sea_orm::ConnectionTrait::execute_unprepared(
        conn,
        &format!(
            "INSERT INTO tags (name, color) VALUES ('{}', {})",
            name.replace('\'', "''"),
            color_sql,
        ),
    )
    .await
    .expect("insert tag should succeed");

    let row = sea_orm::ConnectionTrait::query_one(
        conn,
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            format!(
                "SELECT id FROM tags WHERE name = '{}'",
                name.replace('\'', "''")
            ),
        ),
    )
    .await
    .expect("query failed")
    .expect("tag should exist");

    row.try_get::<i64>("", "id").unwrap()
}

async fn insert_video(conn: &sea_orm::DatabaseConnection, file_path: &str) -> i64 {
    sea_orm::ConnectionTrait::execute_unprepared(
        conn,
        &format!(
            "INSERT INTO videos (file_path, file_name, file_size) VALUES ('{}', 'v.mp4', 100)",
            file_path.replace('\'', "''"),
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
async fn test_create_tag_basic() {
    let db = setup_test_db().await;
    let conn = db.conn();

    let tag_id = insert_tag(conn, "搞笑", Some("#FF0000")).await;
    assert!(tag_id > 0);

    let row = sea_orm::ConnectionTrait::query_one(
        conn,
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            format!("SELECT name, color FROM tags WHERE id = {}", tag_id),
        ),
    )
    .await
    .expect("query failed")
    .expect("tag row should exist");

    let name: String = row.try_get("", "name").unwrap();
    let color: Option<String> = row.try_get("", "color").ok();
    assert_eq!(name, "搞笑");
    assert_eq!(color.as_deref(), Some("#FF0000"));
}

#[tokio::test]
async fn test_create_tag_without_color() {
    let db = setup_test_db().await;
    let conn = db.conn();

    let tag_id = insert_tag(conn, "无色标签", None).await;

    let row = sea_orm::ConnectionTrait::query_one(
        conn,
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            format!("SELECT color FROM tags WHERE id = {}", tag_id),
        ),
    )
    .await
    .expect("query failed")
    .expect("row");

    let color: Option<String> = row.try_get("", "color").ok();
    assert!(color.is_none(), "color 未提供时应为 NULL");
}

#[tokio::test]
async fn test_tag_name_unique_constraint() {
    let db = setup_test_db().await;
    let conn = db.conn();

    insert_tag(conn, "重复", None).await;

    let result = sea_orm::ConnectionTrait::execute_unprepared(
        conn,
        "INSERT INTO tags (name, color) VALUES ('重复', NULL)",
    )
    .await;

    assert!(result.is_err(), "重复 name 应被 UNIQUE 约束拒绝");
    let err = result.err().unwrap().to_string();
    assert!(
        err.to_uppercase().contains("UNIQUE"),
        "错误信息应包含 UNIQUE，便于命令层识别：{}",
        err
    );
}

#[tokio::test]
async fn test_list_tags_alphabetical() {
    let db = setup_test_db().await;
    let conn = db.conn();

    insert_tag(conn, "丙", None).await;
    insert_tag(conn, "甲", None).await;
    insert_tag(conn, "乙", None).await;

    let rows = sea_orm::ConnectionTrait::query_all(
        conn,
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            "SELECT name FROM tags ORDER BY name ASC".to_string(),
        ),
    )
    .await
    .expect("query failed");

    let names: Vec<String> = rows
        .iter()
        .filter_map(|r| r.try_get::<String>("", "name").ok())
        .collect();
    assert_eq!(names.len(), 3);
    // ORDER BY ASC：按 Unicode 码位排序，验证返回稳定顺序
    let mut expected = names.clone();
    expected.sort();
    assert_eq!(names, expected);
}

#[tokio::test]
async fn test_update_tag_name_and_color() {
    let db = setup_test_db().await;
    let conn = db.conn();
    let id = insert_tag(conn, "原名", Some("#000000")).await;

    sea_orm::ConnectionTrait::execute_unprepared(
        conn,
        &format!(
            "UPDATE tags SET name = '新名', color = '#FFFFFF' WHERE id = {}",
            id
        ),
    )
    .await
    .expect("update should succeed");

    let row = sea_orm::ConnectionTrait::query_one(
        conn,
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            format!("SELECT name, color FROM tags WHERE id = {}", id),
        ),
    )
    .await
    .expect("query")
    .expect("row");

    let name: String = row.try_get("", "name").unwrap();
    let color: Option<String> = row.try_get("", "color").ok();
    assert_eq!(name, "新名");
    assert_eq!(color.as_deref(), Some("#FFFFFF"));
}

#[tokio::test]
async fn test_update_tag_clear_color() {
    let db = setup_test_db().await;
    let conn = db.conn();
    let id = insert_tag(conn, "清色", Some("#ABCDEF")).await;

    sea_orm::ConnectionTrait::execute_unprepared(
        conn,
        &format!("UPDATE tags SET color = NULL WHERE id = {}", id),
    )
    .await
    .expect("update should succeed");

    let row = sea_orm::ConnectionTrait::query_one(
        conn,
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            format!("SELECT color FROM tags WHERE id = {}", id),
        ),
    )
    .await
    .expect("query")
    .expect("row");

    let color: Option<String> = row.try_get("", "color").ok();
    assert!(color.is_none(), "color 应被清空");
}

#[tokio::test]
async fn test_delete_tag_cascades_video_tags() {
    let db = setup_test_db().await;
    let conn = db.conn();

    let tag_id = insert_tag(conn, "待删除", None).await;
    let video_id = insert_video(conn, "/v/del.mp4").await;

    sea_orm::ConnectionTrait::execute_unprepared(
        conn,
        &format!(
            "INSERT INTO video_tags (video_id, tag_id) VALUES ({}, {})",
            video_id, tag_id
        ),
    )
    .await
    .expect("link should succeed");

    // 模拟 delete_tag：先删关联，再删 tag
    sea_orm::ConnectionTrait::execute_unprepared(
        conn,
        &format!("DELETE FROM video_tags WHERE tag_id = {}", tag_id),
    )
    .await
    .expect("delete links");

    let result = sea_orm::ConnectionTrait::execute_unprepared(
        conn,
        &format!("DELETE FROM tags WHERE id = {}", tag_id),
    )
    .await
    .expect("delete tag");

    assert_eq!(result.rows_affected(), 1, "应删除 1 行 tag");

    // 验证 tags 与 video_tags 都已清空对应记录
    let tag_left = sea_orm::ConnectionTrait::query_one(
        conn,
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            format!("SELECT id FROM tags WHERE id = {}", tag_id),
        ),
    )
    .await
    .unwrap();
    assert!(tag_left.is_none());

    let link_left = sea_orm::ConnectionTrait::query_one(
        conn,
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            format!("SELECT tag_id FROM video_tags WHERE tag_id = {}", tag_id),
        ),
    )
    .await
    .unwrap();
    assert!(link_left.is_none());
}

#[tokio::test]
async fn test_delete_nonexistent_tag_reports_zero_rows() {
    let db = setup_test_db().await;
    let conn = db.conn();

    let result =
        sea_orm::ConnectionTrait::execute_unprepared(conn, "DELETE FROM tags WHERE id = 99999")
            .await
            .expect("delete should not error");

    assert_eq!(
        result.rows_affected(),
        0,
        "delete_tag 命令依据该值返回 '标签不存在' 错误"
    );
}

#[tokio::test]
async fn test_set_video_tags_replaces_existing() {
    let db = setup_test_db().await;
    let conn = db.conn();

    let video_id = insert_video(conn, "/v/replace.mp4").await;
    let t1 = insert_tag(conn, "T1", None).await;
    let t2 = insert_tag(conn, "T2", None).await;
    let t3 = insert_tag(conn, "T3", None).await;

    // 初次设置：t1, t2
    for tid in [t1, t2] {
        sea_orm::ConnectionTrait::execute_unprepared(
            conn,
            &format!(
                "INSERT OR IGNORE INTO video_tags (video_id, tag_id) VALUES ({}, {})",
                video_id, tid
            ),
        )
        .await
        .unwrap();
    }

    // 替换：仅保留 t3 → 模拟 set_video_tags 先删后插
    sea_orm::ConnectionTrait::execute_unprepared(
        conn,
        &format!("DELETE FROM video_tags WHERE video_id = {}", video_id),
    )
    .await
    .unwrap();
    sea_orm::ConnectionTrait::execute_unprepared(
        conn,
        &format!(
            "INSERT OR IGNORE INTO video_tags (video_id, tag_id) VALUES ({}, {})",
            video_id, t3
        ),
    )
    .await
    .unwrap();

    let rows = sea_orm::ConnectionTrait::query_all(
        conn,
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            format!(
                "SELECT tag_id FROM video_tags WHERE video_id = {} ORDER BY tag_id",
                video_id
            ),
        ),
    )
    .await
    .unwrap();

    let tag_ids: Vec<i64> = rows
        .iter()
        .filter_map(|r| r.try_get::<i64>("", "tag_id").ok())
        .collect();
    assert_eq!(tag_ids, vec![t3], "set_video_tags 应替换为新集合");
}

#[tokio::test]
async fn test_set_video_tags_idempotent_insert_or_ignore() {
    let db = setup_test_db().await;
    let conn = db.conn();

    let video_id = insert_video(conn, "/v/idem.mp4").await;
    let tag_id = insert_tag(conn, "幂等", None).await;

    sea_orm::ConnectionTrait::execute_unprepared(
        conn,
        &format!(
            "INSERT OR IGNORE INTO video_tags (video_id, tag_id) VALUES ({}, {})",
            video_id, tag_id
        ),
    )
    .await
    .unwrap();

    // 重复 insert OR IGNORE 不应报错，且不产生多余行
    sea_orm::ConnectionTrait::execute_unprepared(
        conn,
        &format!(
            "INSERT OR IGNORE INTO video_tags (video_id, tag_id) VALUES ({}, {})",
            video_id, tag_id
        ),
    )
    .await
    .unwrap();

    let count_row = sea_orm::ConnectionTrait::query_one(
        conn,
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            format!(
                "SELECT COUNT(*) AS cnt FROM video_tags WHERE video_id = {} AND tag_id = {}",
                video_id, tag_id
            ),
        ),
    )
    .await
    .unwrap()
    .unwrap();

    let cnt: i64 = count_row.try_get("", "cnt").unwrap();
    assert_eq!(cnt, 1, "复合主键 + INSERT OR IGNORE 应保证仅 1 行");
}

#[tokio::test]
async fn test_get_video_tags_join_order() {
    let db = setup_test_db().await;
    let conn = db.conn();

    let video_id = insert_video(conn, "/v/join.mp4").await;
    let t_b = insert_tag(conn, "BBB", None).await;
    let t_a = insert_tag(conn, "AAA", None).await;
    let t_c = insert_tag(conn, "CCC", None).await;

    for tid in [t_b, t_a, t_c] {
        sea_orm::ConnectionTrait::execute_unprepared(
            conn,
            &format!(
                "INSERT INTO video_tags (video_id, tag_id) VALUES ({}, {})",
                video_id, tid
            ),
        )
        .await
        .unwrap();
    }

    let rows = sea_orm::ConnectionTrait::query_all(
        conn,
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            format!(
                "SELECT t.name FROM tags t \
                 INNER JOIN video_tags vt ON t.id = vt.tag_id \
                 WHERE vt.video_id = {} ORDER BY t.name ASC",
                video_id
            ),
        ),
    )
    .await
    .unwrap();

    let names: Vec<String> = rows
        .iter()
        .filter_map(|r| r.try_get::<String>("", "name").ok())
        .collect();

    assert_eq!(names, vec!["AAA", "BBB", "CCC"], "应按 name ASC 返回");
}

#[tokio::test]
async fn test_video_tags_isolation_between_videos() {
    let db = setup_test_db().await;
    let conn = db.conn();

    let v1 = insert_video(conn, "/v/v1.mp4").await;
    let v2 = insert_video(conn, "/v/v2.mp4").await;
    let shared = insert_tag(conn, "共享", None).await;
    let only_v1 = insert_tag(conn, "仅V1", None).await;

    for (vid, tid) in [(v1, shared), (v1, only_v1), (v2, shared)] {
        sea_orm::ConnectionTrait::execute_unprepared(
            conn,
            &format!(
                "INSERT INTO video_tags (video_id, tag_id) VALUES ({}, {})",
                vid, tid
            ),
        )
        .await
        .unwrap();
    }

    // 删除 v1 的全部关联，不应影响 v2
    sea_orm::ConnectionTrait::execute_unprepared(
        conn,
        &format!("DELETE FROM video_tags WHERE video_id = {}", v1),
    )
    .await
    .unwrap();

    let v2_rows = sea_orm::ConnectionTrait::query_all(
        conn,
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            format!("SELECT tag_id FROM video_tags WHERE video_id = {}", v2),
        ),
    )
    .await
    .unwrap();
    assert_eq!(v2_rows.len(), 1, "v2 的关联不应受影响");
}
