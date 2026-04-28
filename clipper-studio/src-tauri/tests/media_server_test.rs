use clipper_studio_lib::core::media_server::MediaServer;

#[tokio::test]
async fn test_media_server_starts() {
    let server = MediaServer::start().await.expect("server should start");
    assert!(server.port() > 0, "should bind to a non-zero port");
}

#[tokio::test]
async fn test_media_server_file_url_format() {
    let server = MediaServer::start().await.expect("server should start");
    let url = server.file_url("/path/to/video.mp4");

    assert!(
        url.starts_with("http://127.0.0.1:"),
        "url should start with correct host"
    );
    assert!(
        url.contains("/serve?path="),
        "url should contain /serve?path="
    );
    assert!(
        url.contains(&*urlencoding::encode("/path/to/video.mp4")),
        "url should contain encoded path"
    );
}

#[tokio::test]
async fn test_media_server_file_url_encodes_special_chars() {
    let server = MediaServer::start().await.expect("server should start");
    let url = server.file_url("/path/with spaces/视频.flv");

    assert!(!url.contains(' '), "url should not contain raw spaces");
    assert!(
        url.contains(&*urlencoding::encode("/path/with spaces/视频.flv")),
        "url should encode special characters"
    );
}

#[tokio::test]
async fn test_media_server_port_is_unique() {
    let server1 = MediaServer::start().await.expect("server 1 should start");
    let server2 = MediaServer::start().await.expect("server 2 should start");

    assert_ne!(
        server1.port(),
        server2.port(),
        "two servers should get different ports"
    );
}

#[tokio::test]
async fn test_media_server_serve_not_found() {
    let server = MediaServer::start().await.expect("server should start");
    let url = server.file_url("/nonexistent/file.mp4");

    let client = reqwest::Client::new();
    let resp = client
        .get(&url)
        .send()
        .await
        .expect("request should complete");

    assert_eq!(
        resp.status(),
        reqwest::StatusCode::NOT_FOUND,
        "nonexistent file should return 404"
    );
}

#[tokio::test]
async fn test_media_server_serves_real_file() {
    let server = MediaServer::start().await.expect("server should start");

    // Create a temporary file
    let tmp = tempfile::NamedTempFile::with_suffix(".mp4").unwrap();
    std::io::Write::write_all(&mut tmp.as_file().try_clone().unwrap(), b"fake mp4 data").unwrap();
    let path = tmp.path().to_string_lossy().to_string();
    server.allow_prefix(tmp.path().parent().unwrap());

    let url = server.file_url(&path);
    let client = reqwest::Client::new();
    let resp = client
        .get(&url)
        .send()
        .await
        .expect("request should complete");

    assert_eq!(
        resp.status(),
        reqwest::StatusCode::OK,
        "existing file should return 200"
    );

    let content_type = resp
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    assert!(
        content_type.contains("video/mp4"),
        "content type should be video/mp4, got: {}",
        content_type
    );
}

#[tokio::test]
async fn test_media_server_content_type_webm() {
    let server = MediaServer::start().await.expect("server should start");

    let tmp = tempfile::NamedTempFile::with_suffix(".webm").unwrap();
    std::io::Write::write_all(&mut tmp.as_file().try_clone().unwrap(), b"webm data").unwrap();
    let path = tmp.path().to_string_lossy().to_string();
    server.allow_prefix(tmp.path().parent().unwrap());

    let url = server.file_url(&path);
    let client = reqwest::Client::new();
    let resp = client
        .get(&url)
        .send()
        .await
        .expect("request should complete");

    let content_type = resp
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    assert!(
        content_type.contains("video/webm"),
        "content type should be video/webm, got: {}",
        content_type
    );
}

#[tokio::test]
async fn test_media_server_content_type_ts() {
    let server = MediaServer::start().await.expect("server should start");

    let tmp = tempfile::NamedTempFile::with_suffix(".ts").unwrap();
    std::io::Write::write_all(&mut tmp.as_file().try_clone().unwrap(), b"ts data").unwrap();
    let path = tmp.path().to_string_lossy().to_string();
    server.allow_prefix(tmp.path().parent().unwrap());

    let url = server.file_url(&path);
    let client = reqwest::Client::new();
    let resp = client
        .get(&url)
        .send()
        .await
        .expect("request should complete");

    let content_type = resp
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    assert!(
        content_type.contains("video/mp2t"),
        "content type should be video/mp2t, got: {}",
        content_type
    );
}

#[tokio::test]
async fn test_media_server_content_type_unknown() {
    let server = MediaServer::start().await.expect("server should start");

    let tmp = tempfile::NamedTempFile::with_suffix(".xyz").unwrap();
    std::io::Write::write_all(&mut tmp.as_file().try_clone().unwrap(), b"unknown data").unwrap();
    let path = tmp.path().to_string_lossy().to_string();
    server.allow_prefix(tmp.path().parent().unwrap());

    let url = server.file_url(&path);
    let client = reqwest::Client::new();
    let resp = client
        .get(&url)
        .send()
        .await
        .expect("request should complete");

    let content_type = resp
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    assert!(
        content_type.contains("application/octet-stream"),
        "unknown extension should return application/octet-stream, got: {}",
        content_type
    );
}

#[tokio::test]
async fn test_media_server_serves_file_content() {
    let server = MediaServer::start().await.expect("server should start");

    let data = b"hello from clipper studio!";
    let tmp = tempfile::NamedTempFile::with_suffix(".mp4").unwrap();
    std::io::Write::write_all(&mut tmp.as_file().try_clone().unwrap(), data).unwrap();
    let path = tmp.path().to_string_lossy().to_string();
    server.allow_prefix(tmp.path().parent().unwrap());

    let url = server.file_url(&path);
    let client = reqwest::Client::new();
    let body = client
        .get(&url)
        .send()
        .await
        .expect("request should complete")
        .bytes()
        .await
        .expect("should read body");

    assert_eq!(&body[..], data, "response body should match file content");
}

// ============== is_path_allowed / allow_prefix（SEC-FS-03 安全校验） ==============

#[tokio::test]
async fn test_is_path_allowed_empty_whitelist_rejects_all() {
    let server = MediaServer::start().await.expect("start");
    let dir = tempfile::tempdir().unwrap();
    assert!(
        !server.is_path_allowed(dir.path()),
        "未登记前缀时所有路径都应被拒绝"
    );
}

#[tokio::test]
async fn test_is_path_allowed_accepts_exact_prefix() {
    let server = MediaServer::start().await.expect("start");
    let dir = tempfile::tempdir().unwrap();
    server.allow_prefix(dir.path());
    assert!(server.is_path_allowed(dir.path()), "已登记目录应被允许");
}

#[tokio::test]
async fn test_is_path_allowed_accepts_file_under_prefix() {
    let server = MediaServer::start().await.expect("start");
    let dir = tempfile::tempdir().unwrap();
    server.allow_prefix(dir.path());

    let file = dir.path().join("video.mp4");
    std::fs::write(&file, b"x").unwrap();
    assert!(server.is_path_allowed(&file), "前缀下文件应被允许");
}

#[tokio::test]
async fn test_is_path_allowed_rejects_outside_prefix() {
    let server = MediaServer::start().await.expect("start");
    let allowed_dir = tempfile::tempdir().unwrap();
    let outside_dir = tempfile::tempdir().unwrap();
    server.allow_prefix(allowed_dir.path());

    let outside_file = outside_dir.path().join("evil.mp4");
    std::fs::write(&outside_file, b"x").unwrap();
    assert!(!server.is_path_allowed(&outside_file), "前缀外路径应被拒绝");
}

#[tokio::test]
async fn test_is_path_allowed_handles_nonexistent_subdir() {
    // 业务场景：用户指定的输出目录尚未创建，应通过逐级 canonicalize 父目录通过校验
    let server = MediaServer::start().await.expect("start");
    let dir = tempfile::tempdir().unwrap();
    server.allow_prefix(dir.path());

    let new_subdir = dir.path().join("not_yet_created");
    assert!(!new_subdir.exists());
    assert!(
        server.is_path_allowed(&new_subdir),
        "尚未创建的子路径，父目录在白名单内应被允许"
    );
}

#[tokio::test]
async fn test_is_path_allowed_rejects_nonexistent_root() {
    let server = MediaServer::start().await.expect("start");
    // 不登记任何前缀，且路径完全不存在
    let bogus = std::path::PathBuf::from("/this/path/definitely/does/not/exist/12345");
    assert!(
        !server.is_path_allowed(&bogus),
        "完全不存在且无白名单应被拒绝"
    );
}

#[tokio::test]
async fn test_allow_prefix_dedup_same_path() {
    // 重复登记同一路径不应造成重复条目；可通过登记后能正常匹配证明
    let server = MediaServer::start().await.expect("start");
    let dir = tempfile::tempdir().unwrap();
    server.allow_prefix(dir.path());
    server.allow_prefix(dir.path()); // 重复
    server.allow_prefix(dir.path()); // 再重复

    assert!(server.is_path_allowed(dir.path()), "重复登记不影响匹配");
}

#[tokio::test]
async fn test_allow_prefix_multiple_distinct_dirs() {
    let server = MediaServer::start().await.expect("start");
    let dir1 = tempfile::tempdir().unwrap();
    let dir2 = tempfile::tempdir().unwrap();
    server.allow_prefix(dir1.path());
    server.allow_prefix(dir2.path());

    assert!(server.is_path_allowed(dir1.path()));
    assert!(server.is_path_allowed(dir2.path()));
}

#[tokio::test]
async fn test_serve_endpoint_rejects_unallowed_path() {
    // 集成校验：serve 路径校验同样依赖 allow_prefix 白名单，越界应返回 403
    let server = MediaServer::start().await.expect("start");
    let port = server.port();

    // 故意创建实际文件，但不登记其目录
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("uninvited.mp4");
    std::fs::write(&file, b"data").unwrap();

    let url = format!(
        "http://127.0.0.1:{}/serve?path={}",
        port,
        urlencoding::encode(&file.to_string_lossy())
    );

    let resp = reqwest::get(&url).await.expect("request should complete");
    assert_eq!(
        resp.status().as_u16(),
        403,
        "未登记白名单的路径应返回 403 Forbidden"
    );
}
