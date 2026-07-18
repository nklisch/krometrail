use super::*;
use crate::{
    CaptureOrdinal, CapturedFrame, DeviceScaleFactor, ErrorCode, FrameId, ImageFormat,
    ObservedTime, PixelDimensions, RangeResolutionOptions, ResolvedRange, SessionId, SessionRange,
    SessionTime, TargetId, TemporalRangeAnchorKind, VisualEpoch,
};
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
