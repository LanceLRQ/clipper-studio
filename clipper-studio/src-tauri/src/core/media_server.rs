use axum::{
    extract::Query,
    http::StatusCode,
    response::IntoResponse,
    routing::get,
    Router,
};
use serde::Deserialize;
use std::path::PathBuf;
use tokio::net::TcpListener;
use tower_http::cors::CorsLayer;

/// Local HTTP server for serving media files with Range request support.
///
/// This is necessary because:
/// 1. Tauri's asset protocol doesn't reliably support Range requests
/// 2. FLV files need FFmpeg remux before browser playback
/// 3. A proper HTTP server enables seeking in large files
pub struct MediaServer {
    port: u16,
}

#[derive(Debug, Deserialize)]
struct ServeParams {
    path: String,
}

impl MediaServer {
    /// Start the media server on a random available port.
    /// Returns the server instance (with port info).
    pub async fn start() -> Result<Self, Box<dyn std::error::Error>> {
        let app = Router::new()
            .route("/serve", get(serve_file))
            .layer(CorsLayer::permissive());

        // Bind to random available port on localhost
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let port = listener.local_addr()?.port();

        tracing::info!("Media server started on http://127.0.0.1:{}", port);

        // Spawn server in background
        tokio::spawn(async move {
            if let Err(e) = axum::serve(listener, app).await {
                tracing::error!("Media server error: {}", e);
            }
        });

        Ok(Self { port })
    }

    /// Get the port the server is listening on
    pub fn port(&self) -> u16 {
        self.port
    }

    /// Build a URL for serving a file
    pub fn file_url(&self, file_path: &str) -> String {
        format!(
            "http://127.0.0.1:{}/serve?path={}",
            self.port,
            urlencoding::encode(file_path)
        )
    }
}

/// Serve a local file with Range request support.
/// Uses tower-http's ServeFile internally for correct Range/Content-Type handling.
async fn serve_file(
    Query(params): Query<ServeParams>,
    req: axum::http::Request<axum::body::Body>,
) -> impl IntoResponse {
    let path = PathBuf::from(&params.path);

    if !path.exists() || !path.is_file() {
        return (StatusCode::NOT_FOUND, "File not found").into_response();
    }

    // Determine content type from extension
    let content_type = match path
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

    // Use tower-http ServeFile for automatic Range support
    let mime_type: mime::Mime = content_type
        .parse()
        .unwrap_or(mime::APPLICATION_OCTET_STREAM);
    let serve = tower_http::services::ServeFile::new_with_mime(&path, &mime_type);

    match tower::ServiceExt::oneshot(serve, req).await {
        Ok(response) => response.into_response(),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "Failed to serve file").into_response(),
    }
}
