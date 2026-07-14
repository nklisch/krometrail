use clap::{Parser, Subcommand};

/// Local browser capture and agent-control runtime.
#[derive(Debug, Parser)]
#[command(name = "krometrail", version, about = "Rust browser capture runtime")]
pub(crate) struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Debug, Subcommand)]
pub(crate) enum Command {
    /// Check the runtime's browser integration availability.
    Doctor,
    /// Serve browser-control tools over MCP standard input and output.
    Mcp,
}
