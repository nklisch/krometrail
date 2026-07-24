use super::*;

#[test]
fn temporal_video_operation_is_one_stable_registry_contract() {
    assert_eq!(
        TEMPORAL_VIDEO_OPERATION.stable_name,
        "generate_temporal_video"
    );
    assert_eq!(
        TEMPORAL_VIDEO_OPERATION.capability,
        crate::CapabilityId::TemporalVideo
    );
    assert_eq!(
        TEMPORAL_VIDEO_OPERATION.mutability,
        crate::OperationMutability::ReadOnly
    );
    assert!(!TEMPORAL_VIDEO_OPERATION.description.trim().is_empty());
}
use crate::{
    ArtifactId, CaptureGap, CaptureGapReason, CaptureOrdinal, CapturedFrame, DeviceScaleFactor,
    ErrorCode, FrameId, GapId, ImageFormat, ObservedTime, OutputLimitsRequest, PixelDimensions,
    RangeResolutionOptions, ResolvedRange, SessionId, SessionRange, SessionTime, TargetId,
    TemporalRangeAnchorKind, VideoEncodedClip, VideoEncoderIdentity, VideoEncodingProfile,
    VisualEpoch,
};
use sha2::{Digest, Sha256};
use uuid::Uuid;

fn session() -> SessionId {
    SessionId::from_uuid(Uuid::from_u128(1))
}

fn target() -> TargetId {
    TargetId::from_uuid(Uuid::from_u128(2))
}

fn frame_id(value: u128) -> FrameId {
    FrameId::from_uuid(Uuid::from_u128(value))
}

fn dimensions() -> PixelDimensions {
    PixelDimensions::new(4, 4).unwrap()
}

fn geometry() -> VideoOutputGeometry {
    VideoOutputGeometry::new(dimensions(), dimensions(), dimensions()).unwrap()
}

fn frame(id: FrameId, ordinal: u64, time: u64) -> CapturedFrame {
    CapturedFrame::new(
        id,
        session(),
        target(),
        CaptureOrdinal::new(ordinal).unwrap(),
        None,
        ObservedTime::from_nanos(time),
        SessionTime::from_nanos(time),
        ImageFormat::Png,
        dimensions(),
        dimensions(),
        DeviceScaleFactor::new(1.0).unwrap(),
        vec![],
    )
    .unwrap()
}

fn resolved(frame_ids: Vec<FrameId>, end: u64) -> ResolvedRange {
    ResolvedRange::new(
        session(),
        target(),
        TemporalRangeAnchorKind::SessionTime,
        SessionRange::new(SessionTime::ZERO, SessionTime::from_nanos(end)).unwrap(),
        SessionRange::new(SessionTime::ZERO, SessionTime::from_nanos(end)).unwrap(),
        frame_ids,
        vec![],
        vec![],
        vec![],
        vec![],
        vec![],
        RangeResolutionOptions::DEFAULT,
    )
    .unwrap()
}

fn resolved_with_gaps(frame_ids: Vec<FrameId>, end: u64, gaps: Vec<CaptureGap>) -> ResolvedRange {
    ResolvedRange::new(
        session(),
        target(),
        TemporalRangeAnchorKind::SessionTime,
        SessionRange::new(SessionTime::ZERO, SessionTime::from_nanos(end)).unwrap(),
        SessionRange::new(SessionTime::ZERO, SessionTime::from_nanos(end)).unwrap(),
        frame_ids,
        vec![],
        vec![],
        vec![],
        gaps,
        vec![],
        RangeResolutionOptions::DEFAULT,
    )
    .unwrap()
}

fn epoch(frame_ids: Vec<FrameId>) -> VisualEpoch {
    VisualEpoch {
        index: 0,
        frame_ids,
        image: dimensions(),
        viewport: dimensions(),
        device_scale_factor: DeviceScaleFactor::new(1.0).unwrap(),
    }
}

fn one_frame_plan() -> VideoPresentationPlan {
    let id = frame_id(3);
    VideoPresentationPlan::new(
        VideoPresentationPolicy::ModelOptimized,
        SessionRange::new(SessionTime::ZERO, SessionTime::from_nanos(10)).unwrap(),
        SessionRange::new(SessionTime::ZERO, SessionTime::from_nanos(10)).unwrap(),
        SessionRange::new(SessionTime::from_nanos(2), SessionTime::from_nanos(2)).unwrap(),
        epoch(vec![id]),
        vec![id],
        vec![SessionTime::from_nanos(2)],
        vec![id],
        vec![
            VideoPresentationSegment::new(
                0,
                VideoSegmentSource::source_frame(id, SessionTime::from_nanos(2)).unwrap(),
                PresentationRange::new(
                    PresentationTime::ZERO,
                    PresentationTime::from_nanos(1_000_000_000).unwrap(),
                )
                .unwrap(),
                VideoTimingBasis::ModelMeaningfulHold,
            )
            .unwrap(),
        ],
        geometry(),
    )
    .unwrap()
}

fn gap_plan() -> VideoPresentationPlan {
    let first = frame_id(3);
    let second = frame_id(4);
    VideoPresentationPlan::new(
        VideoPresentationPolicy::RealTime,
        SessionRange::new(SessionTime::ZERO, SessionTime::from_nanos(10)).unwrap(),
        SessionRange::new(SessionTime::ZERO, SessionTime::from_nanos(10)).unwrap(),
        SessionRange::new(SessionTime::from_nanos(2), SessionTime::from_nanos(4)).unwrap(),
        epoch(vec![first, second]),
        vec![first, second],
        vec![SessionTime::from_nanos(2), SessionTime::from_nanos(4)],
        vec![],
        vec![
            VideoPresentationSegment::new(
                0,
                VideoSegmentSource::gap_slate(
                    vec![GapId::from_uuid(Uuid::from_u128(80))],
                    SessionRange::new(SessionTime::from_nanos(2), SessionTime::from_nanos(4))
                        .unwrap(),
                )
                .unwrap(),
                PresentationRange::new(
                    PresentationTime::ZERO,
                    PresentationTime::from_nanos(2).unwrap(),
                )
                .unwrap(),
                VideoTimingBasis::RecordedGap,
            )
            .unwrap(),
            VideoPresentationSegment::new(
                1,
                VideoSegmentSource::source_frame(second, SessionTime::from_nanos(4)).unwrap(),
                PresentationRange::new(
                    PresentationTime::from_nanos(2).unwrap(),
                    PresentationTime::from_nanos(250_000_002).unwrap(),
                )
                .unwrap(),
                VideoTimingBasis::TerminalHold,
            )
            .unwrap(),
        ],
        geometry(),
    )
    .unwrap()
}

#[test]
fn stable_policy_and_timing_names_share_their_enum_registries() {
    for policy in VideoPresentationPolicy::ALL {
        let json = serde_json::to_string(policy).unwrap();
        assert_eq!(json, format!("\"{}\"", policy.as_str()));
        assert_eq!(
            serde_json::from_str::<VideoPresentationPolicy>(&json).unwrap(),
            *policy
        );
    }
    for basis in VideoTimingBasis::ALL {
        let json = serde_json::to_string(basis).unwrap();
        assert_eq!(json, format!("\"{}\"", basis.as_str()));
        assert_eq!(
            serde_json::from_str::<VideoTimingBasis>(&json).unwrap(),
            *basis
        );
    }
    assert!(serde_json::from_str::<VideoPresentationPolicy>("\"provider_default\"").is_err());
}

#[test]
fn plan_round_trip_revalidates_version_duration_and_unknown_fields() {
    let plan = one_frame_plan();
    let json = serde_json::to_string(&plan).unwrap();
    assert_eq!(
        serde_json::from_str::<VideoPresentationPlan>(&json).unwrap(),
        plan
    );

    let mut value: serde_json::Value = serde_json::from_str(&json).unwrap();
    value["duration"] = serde_json::json!(1);
    assert!(serde_json::from_value::<VideoPresentationPlan>(value.clone()).is_err());
    value["duration"] = serde_json::json!(1_000_000_000_u64);
    value["version"] = serde_json::json!("temporal-video-plan-v2");
    assert!(serde_json::from_value::<VideoPresentationPlan>(value.clone()).is_err());
    value["version"] = serde_json::json!(TEMPORAL_VIDEO_PLAN_VERSION);
    value["unexpected"] = serde_json::json!(true);
    assert!(serde_json::from_value::<VideoPresentationPlan>(value).is_err());
}

#[test]
fn persisted_plan_rejects_identity_policy_source_kind_and_canonical_timing_drift() {
    let original = serde_json::to_value(one_frame_plan()).unwrap();

    let mut value = original.clone();
    value["segments"][0]["source"]["session_time"] = serde_json::json!(3_u64);
    assert!(serde_json::from_value::<VideoPresentationPlan>(value).is_err());

    let mut value = original.clone();
    value["policy"] = serde_json::json!("real_time");
    assert!(serde_json::from_value::<VideoPresentationPlan>(value).is_err());

    let mut value = original.clone();
    value["segments"][0]["timing_basis"] = serde_json::json!("recorded_gap");
    assert!(serde_json::from_value::<VideoPresentationPlan>(value).is_err());

    let mut value = original;
    value["segments"][0]["presentation"]["end"] = serde_json::json!(999_999_999_u64);
    value["duration"] = serde_json::json!(999_999_999_u64);
    assert!(serde_json::from_value::<VideoPresentationPlan>(value).is_err());
}

#[test]
fn durable_plan_enforces_source_and_presentation_duration_ceilings() {
    let first = frame_id(3);
    let second = frame_id(4);
    let over_source_limit = MAX_VIDEO_SOURCE_DURATION.as_nanos() as u64 + 1;
    let segments = vec![
        VideoPresentationSegment::new(
            0,
            VideoSegmentSource::source_frame(first, SessionTime::ZERO).unwrap(),
            PresentationRange::new(
                PresentationTime::ZERO,
                PresentationTime::from_nanos(over_source_limit).unwrap(),
            )
            .unwrap(),
            VideoTimingBasis::RecordedDelta,
        )
        .unwrap(),
        VideoPresentationSegment::new(
            1,
            VideoSegmentSource::source_frame(second, SessionTime::from_nanos(over_source_limit))
                .unwrap(),
            PresentationRange::new(
                PresentationTime::from_nanos(over_source_limit).unwrap(),
                PresentationTime::from_nanos(over_source_limit + TERMINAL_HOLD_NANOS).unwrap(),
            )
            .unwrap(),
            VideoTimingBasis::TerminalHold,
        )
        .unwrap(),
    ];
    assert_eq!(
        VideoPresentationPlan::new(
            VideoPresentationPolicy::RealTime,
            SessionRange::new(
                SessionTime::ZERO,
                SessionTime::from_nanos(over_source_limit)
            )
            .unwrap(),
            SessionRange::new(
                SessionTime::ZERO,
                SessionTime::from_nanos(over_source_limit)
            )
            .unwrap(),
            SessionRange::new(
                SessionTime::ZERO,
                SessionTime::from_nanos(over_source_limit)
            )
            .unwrap(),
            epoch(vec![first, second]),
            vec![first, second],
            vec![
                SessionTime::ZERO,
                SessionTime::from_nanos(over_source_limit)
            ],
            vec![],
            segments,
            geometry(),
        )
        .unwrap_err()
        .code,
        ErrorCode::ResourceLimitExceeded
    );
    assert!(
        PresentationTime::from_nanos(MAX_VIDEO_PRESENTATION_DURATION.as_nanos() as u64).is_ok()
    );
    assert_eq!(
        PresentationTime::from_nanos(MAX_VIDEO_PRESENTATION_DURATION.as_nanos() as u64 + 1)
            .unwrap_err()
            .code,
        ErrorCode::ResourceLimitExceeded
    );
}

#[test]
fn geometry_preserves_aspect_without_upscale_and_records_even_padding() {
    let source = PixelDimensions::new(5, 3).unwrap();
    let geometry =
        VideoOutputGeometry::new(source, source, PixelDimensions::new(6, 4).unwrap()).unwrap();
    assert_eq!(geometry.pad_right(), 1);
    assert_eq!(geometry.pad_bottom(), 1);
    assert!(
        VideoOutputGeometry::new(
            source,
            PixelDimensions::new(6, 4).unwrap(),
            PixelDimensions::new(6, 4).unwrap(),
        )
        .is_err()
    );
    assert!(
        VideoOutputGeometry::new(
            source,
            PixelDimensions::new(4, 3).unwrap(),
            PixelDimensions::new(4, 4).unwrap(),
        )
        .is_err()
    );
}

#[test]
fn plan_input_rejects_cross_scope_epoch_and_order_drift() {
    let ids = vec![frame_id(3), frame_id(4)];
    let range = resolved(ids.clone(), 10);
    let frames = vec![frame(ids[0], 1, 2), frame(ids[1], 2, 3)];
    assert!(
        VideoPlanInput::new(
            range.clone(),
            epoch(ids.clone()),
            frames.clone(),
            ids.clone(),
            geometry(),
            VideoPresentationPolicy::RealTime,
        )
        .is_ok()
    );

    let mut wrong_target = frames.clone();
    wrong_target[1] = CapturedFrame::new(
        ids[1],
        session(),
        TargetId::from_uuid(Uuid::from_u128(99)),
        CaptureOrdinal::new(2).unwrap(),
        None,
        ObservedTime::from_nanos(3),
        SessionTime::from_nanos(3),
        ImageFormat::Png,
        dimensions(),
        dimensions(),
        DeviceScaleFactor::new(1.0).unwrap(),
        vec![],
    )
    .unwrap();
    assert!(
        VideoPlanInput::new(
            range.clone(),
            epoch(ids.clone()),
            wrong_target,
            vec![],
            geometry(),
            VideoPresentationPolicy::RealTime,
        )
        .is_err()
    );

    let reversed_ordinals = vec![frame(ids[0], 2, 2), frame(ids[1], 1, 3)];
    assert!(
        VideoPlanInput::new(
            range,
            epoch(ids.clone()),
            reversed_ordinals,
            vec![ids[1], ids[0]],
            geometry(),
            VideoPresentationPolicy::RealTime,
        )
        .is_err()
    );
}

#[test]
fn source_frame_and_meaningful_limits_accept_exact_boundary_and_reject_next_unit() {
    let make = |count: usize| {
        let ids: Vec<_> = (0..count)
            .map(|index| frame_id(10 + index as u128))
            .collect();
        let frames: Vec<_> = ids
            .iter()
            .enumerate()
            .map(|(index, id)| frame(*id, index as u64 + 1, index as u64))
            .collect();
        (resolved(ids.clone(), 1_000), epoch(ids), frames)
    };
    let (range, epoch, frames) = make(MAX_VIDEO_SOURCE_FRAMES);
    assert!(
        VideoPlanInput::new(
            range,
            epoch,
            frames,
            vec![],
            geometry(),
            VideoPresentationPolicy::RealTime,
        )
        .is_ok()
    );
    let (range, epoch, frames) = make(MAX_VIDEO_SOURCE_FRAMES + 1);
    assert_eq!(
        VideoPlanInput::new(
            range,
            epoch,
            frames,
            vec![],
            geometry(),
            VideoPresentationPolicy::RealTime,
        )
        .unwrap_err()
        .code,
        ErrorCode::ResourceLimitExceeded
    );

    let (range, epoch, frames) = make(MAX_VIDEO_MEANINGFUL_FRAMES + 1);
    let exact = frames
        .iter()
        .take(MAX_VIDEO_MEANINGFUL_FRAMES)
        .map(CapturedFrame::id)
        .collect();
    assert!(
        VideoPlanInput::new(
            range.clone(),
            epoch.clone(),
            frames.clone(),
            exact,
            geometry(),
            VideoPresentationPolicy::ModelOptimized,
        )
        .is_ok()
    );
    let next = frames.iter().map(CapturedFrame::id).collect();
    assert_eq!(
        VideoPlanInput::new(
            range,
            epoch,
            frames,
            next,
            geometry(),
            VideoPresentationPolicy::ModelOptimized,
        )
        .unwrap_err()
        .code,
        ErrorCode::ResourceLimitExceeded
    );
}

#[test]
fn plan_rejects_noncontiguous_segments_and_wrong_epoch_source() {
    let mut value: serde_json::Value =
        serde_json::from_str(&serde_json::to_string(&one_frame_plan()).unwrap()).unwrap();
    value["segments"][0]["presentation"]["start"] = serde_json::json!(1_u64);
    assert!(serde_json::from_value::<VideoPresentationPlan>(value).is_err());
}

fn identity(build: u8) -> VideoEncoderIdentity {
    VideoEncoderIdentity::new(
        "ffmpeg 7.1",
        [build; 32],
        "libx264",
        "adapter-v1",
        "args-v1",
    )
    .unwrap()
}

fn encoded(identity: VideoEncoderIdentity, profile: VideoEncodingProfile) -> VideoEncodedClip {
    let bytes = vec![1_u8, 2, 3, 4];
    let hash = temporal_vision::OutputHash::from_bytes(Sha256::digest(&bytes).into());
    VideoEncodedClip::new(identity, profile, hash, bytes).unwrap()
}

fn selection() -> VideoSelectionIdentity {
    VideoSelectionIdentity::meaningful_v1([9; 32])
}

#[test]
fn temporal_video_manifest_round_trip_preserves_exact_plan_and_closed_media_profile() {
    let plan = one_frame_plan();
    let scope = resolved(plan.input_frame_ids().to_vec(), 10);
    let profile = VideoEncodingProfile::new(plan.output(), 1024).unwrap();
    let encoded = encoded(identity(7), profile);
    let manifest = TemporalVideoManifest::new(
        ArtifactId::from_uuid(Uuid::from_u128(50)),
        &scope,
        plan,
        Some(selection()),
        &encoded,
    )
    .unwrap();
    assert_eq!(
        manifest.schema_version(),
        TEMPORAL_VIDEO_MANIFEST_SCHEMA_VERSION
    );
    assert_eq!(manifest.media_type(), "video/mp4");
    assert_eq!(manifest.codec(), "h264");
    assert_eq!(manifest.pixel_format(), "yuv420p");
    assert!(!manifest.has_audio());
    assert_eq!(manifest.output_hash(), encoded.output_hash());

    let json = serde_json::to_string(&manifest).unwrap();
    assert_eq!(
        serde_json::from_str::<TemporalVideoManifest>(&json).unwrap(),
        manifest
    );
    assert!(!json.contains("executable"));
    assert!(!json.contains("stderr"));
    assert!(!json.contains("provider"));
    assert!(!json.contains("source_pixels"));
}

#[test]
fn manifest_deserialization_rejects_scope_media_and_length_contradictions() {
    let plan = one_frame_plan();
    let scope = resolved(plan.input_frame_ids().to_vec(), 10);
    let profile = VideoEncodingProfile::new(plan.output(), 1024).unwrap();
    let encoded = encoded(identity(7), profile);
    let manifest = TemporalVideoManifest::new(
        ArtifactId::from_uuid(Uuid::from_u128(50)),
        &scope,
        plan,
        Some(selection()),
        &encoded,
    )
    .unwrap();
    let mut value = serde_json::to_value(manifest).unwrap();
    value["media_type"] = serde_json::json!("video/webm");
    assert!(serde_json::from_value::<TemporalVideoManifest>(value.clone()).is_err());
    value["media_type"] = serde_json::json!("video/mp4");
    value["has_audio"] = serde_json::json!(true);
    assert!(serde_json::from_value::<TemporalVideoManifest>(value.clone()).is_err());
    value["has_audio"] = serde_json::json!(false);
    value["encoded_byte_len"] = serde_json::json!(0);
    assert!(serde_json::from_value::<TemporalVideoManifest>(value.clone()).is_err());
    value["encoded_byte_len"] = serde_json::json!(4);
    value["requested_range"]["end"] = serde_json::json!(9_u64);
    assert!(serde_json::from_value::<TemporalVideoManifest>(value).is_err());
}

#[test]
fn manifest_binds_and_revalidates_exact_canonical_gap_contributors() {
    let plan = gap_plan();
    let gap = CaptureGap::new(
        GapId::from_uuid(Uuid::from_u128(80)),
        session(),
        target(),
        SessionRange::new(SessionTime::from_nanos(2), SessionTime::from_nanos(4)).unwrap(),
        ObservedTime::from_nanos(4),
        CaptureGapReason::FrameRejected,
        None,
        None,
    )
    .unwrap();
    let scope = resolved_with_gaps(plan.input_frame_ids().to_vec(), 10, vec![gap]);
    let profile = VideoEncodingProfile::new(plan.output(), 1024).unwrap();
    let encoded = encoded(identity(7), profile);
    let manifest = TemporalVideoManifest::new(
        ArtifactId::from_uuid(Uuid::from_u128(50)),
        &scope,
        plan,
        None,
        &encoded,
    )
    .unwrap();
    assert_eq!(manifest.gap_evidence().len(), 1);
    assert_eq!(
        manifest.gap_evidence()[0].gap_id(),
        GapId::from_uuid(Uuid::from_u128(80))
    );

    let original = serde_json::to_value(manifest).unwrap();
    let mut value = original.clone();
    value["gap_evidence"][0]["gap_id"] = serde_json::json!(GapId::from_uuid(Uuid::from_u128(81)));
    assert!(serde_json::from_value::<TemporalVideoManifest>(value).is_err());

    let mut value = original.clone();
    value["plan"]["segments"][0]["source"]["gap_ids"][0] =
        serde_json::json!(GapId::from_uuid(Uuid::from_u128(81)));
    assert!(serde_json::from_value::<TemporalVideoManifest>(value).is_err());

    let mut value = original;
    value["gap_evidence"] = serde_json::json!([]);
    assert!(serde_json::from_value::<TemporalVideoManifest>(value).is_err());
}

#[test]
fn generated_video_schemas_publish_strict_wire_shapes_and_hard_bounds() {
    let plan_schema = serde_json::to_string(&schemars::schema_for!(VideoPresentationPlan)).unwrap();
    assert!(plan_schema.contains("additionalProperties\":false"));
    assert!(plan_schema.contains("temporal-video-plan-v1"));
    assert!(plan_schema.contains("maxItems\":120"));
    assert!(plan_schema.contains("maxItems\":512"));
    assert!(plan_schema.contains("maximum\":60000000000"));
    assert!(plan_schema.contains("maximum\":1920"));
    assert!(plan_schema.contains("maximum\":1080"));

    let identity_schema =
        serde_json::to_string(&schemars::schema_for!(VideoEncoderIdentity)).unwrap();
    assert!(identity_schema.contains("additionalProperties\":false"));
    assert!(
        identity_schema.contains("maxLength\":256"),
        "{identity_schema}"
    );
    assert!(identity_schema.contains("^[0-9a-f]{64}$"));

    let manifest_schema =
        serde_json::to_string(&schemars::schema_for!(TemporalVideoManifest)).unwrap();
    assert!(manifest_schema.contains("gap_evidence"));
    assert!(manifest_schema.contains("maximum\":67108864"));
    assert!(manifest_schema.contains("^video/mp4$"));
    assert!(manifest_schema.contains("^[0-9a-f]{64}$"));
    assert!(manifest_schema.contains("const\":false"));
}

#[test]
fn canonical_cache_transcript_is_stable_sensitive_and_contains_no_opaque_process_data() {
    let plan = one_frame_plan();
    let profile = VideoEncodingProfile::new(plan.output(), 1024).unwrap();
    let selection = selection();
    let first =
        canonical_video_cache_parameters(&plan, &identity(7), &profile, Some(&selection)).unwrap();
    let repeated =
        canonical_video_cache_parameters(&plan, &identity(7), &profile, Some(&selection)).unwrap();
    assert_eq!(first, repeated);
    assert_ne!(
        first,
        canonical_video_cache_parameters(&plan, &identity(8), &profile, Some(&selection)).unwrap()
    );
    let smaller_profile = VideoEncodingProfile::new(plan.output(), 512).unwrap();
    assert_ne!(
        first,
        canonical_video_cache_parameters(&plan, &identity(7), &smaller_profile, Some(&selection))
            .unwrap()
    );

    let mut timing_value = serde_json::to_value(&plan).unwrap();
    timing_value["policy"] = serde_json::json!("real_time");
    timing_value["meaningful_frame_ids"] = serde_json::json!([]);
    timing_value["segments"][0]["presentation"]["end"] = serde_json::json!(250_000_000_u64);
    timing_value["segments"][0]["timing_basis"] = serde_json::json!("terminal_hold");
    timing_value["duration"] = serde_json::json!(250_000_000_u64);
    let timing_plan: VideoPresentationPlan = serde_json::from_value(timing_value).unwrap();
    assert_ne!(
        first,
        canonical_video_cache_parameters(&timing_plan, &identity(7), &profile, None).unwrap()
    );

    let replacement = frame_id(99);
    let mut source_value = serde_json::to_value(&plan).unwrap();
    source_value["epoch"]["frame_ids"][0] = serde_json::json!(replacement);
    source_value["input_frame_ids"][0] = serde_json::json!(replacement);
    source_value["meaningful_frame_ids"][0] = serde_json::json!(replacement);
    source_value["segments"][0]["source"]["frame_id"] = serde_json::json!(replacement);
    let source_plan: VideoPresentationPlan = serde_json::from_value(source_value).unwrap();
    assert_ne!(
        first,
        canonical_video_cache_parameters(&source_plan, &identity(7), &profile, Some(&selection),)
            .unwrap()
    );

    let gap_plan = gap_plan();
    assert_ne!(
        first,
        canonical_video_cache_parameters(&gap_plan, &identity(7), &profile, None).unwrap()
    );

    let small = PixelDimensions::new(2, 2).unwrap();
    let small_geometry = VideoOutputGeometry::new(small, small, small).unwrap();
    let mut geometry_value = serde_json::to_value(&plan).unwrap();
    geometry_value["epoch"]["image"] = serde_json::json!({"width": 2, "height": 2});
    geometry_value["output"]["source"] = serde_json::json!({"width": 2, "height": 2});
    geometry_value["output"]["scaled"] = serde_json::json!({"width": 2, "height": 2});
    geometry_value["output"]["canvas"] = serde_json::json!({"width": 2, "height": 2});
    let geometry_plan: VideoPresentationPlan = serde_json::from_value(geometry_value).unwrap();
    let geometry_profile = VideoEncodingProfile::new(small_geometry, 1024).unwrap();
    assert_ne!(
        first,
        canonical_video_cache_parameters(
            &geometry_plan,
            &identity(7),
            &geometry_profile,
            Some(&selection),
        )
        .unwrap()
    );

    let json = std::str::from_utf8(&first).unwrap();
    assert!(json.contains(TEMPORAL_VIDEO_PLAN_VERSION));
    assert!(json.contains("model_optimized"));
    assert!(json.contains("model_meaningful_hold"));
    assert!(json.contains("max_encoded_input_bytes"));
    assert!(json.contains("libx264"));
    assert!(!json.contains("stderr"));
    assert!(!json.contains("/tmp"));
    assert!(!json.contains("source_pixels"));
    assert!(!json.contains("provider"));
}

#[test]
fn retained_video_request_handle_and_publication_are_constructor_validated() {
    let plan = one_frame_plan();
    let scope = resolved(plan.input_frame_ids().to_vec(), 10);
    let request = TemporalVideoGenerationRequest::new(
        scope.clone(),
        VideoPresentationPolicy::ModelOptimized,
        crate::OutputLimitsRequest::new(4, 4, 1024).unwrap(),
    )
    .unwrap();
    assert_eq!(request.range(), &scope);
    assert_eq!(request.policy(), VideoPresentationPolicy::ModelOptimized);

    let profile = VideoEncodingProfile::new(plan.output(), 1024).unwrap();
    let encoded = encoded(identity(7), profile);
    let manifest = TemporalVideoManifest::new(
        ArtifactId::from_uuid(Uuid::from_u128(50)),
        &scope,
        plan,
        Some(selection()),
        &encoded,
    )
    .unwrap();
    let digest = crate::Sha256Digest::from_bytes(*manifest.output_hash().as_bytes());
    let handle = VideoArtifactEvidenceHandle::new(
        manifest.artifact_id(),
        crate::EvidenceScope::from_range(&scope).unwrap(),
        crate::NonEmptyText::new("video/mp4").unwrap(),
        digest,
        manifest.encoded_byte_len(),
        manifest.clone(),
    )
    .unwrap();
    let read = VideoArtifactRead::new(handle.clone(), encoded.owned_encoded_bytes()).unwrap();
    assert_eq!(read.encoded_bytes(), encoded.encoded_bytes());
    assert_eq!(
        serde_json::from_value::<VideoArtifactEvidenceHandle>(
            serde_json::to_value(&handle).unwrap()
        )
        .unwrap(),
        handle
    );

    let sources = manifest
        .plan()
        .input_frame_ids()
        .iter()
        .map(|frame_id| crate::ArtifactSourceFingerprint {
            frame_id: *frame_id,
            encoded_sha256: [3; 32],
        })
        .collect();
    let publication = crate::VideoArtifactPublication::new(
        manifest.session_id(),
        manifest.target_id(),
        sources,
        crate::ArtifactCacheMetadata {
            cache_key: crate::ArtifactCacheKey::from_bytes([1; 32]),
            source_fingerprint: [2; 32],
            parameter_hash: [3; 32],
            visual_epoch_hash: [4; 32],
            cache_schema_version: 1,
            adapter_version: crate::NonEmptyText::new("adapter-v1").unwrap(),
            generator_name: crate::NonEmptyText::new(crate::TEMPORAL_VIDEO_GENERATOR_NAME).unwrap(),
            generator_version: crate::NonEmptyText::new(crate::TEMPORAL_VIDEO_GENERATOR_VERSION)
                .unwrap(),
        },
        manifest,
        encoded.owned_encoded_bytes(),
    )
    .unwrap();
    assert_eq!(publication.encoded_bytes.as_ref(), encoded.encoded_bytes());
}

#[test]
fn temporal_video_limit_refusals_name_frame_and_duration_values() {
    let frame_ids = (0..=MAX_VIDEO_SOURCE_FRAMES)
        .map(|index| frame_id(1_000 + index as u128))
        .collect();
    let frame_error = TemporalVideoGenerationRequest::new(
        resolved(frame_ids, 5_227_000_000),
        VideoPresentationPolicy::RealTime,
        OutputLimitsRequest::new(4, 4, 1024).unwrap(),
    )
    .unwrap_err();
    assert_eq!(frame_error.code, ErrorCode::ResourceLimitExceeded);
    assert_eq!(
        frame_error.message.as_str(),
        "temporal video source plan: 121 frames over 5.227 s exceeds limit 120 frames"
    );
    assert_eq!(frame_error.retry, crate::RetryAdvice::Never);
    assert!(frame_error.recovery.as_ref().is_some_and(|recovery| {
        recovery
            .as_str()
            .contains("split it into consecutive clips")
    }));

    let duration_error = TemporalVideoGenerationRequest::new(
        resolved(
            vec![frame_id(2_000)],
            MAX_VIDEO_SOURCE_DURATION.as_nanos() as u64 + 1,
        ),
        VideoPresentationPolicy::RealTime,
        OutputLimitsRequest::new(4, 4, 1024).unwrap(),
    )
    .unwrap_err();
    assert_eq!(duration_error.code, ErrorCode::ResourceLimitExceeded);
    assert_eq!(
        duration_error.message.as_str(),
        "temporal video source plan: 30.000000001 s exceeds limit 30 s"
    );
    assert!(
        duration_error
            .recovery
            .as_ref()
            .is_some_and(|recovery| recovery.as_str().contains("narrow the resolved range"))
    );
}

#[test]
fn selector_policy_alignment_and_request_limits_fail_explicitly() {
    let plan = one_frame_plan();
    let scope = resolved(plan.input_frame_ids().to_vec(), 10);
    let profile = VideoEncodingProfile::new(plan.output(), 1024).unwrap();
    let encoded = encoded(identity(7), profile);
    assert!(
        TemporalVideoManifest::new(
            ArtifactId::from_uuid(Uuid::from_u128(50)),
            &scope,
            plan,
            None,
            &encoded,
        )
        .is_err()
    );
    assert_eq!(
        TemporalVideoGenerationRequest::new(
            scope,
            VideoPresentationPolicy::RealTime,
            crate::OutputLimitsRequest::new(1, 2, 1024).unwrap(),
        )
        .unwrap_err()
        .code,
        ErrorCode::ResourceLimitExceeded
    );
    assert!(VideoSelectionIdentity::new("/private/selector", "v1", [0; 32]).is_err());
}
