use std::sync::Arc;

use sha2::{Digest, Sha256};

use crate::{
    ArtifactId, ArtifactManifest, ArtifactRead, CancellationSignal, FrameId, NonEmptyText,
    PortFuture, Result, RetrieveArtifactRequest, SessionId, TargetId, TemporalVideoManifest,
    VideoArtifactRead,
    error::{ErrorCode, KrometrailError},
};

pub const TEMPORAL_VIDEO_GENERATOR_NAME: &str = "temporal_video";
pub const TEMPORAL_VIDEO_GENERATOR_VERSION: &str = "retained-generation-v1";

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

/// Scoped by-id read outcome with derived corruption distinct from ordinary expiry.
#[derive(Clone, Debug, PartialEq)]
pub enum ArtifactReadLookup {
    Missing,
    Available(Box<ArtifactRead>),
    Invalidated,
}

#[derive(Clone)]
pub struct VideoArtifactPublication {
    pub session_id: SessionId,
    pub target_id: TargetId,
    pub sources: Vec<ArtifactSourceFingerprint>,
    pub cache: ArtifactCacheMetadata,
    pub manifest: TemporalVideoManifest,
    pub encoded_bytes: Arc<[u8]>,
    cancellation: Option<Arc<dyn CancellationSignal>>,
}

impl VideoArtifactPublication {
    pub fn new(
        session_id: SessionId,
        target_id: TargetId,
        sources: Vec<ArtifactSourceFingerprint>,
        cache: ArtifactCacheMetadata,
        manifest: TemporalVideoManifest,
        encoded_bytes: impl Into<Arc<[u8]>>,
    ) -> Result<Self> {
        let encoded_bytes = encoded_bytes.into();
        if session_id != manifest.session_id()
            || target_id != manifest.target_id()
            || sources.is_empty()
            || sources
                .iter()
                .map(|source| source.frame_id)
                .collect::<Vec<_>>()
                != manifest.plan().input_frame_ids()
            || manifest.media_type() != "video/mp4"
            || encoded_bytes.len() as u64 != manifest.encoded_byte_len()
            || <[u8; 32]>::from(Sha256::digest(encoded_bytes.as_ref()))
                != *manifest.output_hash().as_bytes()
            || cache.generator_name.as_str() != TEMPORAL_VIDEO_GENERATOR_NAME
            || cache.generator_version.as_str() != TEMPORAL_VIDEO_GENERATOR_VERSION
        {
            return Err(invalid_publication(
                "video publication must exactly match its scope, sources, bytes, manifest, and generator identity",
            ));
        }
        Ok(Self {
            session_id,
            target_id,
            sources,
            cache,
            manifest,
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
pub struct StoredVideoArtifact {
    pub cache: ArtifactCacheMetadata,
    pub manifest: TemporalVideoManifest,
    pub encoded_bytes: Arc<[u8]>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum VideoArtifactLookup {
    Miss,
    Hit(Box<StoredVideoArtifact>),
    Invalidated,
}

#[derive(Clone, Debug, PartialEq)]
pub enum VideoArtifactPublish {
    Published(StoredVideoArtifact),
    Existing(StoredVideoArtifact),
}

#[derive(Clone, Debug, PartialEq)]
pub enum VideoArtifactReadLookup {
    Missing,
    Available(Box<VideoArtifactRead>),
    Invalidated,
}

/// Persistence/cache authority for generated artifacts. Physical paths remain private.
pub trait ArtifactStore: Send + Sync {
    /// Reads one exact scoped artifact under the caller's byte ceiling.
    fn read_artifact(
        &self,
        _request: RetrieveArtifactRequest,
    ) -> PortFuture<'_, Result<ArtifactReadLookup>> {
        Box::pin(std::future::ready(Err(KrometrailError::new(
            ErrorCode::Unsupported,
            NonEmptyText::new("this artifact store does not provide coherent scoped reads")
                .expect("static artifact read error is non-empty"),
        ))))
    }

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

    fn read_video_artifact(
        &self,
        _request: RetrieveArtifactRequest,
    ) -> PortFuture<'_, Result<VideoArtifactReadLookup>> {
        Box::pin(std::future::ready(Err(KrometrailError::new(
            ErrorCode::Unsupported,
            NonEmptyText::new("this artifact store does not provide coherent video reads")
                .expect("static video read error is non-empty"),
        ))))
    }

    fn lookup_video_artifact(
        &self,
        _key: ArtifactCacheKey,
        _expected_sources: Vec<ArtifactSourceFingerprint>,
    ) -> PortFuture<'_, Result<VideoArtifactLookup>> {
        Box::pin(std::future::ready(Err(video_unsupported())))
    }

    fn publish_video_artifact(
        &self,
        _publication: VideoArtifactPublication,
    ) -> PortFuture<'_, Result<VideoArtifactPublish>> {
        Box::pin(std::future::ready(Err(video_unsupported())))
    }

    fn video_artifact(
        &self,
        _artifact_id: ArtifactId,
    ) -> PortFuture<'_, Result<Option<StoredVideoArtifact>>> {
        Box::pin(std::future::ready(Err(video_unsupported())))
    }
}

impl<T: ArtifactStore + ?Sized> ArtifactStore for Arc<T> {
    fn read_artifact(
        &self,
        request: RetrieveArtifactRequest,
    ) -> PortFuture<'_, Result<ArtifactReadLookup>> {
        (**self).read_artifact(request)
    }

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

    fn read_video_artifact(
        &self,
        request: RetrieveArtifactRequest,
    ) -> PortFuture<'_, Result<VideoArtifactReadLookup>> {
        (**self).read_video_artifact(request)
    }

    fn lookup_video_artifact(
        &self,
        key: ArtifactCacheKey,
        expected_sources: Vec<ArtifactSourceFingerprint>,
    ) -> PortFuture<'_, Result<VideoArtifactLookup>> {
        (**self).lookup_video_artifact(key, expected_sources)
    }

    fn publish_video_artifact(
        &self,
        publication: VideoArtifactPublication,
    ) -> PortFuture<'_, Result<VideoArtifactPublish>> {
        (**self).publish_video_artifact(publication)
    }

    fn video_artifact(
        &self,
        artifact_id: ArtifactId,
    ) -> PortFuture<'_, Result<Option<StoredVideoArtifact>>> {
        (**self).video_artifact(artifact_id)
    }
}

fn video_unsupported() -> KrometrailError {
    KrometrailError::new(
        ErrorCode::Unsupported,
        NonEmptyText::new("this artifact store does not retain temporal video")
            .expect("static unsupported error is non-empty"),
    )
}

fn invalid_publication(message: &'static str) -> KrometrailError {
    KrometrailError::new(
        ErrorCode::InvalidInput,
        NonEmptyText::new(message).expect("artifact publication errors are non-empty"),
    )
}
