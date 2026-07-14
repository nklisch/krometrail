use std::num::{NonZeroU32, NonZeroUsize};

use png::{ColorType, Decoder};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use temporal_vision::{
    ArtifactKind, ArtifactLabels, ArtifactManifest, ComparisonOutcome, DeclaredGap, ErrorCode,
    Frame, FrameSequence, IntegerScale, Marker, MeasurementParameters, NormalizationParameters,
    ParameterValue, PixelDimensions, PixelFormat, ProcessingLimits, RenderLimits, Rgb8,
    SelectionReason, StoryboardParameters, StoryboardTileLimit, TimeRange, Timestamp,
    generate_storyboard, measure_adjacent, normalize_sequence, select_storyboard_frames,
};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
struct FrameId(String);

impl std::fmt::Display for FrameId {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
struct MarkerId(String);
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
struct GapId(String);
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
struct ArtifactId(String);

type SourceSequence = FrameSequence<FrameId, MarkerId, GapId, Box<[u8]>>;
type NormalizedFrames = temporal_vision::NormalizedSequence<FrameId>;

fn rgba(left: [u8; 3], right: [u8; 3]) -> Box<[u8]> {
    [
        left[0], left[1], left[2], 255, right[0], right[1], right[2], 255,
    ]
    .into()
}

fn fixture() -> (SourceSequence, NormalizedFrames) {
    let dimensions = PixelDimensions::new(2, 1).unwrap();
    let frames = [
        ("f0", 0, [0, 0, 0], [0, 0, 0]),
        ("f1", 10_000_000, [0, 0, 0], [0, 0, 0]),
        ("f2", 20_000_000, [32, 32, 32], [0, 0, 0]),
        ("f3", 20_000_000, [96, 96, 96], [0, 0, 0]),
        ("f4", 30_000_000, [64, 64, 64], [64, 64, 64]),
        ("f5", 40_000_000, [255, 255, 255], [255, 255, 255]),
        ("f6", 50_000_000, [0, 0, 0], [255, 255, 255]),
        ("f7", 60_000_000, [128, 128, 128], [128, 128, 128]),
        ("f8", 70_000_000, [0, 0, 0], [0, 0, 0]),
    ]
    .into_iter()
    .map(|(id, timestamp, left, right)| {
        Frame::new(
            FrameId(id.into()),
            Timestamp::from_nanos(timestamp),
            dimensions,
            PixelFormat::Rgba8SrgbStraight,
            rgba(left, right),
        )
        .unwrap()
    })
    .collect();
    let markers = vec![
        Marker::new(
            MarkerId("m0".into()),
            Timestamp::from_nanos(20_000_000),
            "action",
            "open panel",
        )
        .unwrap(),
        Marker::new(
            MarkerId("m1".into()),
            Timestamp::from_nanos(50_000_000),
            "navigation",
            "next state",
        )
        .unwrap(),
    ];
    let gaps = vec![
        DeclaredGap::new(
            GapId("g0".into()),
            TimeRange::new(
                Timestamp::from_nanos(45_000_000),
                Timestamp::from_nanos(45_000_000),
            )
            .unwrap(),
            "capture saturated",
            None,
        )
        .unwrap(),
    ];
    let source = FrameSequence::new(frames, markers, gaps, None, None).unwrap();
    let normalized = normalize_sequence(
        &source,
        NormalizationParameters::new(
            Rgb8::new(0, 0, 0),
            None,
            IntegerScale::IDENTITY,
            ProcessingLimits::default(),
        ),
    )
    .unwrap();
    (source, normalized)
}

fn request(tile_limit: StoryboardTileLimit, limits: RenderLimits) -> StoryboardParameters {
    StoryboardParameters::new(
        Timestamp::from_nanos(20_000_000),
        tile_limit,
        MeasurementParameters::new(0),
        ArtifactLabels::new("Panel transition", "Synthetic viewport").unwrap(),
        limits,
    )
}

#[test]
fn selection_preserves_anchor_priority_ties_segments_and_exact_roles() {
    let (source, normalized) = fixture();
    let three = select_storyboard_frames(
        &source,
        &normalized,
        Timestamp::from_nanos(20_000_000),
        StoryboardTileLimit::new(3).unwrap(),
        MeasurementParameters::new(0),
    )
    .unwrap();
    assert_eq!(
        three
            .selected_frames()
            .iter()
            .map(|frame| frame.frame_id().0.as_str())
            .collect::<Vec<_>>(),
        ["f1", "f5", "f8"]
    );
    assert_eq!(
        (
            three.before_index(),
            three.during_index(),
            three.after_index()
        ),
        (1, 5, 8)
    );
    assert_eq!(three.continuity_segment_count(), 2);
    assert_eq!(
        three
            .omitted_anchors()
            .iter()
            .map(|anchor| (anchor.frame_index(), anchor.reason()))
            .collect::<Vec<_>>(),
        [
            (2, SelectionReason::FirstChange),
            (4, SelectionReason::PostAnchor),
            (2, SelectionReason::MarkerBoundary),
            (6, SelectionReason::MarkerBoundary),
            (6, SelectionReason::GapBoundary),
        ]
    );

    let default = select_storyboard_frames(
        &source,
        &normalized,
        Timestamp::from_nanos(20_000_000),
        StoryboardTileLimit::default(),
        MeasurementParameters::new(0),
    )
    .unwrap();
    assert_eq!(
        default
            .selected_frames()
            .iter()
            .map(|frame| frame.frame_id().0.as_str())
            .collect::<Vec<_>>(),
        ["f1", "f2", "f3", "f4", "f5", "f6", "f7", "f8"]
    );
    let f3 = default
        .selected_frames()
        .iter()
        .find(|frame| frame.frame_id().0 == "f3")
        .unwrap();
    assert!(f3.reasons().contains(&SelectionReason::LocalChangePeak));
    assert!(f3.reasons().contains(&SelectionReason::ChangeTrend));
    assert!(default.selected_frames().iter().any(|frame| {
        frame
            .reasons()
            .contains(&SelectionReason::ChangedRegionTransition)
    }));
    assert!(default.selected_frames().iter().any(|frame| {
        frame.reasons().contains(&SelectionReason::InformationGain)
            || frame.reasons().contains(&SelectionReason::TemporalCoverage)
    }));

    let summary = default.visual_summary();
    let first = summary.first_change().unwrap();
    assert_eq!(
        (first.frame_id().0.as_str(), first.frame_index()),
        ("f2", 2)
    );
    assert_eq!(first.timestamp(), Timestamp::from_nanos(20_000_000));
    assert_eq!(
        (
            first.comparison().earlier_frame_index(),
            first.comparison().later_frame_index(),
        ),
        (1, 2)
    );
    let baseline = summary.peak_baseline_change().unwrap();
    assert_eq!(
        (baseline.frame_id().0.as_str(), baseline.frame_index()),
        ("f5", 5)
    );
    assert_eq!(
        (
            baseline.comparison().earlier_frame_index(),
            baseline.comparison().later_frame_index(),
        ),
        (1, 5)
    );
    let adjacent_peak = summary.peak_adjacent_changed_area().unwrap();
    assert_eq!(
        (
            adjacent_peak.frame_id().0.as_str(),
            adjacent_peak.frame_index()
        ),
        ("f4", 4)
    );
    assert_eq!(
        (
            adjacent_peak.comparison().earlier_frame_index(),
            adjacent_peak.comparison().later_frame_index(),
        ),
        (3, 4)
    );
    assert!(matches!(
        adjacent_peak.comparison().outcome(),
        ComparisonOutcome::Measured(vector)
            if vector.changed_pixel_proportion().changed() == 2
    ));

    let adjacent = measure_adjacent(&normalized, MeasurementParameters::new(0)).unwrap();
    assert!(matches!(
        adjacent[5].outcome(),
        ComparisonOutcome::GapBoundary { .. }
    ));
    assert_eq!(
        serde_json::to_vec(&default).unwrap(),
        serde_json::to_vec(
            &select_storyboard_frames(
                &source,
                &normalized,
                Timestamp::from_nanos(20_000_000),
                StoryboardTileLimit::default(),
                MeasurementParameters::new(0),
            )
            .unwrap()
        )
        .unwrap()
    );
}

#[test]
fn public_generator_returns_traceable_deterministic_pngs_and_manifests() {
    let (source, normalized) = fixture();
    let first = generate_storyboard(
        ArtifactId("storyboard-a".into()),
        Some(ArtifactId("orientation-a".into())),
        &source,
        &normalized,
        request(
            StoryboardTileLimit::new(3).unwrap(),
            RenderLimits::default(),
        ),
    )
    .unwrap();
    let second = generate_storyboard(
        ArtifactId("storyboard-a".into()),
        Some(ArtifactId("orientation-a".into())),
        &source,
        &normalized,
        request(
            StoryboardTileLimit::new(3).unwrap(),
            RenderLimits::default(),
        ),
    )
    .unwrap();
    assert_eq!(first, second);
    assert_eq!(first.storyboard().image().media_type(), "image/png");
    assert_eq!(
        &first.storyboard().image().bytes()[..8],
        b"\x89PNG\r\n\x1a\n"
    );
    assert_eq!(
        first.storyboard().manifest().selected_frame_ids(),
        &[
            FrameId("f1".into()),
            FrameId("f5".into()),
            FrameId("f8".into())
        ]
    );
    assert_eq!(
        first.storyboard().manifest().artifact_kind(),
        ArtifactKind::Storyboard
    );
    assert_eq!(
        first.orientation().unwrap().manifest().artifact_kind(),
        ArtifactKind::BeforeDuringAfter
    );
    assert_eq!(
        first.orientation().unwrap().manifest().selected_frame_ids(),
        &[
            FrameId("f1".into()),
            FrameId("f5".into()),
            FrameId("f8".into())
        ]
    );

    let bytes = first.storyboard().image().bytes();
    let digest: [u8; 32] = Sha256::digest(bytes).into();
    assert_eq!(
        first.storyboard().manifest().output_hash().as_bytes(),
        &digest
    );
    assert_eq!(
        first.storyboard().manifest().algorithm().name(),
        "temporal-storyboard"
    );
    assert_eq!(first.storyboard().manifest().algorithm().version(), "1.1.0");
    assert_eq!(
        first.storyboard().manifest().storyboard_selection(),
        Some(first.selection())
    );
    assert_eq!(
        first
            .orientation()
            .unwrap()
            .manifest()
            .storyboard_selection(),
        Some(first.selection())
    );
    assert_eq!(
        first.storyboard().manifest().parameters().get("title"),
        Some(&ParameterValue::Text("Panel transition".into()))
    );
    assert_eq!(
        first
            .storyboard()
            .manifest()
            .parameters()
            .get("marker_assignments"),
        Some(&ParameterValue::List(vec![
            object_value(0, &[]),
            object_value(1, &[0]),
            object_value(2, &[1]),
        ]))
    );

    let manifest_json = serde_json::to_vec(first.storyboard().manifest()).unwrap();
    let decoded: ArtifactManifest<ArtifactId, FrameId, MarkerId, GapId> =
        serde_json::from_slice(&manifest_json).unwrap();
    assert_eq!(&decoded, first.storyboard().manifest());

    let (dimensions, pixels) = decode_rgb(bytes);
    assert_eq!(dimensions, first.storyboard().image().dimensions());
    // The source strip starts below the 52 px header. Panel centers retain the
    // selected black, white, and black source pixels after deterministic scaling.
    assert_eq!(rgb_at(&pixels, dimensions, 120, 112), [0, 0, 0]);
    assert_eq!(rgb_at(&pixels, dimensions, 360, 112), [255, 255, 255]);
    assert_eq!(rgb_at(&pixels, dimensions, 600, 112), [0, 0, 0]);
    // Header glyphs and the gap hatch prove visible semantic bands are present
    // without OCR or a full-image golden fixture.
    assert!(region_has_color(
        &pixels,
        dimensions,
        0,
        0,
        180,
        50,
        [244, 247, 250]
    ));
    assert!(region_has_color(
        &pixels,
        dimensions,
        470,
        170,
        20,
        90,
        [255, 196, 64]
    ));

    assert_eq!(
        first.storyboard().manifest().output_hash().to_string(),
        "b606148fe214fd4d68545e1ad3379299f427a8c75e1797f7f7dd34358b1d2417"
    );
}

#[test]
fn manifest_trace_is_required_and_kind_role_validated() {
    let (source, normalized) = fixture();
    let artifacts = generate_storyboard(
        ArtifactId("storyboard-trace".into()),
        Some(ArtifactId("orientation-trace".into())),
        &source,
        &normalized,
        request(
            StoryboardTileLimit::new(3).unwrap(),
            RenderLimits::default(),
        ),
    )
    .unwrap();

    let mut current_without_trace =
        serde_json::to_value(artifacts.storyboard().manifest()).unwrap();
    current_without_trace
        .as_object_mut()
        .unwrap()
        .remove("storyboard_selection");
    assert!(decode_manifest_value(current_without_trace.clone()).is_err());
    current_without_trace["algorithm"]["version"] = serde_json::json!("1.0.0");
    assert!(decode_manifest_value(current_without_trace).is_err());

    let mut wrong_kind = serde_json::to_value(artifacts.storyboard().manifest()).unwrap();
    wrong_kind["artifact_kind"] = serde_json::json!("difference_map");
    wrong_kind["algorithm"]["name"] = serde_json::json!("temporal-difference-map");
    wrong_kind["algorithm"]["version"] = serde_json::json!("v1");
    assert!(decode_manifest_value(wrong_kind).is_err());

    let mut wrong_source = serde_json::to_value(artifacts.storyboard().manifest()).unwrap();
    wrong_source["storyboard_selection"]["selected_frames"][0]["frame_id"] =
        serde_json::json!("not-a-source-frame");
    assert!(decode_manifest_value(wrong_source).is_err());

    let mut wrong_roles =
        serde_json::to_value(artifacts.orientation().unwrap().manifest()).unwrap();
    wrong_roles["selected_frame_ids"][0] = serde_json::json!("f0");
    assert!(decode_manifest_value(wrong_roles).is_err());
}

#[test]
fn orientation_falls_back_to_post_anchor_for_an_unchanged_sequence() {
    let dimensions = PixelDimensions::new(1, 1).unwrap();
    let source = FrameSequence::new(
        [0_u64, 10, 20]
            .into_iter()
            .enumerate()
            .map(|(index, timestamp)| {
                Frame::new(
                    FrameId(format!("s{index}")),
                    Timestamp::from_nanos(timestamp),
                    dimensions,
                    PixelFormat::Rgba8SrgbStraight,
                    rgba([0, 0, 0], [0, 0, 0])[..4].to_vec().into_boxed_slice(),
                )
                .unwrap()
            })
            .collect(),
        Vec::<Marker<MarkerId>>::new(),
        Vec::<DeclaredGap<GapId>>::new(),
        None,
        None,
    )
    .unwrap();
    let normalized = normalize_sequence(
        &source,
        NormalizationParameters::new(
            Rgb8::new(0, 0, 0),
            None,
            IntegerScale::IDENTITY,
            ProcessingLimits::default(),
        ),
    )
    .unwrap();
    let selection = select_storyboard_frames(
        &source,
        &normalized,
        Timestamp::from_nanos(10),
        StoryboardTileLimit::new(3).unwrap(),
        MeasurementParameters::new(0),
    )
    .unwrap();
    assert_eq!(
        (
            selection.before_index(),
            selection.during_index(),
            selection.after_index()
        ),
        (0, 2, 2)
    );
    assert_eq!(selection.visual_summary().first_change(), None);
    assert_eq!(selection.visual_summary().peak_baseline_change(), None);
    assert_eq!(
        selection.visual_summary().peak_adjacent_changed_area(),
        None
    );
}

#[test]
fn tiny_render_limits_reject_without_partial_artifacts() {
    let (source, normalized) = fixture();
    let defaults = RenderLimits::default();
    let limits = [
        RenderLimits::new(
            NonZeroU32::new(479).unwrap(),
            NonZeroU32::new(4096).unwrap(),
            NonZeroUsize::new(defaults.max_canvas_bytes()).unwrap(),
            NonZeroUsize::new(defaults.max_encoded_bytes()).unwrap(),
        ),
        RenderLimits::new(
            NonZeroU32::new(720).unwrap(),
            NonZeroU32::new(100).unwrap(),
            NonZeroUsize::new(defaults.max_canvas_bytes()).unwrap(),
            NonZeroUsize::new(defaults.max_encoded_bytes()).unwrap(),
        ),
        RenderLimits::new(
            NonZeroU32::new(720).unwrap(),
            NonZeroU32::new(4096).unwrap(),
            NonZeroUsize::new(1).unwrap(),
            NonZeroUsize::new(defaults.max_encoded_bytes()).unwrap(),
        ),
        RenderLimits::new(
            NonZeroU32::new(720).unwrap(),
            NonZeroU32::new(4096).unwrap(),
            NonZeroUsize::new(defaults.max_canvas_bytes()).unwrap(),
            NonZeroUsize::new(8).unwrap(),
        ),
    ];
    for limit in limits {
        assert_eq!(
            generate_storyboard(
                ArtifactId("failed".into()),
                None,
                &source,
                &normalized,
                request(StoryboardTileLimit::new(3).unwrap(), limit),
            )
            .unwrap_err()
            .code,
            ErrorCode::ResourceLimitExceeded
        );
    }
}

fn decode_manifest_value(
    value: serde_json::Value,
) -> serde_json::Result<ArtifactManifest<ArtifactId, FrameId, MarkerId, GapId>> {
    serde_json::from_slice(&serde_json::to_vec(&value).unwrap())
}

fn object_value(tile: u64, markers: &[u64]) -> ParameterValue {
    ParameterValue::Object(
        [
            (
                "marker_declaration_indices".into(),
                ParameterValue::List(
                    markers
                        .iter()
                        .copied()
                        .map(ParameterValue::Unsigned)
                        .collect(),
                ),
            ),
            ("tile_index".into(), ParameterValue::Unsigned(tile)),
        ]
        .into_iter()
        .collect(),
    )
}

fn decode_rgb(bytes: &[u8]) -> (PixelDimensions, Vec<u8>) {
    let decoder = Decoder::new(bytes);
    let mut reader = decoder.read_info().unwrap();
    let mut output = vec![0_u8; reader.output_buffer_size()];
    let info = reader.next_frame(&mut output).unwrap();
    assert_eq!(info.color_type, ColorType::Rgb);
    output.truncate(info.buffer_size());
    (
        PixelDimensions::new(info.width, info.height).unwrap(),
        output,
    )
}

fn rgb_at(pixels: &[u8], dimensions: PixelDimensions, x: u32, y: u32) -> [u8; 3] {
    let index = (y as usize * dimensions.width() as usize + x as usize) * 3;
    [pixels[index], pixels[index + 1], pixels[index + 2]]
}

#[allow(clippy::too_many_arguments)]
fn region_has_color(
    pixels: &[u8],
    dimensions: PixelDimensions,
    x: u32,
    y: u32,
    width: u32,
    height: u32,
    color: [u8; 3],
) -> bool {
    (y..y + height)
        .any(|row| (x..x + width).any(|column| rgb_at(pixels, dimensions, column, row) == color))
}
