//! Recording-session and captured-evidence domain contracts.

mod address;
mod frame;
mod gap;
mod retention;
mod session;

pub use address::{ByteOffset, FrameAddress};
pub use frame::{
    CaptureOrdinal, CaptureWarning, CapturedFrame, DeviceScaleFactor, EncodedFrame, ImageFormat,
    PixelDimensions,
};
pub use gap::{CaptureGap, CaptureGapReason};
pub use retention::{
    DEFAULT_ARTIFACT_GRACE, DEFAULT_RETENTION_MAX_AGE, DEFAULT_TRIM_HIGH_WATER_PERCENT, PinChange,
    RecordingBudgetState, RetainedPoint, RetentionLifecycle, RetentionRange, RetentionStatus,
    SessionDeletion, StorageUsage,
};
pub use session::{
    CaptureFailure, CaptureFailureStage, CaptureStatistics, CaptureStreamState,
    CaptureTimingSummary, DEFAULT_DISK_BUDGET_BYTES, DiskBudgetBytes, RecordingSession,
    TargetCaptureStatus,
};
