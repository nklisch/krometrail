use std::sync::Arc;

use sha2::{Digest, Sha256};

use crate::{
    ArtifactId, ArtifactManifest, CancellationSignal, FrameId, NonEmptyText, PortFuture, Result,
    SessionId, TargetId,
    error::{ErrorCode, KrometrailError},
};

/// Stable SHA-256 cache identity. Construction is owned by the computation adapter.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ArtifactCacheKey([u8; 32]);

impl ArtifactCacheKey {
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArtifactSourceFingerprint {
    pub frame_id: FrameId,
    pub encoded_sha256: [u8; 32],
}

#[derive(Clone, Debug, PartialEq)]
pub struct ArtifactCacheMetadata {
    pub cache_key: ArtifactCacheKey,
    pub source_fingerprint: [u8; 32],
    pub parameter_hash: [u8; 32],
    pub visual_epoch_hash: [u8; 32],
    pub cache_schema_version: u32,
    pub adapter_version: NonEmptyText,
    pub generator_name: NonEmptyText,
    pub generator_version: NonEmptyText,
}

#[derive(Clone)]
pub struct ArtifactPublication {
    pub session_id: SessionId,
    pub target_id: TargetId,
    pub sources: Vec<ArtifactSourceFingerprint>,
    pub cache: ArtifactCacheMetadata,
    pub manifest: ArtifactManifest,
    pub media_type: NonEmptyText,
    pub encoded_bytes: Arc<[u8]>,
    cancellation: Option<Arc<dyn CancellationSignal>>,
}

impl ArtifactPublication {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        session_id: SessionId,
        target_id: TargetId,
        sources: Vec<ArtifactSourceFingerprint>,
        cache: ArtifactCacheMetadata,
        manifest: ArtifactManifest,
        media_type: NonEmptyText,
        encoded_bytes: impl Into<Arc<[u8]>>,
    ) -> Result<Self> {
        let encoded_bytes = encoded_bytes.into();
        if sources.is_empty() {
            return Err(invalid_publication(
                "artifact publication must retain source frames",
            ));
        }
        if sources
            .iter()
            .map(|source| source.frame_id)
            .collect::<Vec<_>>()
            != manifest.source_frame_ids()
        {
            return Err(invalid_publication(
                "artifact publication source order must match its manifest",
            ));
        }
        if media_type.as_str() != "image/png" || !encoded_bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
            return Err(invalid_publication(
                "artifact publication must contain declared PNG bytes",
            ));
        }
        let output_hash: [u8; 32] = Sha256::digest(encoded_bytes.as_ref()).into();
        if output_hash != *manifest.output_hash().as_bytes() {
            return Err(invalid_publication(
                "artifact publication bytes do not match the manifest output hash",
            ));
        }
        if manifest.algorithm().name() != cache.generator_name.as_str()
            || manifest.algorithm().version() != cache.generator_version.as_str()
        {
            return Err(invalid_publication(
                "artifact publication generator metadata contradicts its manifest",
            ));
        }
        Ok(Self {
            session_id,
            target_id,
            sources,
            cache,
            manifest,
            media_type,
            encoded_bytes,
            cancellation: None,
        })
    }

    pub fn with_cancellation(mut self, cancellation: Arc<dyn CancellationSignal>) -> Self {
        self.cancellation = Some(cancellation);
        self
    }

    pub fn cancellation(&self) -> Option<&Arc<dyn CancellationSignal>> {
        self.cancellation.as_ref()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct StoredArtifact {
    pub cache: ArtifactCacheMetadata,
    pub manifest: ArtifactManifest,
    pub media_type: NonEmptyText,
    pub encoded_bytes: Arc<[u8]>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ArtifactLookup {
    Miss,
    Hit(Box<StoredArtifact>),
    Invalidated,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ArtifactPublish {
    Published(StoredArtifact),
    Existing(StoredArtifact),
}

/// Persistence/cache authority for generated artifacts. Physical paths remain private.
pub trait ArtifactStore: Send + Sync {
    fn lookup_artifact(
        &self,
        key: ArtifactCacheKey,
        expected_sources: Vec<ArtifactSourceFingerprint>,
    ) -> PortFuture<'_, Result<ArtifactLookup>>;

    fn publish_artifact(
        &self,
        publication: ArtifactPublication,
    ) -> PortFuture<'_, Result<ArtifactPublish>>;

    fn artifact(&self, artifact_id: ArtifactId) -> PortFuture<'_, Result<Option<StoredArtifact>>>;
}

impl<T: ArtifactStore + ?Sized> ArtifactStore for Arc<T> {
    fn lookup_artifact(
        &self,
        key: ArtifactCacheKey,
        expected_sources: Vec<ArtifactSourceFingerprint>,
    ) -> PortFuture<'_, Result<ArtifactLookup>> {
        (**self).lookup_artifact(key, expected_sources)
    }

    fn publish_artifact(
        &self,
        publication: ArtifactPublication,
    ) -> PortFuture<'_, Result<ArtifactPublish>> {
        (**self).publish_artifact(publication)
    }

    fn artifact(&self, artifact_id: ArtifactId) -> PortFuture<'_, Result<Option<StoredArtifact>>> {
        (**self).artifact(artifact_id)
    }
}

fn invalid_publication(message: &'static str) -> KrometrailError {
    KrometrailError::new(
        ErrorCode::InvalidInput,
        NonEmptyText::new(message).expect("artifact publication errors are non-empty"),
    )
}
