//! Model Context Protocol adapter for Krometrail browser control.

mod catalogue;
mod config;
mod protocol;
mod registry;
mod request_lifecycle;
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

mod stdio;
