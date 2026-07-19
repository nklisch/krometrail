//! The temporal debug bundle's concrete default artifact policy.
//!
//! This module produces only the existing
//! storyboard + difference-map generator requests; it adds no generator family,
//! parameter, or output limit.

use krometrail_core::{
    AnalysisScale, ArtifactFailurePolicy, ArtifactGenerationRequest, ArtifactGeneratorRequest,
    ArtifactLabelsRequest, ArtifactMarker, DEFAULT_ARTIFACT_BLACK_BACKGROUND,
    DEFAULT_ARTIFACT_NOISE_FLOOR, DEFAULT_ARTIFACT_TILE_LIMIT, DEFAULT_DIFFERENCE_MAP_MAX_BYTES,
    DEFAULT_DIFFERENCE_MAP_MAX_HEIGHT, DEFAULT_DIFFERENCE_MAP_MAX_WIDTH,
    DEFAULT_STORYBOARD_MAX_BYTES, DEFAULT_STORYBOARD_MAX_HEIGHT, DEFAULT_STORYBOARD_MAX_WIDTH,
    DifferenceMapRequest, FrameSelector, NonEmptyText, NormalizationRequest, OrientationPolicy,
    OutputLimitsRequest, ResolvedRange, Result, StoryboardRequest,
};
use temporal_vision::{FrequencyMode, Rgb8};

/// Storyboard tile budget for the default evidence policy.
pub(crate) const STORYBOARD_TILE_LIMIT: u8 = DEFAULT_ARTIFACT_TILE_LIMIT;

/// Noise floor shared by storyboard and difference-map generators.
/// Matches `temporal_vision::MeasurementParameters::DEFAULT_NOISE_FLOOR` (512)
/// without importing the measurement API into the bundle policy.
pub(crate) const DEFAULT_NOISE_FLOOR: u16 = DEFAULT_ARTIFACT_NOISE_FLOOR;

/// Storyboard output ceiling: `1920 × 2048`, `16 MiB`.
const STORYBOARD_MAX_WIDTH: u32 = DEFAULT_STORYBOARD_MAX_WIDTH;
const STORYBOARD_MAX_HEIGHT: u32 = DEFAULT_STORYBOARD_MAX_HEIGHT;
const STORYBOARD_MAX_BYTES: u64 = DEFAULT_STORYBOARD_MAX_BYTES;

/// Difference-map output ceiling: `8192 × 8192`, `64 MiB`.
const DIFFERENCE_MAP_MAX_WIDTH: u32 = DEFAULT_DIFFERENCE_MAP_MAX_WIDTH;
const DIFFERENCE_MAP_MAX_HEIGHT: u32 = DEFAULT_DIFFERENCE_MAP_MAX_HEIGHT;
const DIFFERENCE_MAP_MAX_BYTES: u64 = DEFAULT_DIFFERENCE_MAP_MAX_BYTES;

const STORYBOARD_TITLE: &str = "TEMPORAL STORYBOARD";
const STORYBOARD_SOURCE: &str = "KROMETRAIL RETAINED SOURCE FRAMES";

/// Declared black RGB background shared by storyboard and difference-map normalization.
const BLACK_BACKGROUND: Rgb8 = DEFAULT_ARTIFACT_BLACK_BACKGROUND;

/// Materializes the exact two-generator request for the default policy.
///
/// The storyboard uses the resolved effective anchor, eight tiles, the default
/// noise floor, `FitLimits` normalization with no crop and a black background,
/// and the declared `1920 × 2048` / `16 MiB` output ceiling. Orientation is
/// included unless the caller explicitly omits it.
///
/// The difference map uses epoch-local `FrameSelector::First`, normalized
/// frequency, the default spectral palette through the existing generator, no
/// explicit repeated-change separation, the same noise floor and normalization,
/// a black canvas, and the `8192 × 8192` / `64 MiB` output ceiling.
///
/// No motion history, filmstrip, region, comparison, or inferred output is
/// requested. The failure policy is `AllowPartial` so per-epoch failures produce
/// usable degraded bundles rather than whole-request failure.
pub(crate) fn default_artifact_request(
    range: &ResolvedRange,
    markers: &[ArtifactMarker],
    orientation: OrientationPolicy,
) -> Result<ArtifactGenerationRequest> {
    ArtifactGenerationRequest::new(
        range.clone(),
        markers.to_vec(),
        default_generators(range, orientation),
        ArtifactFailurePolicy::AllowPartial,
    )
}

/// Returns the exact generator list for the current default policy.
///
/// Exposed separately so the effective policy can carry the same generator
/// values the artifact service receives, without duplicating the request body.
pub(crate) fn default_generators(
    range: &ResolvedRange,
    orientation: OrientationPolicy,
) -> Vec<ArtifactGeneratorRequest> {
    vec![
        ArtifactGeneratorRequest::Storyboard(storyboard_request(range, orientation)),
        ArtifactGeneratorRequest::DifferenceMap(difference_map_request()),
    ]
}

fn storyboard_request(range: &ResolvedRange, orientation: OrientationPolicy) -> StoryboardRequest {
    StoryboardRequest {
        anchor: range.resolved_anchor.effective_time,
        tile_limit: STORYBOARD_TILE_LIMIT,
        noise_floor: DEFAULT_NOISE_FLOOR,
        normalization: NormalizationRequest::new(None, BLACK_BACKGROUND, AnalysisScale::FitLimits)
            .expect("default storyboard normalization is valid"),
        labels: ArtifactLabelsRequest::new(
            NonEmptyText::new(STORYBOARD_TITLE).expect("storyboard title is non-empty"),
            NonEmptyText::new(STORYBOARD_SOURCE).expect("storyboard source label is non-empty"),
        ),
        include_orientation: orientation == OrientationPolicy::Include,
        output: OutputLimitsRequest::new(
            STORYBOARD_MAX_WIDTH,
            STORYBOARD_MAX_HEIGHT,
            STORYBOARD_MAX_BYTES,
        )
        .expect("default storyboard output limits are valid"),
    }
}

fn difference_map_request() -> DifferenceMapRequest {
    DifferenceMapRequest {
        reference: FrameSelector::First,
        frequency_mode: FrequencyMode::NormalizedFrequency,
        repeated_change_separation_nanos: None,
        noise_floor: DEFAULT_NOISE_FLOOR,
        normalization: NormalizationRequest::new(None, BLACK_BACKGROUND, AnalysisScale::FitLimits)
            .expect("default difference-map normalization is valid"),
        canvas_background: BLACK_BACKGROUND,
        output: OutputLimitsRequest::new(
            DIFFERENCE_MAP_MAX_WIDTH,
            DIFFERENCE_MAP_MAX_HEIGHT,
            DIFFERENCE_MAP_MAX_BYTES,
        )
        .expect("default difference-map output limits are valid"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use krometrail_core::{
        CaptureGapPolicy, InteractionId, RangeResolutionOptions, RetentionPolicy, SessionId,
        SessionRange, SessionTime, TargetId, TemporalRangeAnchorKind,
    };
    use uuid::Uuid;

    fn resolved_range() -> ResolvedRange {
        let session = SessionId::from_uuid(Uuid::from_u128(1));
        let target = TargetId::from_uuid(Uuid::from_u128(2));
        let range =
            SessionRange::new(SessionTime::ZERO, SessionTime::from_nanos(1_000_000)).unwrap();
        ResolvedRange::new(
            session,
            target,
            TemporalRangeAnchorKind::SessionTime,
            range,
            range,
            vec![krometrail_core::FrameId::from_uuid(Uuid::from_u128(3))],
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            RangeResolutionOptions {
                retention: RetentionPolicy::AllowPartial,
                capture_gaps: CaptureGapPolicy::Include,
                ..RangeResolutionOptions::DEFAULT
            },
        )
        .unwrap()
    }

    fn interaction_anchor() -> ResolvedRange {
        let session = SessionId::from_uuid(Uuid::from_u128(1));
        let target = TargetId::from_uuid(Uuid::from_u128(2));
        let interaction_id = InteractionId::from_uuid(Uuid::from_u128(7));
        let _anchor = krometrail_core::InteractionAnchor::new(
            interaction_id,
            session,
            target,
            krometrail_core::BrowserOperationKind::Click,
            krometrail_core::InteractionTiming::new(
                SessionTime::from_nanos(100),
                SessionTime::from_nanos(200),
                SessionTime::from_nanos(300),
                Some(SessionTime::from_nanos(300)),
            )
            .unwrap(),
        )
        .unwrap();
        ResolvedRange::new_with_anchor(
            session,
            target,
            TemporalRangeAnchorKind::Interaction,
            krometrail_core::ResolvedAnchor::new(
                krometrail_core::ResolvedAnchorReference::Interaction { interaction_id },
                SessionTime::from_nanos(200),
                SessionTime::from_nanos(200),
            )
            .unwrap(),
            SessionRange::new(SessionTime::ZERO, SessionTime::from_nanos(1_000_000)).unwrap(),
            SessionRange::new(SessionTime::ZERO, SessionTime::from_nanos(1_000_000)).unwrap(),
            vec![krometrail_core::FrameId::from_uuid(Uuid::from_u128(3))],
            vec![interaction_id],
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            RangeResolutionOptions {
                retention: RetentionPolicy::AllowPartial,
                capture_gaps: CaptureGapPolicy::Include,
                ..RangeResolutionOptions::DEFAULT
            },
        )
        .unwrap()
    }

    #[test]
    fn default_generators_are_byte_stable_with_orientation_on_and_off() {
        let range = resolved_range();
        let include = default_generators(&range, OrientationPolicy::Include);
        let omit = default_generators(&range, OrientationPolicy::Omit);
        let include_encoded = serde_json::to_vec(&include).unwrap();
        let omit_encoded = serde_json::to_vec(&omit).unwrap();
        // Re-running the same policy produces byte-identical output.
        assert_eq!(
            include_encoded,
            serde_json::to_vec(&default_generators(&range, OrientationPolicy::Include)).unwrap()
        );
        assert_eq!(
            omit_encoded,
            serde_json::to_vec(&default_generators(&range, OrientationPolicy::Omit)).unwrap()
        );
        // Orientation off changes exactly one field and nothing else.
        let mut include_value: serde_json::Value =
            serde_json::from_slice(&include_encoded).unwrap();
        let mut omit_value: serde_json::Value = serde_json::from_slice(&omit_encoded).unwrap();
        assert_eq!(
            include_value[0]["generator"], "storyboard",
            "storyboard remains the first generator"
        );
        assert_eq!(
            include_value[1]["generator"], "difference_map",
            "difference map remains the second generator"
        );
        assert_eq!(include_value[0]["include_orientation"], true);
        assert_eq!(omit_value[0]["include_orientation"], false);
        include_value[0]["include_orientation"] = serde_json::Value::Null;
        omit_value[0]["include_orientation"] = serde_json::Value::Null;
        assert_eq!(include_value, omit_value);
    }

    #[test]
    fn default_generators_contain_only_storyboard_and_difference_map_policy() {
        let range = resolved_range();
        let generators = default_generators(&range, OrientationPolicy::Include);
        assert_eq!(generators.len(), 2, "exactly two generators");
        assert!(matches!(
            generators[0],
            ArtifactGeneratorRequest::Storyboard(_)
        ));
        assert!(matches!(
            generators[1],
            ArtifactGeneratorRequest::DifferenceMap(_)
        ));
        let request = default_artifact_request(&range, &[], OrientationPolicy::Include).unwrap();
        assert_eq!(
            request.failure_policy(),
            ArtifactFailurePolicy::AllowPartial
        );
        assert_eq!(request.range(), &range);
        assert_eq!(request.generators().len(), 2);
    }

    #[test]
    fn default_generator_values_match_the_designed_policy() {
        let range = interaction_anchor();
        let generators = default_generators(&range, OrientationPolicy::Include);
        let ArtifactGeneratorRequest::Storyboard(storyboard) = &generators[0] else {
            panic!("first generator is the storyboard");
        };
        assert_eq!(storyboard.anchor, range.resolved_anchor.effective_time);
        assert_eq!(storyboard.tile_limit, 8);
        assert_eq!(storyboard.noise_floor, 512);
        assert_eq!(storyboard.normalization.scale, AnalysisScale::FitLimits);
        assert!(storyboard.normalization.crop.is_none());
        assert_eq!(storyboard.normalization.background, BLACK_BACKGROUND);
        assert_eq!(storyboard.labels.title.as_str(), "TEMPORAL STORYBOARD");
        assert_eq!(
            storyboard.labels.source.as_str(),
            "KROMETRAIL RETAINED SOURCE FRAMES"
        );
        assert!(storyboard.include_orientation);
        assert_eq!(storyboard.output.max_width(), 1920);
        assert_eq!(storyboard.output.max_height(), 2048);
        assert_eq!(storyboard.output.max_encoded_bytes(), 16 * 1024 * 1024);

        let ArtifactGeneratorRequest::DifferenceMap(difference_map) = &generators[1] else {
            panic!("second generator is the difference map");
        };
        assert_eq!(difference_map.reference, FrameSelector::First);
        assert_eq!(
            difference_map.frequency_mode,
            FrequencyMode::NormalizedFrequency
        );
        assert!(difference_map.repeated_change_separation_nanos.is_none());
        assert_eq!(difference_map.noise_floor, 512);
        assert_eq!(difference_map.normalization.scale, AnalysisScale::FitLimits);
        assert!(difference_map.normalization.crop.is_none());
        assert_eq!(difference_map.normalization.background, BLACK_BACKGROUND);
        assert_eq!(difference_map.canvas_background, BLACK_BACKGROUND);
        assert_eq!(difference_map.output.max_width(), 8192);
        assert_eq!(difference_map.output.max_height(), 8192);
        assert_eq!(difference_map.output.max_encoded_bytes(), 64 * 1024 * 1024);
    }
}
