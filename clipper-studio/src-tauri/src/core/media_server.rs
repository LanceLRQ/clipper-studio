use axum::{
    extract::{Query, State},
    http::{header, HeaderValue, Method, StatusCode},
    response::IntoResponse,
    routing::get,
    Router,
};
use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use tokio::net::TcpListener;
use tower_http::cors::{AllowOrigin, CorsLayer};

/// Local HTTP server for serving media files with Range request support.
///
/// 之所以需要自建本地 HTTP 服务器：
/// 1. Tauri 的 asset 协议对 Range 请求支持不稳定
/// 2. FLV 等格式需要 FFmpeg 二次处理后才能在浏览器播放
/// 3. 自建服务能可靠地支持大文件 seek
///
/// 安全：
/// - 绑定在 `127.0.0.1` 随机端口
/// - 通过 `allow_prefix` 显式登记白名单目录，任何越界路径返回 403
/// - CORS 仅允许 Tauri webview 及本地开发 origin
pub struct MediaServer {
    port: u16,
    allowed_prefixes: Arc<RwLock<Vec<PathBuf>>>,
}

#[derive(Clone)]
struct ServerState {
    allowed_prefixes: Arc<RwLock<Vec<PathBuf>>>,
}

#[derive(Debug, Deserialize)]
struct ServeParams {
    path: String,
}

impl MediaServer {
    /// Start the media server on a random available port.
    pub async fn start() -> Result<Self, Box<dyn std::error::Error>> {
        let allowed_prefixes: Arc<RwLock<Vec<PathBuf>>> = Arc::new(RwLock::new(Vec::new()));

        let state = ServerState {
            allowed_prefixes: allowed_prefixes.clone(),
        };

        // CORS：仅允许 Tauri webview 和本地开发 origin；其它 origin（如恶意浏览器页面）会被拒绝
        let cors = CorsLayer::new()
            .allow_methods([Method::GET, Method::HEAD])
            .allow_headers([header::RANGE, header::ACCEPT, header::CONTENT_TYPE])
            .allow_origin(AllowOrigin::predicate(|origin: &HeaderValue, _req| {
                let s = origin.to_str().unwrap_or("");
                s == "tauri://localhost"
                    || s == "http://tauri.localhost"
                    || s == "https://tauri.localhost"
                    || s.starts_with("http://localhost:")
                    || s.starts_with("http://127.0.0.1:")
            }));

        let app = Router::new()
            .route("/serve", get(serve_file))
            .with_state(state)
            .layer(cors);

        // 绑定到 127.0.0.1 的随机可用端口
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let port = listener.local_addr()?.port();

        tracing::info!("Media server started on http://127.0.0.1:{}", port);

        tokio::spawn(async move {
            if let Err(e) = axum::serve(listener, app).await {
                tracing::error!("Media server error: {}", e);
            }
        });

        Ok(Self {
            port,
            allowed_prefixes,
        })
    }

    /// Get the port the server is listening on
    pub fn port(&self) -> u16 {
        self.port
    }

    /// 登记白名单前缀。只有位于白名单任一前缀下的文件才可通过 `/serve` 读取。
    ///
    /// 目录未创建时 `canonicalize` 可能失败，退回原始路径作为后备前缀，
    /// 等目录创建并再次登记后自然覆盖。
    pub fn allow_prefix(&self, path: impl AsRef<Path>) {
        let p = path.as_ref();
        let canonical = std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
        if let Ok(mut list) = self.allowed_prefixes.write() {
            if !list.iter().any(|existing| existing == &canonical) {
                tracing::info!("[media_server] allow prefix: {}", canonical.display());
                list.push(canonical);
            }
        }
    }

    /// 检查给定路径是否位于已登记的白名单前缀下。
    ///
    /// 路径尚未创建时（例如用户指定的输出目录尚未建立），会逐级上溯到存在且可
    /// canonicalize 的父目录再做前缀匹配，从而兼顾"新建子目录"场景。
    pub fn is_path_allowed(&self, path: impl AsRef<Path>) -> bool {
        let p = path.as_ref();
        let mut cursor = p.to_path_buf();
        let canonical = loop {
            if let Ok(c) = std::fs::canonicalize(&cursor) {
                break Some(c);
            }
            if !cursor.pop() {
                break None;
            }
        };
        let Some(canonical) = canonical else {
            return false;
        };
        match self.allowed_prefixes.read() {
            Ok(list) => list.iter().any(|prefix| canonical.starts_with(prefix)),
            Err(_) => false,
        }
    }

    /// Build a URL for serving a file. 调用方需确保该路径所在前缀已通过 `allow_prefix` 登记。
    pub fn file_url(&self, file_path: &str) -> String {
        format!(
            "http://127.0.0.1:{}/serve?path={}",
            self.port,
            urlencoding::encode(file_path)
        )
    }
}

/// Serve a local file with Range request support.
async fn serve_file(
    State(state): State<ServerState>,
    Query(params): Query<ServeParams>,
    req: axum::http::Request<axum::body::Body>,
) -> impl IntoResponse {
    let requested = PathBuf::from(&params.path);

    if !requested.exists() || !requested.is_file() {
        return (StatusCode::NOT_FOUND, "File not found").into_response();
    }

    // 白名单校验：对 canonical 路径做前缀匹配，防止 `..`、符号链接逃逸到敏感目录
    let canonical = match std::fs::canonicalize(&requested) {
        Ok(p) => p,
        Err(_) => return (StatusCode::NOT_FOUND, "File not found").into_response(),
    };
    let allowed = match state.allowed_prefixes.read() {
        Ok(list) => list.iter().any(|prefix| canonical.starts_with(prefix)),
        Err(_) => false,
    };
    if !allowed {
        tracing::warn!(
            "[media_server] 拒绝访问越界路径: {}",
            canonical.display()
        );
        return (StatusCode::FORBIDDEN, "Path not allowed").into_response();
    }

    // 根据扩展名确定 Content-Type
    let content_type = match canonical
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase()
        .as_str()
    {
        "mp4" | "m4v" => "video/mp4",
        "mkv" => "video/x-matroska",
        "webm" => "video/webm",
        "flv" => "video/x-flv",
        "ts" => "video/mp2t",
        "mov" => "video/quicktime",
        "avi" => "video/x-msvideo",
        "mp3" => "audio/mpeg",
        "m4a" | "aac" => "audio/mp4",
        _ => "application/octet-stream",
    };

    // 借助 tower-http 的 ServeFile 处理 Range / Content-Length / Content-Type
    let mime_type: mime::Mime = content_type
        .parse()
        .unwrap_or(mime::APPLICATION_OCTET_STREAM);
    let serve = tower_http::services::ServeFile::new_with_mime(&canonical, &mime_type);

    match tower::ServiceExt::oneshot(serve, req).await {
        Ok(response) => response.into_response(),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "Failed to serve file").into_response(),
    }
}
