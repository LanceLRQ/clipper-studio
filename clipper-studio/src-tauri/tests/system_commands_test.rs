//! commands/system.rs 杂项命令集成测试
//!
//! 覆盖与 AppState 解耦的纯 SQL / JSON 行为：
//! - track_event：INSERT analytics_events + properties JSON 序列化 + 单引号转义
//! - has_workspaces 片段：COUNT(*) FROM workspaces
//! - check_ffmpeg / get_app_info 的 JSON 形状（不依赖真实 ffmpeg 二进制）

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

/// 复刻 track_event 命令的 INSERT 逻辑
async fn track_event_for(
    db: &Database,
    event: &str,
    properties: Option<serde_json::Value>,
) -> Result<(), String> {
    let props_sql = properties
        .map(|p| format!("'{}'", p.to_string().replace('\'', "''")))
        .unwrap_or("NULL".to_string());

    sea_orm::ConnectionTrait::execute_unprepared(
        db.conn(),
        &format!(
            "INSERT INTO analytics_events (event, properties) VALUES ('{}', {})",
            event.replace('\'', "''"),
            props_sql
        ),
    )
    .await
    .map_err(|e| e.to_string())?;
    Ok(())
}

// ============== track_event ==============

#[tokio::test]
async fn test_track_event_no_properties_inserts_null() {
    let db = setup_test_db().await;
    track_event_for(&db, "app.launch", None).await.unwrap();

    let row = sea_orm::ConnectionTrait::query_one(
        db.conn(),
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            "SELECT event, properties, created_at FROM analytics_events WHERE event = 'app.launch'"
                .to_string(),
        ),
    )
    .await
    .unwrap()
    .unwrap();

    let event: String = row.try_get("", "event").unwrap();
    let props: Option<String> = row.try_get("", "properties").ok();
    let created: String = row.try_get("", "created_at").unwrap();

    assert_eq!(event, "app.launch");
    assert!(props.is_none(), "未提供 properties 时应为 NULL");
    assert!(!created.is_empty(), "created_at 应有默认 datetime");
}

#[tokio::test]
async fn test_track_event_with_properties_serializes_json() {
    let db = setup_test_db().await;
    let props = serde_json::json!({
        "video_id": 42,
        "duration_ms": 60000,
        "source": "recorder",
    });
    track_event_for(&db, "video.imported", Some(props.clone()))
        .await
        .unwrap();

    let row = sea_orm::ConnectionTrait::query_one(
        db.conn(),
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            "SELECT properties FROM analytics_events WHERE event = 'video.imported'".to_string(),
        ),
    )
    .await
    .unwrap()
    .unwrap();
    let raw: String = row.try_get("", "properties").unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&raw).expect("valid JSON");

    assert_eq!(parsed["video_id"], 42);
    assert_eq!(parsed["duration_ms"], 60000);
    assert_eq!(parsed["source"], "recorder");
}

#[tokio::test]
async fn test_track_event_escapes_quote_in_event_name() {
    let db = setup_test_db().await;
    // 事件名含单引号
    track_event_for(&db, "user's_action", None).await.unwrap();

    let row = sea_orm::ConnectionTrait::query_one(
        db.conn(),
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            "SELECT event FROM analytics_events WHERE event = 'user''s_action'".to_string(),
        ),
    )
    .await
    .unwrap()
    .unwrap();
    let event: String = row.try_get("", "event").unwrap();
    assert_eq!(event, "user's_action", "单引号应被正确转义并完整存储");
}

#[tokio::test]
async fn test_track_event_escapes_quote_in_properties_json() {
    let db = setup_test_db().await;
    let props = serde_json::json!({ "comment": "it's great" });
    track_event_for(&db, "feedback", Some(props)).await.unwrap();

    let row = sea_orm::ConnectionTrait::query_one(
        db.conn(),
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            "SELECT properties FROM analytics_events WHERE event = 'feedback'".to_string(),
        ),
    )
    .await
    .unwrap()
    .unwrap();
    let raw: String = row.try_get("", "properties").unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&raw).expect("valid JSON");
    assert_eq!(parsed["comment"], "it's great");
}

#[tokio::test]
async fn test_track_event_multiple_inserts_keep_separate_rows() {
    let db = setup_test_db().await;
    for i in 0..5 {
        track_event_for(&db, "loop", Some(serde_json::json!({ "i": i })))
            .await
            .unwrap();
    }

    let row = sea_orm::ConnectionTrait::query_one(
        db.conn(),
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            "SELECT COUNT(*) as cnt FROM analytics_events WHERE event = 'loop'".to_string(),
        ),
    )
    .await
    .unwrap()
    .unwrap();
    let cnt: i64 = row.try_get("", "cnt").unwrap();
    assert_eq!(cnt, 5);
}

#[tokio::test]
async fn test_analytics_events_index_event_created() {
    // SCHEMA 上的索引保证：按 event + created_at 查询应可用（不验性能，仅功能可达）
    let db = setup_test_db().await;
    track_event_for(&db, "x", None).await.unwrap();
    let rows = sea_orm::ConnectionTrait::query_all(
        db.conn(),
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            "SELECT event FROM analytics_events WHERE event = 'x' ORDER BY created_at DESC"
                .to_string(),
        ),
    )
    .await
    .unwrap();
    assert_eq!(rows.len(), 1);
}

// ============== get_app_info: has_workspaces 片段 ==============

#[tokio::test]
async fn test_has_workspaces_false_when_empty() {
    let db = setup_test_db().await;
    // 复刻 has_workspaces SQL
    let row = sea_orm::ConnectionTrait::query_one(
        db.conn(),
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            "SELECT COUNT(*) as cnt FROM workspaces".to_string(),
        ),
    )
    .await
    .unwrap()
    .unwrap();
    let cnt: i32 = row.try_get("", "cnt").unwrap_or(0);
    assert_eq!(cnt, 0);
    assert!(cnt <= 0, "空表 has_workspaces 应为 false");
}

#[tokio::test]
async fn test_has_workspaces_true_after_insert() {
    let db = setup_test_db().await;
    exec(
        db.conn(),
        "INSERT INTO workspaces (name, path, adapter_id) VALUES ('ws', '/p', 'generic')",
    )
    .await;

    let row = sea_orm::ConnectionTrait::query_one(
        db.conn(),
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            "SELECT COUNT(*) as cnt FROM workspaces".to_string(),
        ),
    )
    .await
    .unwrap()
    .unwrap();
    let cnt: i32 = row.try_get("", "cnt").unwrap_or(0);
    assert!(cnt > 0, "插入后 has_workspaces 应为 true");
}

// ============== check_ffmpeg JSON 形状 ==============

/// 复刻 check_ffmpeg 的 JSON 构造（不调用真实 ffmpeg）
fn build_check_ffmpeg_json(
    ffmpeg_path: &str,
    ffmpeg_version: Option<&str>,
    ffprobe_path: &str,
) -> serde_json::Value {
    serde_json::json!({
        "ffmpeg": {
            "available": !ffmpeg_path.is_empty(),
            "path": ffmpeg_path,
            "version": ffmpeg_version,
        },
        "ffprobe": {
            "available": !ffprobe_path.is_empty(),
            "path": ffprobe_path,
        }
    })
}

#[test]
fn test_check_ffmpeg_json_when_both_present() {
    let json = build_check_ffmpeg_json("/bin/ffmpeg", Some("4.4.2"), "/bin/ffprobe");
    assert_eq!(json["ffmpeg"]["available"], true);
    assert_eq!(json["ffmpeg"]["path"], "/bin/ffmpeg");
    assert_eq!(json["ffmpeg"]["version"], "4.4.2");
    assert_eq!(json["ffprobe"]["available"], true);
    assert_eq!(json["ffprobe"]["path"], "/bin/ffprobe");
}

#[test]
fn test_check_ffmpeg_json_when_unavailable() {
    let json = build_check_ffmpeg_json("", None, "");
    assert_eq!(json["ffmpeg"]["available"], false);
    assert_eq!(json["ffmpeg"]["path"], "");
    assert!(json["ffmpeg"]["version"].is_null());
    assert_eq!(json["ffprobe"]["available"], false);
}

#[test]
fn test_check_ffmpeg_json_partial_only_ffmpeg() {
    let json = build_check_ffmpeg_json("/bin/ffmpeg", Some("5.1"), "");
    assert_eq!(json["ffmpeg"]["available"], true);
    assert_eq!(json["ffprobe"]["available"], false);
    assert_eq!(json["ffmpeg"]["version"], "5.1");
}

#[test]
fn test_check_ffmpeg_json_keys_stable() {
    // 前端依赖固定字段名，这里防止键名漂移
    let json = build_check_ffmpeg_json("/x", None, "/y");
    let ffmpeg = json["ffmpeg"].as_object().unwrap();
    let ffprobe = json["ffprobe"].as_object().unwrap();
    assert!(ffmpeg.contains_key("available"));
    assert!(ffmpeg.contains_key("path"));
    assert!(ffmpeg.contains_key("version"));
    assert!(ffprobe.contains_key("available"));
    assert!(ffprobe.contains_key("path"));
}

// ============== AppInfo 字段语义（不依赖 AppState）==============

#[test]
fn test_app_info_version_matches_cargo_pkg_version() {
    // get_app_info 中：version: env!("CARGO_PKG_VERSION").to_string()
    let v = env!("CARGO_PKG_VERSION");
    assert!(!v.is_empty(), "CARGO_PKG_VERSION 应非空");
    // 形如 0.1.1
    assert!(v.split('.').count() >= 2, "版本号应至少含两段：{}", v);
}
