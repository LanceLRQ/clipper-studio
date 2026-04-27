//! commands/danmaku.rs 集成测试
//!
//! 验证 load_danmaku / get_danmaku_density 命令围绕 video_id → 文件路径 →
//! .xml 旁路文件的查找与解析逻辑（不依赖 DanmakuFactory 二进制）。

use clipper_studio_lib::core::danmaku::{compute_density, normalize_density, parse_bilibili_xml};
use clipper_studio_lib::db::Database;
use std::path::Path;
use tempfile::TempDir;

async fn setup_test_db() -> Database {
    let db = Database::connect(std::path::Path::new(":memory:"))
        .await
        .expect("failed to connect to in-memory SQLite");
    db.run_migrations().await.expect("failed to run migrations");
    db
}

async fn insert_video(
    conn: &sea_orm::DatabaseConnection,
    file_path: &str,
    duration_ms: Option<i64>,
) -> i64 {
    let dur_sql = duration_ms
        .map(|d| d.to_string())
        .unwrap_or_else(|| "NULL".to_string());

    sea_orm::ConnectionTrait::execute_unprepared(
        conn,
        &format!(
            "INSERT INTO videos (file_path, file_name, file_size, duration_ms) \
             VALUES ('{}', 'v.mp4', 100, {})",
            file_path.replace('\'', "''"),
            dur_sql,
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
    .unwrap()
    .unwrap();
    row.try_get::<i64>("", "id").unwrap()
}

/// 复刻 load_danmaku 的核心逻辑：通过 video_id 查 file_path，找到同名 .xml 并解析
async fn load_danmaku_for(
    conn: &sea_orm::DatabaseConnection,
    video_id: i64,
) -> Result<clipper_studio_lib::core::danmaku::DanmakuParseResult, String> {
    let row = sea_orm::ConnectionTrait::query_one(
        conn,
        sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            format!("SELECT file_path FROM videos WHERE id = {}", video_id),
        ),
    )
    .await
    .map_err(|e| e.to_string())?
    .ok_or("视频不存在".to_string())?;

    let file_path: String = row.try_get("", "file_path").unwrap_or_default();
    let xml_path = Path::new(&file_path).with_extension("xml");
    if !xml_path.exists() {
        return Err("未找到关联的弹幕 XML 文件".to_string());
    }
    parse_bilibili_xml(&xml_path).await
}

/// 在 dir 中创建 video.mp4 并写入同名 video.xml；返回 video 文件路径。
fn write_video_with_danmaku(dir: &TempDir, xml_body: &str) -> String {
    let video_path = dir.path().join("video.mp4");
    std::fs::write(&video_path, b"fake mp4").unwrap();
    let xml_path = dir.path().join("video.xml");
    std::fs::write(&xml_path, xml_body).unwrap();
    video_path.to_string_lossy().to_string()
}

const SAMPLE_XML: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<i>
  <d p="1.0,1,25,16777215,0,0,0,0">早上好</d>
  <d p="3.5,1,25,16777215,0,0,0,0">666</d>
  <d p="3.7,5,25,16777215,0,0,0,0">置顶弹幕</d>
  <d p="10.0,1,25,16777215,0,0,0,0">下午好</d>
</i>
"#;

#[tokio::test]
async fn test_load_danmaku_video_not_found() {
    let db = setup_test_db().await;
    let err = load_danmaku_for(db.conn(), 9999).await.expect_err("应失败");
    assert!(err.contains("视频不存在"), "错误信息应明确：{}", err);
}

#[tokio::test]
async fn test_load_danmaku_xml_missing() {
    let db = setup_test_db().await;
    let dir = tempfile::tempdir().unwrap();
    let video_path = dir.path().join("only_video.mp4");
    std::fs::write(&video_path, b"x").unwrap();

    let id = insert_video(db.conn(), &video_path.to_string_lossy(), Some(60_000)).await;
    let err = load_danmaku_for(db.conn(), id).await.expect_err("应失败");
    assert!(
        err.contains("XML"),
        "无 .xml 旁路时应明确提示，实际：{}",
        err
    );
}

#[tokio::test]
async fn test_load_danmaku_parses_items_in_time_order() {
    let db = setup_test_db().await;
    let dir = tempfile::tempdir().unwrap();
    let path = write_video_with_danmaku(&dir, SAMPLE_XML);

    let id = insert_video(db.conn(), &path, Some(60_000)).await;
    let result = load_danmaku_for(db.conn(), id).await.expect("parse");

    assert!(!result.is_truncated);
    assert!(result.parse_error.is_none());
    // 4 条弹幕 (special mode 7+ 才会被跳过；mode 5 = top 仍会保留)
    assert_eq!(result.items.len(), 4, "应解析 4 条弹幕");

    let times: Vec<i64> = result.items.iter().map(|d| d.time_ms).collect();
    let mut sorted = times.clone();
    sorted.sort();
    assert_eq!(times, sorted, "items 应按 time_ms 升序");
    assert_eq!(times.first(), Some(&1000));
    assert_eq!(times.last(), Some(&10000));
}

#[tokio::test]
async fn test_load_danmaku_skips_special_modes() {
    let db = setup_test_db().await;
    let dir = tempfile::tempdir().unwrap();
    let xml = r#"<?xml version="1.0"?>
<i>
  <d p="0.5,1,25,16777215,0,0,0,0">普通</d>
  <d p="0.6,7,25,16777215,0,0,0,0">高级弹幕</d>
  <d p="0.7,8,25,16777215,0,0,0,0">代码弹幕</d>
  <d p="0.8,4,25,16777215,0,0,0,0">底部</d>
</i>"#;
    let path = write_video_with_danmaku(&dir, xml);

    let id = insert_video(db.conn(), &path, Some(10_000)).await;
    let result = load_danmaku_for(db.conn(), id).await.unwrap();

    assert_eq!(result.items.len(), 2, "mode 7/8 应被跳过");
    let texts: Vec<&str> = result.items.iter().map(|d| d.text.as_str()).collect();
    assert!(texts.contains(&"普通"));
    assert!(texts.contains(&"底部"));
}

#[tokio::test]
async fn test_load_danmaku_handles_truncated_xml() {
    let db = setup_test_db().await;
    let dir = tempfile::tempdir().unwrap();
    // 故意构造 XML 解析错误：未闭合的属性引号触发 quick-xml Err
    let xml = r#"<?xml version="1.0"?>
<i>
  <d p="1.0,1,25,16777215,0,0,0,0">完整</d>
  <d p="2.0,1,25,16777215,0,0,0,0>未闭合属性</d>
</i>"#;
    let path = write_video_with_danmaku(&dir, xml);

    let id = insert_video(db.conn(), &path, Some(10_000)).await;
    let result = load_danmaku_for(db.conn(), id).await.unwrap();

    assert!(result.is_truncated, "截断 XML 应被标记 is_truncated");
    assert!(result.parse_error.is_some());
    // 截断前已成功解析的条目应保留
    assert!(!result.items.is_empty());
}

#[tokio::test]
async fn test_load_danmaku_empty_xml_returns_empty_list() {
    let db = setup_test_db().await;
    let dir = tempfile::tempdir().unwrap();
    let path = write_video_with_danmaku(&dir, "<?xml version=\"1.0\"?><i></i>");

    let id = insert_video(db.conn(), &path, Some(10_000)).await;
    let result = load_danmaku_for(db.conn(), id).await.unwrap();

    assert!(result.items.is_empty());
    assert!(!result.is_truncated);
}

// ============== get_danmaku_density 等价路径 ==============

#[tokio::test]
async fn test_get_danmaku_density_returns_normalized_window() {
    let db = setup_test_db().await;
    let dir = tempfile::tempdir().unwrap();
    let path = write_video_with_danmaku(&dir, SAMPLE_XML);

    let id = insert_video(db.conn(), &path, Some(15_000)).await;
    let parse = load_danmaku_for(db.conn(), id).await.unwrap();

    // window=5000ms → 3 个窗口（[0-5s], [5-10s], [10-15s]）
    let density = compute_density(&parse.items, 15_000, 5_000);
    assert_eq!(density.len(), 3);
    // 1.0/3.5/3.7s 落在第 0 窗口（3 条），10s 落在第 2 窗口（1 条）
    assert_eq!(density[0], 3);
    assert_eq!(density[1], 0);
    assert_eq!(density[2], 1);

    let normalized = normalize_density(&density);
    assert_eq!(normalized.len(), 3);
    assert!(
        (normalized[0] - 1.0).abs() < f32::EPSILON,
        "max 应归一为 1.0"
    );
    assert_eq!(normalized[1], 0.0);
    assert!((normalized[2] - 1.0 / 3.0).abs() < f32::EPSILON);
}

#[tokio::test]
async fn test_get_danmaku_density_zero_duration_returns_empty() {
    // duration_ms=0 时，命令会用 unwrap_or(0) 取 0，density 模块返回空向量
    let parse = clipper_studio_lib::core::danmaku::DanmakuParseResult {
        items: vec![],
        is_truncated: false,
        parse_error: None,
    };
    let density = compute_density(&parse.items, 0, 5_000);
    assert!(density.is_empty(), "duration 0 应返回空 density");

    let normalized = normalize_density(&density);
    assert!(normalized.is_empty());
}

#[tokio::test]
async fn test_get_danmaku_density_default_window_5s() {
    let db = setup_test_db().await;
    let dir = tempfile::tempdir().unwrap();
    let path = write_video_with_danmaku(&dir, SAMPLE_XML);
    let id = insert_video(db.conn(), &path, Some(20_000)).await;
    let parse = load_danmaku_for(db.conn(), id).await.unwrap();

    // 模拟命令 window_ms=None → 默认 5000
    let win: i64 = 5_000;
    let density = compute_density(&parse.items, 20_000, win);
    let expected_windows = ((20_000 + win - 1) / win) as usize;
    assert_eq!(density.len(), expected_windows);
}

#[tokio::test]
async fn test_xml_lookup_uses_video_path_stem() {
    // 验证 with_extension("xml") 正确替换扩展名
    let dir = tempfile::tempdir().unwrap();
    let video_path = dir.path().join("recording_2026.flv");
    std::fs::write(&video_path, b"x").unwrap();
    let xml_path = video_path.with_extension("xml");
    assert!(
        xml_path.to_string_lossy().ends_with("recording_2026.xml"),
        "with_extension 应替换 .flv 为 .xml"
    );
}
