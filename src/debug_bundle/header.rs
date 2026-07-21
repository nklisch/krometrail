//! Non-diagnostic header composition for the temporal debug bundle.
//!
//! The header carries a bounded text summary, the fixed observation/proximity
//! posture, and per-epoch visual summaries extracted from available storyboard
//! manifests. Summary language is restricted to observed/measured/selected/
//! co-occurred/nearest-by-session-time vocabulary and always states that
//! measurements and proximity do not establish diagnosis or causality.

use krometrail_core::{
    ArtifactOutcome, BundleEpochVisualSummary, MAX_BUNDLE_HEADER_BYTES, NonEmptyText,
    ResolvedRange, Result, TemporalDebugHeader,
};
use temporal_vision::ArtifactKind;

/// Typed visual evidence state observed for the resolved range.
///
/// Replacing a boolean `has_focus` flag with this enum lets the header describe
/// the three genuinely different visual outcomes: a measured change, a measured
/// no-change, and the absence of any usable storyboard trace. Without this
/// distinction an unavailable storyboard would be reported as "no visual
/// change", which asserts a measurement the bundle never made.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum VisualEvidenceState {
    /// A storyboard trace was available and thresholded visual change was measured.
    MeasuredChange,
    /// A storyboard trace was available but no thresholded change was measured.
    MeasuredNoChange,
    /// No storyboard trace was available for the resolved range.
    Unavailable,
}

/// Typed browser-event evidence state observed for the resolved range.
///
/// The header may only assert co-occurrence between browser events and visual
/// moments when context is available and selected events exist. When context is
/// unavailable, no co-occurrence is asserted.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum BrowserEventEvidenceState {
    /// Context was available; carries the compact selected event count.
    Available { selected: u64 },
    /// Context was unavailable.
    Unavailable,
}

/// Composes the deterministic header from the resolved range, available
/// artifact outcomes, and the typed visual/browser-event evidence state.
///
/// `visual` distinguishes measured change, measured no-change, and unavailable
/// storyboard evidence; `browser_events` distinguishes available (with selected
/// count) from unavailable context. The summary asserts only what evidence
/// supports and never claims co-occurrence when context is unavailable or empty.
pub(crate) fn compose_header(
    range: &ResolvedRange,
    outcomes: &[ArtifactOutcome],
    visual: VisualEvidenceState,
    browser_events: BrowserEventEvidenceState,
) -> Result<TemporalDebugHeader> {
    let visual_summaries = extract_epoch_summaries(outcomes);
    let summary = compose_summary(range, visual, browser_events)?;
    TemporalDebugHeader::new(summary, visual_summaries)
}

/// Extracts per-epoch visual summaries from available storyboard manifests.
///
/// Only `ArtifactKind::Storyboard` manifests are read; orientation duplicates
/// and difference-map outcomes are skipped. The result is sorted by epoch index
/// and deduplicated, satisfying the header's unique-ordered-epoch invariant.
fn extract_epoch_summaries(outcomes: &[ArtifactOutcome]) -> Vec<BundleEpochVisualSummary> {
    let mut summaries: Vec<BundleEpochVisualSummary> = outcomes
        .iter()
        .filter_map(|outcome| match outcome {
            ArtifactOutcome::Available {
                epoch_index,
                artifact,
                ..
            } if artifact.manifest.artifact_kind() == ArtifactKind::Storyboard => artifact
                .manifest
                .storyboard_selection()
                .map(|selection| BundleEpochVisualSummary {
                    epoch_index: *epoch_index,
                    summary: selection.visual_summary().clone(),
                }),
            _ => None,
        })
        .collect();
    summaries.sort_by_key(|summary| summary.epoch_index);
    summaries.dedup_by_key(|summary| summary.epoch_index);
    summaries
}

fn compose_summary(
    range: &ResolvedRange,
    visual: VisualEvidenceState,
    browser_events: BrowserEventEvidenceState,
) -> Result<NonEmptyText> {
    let frame_count = range.frame_ids.len();
    let start = range.resolved_range.start().as_nanos();
    let end = range.resolved_range.end().as_nanos();
    let change = match visual {
        VisualEvidenceState::MeasuredChange => {
            "Visual changes measured at selected storyboard moments."
        }
        VisualEvidenceState::MeasuredNoChange => {
            "No thresholded visual change was measured in retained comparable frames."
        }
        VisualEvidenceState::Unavailable => {
            "Visual change was not measured; storyboard evidence was unavailable."
        }
    };
    let events_clause = match browser_events {
        BrowserEventEvidenceState::Available { selected: 0 } => {
            "No browser events matched the resolved range."
        }
        BrowserEventEvidenceState::Available { .. } => {
            "Browser events co-occurred nearest by session-time distance."
        }
        BrowserEventEvidenceState::Unavailable => {
            "Browser events were unavailable; no co-occurrence is asserted."
        }
    };
    let text = format!(
        "Observed target {} from {} to {} ns. {} source frames retained. {} \
         {} \
         Measurements and proximity do not establish diagnosis or causality.",
        range.target_id, start, end, frame_count, change, events_clause
    );
    if text.len() > MAX_BUNDLE_HEADER_BYTES {
        return Err(krometrail_core::KrometrailError::new(
            krometrail_core::ErrorCode::InvalidInput,
            NonEmptyText::new("temporal debug header exceeds 512 UTF-8 bytes")
                .expect("static header error is non-empty"),
        ));
    }
    NonEmptyText::new(text).map_err(|_| {
        krometrail_core::KrometrailError::new(
            krometrail_core::ErrorCode::InvalidInput,
            NonEmptyText::new("temporal debug header summary must not be empty")
                .expect("static header error is non-empty"),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use krometrail_core::{
        ArtifactCacheDisposition, ArtifactHandle, ArtifactId, CaptureGapPolicy, FrameId,
        RangeResolutionOptions, ResolvedRange, RetentionPolicy, SessionId, SessionRange,
        SessionTime, TargetId, TemporalRangeAnchorKind,
    };
    use temporal_vision::{
        AlgorithmDescriptor, ArtifactManifest, EvidenceClass, Frame, FrameSequence, IntegerScale,
        MeasurementParameters, NormalizationParameters, OutputHash, Parameters, PixelDimensions,
        PixelFormat, ProcessingLimits, Rgb8, StoryboardTileLimit, Timestamp, normalize_sequence,
        select_storyboard_frames,
    };
    use uuid::Uuid;

    fn resolved_range() -> ResolvedRange {
        let session = SessionId::from_uuid(Uuid::from_u128(1));
        let target = TargetId::from_uuid(Uuid::from_u128(2));
        let range = SessionRange::new(
            SessionTime::from_nanos(0),
            SessionTime::from_nanos(1_000_000),
        )
        .unwrap();
        ResolvedRange::new(
            session,
            target,
            TemporalRangeAnchorKind::SessionTime,
            range,
            range,
            vec![FrameId::from_uuid(Uuid::from_u128(3))],
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

    fn changed_outcome(epoch_index: u32, change_time: u64) -> ArtifactOutcome {
        let dimensions = PixelDimensions::new(1, 1).unwrap();
        let frame0 = Frame::new(
            FrameId::from_uuid(Uuid::from_u128(100 + epoch_index as u128 * 2)),
            Timestamp::from_nanos(0),
            dimensions,
            PixelFormat::Rgba8SrgbStraight,
            vec![0_u8, 0, 0, 255].into_boxed_slice(),
        )
        .unwrap();
        let frame1 = Frame::new(
            FrameId::from_uuid(Uuid::from_u128(101 + epoch_index as u128 * 2)),
            Timestamp::from_nanos(change_time),
            dimensions,
            PixelFormat::Rgba8SrgbStraight,
            vec![255_u8, 255, 255, 255].into_boxed_slice(),
        )
        .unwrap();
        let source = FrameSequence::new(vec![frame0, frame1], vec![], vec![], None, None).unwrap();
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
            Timestamp::from_nanos(0),
            StoryboardTileLimit::new(3).unwrap(),
            MeasurementParameters::new(0),
        )
        .unwrap();
        let manifest = ArtifactManifest::from_storyboard_sequence(
            ArtifactId::from_uuid(Uuid::from_u128(epoch_index as u128 + 1)),
            ArtifactKind::Storyboard,
            EvidenceClass::SourceDerived,
            AlgorithmDescriptor::new("test-storyboard", "1.1.0").unwrap(),
            &source,
            selection
                .selected_frames()
                .iter()
                .map(|f| *f.frame_id())
                .collect(),
            selection,
            vec![],
            Parameters::default(),
            dimensions,
            OutputHash::from_bytes([0_u8; 32]),
        )
        .unwrap();
        ArtifactOutcome::Available {
            epoch_index,
            generator_index: 0,
            artifact: Box::new(ArtifactHandle {
                artifact_id: *manifest.artifact_id(),
                cache: ArtifactCacheDisposition::Generated,
                media_type: NonEmptyText::new("image/png").unwrap(),
                encoded_byte_len: 1,
                manifest,
            }),
        }
    }

    fn unavailable_outcome(epoch_index: u32) -> ArtifactOutcome {
        ArtifactOutcome::Unavailable {
            epoch_index,
            generator_index: 0,
            artifact_kind: ArtifactKind::Storyboard,
            error: krometrail_core::KrometrailError::new(
                krometrail_core::ErrorCode::ArtifactGenerationFailed,
                NonEmptyText::new("fixture storyboard unavailable").unwrap(),
            ),
        }
    }

    #[test]
    fn header_with_focus_uses_only_approved_language() {
        let range = resolved_range();
        let outcomes = vec![changed_outcome(0, 100)];
        let header = compose_header(
            &range,
            &outcomes,
            VisualEvidenceState::MeasuredChange,
            BrowserEventEvidenceState::Available { selected: 3 },
        )
        .unwrap();
        assert_eq!(
            header.posture,
            krometrail_core::EvidencePosture::ObservedChangeAndTemporalProximityOnly
        );
        let summary = header.summary.as_str();
        for forbidden in [
            "caused",
            "triggered",
            "diagnosed",
            "fixed",
            "smooth",
            "flicker",
            "reversal",
            "stable",
        ] {
            assert!(
                !summary.to_lowercase().contains(forbidden),
                "header summary leaked forbidden term: {forbidden}"
            );
        }
        assert!(summary.contains("Observed"));
        assert!(summary.contains("measured"));
        assert!(summary.contains("proximity"));
        assert!(summary.contains("do not establish"));
        assert!(summary.len() <= MAX_BUNDLE_HEADER_BYTES);
    }

    #[test]
    fn header_measured_no_change_states_no_thresholded_change() {
        let range = resolved_range();
        let outcomes: Vec<ArtifactOutcome> = vec![];
        let header = compose_header(
            &range,
            &outcomes,
            VisualEvidenceState::MeasuredNoChange,
            BrowserEventEvidenceState::Available { selected: 0 },
        )
        .unwrap();
        assert!(
            header
                .summary
                .as_str()
                .contains("No thresholded visual change")
        );
        // Available context with no events does not assert co-occurrence.
        assert!(
            header
                .summary
                .as_str()
                .contains("No browser events matched the resolved range")
        );
    }

    #[test]
    fn header_unavailable_storyboard_does_not_claim_no_change() {
        let range = resolved_range();
        let outcomes: Vec<ArtifactOutcome> = vec![unavailable_outcome(0)];
        let header = compose_header(
            &range,
            &outcomes,
            VisualEvidenceState::Unavailable,
            BrowserEventEvidenceState::Unavailable,
        )
        .unwrap();
        let summary = header.summary.as_str();
        // Never claims "no thresholded visual change" when no storyboard trace
        // was available — that would assert a measurement the bundle never made.
        assert!(!summary.contains("No thresholded visual change"));
        assert!(summary.contains("storyboard evidence was unavailable"));
        // No browser-event co-occurrence is asserted when context is unavailable.
        assert!(!summary.contains("Browser events co-occurred"));
        assert!(summary.contains("Browser events were unavailable"));
    }

    #[test]
    fn header_extracts_epoch_summaries_from_available_storyboards_only() {
        let range = resolved_range();
        let outcomes = vec![
            changed_outcome(0, 100),
            changed_outcome(1, 200),
            unavailable_outcome(2),
        ];
        let header = compose_header(
            &range,
            &outcomes,
            VisualEvidenceState::MeasuredChange,
            BrowserEventEvidenceState::Available { selected: 1 },
        )
        .unwrap();
        assert_eq!(header.visual_summaries.len(), 2);
        assert_eq!(header.visual_summaries[0].epoch_index, 0);
        assert_eq!(header.visual_summaries[1].epoch_index, 1);
    }
}
