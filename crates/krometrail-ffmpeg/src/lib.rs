//! Qualified adapter for a user-installed FFmpeg executable.
//!
//! This crate never acquires or redistributes FFmpeg. Qualification and encoding use one
//! fixed direct-process policy, private request state, bounded output, and checked MP4/H.264
//! validation before bytes cross the core port boundary.

mod error;
mod job;
mod mp4;
mod policy;
mod process;

pub use policy::{
    FFMPEG_ARGUMENT_POLICY_VERSION, FFMPEG_ENCODER_ALLOWLIST, FFMPEG_QUALIFICATION_TIMEOUT,
    FFMPEG_TERMINATION_GRACE, MAX_FFMPEG_DISCOVERY_CANDIDATES, MAX_FFMPEG_STDERR_BYTES,
    MAX_FFMPEG_VERSION_REPORT_BYTES,
};

pub const FFMPEG_ADAPTER_VERSION: &str = env!("CARGO_PKG_VERSION");
