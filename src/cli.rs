use clap::{Parser, Subcommand};

/// The intentionally small command surface while the browser-control commands are assembled.
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
}
