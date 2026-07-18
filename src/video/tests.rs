use super::plan::{self, build_presentation_plan};
use krometrail_core::{
    CaptureGap, CaptureGapReason, CaptureOrdinal, CapturedFrame, DeviceScaleFactor, ErrorCode,
    FrameId, GapId, ImageFormat, MAX_VIDEO_PRESENTATION_SEGMENTS, ObservedTime, PixelDimensions,
    RangeResolutionOptions, ResolvedRange, SessionId, SessionRange, SessionTime, TargetId,
    TemporalRangeAnchorKind, VideoOutputGeometry, VideoPlanInput, VideoPresentationPlan,
    VideoPresentationPolicy, VideoSegmentSource, VideoTimingBasis, VisualEpoch,
};
use uuid::Uuid;

const MS: u64 = 1_000_000;

fn session() -> SessionId {
    SessionId::from_uuid(Uuid::from_u128(1))
}

fn target() -> TargetId {
    TargetId::from_uuid(Uuid::from_u128(2))
}

fn frame_id(value: u128) -> FrameId {
    FrameId::from_uuid(Uuid::from_u128(value))
}

fn gap_id(value: u128) -> GapId {
    GapId::from_uuid(Uuid::from_u128(value))
}

fn dimensions() -> PixelDimensions {
    PixelDimensions::new(4, 4).unwrap()
}

fn frame(id: FrameId, ordinal: u64, time_ms: u64) -> CapturedFrame {
    CapturedFrame::new(
        id,
        session(),
        target(),
        CaptureOrdinal::new(ordinal).unwrap(),
        None,
        ObservedTime::from_nanos(time_ms * MS),
        SessionTime::from_nanos(time_ms * MS),
        ImageFormat::Png,
        dimensions(),
        dimensions(),
        DeviceScaleFactor::new(1.0).unwrap(),
        vec![],
    )
    .unwrap()
}

fn gap(id: GapId, start_ms: u64, end_ms: u64) -> CaptureGap {
    CaptureGap::new(
        id,
        session(),
        target(),
        SessionRange::new(
            SessionTime::from_nanos(start_ms * MS),
            SessionTime::from_nanos(end_ms * MS),
        )
        .unwrap(),
        ObservedTime::from_nanos(end_ms.max(1_000) * MS),
        CaptureGapReason::PersistenceRejected,
        None,
        None,
    )
    .unwrap()
}

fn input(
    times_ms: &[u64],
    gaps: Vec<CaptureGap>,
    meaningful: &[usize],
    policy: VideoPresentationPolicy,
) -> VideoPlanInput {
    let ids: Vec<_> = (0..times_ms.len())
        .map(|index| frame_id(10 + index as u128))
        .collect();
    let frames: Vec<_> = ids
        .iter()
        .zip(times_ms)
        .enumerate()
        .map(|(index, (id, time))| frame(*id, index as u64 + 1, *time))
        .collect();
    let max_time = times_ms.iter().copied().max().unwrap_or(0) * MS;
    let range = ResolvedRange::new(
        session(),
        target(),
        TemporalRangeAnchorKind::SessionTime,
        SessionRange::new(SessionTime::ZERO, SessionTime::from_nanos(max_time)).unwrap(),
        SessionRange::new(SessionTime::ZERO, SessionTime::from_nanos(max_time)).unwrap(),
        ids.clone(),
        vec![],
        vec![],
        vec![],
        gaps,
        vec![],
        RangeResolutionOptions::DEFAULT,
    )
    .unwrap();
    let epoch = VisualEpoch {
        index: 0,
        frame_ids: ids.clone(),
        image: dimensions(),
        viewport: dimensions(),
        device_scale_factor: DeviceScaleFactor::new(1.0).unwrap(),
    };
    let geometry = VideoOutputGeometry::new(dimensions(), dimensions(), dimensions()).unwrap();
    VideoPlanInput::new(
        range,
        epoch,
        frames,
        meaningful.iter().map(|index| ids[*index]).collect(),
        geometry,
        policy,
    )
    .unwrap()
}

fn durations(plan: &VideoPresentationPlan) -> Vec<u64> {
    plan.segments()
        .iter()
        .map(|segment| segment.presentation().duration_nanos())
        .collect()
}

#[test]
fn real_time_preserves_deltas_ties_and_terminal_hold() {
    let (minimum, terminal, _, _) = plan::timing_constants();
    let ordinary = build_presentation_plan(input(
        &[0, 100, 350],
        vec![],
        &[],
        VideoPresentationPolicy::RealTime,
    ))
    .unwrap();
    assert_eq!(durations(&ordinary), vec![100 * MS, 250 * MS, terminal]);
    assert_eq!(ordinary.duration().as_nanos(), 600 * MS);

    let tied = build_presentation_plan(input(
        &[0, 0],
        vec![],
        &[],
        VideoPresentationPolicy::RealTime,
    ))
    .unwrap();
    assert_eq!(durations(&tied), vec![minimum, terminal]);
    assert_eq!(
        tied.segments()[0].timing_basis(),
        VideoTimingBasis::MinimumVisibleFrame
    );

    let single =
        build_presentation_plan(input(&[0], vec![], &[], VideoPresentationPolicy::RealTime))
            .unwrap();
    assert_eq!(durations(&single), vec![terminal]);
    assert_eq!(
        single.segments()[0].timing_basis(),
        VideoTimingBasis::TerminalHold
    );
}

#[test]
fn model_policy_holds_meaningful_frames_and_gap_slates_explicitly() {
    let (_, _, meaningful_hold, gap_hold) = plan::timing_constants();
    let plan = build_presentation_plan(input(
        &[0, 100, 300],
        vec![gap(gap_id(30), 100, 200)],
        &[0],
        VideoPresentationPolicy::ModelOptimized,
    ))
    .unwrap();
    assert_eq!(
        durations(&plan),
        vec![meaningful_hold, gap_hold, 100 * MS, 250 * MS]
    );
    assert_eq!(
        plan.segments()[0].timing_basis(),
        VideoTimingBasis::ModelMeaningfulHold
    );
    assert_eq!(
        plan.segments()[1].timing_basis(),
        VideoTimingBasis::ModelGapHold
    );
}

#[test]
fn gaps_clip_coalesce_and_serialize_independently_of_input_order() {
    let first = gap(gap_id(40), 50, 200);
    let second = gap(gap_id(30), 150, 300);
    let plan_a = build_presentation_plan(input(
        &[100, 200, 300, 400],
        vec![first.clone(), second.clone()],
        &[],
        VideoPresentationPolicy::RealTime,
    ))
    .unwrap();
    let plan_b = build_presentation_plan(input(
        &[100, 200, 300, 400],
        vec![second, first],
        &[],
        VideoPresentationPolicy::RealTime,
    ))
    .unwrap();
    assert_eq!(
        serde_json::to_vec(&plan_a).unwrap(),
        serde_json::to_vec(&plan_b).unwrap()
    );
    assert_eq!(plan_a.input_frame_ids().len(), 4);
    let VideoSegmentSource::GapSlate {
        gap_ids,
        source_range,
    } = plan_a.segments()[0].source()
    else {
        panic!("expected coalesced gap slate")
    };
    assert_eq!(gap_ids, &vec![gap_id(30), gap_id(40)]);
    assert_eq!(source_range.start(), SessionTime::from_nanos(100 * MS));
    assert_eq!(source_range.end(), SessionTime::from_nanos(300 * MS));
    assert_eq!(
        plan_a.segments()[0].presentation().duration_nanos(),
        200 * MS
    );
}

#[test]
fn gap_boundaries_split_holds_without_inventing_frames() {
    let plan = build_presentation_plan(input(
        &[100, 300, 400],
        vec![gap(gap_id(30), 150, 250)],
        &[],
        VideoPresentationPolicy::RealTime,
    ))
    .unwrap();
    assert_eq!(
        durations(&plan),
        vec![50 * MS, 100 * MS, 50 * MS, 100 * MS, 250 * MS]
    );
    let source_ids: Vec<_> = plan
        .segments()
        .iter()
        .filter_map(|segment| match segment.source() {
            VideoSegmentSource::SourceFrame { frame_id, .. } => Some(*frame_id),
            VideoSegmentSource::GapSlate { .. } => None,
        })
        .collect();
    assert!(
        source_ids
            .iter()
            .all(|id| plan.input_frame_ids().contains(id))
    );
}

#[test]
fn fully_obscured_meaningful_frame_fails_instead_of_claiming_a_hold() {
    let error = build_presentation_plan(input(
        &[100, 200, 300],
        vec![gap(gap_id(30), 100, 250)],
        &[0],
        VideoPresentationPolicy::ModelOptimized,
    ))
    .unwrap_err();
    assert_eq!(error.code, ErrorCode::InvalidInput);
}

#[test]
fn exact_segment_limit_passes_and_next_unit_fails() {
    assert!(plan::segment_limit_probe(MAX_VIDEO_PRESENTATION_SEGMENTS).is_ok());
    assert_eq!(
        plan::segment_limit_probe(MAX_VIDEO_PRESENTATION_SEGMENTS + 1)
            .unwrap_err()
            .code,
        ErrorCode::ResourceLimitExceeded
    );
}

#[test]
fn excessive_model_gap_holds_fail_without_truncating_segments() {
    let times: Vec<_> = (0..120).map(|index| index * 250).collect();
    let gaps: Vec<_> = (0..119)
        .map(|index| {
            gap(
                gap_id(1_000 + index as u128),
                index * 250 + 1,
                index * 250 + 2,
            )
        })
        .collect();
    let error = build_presentation_plan(input(
        &times,
        gaps,
        &[],
        VideoPresentationPolicy::ModelOptimized,
    ))
    .unwrap_err();
    assert_eq!(error.code, ErrorCode::ResourceLimitExceeded);
}
