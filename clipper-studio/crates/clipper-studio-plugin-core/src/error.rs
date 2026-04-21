use thiserror::Error;

/// Unified plugin error type
#[derive(Debug, Error)]
pub enum PluginError {
    #[error("Plugin not found: {0}")]
    NotFound(String),

    #[error("Plugin error: {0}")]
    Plugin(String),

    #[error("Transport error: {0}")]
    Transport(String),

    #[error("Invalid payload: {0}")]
    InvalidPayload(String),

    #[error("Invalid response: {0}")]
    InvalidResponse(String),

    #[error("Unsupported action: {0}")]
    UnsupportedAction(String),

    #[error("Initialization failed: {0}")]
    Init(String),

    #[error("Shutdown failed: {0}")]
    Shutdown(String),
}

impl From<PluginError> for String {
    fn from(e: PluginError) -> String {
        e.to_string()
    }
}
