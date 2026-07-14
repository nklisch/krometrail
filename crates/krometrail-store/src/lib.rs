//! Recording persistence adapters.

pub mod index;
pub mod recovery;
pub mod segments;

mod recording;

pub use index::{IndexStoreConfig, SqliteIndex};
pub use recording::IndexedRecordingSink;
pub use recovery::{RecoveryReport, recover};
pub use segments::{
    FrameWriteCommit, RotationConfig, SegmentRegistration, SegmentState, SegmentStoreConfig,
    SegmentWriter,
};

use krometrail_core::{ErrorCode, KrometrailError, NonEmptyText};

fn persistence_error(message: impl Into<String>) -> KrometrailError {
    KrometrailError::new(
        ErrorCode::PersistenceFailed,
        NonEmptyText::new(message).expect("store errors must have a non-empty message"),
    )
}
