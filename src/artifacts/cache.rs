use std::sync::Arc;

use krometrail_core::{
    ArtifactCacheKey, ArtifactCacheMetadata, ArtifactSourceFingerprint, EncodedFrame, FrameId,
    ImageFormat, NonEmptyText, SessionId, TargetId,
};
use sha2::{Digest, Sha256};
use temporal_vision::{ArtifactKind, GeneratorDescriptor};

pub(crate) const CACHE_SCHEMA_VERSION: u32 = 1;

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

/// Complete identity for one decoded frame within one visual epoch.
// This key is intentionally staged ahead of service wiring in the next child story.
#[allow(dead_code)]
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) struct DecodedFrameKey {
    pub session_id: SessionId,
    pub target_id: TargetId,
    pub frame_id: FrameId,
    pub capture_ordinal: u64,
    pub session_time_nanos: u64,
    pub source_format: ImageFormat,
    pub image_dimensions: krometrail_core::PixelDimensions,
    pub viewport_dimensions: krometrail_core::PixelDimensions,
    pub device_scale_bits: u64,
    pub encoded_sha256: [u8; 32],
    pub visual_epoch_hash: [u8; 32],
    pub decoder_profile: Arc<str>,
    pub decoder_algorithm_version: Arc<str>,
}

#[allow(dead_code)]
impl DecodedFrameKey {
    pub(crate) fn from_frame(
        frame: &EncodedFrame,
        visual_epoch_hash: [u8; 32],
        decoder_profile: &str,
        decoder_algorithm_version: &str,
    ) -> Self {
        let metadata = frame.metadata();
        Self {
            session_id: metadata.session_id(),
            target_id: metadata.target_id(),
            frame_id: metadata.id(),
            capture_ordinal: metadata.capture_ordinal().get(),
            session_time_nanos: metadata.session_time().as_nanos(),
            source_format: metadata.format(),
            image_dimensions: metadata.image(),
            viewport_dimensions: metadata.viewport(),
            device_scale_bits: metadata.device_scale_factor().get().to_bits(),
            encoded_sha256: Sha256::digest(frame.bytes()).into(),
            visual_epoch_hash,
            decoder_profile: Arc::from(decoder_profile),
            decoder_algorithm_version: Arc::from(decoder_algorithm_version),
        }
    }
}

/// Complete identity for one normalized frame. Measurement parameters intentionally do not
/// appear here: they affect artifact measurements, not normalized pixels.
#[allow(dead_code)]
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) struct NormalizedFrameKey {
    pub decoded: DecodedFrameKey,
    pub visual_epoch_hash: [u8; 32],
    pub effective_crop: temporal_vision::PixelRect,
    pub effective_scale: temporal_vision::IntegerScale,
    pub background: temporal_vision::Rgb8,
    pub mask_or_region_digest: [u8; 32],
    pub normalization_recipe_version: Arc<str>,
    pub transfer_lut_version: Arc<str>,
    pub normalization_algorithm_version: Arc<str>,
}

#[allow(dead_code)]
impl NormalizedFrameKey {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        decoded: DecodedFrameKey,
        visual_epoch_hash: [u8; 32],
        effective_crop: temporal_vision::PixelRect,
        effective_scale: temporal_vision::IntegerScale,
        background: temporal_vision::Rgb8,
        mask_or_region_digest: [u8; 32],
        normalization_recipe_version: &str,
        transfer_lut_version: &str,
        normalization_algorithm_version: &str,
    ) -> Self {
        Self {
            decoded,
            visual_epoch_hash,
            effective_crop,
            effective_scale,
            background,
            mask_or_region_digest,
            normalization_recipe_version: Arc::from(normalization_recipe_version),
            transfer_lut_version: Arc::from(transfer_lut_version),
            normalization_algorithm_version: Arc::from(normalization_algorithm_version),
        }
    }
}

#[allow(dead_code)]
pub(crate) fn visual_epoch_hash(sources: &[SourceFingerprint]) -> [u8; 32] {
    hash_epoch(sources)
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
        let key = DecodedFrameKey::from_frame(&frame, [8; 32], "profile", "decoder-v1");
        assert_eq!(key.session_id, frame.metadata().session_id());
        assert_eq!(key.target_id, frame.metadata().target_id());
        assert_eq!(key.frame_id, frame.metadata().id());
        assert_eq!(key.capture_ordinal, 4);
        assert_eq!(key.session_time_nanos, 5);
        assert_eq!(key.source_format, ImageFormat::Png);
        assert_eq!(key.image_dimensions, frame.metadata().image());
        assert_eq!(key.viewport_dimensions, frame.metadata().viewport());
        assert_eq!(key.device_scale_bits, 2.0_f64.to_bits());
        assert_eq!(key.encoded_sha256, fingerprint.encoded_sha256);
        assert_eq!(key.visual_epoch_hash, [8; 32]);
        assert_eq!(&*key.decoder_profile, "profile");
        assert_eq!(&*key.decoder_algorithm_version, "decoder-v1");
    }

    #[test]
    fn cache_key_is_sensitive_to_every_source_field_and_order() {
        let base = vec![source(1), source(2)];
        let expected = key(&base, ArtifactKind::Storyboard, b"params", "adapter-v1");
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

    fn decoded_key() -> DecodedFrameKey {
        DecodedFrameKey {
            session_id: SessionId::from_uuid(uuid::Uuid::from_u128(10)),
            target_id: TargetId::from_uuid(uuid::Uuid::from_u128(11)),
            frame_id: FrameId::from_uuid(uuid::Uuid::from_u128(12)),
            capture_ordinal: 13,
            session_time_nanos: 14,
            source_format: ImageFormat::Png,
            image_dimensions: PixelDimensions::new(15, 16).unwrap(),
            viewport_dimensions: PixelDimensions::new(17, 18).unwrap(),
            device_scale_bits: 19.0_f64.to_bits(),
            encoded_sha256: [20; 32],
            visual_epoch_hash: [21; 32],
            decoder_profile: Arc::from("decoder-profile"),
            decoder_algorithm_version: Arc::from("decoder-algorithm"),
        }
    }

    #[test]
    fn decoded_work_key_is_sensitive_to_every_field_and_epoch() {
        let base = decoded_key();
        let mut variants = Vec::new();
        let mut changed = base.clone();
        changed.session_id = SessionId::from_uuid(uuid::Uuid::from_u128(30));
        variants.push(changed);
        let mut changed = base.clone();
        changed.target_id = TargetId::from_uuid(uuid::Uuid::from_u128(31));
        variants.push(changed);
        let mut changed = base.clone();
        changed.frame_id = FrameId::from_uuid(uuid::Uuid::from_u128(32));
        variants.push(changed);
        let mut changed = base.clone();
        changed.capture_ordinal += 1;
        variants.push(changed);
        let mut changed = base.clone();
        changed.session_time_nanos += 1;
        variants.push(changed);
        let mut changed = base.clone();
        changed.source_format = ImageFormat::Jpeg;
        variants.push(changed);
        let mut changed = base.clone();
        changed.image_dimensions = PixelDimensions::new(16, 16).unwrap();
        variants.push(changed);
        let mut changed = base.clone();
        changed.viewport_dimensions = PixelDimensions::new(17, 19).unwrap();
        variants.push(changed);
        let mut changed = base.clone();
        changed.device_scale_bits = 2.0_f64.to_bits();
        variants.push(changed);
        let mut changed = base.clone();
        changed.encoded_sha256[0] ^= 1;
        variants.push(changed);
        let mut changed = base.clone();
        changed.visual_epoch_hash[0] ^= 1;
        variants.push(changed);
        let mut changed = base.clone();
        changed.decoder_profile = Arc::from("other-profile");
        variants.push(changed);
        let mut changed = base.clone();
        changed.decoder_algorithm_version = Arc::from("other-algorithm");
        variants.push(changed);
        assert!(variants.iter().all(|variant| variant != &base));

        let mut ordered = base.clone();
        ordered.frame_id = base.frame_id;
        ordered.capture_ordinal = base.capture_ordinal + 1;
        assert_ne!(ordered, base);
    }

    #[test]
    fn normalized_work_key_is_sensitive_to_each_normalization_field() {
        let base = NormalizedFrameKey::new(
            decoded_key(),
            [40; 32],
            temporal_vision::PixelRect::new(1, 2, 3, 4).unwrap(),
            temporal_vision::IntegerScale::IDENTITY,
            temporal_vision::Rgb8::new(5, 6, 7),
            [41; 32],
            "recipe-v1",
            "lut-v1",
            "normalizer-v1",
        );
        let mut variants = Vec::new();
        let mut changed = base.clone();
        changed.decoded.frame_id = FrameId::from_uuid(uuid::Uuid::from_u128(50));
        variants.push(changed);
        let mut changed = base.clone();
        changed.visual_epoch_hash[0] ^= 1;
        variants.push(changed);
        let mut changed = base.clone();
        changed.effective_crop = temporal_vision::PixelRect::new(1, 2, 2, 4).unwrap();
        variants.push(changed);
        let mut changed = base.clone();
        changed.effective_scale =
            temporal_vision::IntegerScale::down(std::num::NonZeroU8::new(2).unwrap()).unwrap();
        variants.push(changed);
        let mut changed = base.clone();
        changed.background = temporal_vision::Rgb8::new(8, 6, 7);
        variants.push(changed);
        let mut changed = base.clone();
        changed.mask_or_region_digest[0] ^= 1;
        variants.push(changed);
        let mut changed = base.clone();
        changed.normalization_recipe_version = Arc::from("recipe-v2");
        variants.push(changed);
        let mut changed = base.clone();
        changed.transfer_lut_version = Arc::from("lut-v2");
        variants.push(changed);
        let mut changed = base.clone();
        changed.normalization_algorithm_version = Arc::from("normalizer-v2");
        variants.push(changed);
        assert!(variants.iter().all(|variant| variant != &base));
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
}
