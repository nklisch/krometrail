//! Qualified adapter for a user-installed FFmpeg executable.
//!
//! This crate never acquires or redistributes FFmpeg. Qualification and encoding use one
//! fixed direct-process policy, private request state, bounded output, and checked MP4/H.264
//! validation before bytes cross the core port boundary.

mod control;
mod discovery;
mod encoder;
mod error;
mod job;
mod mp4;
mod policy;
mod process;
mod qualification;

pub use discovery::FfmpegDiscoveryOptions;
pub use encoder::QualifiedFfmpegEncoder;
pub use mp4::{Mp4Check, Mp4Property, OutputValidationDetail};
pub use qualification::{
    FfmpegQualification, FfmpegQualificationStage, FfmpegUnavailable, FfmpegUnavailableReason,
    qualify_ffmpeg,
};

pub use policy::{
    FFMPEG_ARGUMENT_POLICY_VERSION, FFMPEG_ENCODER_ALLOWLIST, FFMPEG_QUALIFICATION_TIMEOUT,
    FFMPEG_TERMINATION_GRACE, MAX_FFMPEG_DISCOVERY_CANDIDATES, MAX_FFMPEG_STDERR_BYTES,
    MAX_FFMPEG_VERSION_REPORT_BYTES,
};

pub const FFMPEG_ADAPTER_VERSION: &str = env!("CARGO_PKG_VERSION");
