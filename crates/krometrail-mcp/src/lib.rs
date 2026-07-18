//! Model Context Protocol adapter for Krometrail browser control.

mod config;
mod registry;
mod resources;
mod response;
mod schema;
mod server;
mod session;
#[cfg(test)]
mod test_fixture;

pub use config::{DiagnosticContext, McpConfig, McpDependencies};
pub use server::{McpService, build_service};
pub use session::BrowserSessionOwner;
