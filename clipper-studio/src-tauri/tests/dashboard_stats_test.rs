//! commands/system.rs::get_dashboard_stats 集成测试
//!
//! 验证仪表盘聚合 SQL 在不同 workspace_id 过滤下的正确性：
//! - 视频计数/时长/存储/字幕/弹幕统计
//! - 主播 / 会话 / 切片任务计数
//! - top_streamers / recent_clips JOIN 分组排序

use clipper_studio_lib::db::Database;

async fn setup_test_db() -> Database {
    let db = Database::connect(std::path::Path::new(":memory:"))
        .await
        .expect("failed to connect to in-memory SQLite");
    db.run_migrations().await.expect("failed to run migrations");
    db
}

async fn exec(conn: &sea_orm::DatabaseConnection, sql: &str) {
    sea_orm::ConnectionTrait::execute_unprepared(conn, sql)
        .await
        .unwrap_or_else(|e| panic!("SQL failed: {}\n{}", e, sql));
}

async fn query_i64(conn: &sea_orm::DatabaseConnection, sql: &str, col: &str) -> i64 {
    let row = sea_orm::ConnectionTrait::query_one(
        conn,
        sea_orm::Statement::from_string(sea_orm::DatabaseBackend::Sqlite, sql.to_string()),
    )
    .await
    .unwrap()
    .expect("expect row");
    row.try_get::<i64>("", col).unwrap_or(0)
}

/// Seed 一组完整的统计数据：2 个 workspace、3 个主播、4 段视频、2 个会话、3 个切片任务、2 个产出
async fn seed_dashboard_data(conn: &sea_orm::DatabaseConnection) -> (i64, i64) {
    exec(
        conn,
        "INSERT INTO workspaces (id, name, path, adapter_id) VALUES \
         (1, 'WS1', '/ws1', 'generic'), (2, 'WS2', '/ws2', 'generic')",
    )
    .await;

    exec(
        conn,
        "INSERT INTO streamers (id, platform, room_id, name) VALUES \
         (1, 'bilibili', '111', '主播甲'), \
         (2, 'bilibili', '222', '主播乙'), \
         (3, 'bilibili', '333', '主播丙')",
    )
    .await;

    exec(
        conn,
        "INSERT INTO recording_sessions (id, workspace_id, streamer_id) VALUES \
         (1, 1, 1), (2, 2, 2)",
    )
    .await;

    // ws1: 3 段视频，覆盖 has_subtitle/has_danmaku 各种组合
    exec(
        conn,
        "INSERT INTO videos (id, file_path, file_name, file_size, duration_ms, has_subtitle, has_danmaku, workspace_id, streamer_id, session_id) VALUES \
         (1, '/ws1/a.mp4', 'a.mp4', 1000, 10000, 1, 1, 1, 1, 1), \
         (2, '/ws1/b.mp4', 'b.mp4', 2000, 20000, 1, 0, 1, 1, 1), \
         (3, '/ws1/c.mp4', 'c.mp4', 4000, 40000, 0, 1, 1, 2, 1)",
    )
    .await;

    // ws2: 1 段视频
    exec(
        conn,
        "INSERT INTO videos (id, file_path, file_name, file_size, duration_ms, has_subtitle, has_danmaku, workspace_id, streamer_id) VALUES \
         (4, '/ws2/d.mp4', 'd.mp4', 8000, 80000, 0, 0, 2, 3)",
    )
    .await;

    // clip_tasks: 3 个，1 completed/1 failed/1 pending；title 与时间不同
    exec(
        conn,
        "INSERT INTO clip_tasks (id, video_id, start_time_ms, end_time_ms, title, status, created_at) VALUES \
         (1, 1, 0, 5000, '切片1', 'completed', '2026-01-01 10:00:00'), \
         (2, 2, 0, 3000, '切片2', 'failed',    '2026-01-02 10:00:00'), \
         (3, 4, 0, 2000, '切片3', 'pending',   '2026-01-03 10:00:00')",
    )
    .await;

    // clip_outputs：2 条，分别属于 ws1/ws2 的 task
    exec(
        conn,
        "INSERT INTO clip_outputs (clip_task_id, video_id, output_path, format, variant, file_size) VALUES \
         (1, 1, '/out/1.mp4', 'mp4', 'normal', 500), \
         (3, 4, '/out/3.mp4', 'mp4', 'normal', 700)",
    )
    .await;

    (1, 2)
}

// ============== videos 聚合 ==============

#[tokio::test]
async fn test_video_stats_no_workspace_filter() {
    let db = setup_test_db().await;
    seed_dashboard_data(db.conn()).await;

    let row = sea_orm::ConnectionTrait::query_one(
        db.conn(),
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            "SELECT COUNT(*) as cnt, COALESCE(SUM(duration_ms),0) as dur, \
             COALESCE(SUM(file_size),0) as sz, \
             COALESCE(SUM(CASE WHEN has_subtitle=1 THEN 1 ELSE 0 END),0) as sub_cnt, \
             COALESCE(SUM(CASE WHEN has_danmaku=1 THEN 1 ELSE 0 END),0) as dm_cnt \
             FROM videos"
                .to_string(),
        ),
    )
    .await
    .unwrap()
    .unwrap();

    assert_eq!(row.try_get::<i64>("", "cnt").unwrap(), 4);
    assert_eq!(row.try_get::<i64>("", "dur").unwrap(), 150_000);
    assert_eq!(row.try_get::<i64>("", "sz").unwrap(), 15_000);
    assert_eq!(row.try_get::<i64>("", "sub_cnt").unwrap(), 2);
    // 视频 1(dm=1) + 视频 3(dm=1) = 2，视频 2/4 dm=0
    assert_eq!(row.try_get::<i64>("", "dm_cnt").unwrap(), 2);
}

#[tokio::test]
async fn test_video_stats_filtered_by_workspace() {
    let db = setup_test_db().await;
    seed_dashboard_data(db.conn()).await;

    let row = sea_orm::ConnectionTrait::query_one(
        db.conn(),
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            "SELECT COUNT(*) as cnt, COALESCE(SUM(file_size),0) as sz \
             FROM videos WHERE workspace_id = 1"
                .to_string(),
        ),
    )
    .await
    .unwrap()
    .unwrap();

    assert_eq!(row.try_get::<i64>("", "cnt").unwrap(), 3, "ws1 应有 3 段");
    assert_eq!(row.try_get::<i64>("", "sz").unwrap(), 7000);
}

#[tokio::test]
async fn test_video_stats_empty_db_returns_zero() {
    let db = setup_test_db().await;
    let cnt = query_i64(
        db.conn(),
        "SELECT COUNT(*) as cnt, COALESCE(SUM(duration_ms),0) as dur FROM videos",
        "cnt",
    )
    .await;
    let dur = query_i64(
        db.conn(),
        "SELECT COALESCE(SUM(duration_ms),0) as dur FROM videos",
        "dur",
    )
    .await;
    assert_eq!(cnt, 0);
    assert_eq!(dur, 0, "COALESCE 应保证空表返回 0 而非 NULL");
}

// ============== streamers ==============

#[tokio::test]
async fn test_streamer_count_global_uses_streamers_table() {
    let db = setup_test_db().await;
    seed_dashboard_data(db.conn()).await;

    let cnt = query_i64(db.conn(), "SELECT COUNT(*) as cnt FROM streamers", "cnt").await;
    assert_eq!(cnt, 3, "全局应统计 streamers 表全部行");
}

#[tokio::test]
async fn test_streamer_count_per_workspace_uses_distinct() {
    let db = setup_test_db().await;
    seed_dashboard_data(db.conn()).await;

    // ws1 的视频涉及 streamer 1, 2 → DISTINCT = 2
    let cnt = query_i64(
        db.conn(),
        "SELECT COUNT(DISTINCT streamer_id) as cnt FROM videos \
         WHERE workspace_id = 1 AND streamer_id IS NOT NULL",
        "cnt",
    )
    .await;
    assert_eq!(cnt, 2);

    // ws2 仅 streamer 3
    let cnt2 = query_i64(
        db.conn(),
        "SELECT COUNT(DISTINCT streamer_id) as cnt FROM videos \
         WHERE workspace_id = 2 AND streamer_id IS NOT NULL",
        "cnt",
    )
    .await;
    assert_eq!(cnt2, 1);
}

// ============== sessions ==============

#[tokio::test]
async fn test_session_count_per_workspace() {
    let db = setup_test_db().await;
    seed_dashboard_data(db.conn()).await;

    let ws1 = query_i64(
        db.conn(),
        "SELECT COUNT(*) as cnt FROM recording_sessions WHERE workspace_id = 1",
        "cnt",
    )
    .await;
    assert_eq!(ws1, 1);

    let global = query_i64(
        db.conn(),
        "SELECT COUNT(*) as cnt FROM recording_sessions",
        "cnt",
    )
    .await;
    assert_eq!(global, 2);
}

// ============== clip stats ==============

#[tokio::test]
async fn test_clip_stats_global() {
    let db = setup_test_db().await;
    seed_dashboard_data(db.conn()).await;

    let row = sea_orm::ConnectionTrait::query_one(
        db.conn(),
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            "SELECT COUNT(*) as total, \
             COALESCE(SUM(CASE WHEN t.status='completed' THEN 1 ELSE 0 END),0) as done, \
             COALESCE(SUM(CASE WHEN t.status='failed' THEN 1 ELSE 0 END),0) as fail \
             FROM clip_tasks t"
                .to_string(),
        ),
    )
    .await
    .unwrap()
    .unwrap();

    assert_eq!(row.try_get::<i64>("", "total").unwrap(), 3);
    assert_eq!(row.try_get::<i64>("", "done").unwrap(), 1);
    assert_eq!(row.try_get::<i64>("", "fail").unwrap(), 1);
}

#[tokio::test]
async fn test_clip_stats_filtered_by_workspace_via_join() {
    let db = setup_test_db().await;
    seed_dashboard_data(db.conn()).await;

    // ws1: task 1(completed) + task 2(failed) = 2 条
    let row = sea_orm::ConnectionTrait::query_one(
        db.conn(),
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            "SELECT COUNT(*) as total, \
             COALESCE(SUM(CASE WHEN t.status='completed' THEN 1 ELSE 0 END),0) as done, \
             COALESCE(SUM(CASE WHEN t.status='failed' THEN 1 ELSE 0 END),0) as fail \
             FROM clip_tasks t INNER JOIN videos v ON t.video_id = v.id \
             WHERE v.workspace_id = 1"
                .to_string(),
        ),
    )
    .await
    .unwrap()
    .unwrap();

    assert_eq!(row.try_get::<i64>("", "total").unwrap(), 2);
    assert_eq!(row.try_get::<i64>("", "done").unwrap(), 1);
    assert_eq!(row.try_get::<i64>("", "fail").unwrap(), 1);
}

#[tokio::test]
async fn test_clip_output_bytes_sum() {
    let db = setup_test_db().await;
    seed_dashboard_data(db.conn()).await;

    let total = query_i64(
        db.conn(),
        "SELECT COALESCE(SUM(co.file_size),0) as sz FROM clip_outputs co",
        "sz",
    )
    .await;
    assert_eq!(total, 1200);

    let ws1 = query_i64(
        db.conn(),
        "SELECT COALESCE(SUM(co.file_size),0) as sz FROM clip_outputs co \
         INNER JOIN videos v ON co.video_id = v.id WHERE v.workspace_id = 1",
        "sz",
    )
    .await;
    assert_eq!(ws1, 500);
}

// ============== recent / top ==============

#[tokio::test]
async fn test_recent_clips_order_desc_limit_10() {
    let db = setup_test_db().await;
    seed_dashboard_data(db.conn()).await;

    let rows = sea_orm::ConnectionTrait::query_all(
        db.conn(),
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            "SELECT t.id, COALESCE(t.title,'') as title FROM clip_tasks t \
             ORDER BY t.created_at DESC LIMIT 10"
                .to_string(),
        ),
    )
    .await
    .unwrap();

    let titles: Vec<String> = rows
        .iter()
        .filter_map(|r| r.try_get::<String>("", "title").ok())
        .collect();
    assert_eq!(titles, vec!["切片3", "切片2", "切片1"], "应按时间倒序");
}

#[tokio::test]
async fn test_top_streamers_group_and_order() {
    let db = setup_test_db().await;
    seed_dashboard_data(db.conn()).await;

    let rows = sea_orm::ConnectionTrait::query_all(
        db.conn(),
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            "SELECT st.name, COUNT(v.id) as vcnt, COALESCE(SUM(v.duration_ms),0) as dur \
             FROM streamers st INNER JOIN videos v ON st.id = v.streamer_id \
             GROUP BY st.id ORDER BY vcnt DESC LIMIT 5"
                .to_string(),
        ),
    )
    .await
    .unwrap();

    // 主播甲 = 2 段（id 1,2），乙 = 1 段（id 3），丙 = 1 段（id 4）
    let first = &rows[0];
    let name: String = first.try_get("", "name").unwrap();
    let vcnt: i64 = first.try_get("", "vcnt").unwrap();
    assert_eq!(name, "主播甲");
    assert_eq!(vcnt, 2);

    // 时长检查：甲 = 10000 + 20000 = 30000
    let dur: i64 = first.try_get("", "dur").unwrap();
    assert_eq!(dur, 30_000);
}

#[tokio::test]
async fn test_top_streamers_filtered_by_workspace() {
    let db = setup_test_db().await;
    seed_dashboard_data(db.conn()).await;

    let rows = sea_orm::ConnectionTrait::query_all(
        db.conn(),
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            "SELECT st.name, COUNT(v.id) as vcnt FROM streamers st \
             INNER JOIN videos v ON st.id = v.streamer_id \
             WHERE v.workspace_id = 2 GROUP BY st.id ORDER BY vcnt DESC LIMIT 5"
                .to_string(),
        ),
    )
    .await
    .unwrap();

    assert_eq!(rows.len(), 1, "ws2 仅有主播丙");
    let name: String = rows[0].try_get("", "name").unwrap();
    assert_eq!(name, "主播丙");
}

#[tokio::test]
async fn test_recent_clips_filtered_by_workspace() {
    let db = setup_test_db().await;
    seed_dashboard_data(db.conn()).await;

    // ws2 的视频 id=4 → task 3 一条
    let rows = sea_orm::ConnectionTrait::query_all(
        db.conn(),
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            "SELECT t.id, COALESCE(t.title,'') as title FROM clip_tasks t \
             INNER JOIN videos v ON t.video_id = v.id \
             WHERE v.workspace_id = 2 ORDER BY t.created_at DESC LIMIT 10"
                .to_string(),
        ),
    )
    .await
    .unwrap();

    assert_eq!(rows.len(), 1);
    let title: String = rows[0].try_get("", "title").unwrap();
    assert_eq!(title, "切片3");
}

#[tokio::test]
async fn test_empty_db_zero_stats() {
    let db = setup_test_db().await;
    // 不调用 seed
    assert_eq!(
        query_i64(db.conn(), "SELECT COUNT(*) as cnt FROM videos", "cnt").await,
        0
    );
    assert_eq!(
        query_i64(db.conn(), "SELECT COUNT(*) as cnt FROM streamers", "cnt").await,
        0
    );
    assert_eq!(
        query_i64(db.conn(), "SELECT COUNT(*) as cnt FROM clip_tasks", "cnt").await,
        0
    );
    assert_eq!(
        query_i64(
            db.conn(),
            "SELECT COALESCE(SUM(file_size),0) as sz FROM clip_outputs",
            "sz",
        )
        .await,
        0
    );
}
