//! Recording persistence adapters.

pub(crate) mod artifacts;
pub mod budget_registry;
pub mod index;
pub mod instance;
mod permissions;
pub mod recovery;
pub mod segments;

mod recording;
mod retention;

pub use budget_registry::{BudgetRegistry, BudgetShare};
pub use index::{IndexStoreConfig, SqliteIndex};
pub use instance::{
    InstanceOwnership, clear_legacy_flat_store, has_legacy_flat_store, reclaim_instance_root,
    sibling_instance_roots,
};
pub use recording::RecordingStore;
pub use recovery::{RecoveryReport, recover};
pub use segments::{
    FrameWriteCommit, RotationConfig, SegmentRegistration, SegmentState, SegmentStoreConfig,
    SegmentWriter,
};

#[cfg(feature = "qualification-support")]
pub mod qualification_support {
    use krometrail_core::{ArtifactId, Result};

    use crate::RecordingStore;

    /// Injects a corrupt ready payload plus a staged temporary payload through the store's
    /// private artifact boundary. Recovery qualification uses this only to exercise the existing
    /// startup authority; no path or file layout escapes the store crate.
    pub fn inject_corrupt_ready_artifact(store: &RecordingStore, id: ArtifactId) -> Result<()> {
        store.qualification_inject_corrupt_ready_artifact(id)
    }

    /// Reports whether recovery removed both the corrupt publication and its staged temporary.
    pub fn artifact_recovery_files_absent(store: &RecordingStore, id: ArtifactId) -> Result<bool> {
        store.qualification_artifact_recovery_files_absent(id)
    }
}

use krometrail_core::{ErrorCode, KrometrailError, NonEmptyText};

fn persistence_error(message: impl Into<String>) -> KrometrailError {
    KrometrailError::new(
        ErrorCode::PersistenceFailed,
        NonEmptyText::new(message).expect("store errors must have a non-empty message"),
    )
}
