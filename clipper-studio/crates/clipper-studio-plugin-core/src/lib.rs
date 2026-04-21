//! ClipperStudio Plugin Core
//!
//! Core types and traits for the plugin system.
//!
//! # Plugin Types
//!
//! - **Builtin**: Compiled into the main application, implements [`PluginInstance`]
//! - **External HTTP**: Runs as an external process, accessed via HTTP
//! - **External Stdio**: Runs as an external tool, accessed via stdio

pub mod builtin;
pub mod error;
pub mod manifest;
pub mod transport;

pub use builtin::*;
pub use error::PluginError;
pub use manifest::*;
pub use transport::*;
