use clap::{Parser, Subcommand};

/// The intentionally small command surface while browser transport is being built.
#[derive(Debug, Parser)]
#[command(
    name = "krometrail",
    version,
    about = "Rust browser capture runtime (browser transport is not yet available)"
)]
pub(crate) struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Debug, Subcommand)]
pub(crate) enum Command {
    /// Check the runtime's browser integration availability.
    Doctor,
}
