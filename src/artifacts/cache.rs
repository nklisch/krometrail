use krometrail_core::{
    ArtifactCacheKey, ArtifactCacheMetadata, ArtifactSourceFingerprint, EncodedFrame, FrameId,
    ImageFormat, NonEmptyText, SessionId, TEMPORAL_VIDEO_GENERATOR_NAME,
    TEMPORAL_VIDEO_GENERATOR_VERSION, TargetId, VideoSelectionIdentity,
};
use sha2::{Digest, Sha256};
use temporal_vision::{ArtifactKind, GeneratorDescriptor};

pub(crate) const CACHE_SCHEMA_VERSION: u32 = 1;
pub(crate) const RETAINED_VIDEO_ADAPTER_VERSION: &str = "krometrail-retained-video-v1";

/// Exact retained source identity and metadata used by both cache identity and store revalidation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct SourceFingerprint {
    pub frame_id: FrameId,
    pub capture_ordinal: u64,
    pub session_time_nanos: u64,
    pub format: ImageFormat,
    pub image_width: u32,
    pub image_height: u32,
    pub viewport_width: u32,
    pub viewport_height: u32,
    pub device_scale_bits: u64,
    pub encoded_sha256: [u8; 32],
}

impl SourceFingerprint {
    pub(crate) fn from_frame(frame: &EncodedFrame) -> Self {
        let metadata = frame.metadata();
        Self {
            frame_id: metadata.id(),
            capture_ordinal: metadata.capture_ordinal().get(),
            session_time_nanos: metadata.session_time().as_nanos(),
            format: metadata.format(),
            image_width: metadata.image().width(),
            image_height: metadata.image().height(),
            viewport_width: metadata.viewport().width(),
            viewport_height: metadata.viewport().height(),
            device_scale_bits: metadata.device_scale_factor().get().to_bits(),
            encoded_sha256: Sha256::digest(frame.bytes()).into(),
        }
    }

    pub(crate) fn store_fingerprint(&self) -> ArtifactSourceFingerprint {
        ArtifactSourceFingerprint {
            frame_id: self.frame_id,
            encoded_sha256: self.encoded_sha256,
        }
    }
}

pub(crate) struct CacheIdentityInput<'a> {
    pub session_id: SessionId,
    pub target_id: TargetId,
    pub sources: &'a [SourceFingerprint],
    pub artifact_kind: ArtifactKind,
    /// Canonical effective generator, normalization, marker, and gap transcript.
    pub canonical_parameters: &'a [u8],
    pub descriptor: GeneratorDescriptor,
    pub adapter_version: &'a str,
    pub decoder_profile: &'a str,
}

pub(crate) fn cache_metadata(input: CacheIdentityInput<'_>) -> ArtifactCacheMetadata {
    let source_fingerprint = hash_sources(input.sources, "krometrail-artifact-sources-v1");
    let visual_epoch_hash = hash_epoch(input.sources);
    let parameter_hash = framed_hash(
        "krometrail-artifact-parameters-v1",
        [input.canonical_parameters],
    );

    let mut transcript = FramedHasher::new("krometrail-artifact-cache-key");
    transcript.u32(CACHE_SCHEMA_VERSION);
    transcript.bytes(input.session_id.as_uuid().as_bytes());
    transcript.bytes(input.target_id.as_uuid().as_bytes());
    transcript.bytes(input.artifact_kind.as_str().as_bytes());
    transcript.bytes(input.descriptor.name.as_bytes());
    transcript.bytes(input.descriptor.version.as_bytes());
    transcript.bytes(input.adapter_version.as_bytes());
    transcript.bytes(input.decoder_profile.as_bytes());
    transcript.bytes(input.canonical_parameters);
    transcript.u64(input.sources.len() as u64);
    for source in input.sources {
        write_source(&mut transcript, source);
    }

    ArtifactCacheMetadata {
        cache_key: ArtifactCacheKey::from_bytes(transcript.finish()),
        source_fingerprint,
        parameter_hash,
        visual_epoch_hash,
        cache_schema_version: CACHE_SCHEMA_VERSION,
        adapter_version: NonEmptyText::new(format!(
            "{};{}",
            input.adapter_version, input.decoder_profile
        ))
        .expect("adapter and decoder versions are non-empty constants"),
        generator_name: NonEmptyText::new(input.descriptor.name)
            .expect("generator registry names are non-empty"),
        generator_version: NonEmptyText::new(input.descriptor.version)
            .expect("generator registry versions are non-empty"),
    }
}

pub(crate) struct VideoCacheIdentityInput<'a> {
    pub session_id: SessionId,
    pub target_id: TargetId,
    pub sources: &'a [SourceFingerprint],
    pub canonical_parameters: &'a [u8],
    pub selector: Option<&'a VideoSelectionIdentity>,
}

pub(crate) fn video_cache_metadata(input: VideoCacheIdentityInput<'_>) -> ArtifactCacheMetadata {
    video_cache_metadata_for_adapter(input, RETAINED_VIDEO_ADAPTER_VERSION)
}

fn video_cache_metadata_for_adapter(
    input: VideoCacheIdentityInput<'_>,
    adapter_version: &str,
) -> ArtifactCacheMetadata {
    let source_fingerprint = hash_sources(input.sources, "krometrail-video-sources-v1");
    let visual_epoch_hash = hash_epoch(input.sources);
    let parameter_hash = framed_hash(
        "krometrail-video-parameters-v1",
        [input.canonical_parameters],
    );
    let mut transcript = FramedHasher::new("krometrail-video-cache-key");
    transcript.u32(CACHE_SCHEMA_VERSION);
    transcript.bytes(input.session_id.as_uuid().as_bytes());
    transcript.bytes(input.target_id.as_uuid().as_bytes());
    transcript.bytes(TEMPORAL_VIDEO_GENERATOR_NAME.as_bytes());
    transcript.bytes(TEMPORAL_VIDEO_GENERATOR_VERSION.as_bytes());
    transcript.bytes(adapter_version.as_bytes());
    transcript.bytes(input.canonical_parameters);
    if let Some(selector) = input.selector {
        transcript.bytes(selector.name().as_bytes());
        transcript.bytes(selector.version().as_bytes());
        transcript.bytes(selector.parameters_sha256());
    } else {
        transcript.bytes(b"no-selection");
    }
    transcript.u64(input.sources.len() as u64);
    for source in input.sources {
        write_source(&mut transcript, source);
    }
    ArtifactCacheMetadata {
        cache_key: ArtifactCacheKey::from_bytes(transcript.finish()),
        source_fingerprint,
        parameter_hash,
        visual_epoch_hash,
        cache_schema_version: CACHE_SCHEMA_VERSION,
        adapter_version: NonEmptyText::new(adapter_version)
            .expect("video adapter version is non-empty"),
        generator_name: NonEmptyText::new(TEMPORAL_VIDEO_GENERATOR_NAME)
            .expect("static video generator name is non-empty"),
        generator_version: NonEmptyText::new(TEMPORAL_VIDEO_GENERATOR_VERSION)
            .expect("static video generator version is non-empty"),
    }
}

fn hash_sources(sources: &[SourceFingerprint], domain: &'static str) -> [u8; 32] {
    let mut transcript = FramedHasher::new(domain);
    transcript.u64(sources.len() as u64);
    for source in sources {
        write_source(&mut transcript, source);
    }
    transcript.finish()
}

fn hash_epoch(sources: &[SourceFingerprint]) -> [u8; 32] {
    let mut transcript = FramedHasher::new("krometrail-visual-epoch-v1");
    transcript.u64(sources.len() as u64);
    for source in sources {
        transcript.bytes(source.frame_id.as_uuid().as_bytes());
        transcript.u32(source.image_width);
        transcript.u32(source.image_height);
        transcript.u32(source.viewport_width);
        transcript.u32(source.viewport_height);
        transcript.u64(source.device_scale_bits);
    }
    transcript.finish()
}

fn write_source(transcript: &mut FramedHasher, source: &SourceFingerprint) {
    transcript.bytes(source.frame_id.as_uuid().as_bytes());
    transcript.u64(source.capture_ordinal);
    transcript.u64(source.session_time_nanos);
    transcript.bytes(match source.format {
        ImageFormat::Jpeg => b"jpeg",
        ImageFormat::Png => b"png",
    });
    transcript.u32(source.image_width);
    transcript.u32(source.image_height);
    transcript.u32(source.viewport_width);
    transcript.u32(source.viewport_height);
    transcript.u64(source.device_scale_bits);
    transcript.bytes(&source.encoded_sha256);
}

fn framed_hash<'a>(domain: &'static str, fields: impl IntoIterator<Item = &'a [u8]>) -> [u8; 32] {
    let mut transcript = FramedHasher::new(domain);
    for field in fields {
        transcript.bytes(field);
    }
    transcript.finish()
}

struct FramedHasher(Sha256);

impl FramedHasher {
    fn new(domain: &'static str) -> Self {
        let mut value = Self(Sha256::new());
        value.bytes(domain.as_bytes());
        value
    }

    fn bytes(&mut self, value: &[u8]) {
        self.0.update((value.len() as u64).to_be_bytes());
        self.0.update(value);
    }

    fn u32(&mut self, value: u32) {
        self.bytes(&value.to_be_bytes());
    }
    fn u64(&mut self, value: u64) {
        self.bytes(&value.to_be_bytes());
    }
    fn finish(self) -> [u8; 32] {
        self.0.finalize().into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use krometrail_core::{
        CaptureOrdinal, CapturedFrame, DeviceScaleFactor, ObservedTime, PixelDimensions,
        SessionTime,
    };
    use temporal_vision::generator_descriptor;

    fn source(id: u128) -> SourceFingerprint {
        SourceFingerprint {
            frame_id: FrameId::from_uuid(uuid::Uuid::from_u128(id)),
            capture_ordinal: id as u64,
            session_time_nanos: id as u64 * 10,
            format: ImageFormat::Jpeg,
            image_width: 4,
            image_height: 3,
            viewport_width: 4,
            viewport_height: 3,
            device_scale_bits: 1.0_f64.to_bits(),
            encoded_sha256: [id as u8; 32],
        }
    }

    fn key(
        sources: &[SourceFingerprint],
        kind: ArtifactKind,
        parameters: &[u8],
        adapter: &str,
    ) -> ArtifactCacheKey {
        cache_metadata(CacheIdentityInput {
            session_id: SessionId::from_uuid(uuid::Uuid::from_u128(100)),
            target_id: TargetId::from_uuid(uuid::Uuid::from_u128(101)),
            sources,
            artifact_kind: kind,
            canonical_parameters: parameters,
            descriptor: generator_descriptor(kind),
            adapter_version: adapter,
            decoder_profile: "decoder-v1",
        })
        .cache_key
    }

    fn video_key(
        sources: &[SourceFingerprint],
        parameters: &[u8],
        selector: Option<&VideoSelectionIdentity>,
    ) -> ArtifactCacheMetadata {
        video_cache_metadata(VideoCacheIdentityInput {
            session_id: SessionId::from_uuid(uuid::Uuid::from_u128(100)),
            target_id: TargetId::from_uuid(uuid::Uuid::from_u128(101)),
            sources,
            canonical_parameters: parameters,
            selector,
        })
    }

    #[test]
    fn exact_encoded_frame_becomes_the_store_and_cache_fingerprint() {
        let metadata = CapturedFrame::new(
            FrameId::from_uuid(uuid::Uuid::from_u128(1)),
            SessionId::from_uuid(uuid::Uuid::from_u128(2)),
            TargetId::from_uuid(uuid::Uuid::from_u128(3)),
            CaptureOrdinal::new(4).unwrap(),
            None,
            ObservedTime::from_nanos(6),
            SessionTime::from_nanos(5),
            ImageFormat::Png,
            PixelDimensions::new(2, 1).unwrap(),
            PixelDimensions::new(1, 1).unwrap(),
            DeviceScaleFactor::new(2.0).unwrap(),
            vec![],
        )
        .unwrap();
        let frame = EncodedFrame::new(metadata, b"exact encoded bytes".to_vec()).unwrap();
        let fingerprint = SourceFingerprint::from_frame(&frame);
        assert_eq!(fingerprint.frame_id, frame.metadata().id());
        assert_eq!(fingerprint.capture_ordinal, 4);
        assert_eq!(fingerprint.session_time_nanos, 5);
        assert_eq!(
            fingerprint.encoded_sha256,
            <[u8; 32]>::from(Sha256::digest(frame.bytes()))
        );
        assert_eq!(
            fingerprint.store_fingerprint().encoded_sha256,
            fingerprint.encoded_sha256
        );
    }

    #[test]
    fn cache_key_is_sensitive_to_every_source_field_and_order() {
        let base = vec![source(1), source(2)];
        let expected = key(&base, ArtifactKind::Storyboard, b"params", "adapter-v1");
        assert_eq!(
            expected.as_bytes(),
            &[
                46, 45, 66, 88, 219, 5, 46, 228, 36, 188, 163, 34, 154, 106, 77, 94, 90, 95, 39,
                234, 10, 237, 19, 171, 176, 63, 18, 227, 70, 227, 132, 11,
            ],
            "the stable image cache transcript changed"
        );
        let mut variants = Vec::new();
        let mut changed = base.clone();
        changed[0].frame_id = FrameId::from_uuid(uuid::Uuid::from_u128(9));
        variants.push(changed);
        let mut changed = base.clone();
        changed[0].capture_ordinal += 1;
        variants.push(changed);
        let mut changed = base.clone();
        changed[0].session_time_nanos += 1;
        variants.push(changed);
        let mut changed = base.clone();
        changed[0].format = ImageFormat::Png;
        variants.push(changed);
        let mut changed = base.clone();
        changed[0].image_width += 1;
        variants.push(changed);
        let mut changed = base.clone();
        changed[0].image_height += 1;
        variants.push(changed);
        let mut changed = base.clone();
        changed[0].viewport_width += 1;
        variants.push(changed);
        let mut changed = base.clone();
        changed[0].viewport_height += 1;
        variants.push(changed);
        let mut changed = base.clone();
        changed[0].device_scale_bits = 2.0_f64.to_bits();
        variants.push(changed);
        let mut changed = base.clone();
        changed[0].encoded_sha256[0] ^= 1;
        variants.push(changed);
        let mut changed = base.clone();
        changed.reverse();
        variants.push(changed);
        for changed in variants {
            assert_ne!(
                key(&changed, ArtifactKind::Storyboard, b"params", "adapter-v1"),
                expected
            );
        }
    }

    #[test]
    fn cache_key_binds_kind_parameters_markers_gaps_and_algorithm_versions() {
        let sources = vec![source(1)];
        let expected = key(
            &sources,
            ArtifactKind::Storyboard,
            b"normalization;marker=a;gap=g",
            "adapter-v1",
        );
        assert_ne!(
            key(
                &sources,
                ArtifactKind::BeforeDuringAfter,
                b"normalization;marker=a;gap=g",
                "adapter-v1"
            ),
            expected
        );
        assert_ne!(
            key(
                &sources,
                ArtifactKind::Storyboard,
                b"normalization;marker=b;gap=g",
                "adapter-v1"
            ),
            expected
        );
        assert_ne!(
            key(
                &sources,
                ArtifactKind::Storyboard,
                b"normalization;marker=a;gap=h",
                "adapter-v1"
            ),
            expected
        );
        assert_ne!(
            key(
                &sources,
                ArtifactKind::Storyboard,
                b"normalization;marker=a;gap=g",
                "adapter-v2"
            ),
            expected
        );

        let descriptor = GeneratorDescriptor {
            name: "temporal-storyboard",
            version: "future",
        };
        let changed = cache_metadata(CacheIdentityInput {
            session_id: SessionId::from_uuid(uuid::Uuid::from_u128(100)),
            target_id: TargetId::from_uuid(uuid::Uuid::from_u128(101)),
            sources: &sources,
            artifact_kind: ArtifactKind::Storyboard,
            canonical_parameters: b"normalization;marker=a;gap=g",
            descriptor,
            adapter_version: "adapter-v1",
            decoder_profile: "decoder-v1",
        })
        .cache_key;
        assert_ne!(changed, expected);
    }

    #[test]
    fn materialized_defaults_have_one_canonical_identity() {
        let sources = vec![source(1)];
        let materialized = b"scale=identity;background=0,0,0;max_bytes=67108864";
        assert_eq!(
            key(
                &sources,
                ArtifactKind::Storyboard,
                materialized,
                "adapter-v1"
            ),
            key(
                &sources,
                ArtifactKind::Storyboard,
                materialized,
                "adapter-v1"
            ),
        );
    }

    #[test]
    fn video_cache_identity_is_stable_sensitive_and_uses_the_video_generator() {
        let sources = vec![source(1), source(2)];
        let selector = VideoSelectionIdentity::meaningful_v1([7; 32]);
        let expected = video_key(&sources, b"canonical-plan-a", Some(&selector));
        assert_eq!(
            expected,
            video_key(&sources, b"canonical-plan-a", Some(&selector))
        );
        assert_eq!(
            expected.generator_name.as_str(),
            TEMPORAL_VIDEO_GENERATOR_NAME
        );
        assert_eq!(
            expected.generator_version.as_str(),
            TEMPORAL_VIDEO_GENERATOR_VERSION
        );

        let mut reordered = sources.clone();
        reordered.reverse();
        assert_ne!(
            expected.cache_key,
            video_key(&reordered, b"canonical-plan-a", Some(&selector)).cache_key
        );
        assert_ne!(
            expected.cache_key,
            video_key(&sources, b"canonical-plan-b", Some(&selector)).cache_key
        );
        assert_ne!(
            expected.cache_key,
            video_key(
                &sources,
                b"canonical-plan-a",
                Some(&VideoSelectionIdentity::meaningful_v1([8; 32]))
            )
            .cache_key
        );
        assert_ne!(
            expected.cache_key,
            video_key(&sources, b"canonical-plan-a", None).cache_key
        );
        let changed_adapter = video_cache_metadata_for_adapter(
            VideoCacheIdentityInput {
                session_id: SessionId::from_uuid(uuid::Uuid::from_u128(100)),
                target_id: TargetId::from_uuid(uuid::Uuid::from_u128(101)),
                sources: &sources,
                canonical_parameters: b"canonical-plan-a",
                selector: Some(&selector),
            },
            "krometrail-retained-video-v2",
        );
        assert_ne!(expected.cache_key, changed_adapter.cache_key);
        assert_eq!(
            expected.adapter_version.as_str(),
            RETAINED_VIDEO_ADAPTER_VERSION
        );
    }
}
