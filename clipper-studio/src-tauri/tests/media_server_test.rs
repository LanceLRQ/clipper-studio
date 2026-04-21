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
