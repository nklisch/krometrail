//! Recording-session and captured-evidence domain contracts.

mod frame;
mod gap;
mod session;

pub use frame::{
    CaptureOrdinal, CaptureWarning, CapturedFrame, DeviceScaleFactor, EncodedFrame, ImageFormat,
    PixelDimensions,
};
pub use gap::{CaptureGap, CaptureGapReason};
pub use session::{
    CaptureStatistics, CaptureStreamState, CaptureTimingSummary, DiskBudgetBytes, RecordingSession,
    TargetCaptureStatus,
};
