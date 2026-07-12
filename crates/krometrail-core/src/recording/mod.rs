//! Recording-session and captured-evidence domain contracts.

mod frame;
mod gap;
mod session;

pub use frame::{
    CaptureWarning, CapturedFrame, DeviceScaleFactor, EncodedFrame, ImageFormat, PixelDimensions,
};
pub use gap::{CaptureGap, CaptureGapReason};
pub use session::{CaptureStatistics, DiskBudgetBytes, RecordingSession};
