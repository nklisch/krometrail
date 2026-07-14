//! Model Context Protocol adapter for Krometrail browser control.

mod config;
mod registry;
mod response;
mod schema;
mod server;
mod session;

pub use config::McpConfig;
pub use server::{McpService, build_service};
pub use session::BrowserSessionOwner;
