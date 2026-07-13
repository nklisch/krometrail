//! Recording persistence adapters.

pub mod segments;

pub use segments::{RotationConfig, SegmentStoreConfig, SegmentWriter};

use krometrail_core::{ErrorCode, KrometrailError, NonEmptyText};

fn persistence_error(message: impl Into<String>) -> KrometrailError {
    KrometrailError::new(
        ErrorCode::PersistenceFailed,
        NonEmptyText::new(message).expect("store errors must have a non-empty message"),
    )
}
