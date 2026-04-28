//! commands/asr.rs 查询类集成测试
//!
//! 直接调用 asr/service.rs 中的 list_asr_tasks / list_subtitles /
//! search_subtitles / search_subtitles_global，因这些函数仅依赖 `&Database`，
//! 不持有 AppState，可端到端测试 SQL + FTS5 行为。

use clipper_studio_lib::asr::service::{
    list_asr_tasks, list_subtitles, search_subtitles, search_subtitles_global,
};
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

async fn insert_subtitle(db: &Database, video_id: i64, start: i64, end: i64, text: &str) {
    exec(
        db.conn(),
        &format!(
            "INSERT INTO subtitle_segments (video_id, start_ms, end_ms, text) \
             VALUES ({}, {}, {}, '{}')",
            video_id,
            start,
            end,
            text.replace('\'', "''"),
        ),
    )
    .await;
}

async fn insert_asr_task(db: &Database, video_id: i64, status: &str, progress: f64) {
    exec(
        db.conn(),
        &format!(
            "INSERT INTO asr_tasks (video_id, status, progress) \
             VALUES ({}, '{}', {})",
            video_id, status, progress
        ),
    )
    .await;
}

// ============== list_asr_tasks ==============

#[tokio::test]
async fn test_list_asr_tasks_empty() {
    let db = setup_test_db().await;
    let tasks = list_asr_tasks(&db, None).await.unwrap();
    assert!(tasks.is_empty());
}

#[tokio::test]
async fn test_list_asr_tasks_filtered_by_video() {
    let db = setup_test_db().await;
    insert_video(&db, 1, "/v1.mp4", "v1.mp4").await;
    insert_video(&db, 2, "/v2.mp4", "v2.mp4").await;
    insert_asr_task(&db, 1, "completed", 1.0).await;
    insert_asr_task(&db, 1, "pending", 0.0).await;
    insert_asr_task(&db, 2, "processing", 0.5).await;

    let v1_tasks = list_asr_tasks(&db, Some(1)).await.unwrap();
    assert_eq!(v1_tasks.len(), 2);
    assert!(v1_tasks.iter().all(|t| t.video_id == 1));

    let v2_tasks = list_asr_tasks(&db, Some(2)).await.unwrap();
    assert_eq!(v2_tasks.len(), 1);
    assert_eq!(v2_tasks[0].status, "processing");
    assert!((v2_tasks[0].progress - 0.5).abs() < 1e-9);

    let all = list_asr_tasks(&db, None).await.unwrap();
    assert_eq!(all.len(), 3);
}

#[tokio::test]
async fn test_list_asr_tasks_order_by_created_at_desc() {
    let db = setup_test_db().await;
    insert_video(&db, 1, "/v.mp4", "v.mp4").await;
    insert_asr_task(&db, 1, "pending", 0.0).await;
    tokio::time::sleep(std::time::Duration::from_millis(1100)).await;
    insert_asr_task(&db, 1, "completed", 1.0).await;

    let tasks = list_asr_tasks(&db, Some(1)).await.unwrap();
    assert_eq!(tasks[0].status, "completed", "最新任务应排在最前");
    assert_eq!(tasks[1].status, "pending");
}

#[tokio::test]
async fn test_list_asr_tasks_default_progress_zero() {
    let db = setup_test_db().await;
    insert_video(&db, 1, "/v.mp4", "v.mp4").await;
    exec(db.conn(), "INSERT INTO asr_tasks (video_id) VALUES (1)").await;

    let tasks = list_asr_tasks(&db, Some(1)).await.unwrap();
    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0].status, "pending", "默认 status");
    assert_eq!(tasks[0].progress, 0.0);
    assert_eq!(tasks[0].retry_count, 0);
}

// ============== list_subtitles ==============

#[tokio::test]
async fn test_list_subtitles_empty() {
    let db = setup_test_db().await;
    let subs = list_subtitles(&db, 1).await.unwrap();
    assert!(subs.is_empty());
}

#[tokio::test]
async fn test_list_subtitles_ordered_by_start_ms_asc() {
    let db = setup_test_db().await;
    insert_video(&db, 1, "/v.mp4", "v.mp4").await;
    insert_subtitle(&db, 1, 5000, 6000, "second").await;
    insert_subtitle(&db, 1, 1000, 2000, "first").await;
    insert_subtitle(&db, 1, 3000, 4000, "middle").await;

    let subs = list_subtitles(&db, 1).await.unwrap();
    assert_eq!(subs.len(), 3);
    assert_eq!(subs[0].text, "first");
    assert_eq!(subs[1].text, "middle");
    assert_eq!(subs[2].text, "second");
}

#[tokio::test]
async fn test_list_subtitles_isolated_by_video_id() {
    let db = setup_test_db().await;
    insert_video(&db, 1, "/v1.mp4", "v1.mp4").await;
    insert_video(&db, 2, "/v2.mp4", "v2.mp4").await;
    insert_subtitle(&db, 1, 0, 1000, "video 1 sub").await;
    insert_subtitle(&db, 2, 0, 1000, "video 2 sub").await;

    let v1 = list_subtitles(&db, 1).await.unwrap();
    assert_eq!(v1.len(), 1);
    assert_eq!(v1[0].text, "video 1 sub");

    let v2 = list_subtitles(&db, 2).await.unwrap();
    assert_eq!(v2.len(), 1);
    assert_eq!(v2[0].text, "video 2 sub");
}

#[tokio::test]
async fn test_list_subtitles_default_language_and_source() {
    let db = setup_test_db().await;
    insert_video(&db, 1, "/v.mp4", "v.mp4").await;
    insert_subtitle(&db, 1, 0, 1000, "x").await;

    let subs = list_subtitles(&db, 1).await.unwrap();
    assert_eq!(subs[0].language, "zh", "language 默认 zh");
    assert_eq!(subs[0].source, "asr", "source 默认 asr");
}

// ============== search_subtitles (FTS5) ==============

#[tokio::test]
async fn test_search_subtitles_finds_matching_text() {
    let db = setup_test_db().await;
    insert_video(&db, 1, "/v.mp4", "v.mp4").await;
    insert_subtitle(&db, 1, 0, 1000, "hello world").await;
    insert_subtitle(&db, 1, 1000, 2000, "goodbye world").await;
    insert_subtitle(&db, 1, 2000, 3000, "totally unrelated").await;

    let results = search_subtitles(&db, "hello", None).await.unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].text, "hello world");
}

#[tokio::test]
async fn test_search_subtitles_filtered_by_video_id() {
    let db = setup_test_db().await;
    insert_video(&db, 1, "/v1.mp4", "v1.mp4").await;
    insert_video(&db, 2, "/v2.mp4", "v2.mp4").await;
    insert_subtitle(&db, 1, 0, 1000, "shared keyword").await;
    insert_subtitle(&db, 2, 0, 1000, "shared keyword").await;

    let v1_only = search_subtitles(&db, "shared", Some(1)).await.unwrap();
    assert_eq!(v1_only.len(), 1);
    assert_eq!(v1_only[0].video_id, 1);

    let global = search_subtitles(&db, "shared", None).await.unwrap();
    assert_eq!(global.len(), 2);
}

#[tokio::test]
async fn test_search_subtitles_no_match_returns_empty() {
    let db = setup_test_db().await;
    insert_video(&db, 1, "/v.mp4", "v.mp4").await;
    insert_subtitle(&db, 1, 0, 1000, "hello").await;

    let results = search_subtitles(&db, "nothing_matches", None)
        .await
        .unwrap();
    assert!(results.is_empty());
}

#[tokio::test]
async fn test_search_subtitles_returns_in_start_ms_order() {
    let db = setup_test_db().await;
    insert_video(&db, 1, "/v.mp4", "v.mp4").await;
    insert_subtitle(&db, 1, 5000, 6000, "keyword later").await;
    insert_subtitle(&db, 1, 1000, 2000, "keyword first").await;

    let results = search_subtitles(&db, "keyword", Some(1)).await.unwrap();
    assert_eq!(results.len(), 2);
    assert!(results[0].start_ms < results[1].start_ms);
}

// ============== search_subtitles_global ==============

#[tokio::test]
async fn test_search_global_includes_video_metadata() {
    let db = setup_test_db().await;
    exec(
        db.conn(),
        "INSERT INTO streamers (id, platform, room_id, name) VALUES (1, 'b', '111', '主播甲')",
    )
    .await;
    exec(
        db.conn(),
        "INSERT INTO videos (id, file_path, file_name, file_size, duration_ms, streamer_id, stream_title, recorded_at) \
         VALUES (1, '/v.mp4', 'v.mp4', 100, 60000, 1, '直播标题', '2026-01-01 10:00:00')",
    )
    .await;
    insert_subtitle(&db, 1, 1000, 2000, "搜索关键字").await;

    let results = search_subtitles_global(&db, "搜索关键字").await.unwrap();
    assert_eq!(results.len(), 1);
    let r = &results[0];
    assert_eq!(r.video_file_name, "v.mp4");
    assert_eq!(r.video_duration_ms, Some(60000));
    assert_eq!(r.streamer_name.as_deref(), Some("主播甲"));
    assert_eq!(r.stream_title.as_deref(), Some("直播标题"));
    assert_eq!(r.recorded_at.as_deref(), Some("2026-01-01 10:00:00"));
}

#[tokio::test]
async fn test_search_global_empty_query_returns_empty() {
    let db = setup_test_db().await;
    insert_video(&db, 1, "/v.mp4", "v.mp4").await;
    insert_subtitle(&db, 1, 0, 1000, "anything").await;

    // sanitize_fts5_query 对空字符串返回 ""，命令直接 return Ok(vec![])
    let results = search_subtitles_global(&db, "   ").await.unwrap();
    assert!(results.is_empty(), "全空白查询应返回空");
}

#[tokio::test]
async fn test_search_global_handles_quote_in_query() {
    let db = setup_test_db().await;
    insert_video(&db, 1, "/v.mp4", "v.mp4").await;
    insert_subtitle(&db, 1, 0, 1000, r#"包含"双引号"的字幕"#).await;

    // 不应 panic 或 SQL 注入崩溃
    let results = search_subtitles_global(&db, r#""双引号""#).await.unwrap();
    // 至少不报错；命中与否依赖 FTS5 分词，仅验证不崩
    let _ = results;
}

#[tokio::test]
async fn test_search_global_left_join_handles_missing_streamer() {
    let db = setup_test_db().await;
    // 视频无 streamer_id，LEFT JOIN 应返回 streamer_name=None
    exec(
        db.conn(),
        "INSERT INTO videos (id, file_path, file_name, file_size) \
         VALUES (1, '/v.mp4', 'orphan.mp4', 100)",
    )
    .await;
    insert_subtitle(&db, 1, 0, 1000, "孤儿视频").await;

    let results = search_subtitles_global(&db, "孤儿视频").await.unwrap();
    assert_eq!(results.len(), 1);
    assert!(results[0].streamer_name.is_none());
    assert!(results[0].stream_title.is_none());
}

#[tokio::test]
async fn test_search_global_limit_200() {
    let db = setup_test_db().await;
    insert_video(&db, 1, "/v.mp4", "v.mp4").await;
    // 插入 250 条带相同关键字的字幕
    for i in 0..250 {
        insert_subtitle(&db, 1, i * 1000, i * 1000 + 500, "limittest").await;
    }

    let results = search_subtitles_global(&db, "limittest").await.unwrap();
    assert!(
        results.len() <= 200,
        "全局搜索应受 LIMIT 200 约束，实际：{}",
        results.len()
    );
}
