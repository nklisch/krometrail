use crate::{ArtifactStore, FrameSource, RetentionStore};

/// One coherent authority for progressive reads, artifact lookup, and retention.
///
/// The marker deliberately adds no forwarding methods: each existing port remains
/// the sole contract for its operation family.
pub trait ProgressiveEvidenceStore: FrameSource + ArtifactStore + RetentionStore {}

impl<T> ProgressiveEvidenceStore for T where T: FrameSource + ArtifactStore + RetentionStore + ?Sized
{}
