pub(crate) mod files;
pub(crate) mod recovery;

use std::{
    collections::{BTreeSet, HashMap},
    sync::{
        Arc, Mutex as StdMutex, Weak,
        atomic::{AtomicBool, Ordering},
    },
};

use krometrail_core::{
    ArtifactCacheKey, ArtifactManifest, ArtifactSourceFingerprint, ErrorCode, KrometrailError,
    NonEmptyText, SessionId, StoredArtifact, StoredVideoArtifact, TemporalVideoManifest,
};
use sha2::{Digest, Sha256};
use tokio::sync::{Mutex, Notify};

use crate::{
    index::artifacts::{ArtifactRow, ArtifactSourceRow, ArtifactState, RetainedArtifactKind},
    persistence_error,
};

pub(crate) struct PublicationRegistry {
    state: Arc<StdMutex<PublicationRegistryState>>,
    notify: Arc<Notify>,
}

#[derive(Default)]
struct PublicationRegistryState {
    deleted: BTreeSet<SessionId>,
    active: HashMap<SessionId, Vec<Arc<AtomicBool>>>,
}

pub(crate) struct PublicationGuard {
    session_id: SessionId,
    cancellation: Arc<AtomicBool>,
    state: Arc<StdMutex<PublicationRegistryState>>,
    notify: Arc<Notify>,
}

impl PublicationRegistry {
    pub(crate) fn new() -> Self {
        Self {
            state: Arc::new(StdMutex::new(PublicationRegistryState::default())),
            notify: Arc::new(Notify::new()),
        }
    }

    pub(crate) fn begin(&self, session_id: SessionId) -> krometrail_core::Result<PublicationGuard> {
        let cancellation = Arc::new(AtomicBool::new(false));
        let mut state = self
            .state
            .lock()
            .expect("artifact publication registry poisoned");
        if state.deleted.contains(&session_id) {
            return Err(deleted_error(session_id));
        }
        state
            .active
            .entry(session_id)
            .or_default()
            .push(Arc::clone(&cancellation));
        drop(state);
        Ok(PublicationGuard {
            session_id,
            cancellation,
            state: Arc::clone(&self.state),
            notify: Arc::clone(&self.notify),
        })
    }

    pub(crate) fn is_deleted(&self, session_id: SessionId) -> bool {
        self.state
            .lock()
            .expect("artifact publication registry poisoned")
            .deleted
            .contains(&session_id)
    }

    pub(crate) fn mark_deleted(&self, session_id: SessionId) {
        let mut state = self
            .state
            .lock()
            .expect("artifact publication registry poisoned");
        state.deleted.insert(session_id);
        if let Some(active) = state.active.get(&session_id) {
            for cancellation in active {
                cancellation.store(true, Ordering::Release);
            }
        }
    }

    pub(crate) async fn drain(&self, session_id: SessionId) {
        loop {
            let notified = self.notify.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            let empty = self
                .state
                .lock()
                .expect("artifact publication registry poisoned")
                .active
                .get(&session_id)
                .is_none_or(Vec::is_empty);
            if empty {
                return;
            }
            notified.await;
        }
    }
}

impl PublicationGuard {
    pub(crate) fn cancellation(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.cancellation)
    }
    pub(crate) fn is_cancelled(&self) -> bool {
        self.cancellation.load(Ordering::Acquire)
    }
}

impl Drop for PublicationGuard {
    fn drop(&mut self) {
        let mut state = self
            .state
            .lock()
            .expect("artifact publication registry poisoned");
        if let Some(active) = state.active.get_mut(&self.session_id) {
            active.retain(|token| !Arc::ptr_eq(token, &self.cancellation));
            if active.is_empty() {
                state.active.remove(&self.session_id);
            }
        }
        drop(state);
        self.notify.notify_waiters();
    }
}

pub(crate) struct CacheLocks {
    locks: StdMutex<HashMap<ArtifactCacheKey, Weak<Mutex<()>>>>,
}

impl CacheLocks {
    pub(crate) fn new() -> Self {
        Self {
            locks: StdMutex::new(HashMap::new()),
        }
    }

    pub(crate) fn for_key(&self, key: ArtifactCacheKey) -> Arc<Mutex<()>> {
        let mut locks = self
            .locks
            .lock()
            .expect("artifact cache lock registry poisoned");
        locks.retain(|_, lock| lock.strong_count() > 0);
        locks.entry(key).or_default().upgrade().unwrap_or_else(|| {
            let lock = Arc::new(Mutex::new(()));
            locks.insert(key, Arc::downgrade(&lock));
            lock
        })
    }
}

pub(crate) enum RetainedStoredArtifact {
    Image(Box<StoredArtifact>),
    Video(Box<StoredVideoArtifact>),
}

pub(crate) fn validate_stored_artifact(
    row: &ArtifactRow,
    sources: &[ArtifactSourceRow],
    bytes: Arc<[u8]>,
    expected_sources: Option<&[ArtifactSourceFingerprint]>,
) -> krometrail_core::Result<RetainedStoredArtifact> {
    if row.state != ArtifactState::Ready || u64::try_from(bytes.len()).ok() != Some(row.byte_len) {
        return Err(corrupt_error());
    }
    let manifest_hash: [u8; 32] = Sha256::digest(row.manifest_json.as_bytes()).into();
    let output_hash: [u8; 32] = Sha256::digest(bytes.as_ref()).into();
    if manifest_hash != row.manifest_hash || output_hash != row.output_hash {
        return Err(corrupt_error());
    }
    if sources
        .iter()
        .enumerate()
        .any(|(position, source)| source.source_position != position)
    {
        return Err(corrupt_error());
    }
    if let Some(expected) = expected_sources {
        if expected.len() != sources.len()
            || expected.iter().zip(sources).any(|(expected, stored)| {
                expected.frame_id != stored.frame_id
                    || expected.encoded_sha256 != stored.encoded_hash
            })
        {
            return Err(corrupt_error());
        }
    }
    match row.kind {
        RetainedArtifactKind::Image(kind) => {
            if row.media_type.as_str() != "image/png" || !bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
                return Err(corrupt_error());
            }
            let manifest: ArtifactManifest =
                serde_json::from_str(&row.manifest_json).map_err(|_| corrupt_error())?;
            if manifest.artifact_id() != &row.artifact_id
                || manifest.artifact_kind() != kind
                || manifest.range().start().as_nanos() != row.start_time_nanos
                || manifest.range().end().as_nanos() != row.end_time_nanos
                || manifest.output_hash().as_bytes() != &row.output_hash
                || manifest.algorithm().name() != row.cache.generator_name.as_str()
                || manifest.algorithm().version() != row.cache.generator_version.as_str()
                || sources
                    .iter()
                    .map(|source| source.frame_id)
                    .collect::<Vec<_>>()
                    != manifest.source_frame_ids()
            {
                return Err(corrupt_error());
            }
            Ok(RetainedStoredArtifact::Image(Box::new(StoredArtifact {
                cache: row.cache.clone(),
                manifest,
                media_type: row.media_type.clone(),
                encoded_bytes: bytes,
            })))
        }
        RetainedArtifactKind::TemporalVideo => {
            if row.media_type.as_str() != "video/mp4" {
                return Err(corrupt_error());
            }
            let manifest: TemporalVideoManifest =
                serde_json::from_str(&row.manifest_json).map_err(|_| corrupt_error())?;
            if manifest.artifact_id() != row.artifact_id
                || manifest.resolved_range().start().as_nanos() != row.start_time_nanos
                || manifest.resolved_range().end().as_nanos() != row.end_time_nanos
                || manifest.output_hash().as_bytes() != &row.output_hash
                || row.cache.generator_name.as_str()
                    != krometrail_core::TEMPORAL_VIDEO_GENERATOR_NAME
                || row.cache.generator_version.as_str()
                    != krometrail_core::TEMPORAL_VIDEO_GENERATOR_VERSION
                || sources
                    .iter()
                    .map(|source| source.frame_id)
                    .collect::<Vec<_>>()
                    != manifest.plan().input_frame_ids()
            {
                return Err(corrupt_error());
            }
            Ok(RetainedStoredArtifact::Video(Box::new(
                StoredVideoArtifact {
                    cache: row.cache.clone(),
                    manifest,
                    encoded_bytes: bytes,
                },
            )))
        }
    }
}

pub(crate) fn source_fingerprints(sources: &[ArtifactSourceRow]) -> Vec<ArtifactSourceFingerprint> {
    sources
        .iter()
        .map(|source| ArtifactSourceFingerprint {
            frame_id: source.frame_id,
            encoded_sha256: source.encoded_hash,
        })
        .collect()
}

fn corrupt_error() -> KrometrailError {
    persistence_error("stored artifact failed provenance or byte validation")
}

fn deleted_error(session_id: SessionId) -> KrometrailError {
    KrometrailError::new(
        ErrorCode::NotFound,
        NonEmptyText::new("recording session has been deleted")
            .expect("static deletion error is non-empty"),
    )
    .with_context(krometrail_core::ErrorContext {
        session_id: Some(session_id),
        ..Default::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn publication_drop_after_notification_registration_is_not_lost() {
        let registry = PublicationRegistry::new();
        let session_id = SessionId::from_uuid(Uuid::from_u128(1));
        let guard = registry.begin(session_id).unwrap();
        let notified = registry.notify.notified();
        tokio::pin!(notified);
        notified.as_mut().enable();

        drop(guard);

        tokio::time::timeout(std::time::Duration::from_millis(100), notified)
            .await
            .expect("registered publication notification");
        registry.drain(session_id).await;
    }
}
