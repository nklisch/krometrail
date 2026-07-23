//! Deterministic major-change focus extraction from typed storyboard traces.
//!
//! Focus times drive the one compact `TemporalContextRequest` the bundle issues
//! after artifact generation. This module reads only the typed
//! `StoryboardVisualSummary` and selected-frame reasons already attached to
//! available storyboard manifests. It never calls `measure_*` or `select_*` and
//! never parses free-form generator parameters.

use std::collections::BTreeSet;

use krometrail_core::{ArtifactOutcome, FrameId, MAX_FOCUS_TIMES, SessionTime};
use temporal_vision::{ArtifactKind, SelectionReason};

/// Policy rank for focus candidates. Lower ranks are kept first when the same
/// session time appears in multiple summaries or selected frames.
const RANK_FIRST_CHANGE: u8 = 0;
const RANK_PEAK_BASELINE: u8 = 1;
const RANK_PEAK_ADJACENT: u8 = 2;
const RANK_SELECTED_FRAME: u8 = 3;

/// The major-change selection reasons that elevate a selected frame to a focus
/// candidate. Pre/post-anchor, final, marker, gap, and coverage reasons do not
/// by themselves represent a major visual change.
const MAJOR_REASONS: &[SelectionReason] = &[
    SelectionReason::FirstChange,
    SelectionReason::PeakBaselineChange,
    SelectionReason::LocalChangePeak,
    SelectionReason::ChangedRegionTransition,
    SelectionReason::ChangeTrend,
    SelectionReason::InformationGain,
];

/// One focus candidate carrying its policy rank and deterministic tie-break key.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct FocusCandidate {
    rank: u8,
    session_nanos: u64,
    epoch_index: u32,
    frame_index: usize,
    frame_id: FrameId,
}

impl FocusCandidate {
    /// Ordered by policy rank, then epoch, then frame index, then frame ID, so
    /// deduplication and the 16-cap keep the highest-priority evidence.
    fn rank_key(&self) -> (u8, u32, usize, FrameId) {
        (self.rank, self.epoch_index, self.frame_index, self.frame_id)
    }
}

impl Ord for FocusCandidate {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.rank_key().cmp(&other.rank_key())
    }
}

impl PartialOrd for FocusCandidate {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

/// Extracts at most `MAX_FOCUS_TIMES` focus times from available storyboard
/// manifests.
///
/// Candidate order: every available storyboard's first-change summary, then
/// peak-baseline summary, then peak-adjacent-area summary, then selected frames
/// carrying any major-change reason. Candidates are deduplicated by exact
/// `SessionTime` (keeping the earliest policy rank; ties break by epoch, frame
/// index, and `FrameId`), capped at 16, and finally sorted chronologically.
///
/// Only `ArtifactKind::Storyboard` manifests are read; orientation
/// (`BeforeDuringAfter`) duplicates are skipped, and difference-map pixels are
/// never consulted. Unavailable storyboard outcomes and manifests without a
/// trace contribute no candidates. If every storyboard outcome is unavailable or
/// unchanged, the result is an empty focus list.
pub(crate) fn extract_focus_times(outcomes: &[ArtifactOutcome]) -> Vec<SessionTime> {
    let mut candidates = BTreeSet::new();

    // Add both candidate families while reading each storyboard selection once.
    for outcome in outcomes {
        let ArtifactOutcome::Available {
            epoch_index,
            artifact,
            ..
        } = outcome
        else {
            continue;
        };
        if artifact.manifest.artifact_kind() != ArtifactKind::Storyboard {
            continue;
        }
        let Some(selection) = artifact.manifest.storyboard_selection() else {
            continue;
        };
        let epoch_index = *epoch_index;

        // Ranks 0..=2: the three visual-summary moments.
        let summary = selection.visual_summary();
        push_moment(
            &mut candidates,
            summary.first_change(),
            RANK_FIRST_CHANGE,
            epoch_index,
        );
        push_moment(
            &mut candidates,
            summary.peak_baseline_change(),
            RANK_PEAK_BASELINE,
            epoch_index,
        );
        push_moment(
            &mut candidates,
            summary.peak_adjacent_changed_area(),
            RANK_PEAK_ADJACENT,
            epoch_index,
        );

        // Rank 3: selected frames carrying a major-change reason.
        for frame in selection.selected_frames() {
            let has_major = frame
                .reasons()
                .iter()
                .any(|reason| MAJOR_REASONS.contains(reason));
            if !has_major {
                continue;
            }
            candidates.insert(FocusCandidate {
                rank: RANK_SELECTED_FRAME,
                session_nanos: frame.timestamp().as_nanos(),
                epoch_index,
                frame_index: frame.frame_index(),
                frame_id: *frame.frame_id(),
            });
        }
    }

    // Deduplicate by session time, keeping the earliest policy rank. The
    // BTreeSet is ordered by rank_key, so the first occurrence of any session
    // time is the highest-priority candidate.
    let mut seen_times: BTreeSet<u64> = BTreeSet::new();
    let mut deduped: Vec<FocusCandidate> = Vec::new();
    for candidate in candidates {
        if seen_times.insert(candidate.session_nanos) {
            deduped.push(candidate);
        }
    }

    // Cap at MAX_FOCUS_TIMES (highest priority first).
    deduped.truncate(MAX_FOCUS_TIMES);

    // Sort chronologically for the compact context request.
    deduped.sort_by_key(|candidate| candidate.session_nanos);
    deduped
        .into_iter()
        .map(|candidate| SessionTime::from_nanos(candidate.session_nanos))
        .collect()
}

fn push_moment(
    candidates: &mut BTreeSet<FocusCandidate>,
    moment: Option<&temporal_vision::VisualChangeMoment<FrameId>>,
    rank: u8,
    epoch_index: u32,
) {
    if let Some(moment) = moment {
        candidates.insert(FocusCandidate {
            rank,
            session_nanos: moment.timestamp().as_nanos(),
            epoch_index,
            frame_index: moment.frame_index(),
            frame_id: *moment.frame_id(),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use krometrail_core::{
        ArtifactCacheDisposition, ArtifactGenerationResult, ArtifactHandle, ArtifactId,
        CaptureGapPolicy, DeviceScaleFactor, GapId, NonEmptyText, RangeResolutionOptions,
        ResolvedRange, RetentionPolicy, SessionId, SessionRange, TargetId, TemporalRangeAnchorKind,
        VisualEpoch,
    };
    use temporal_vision::{
        AlgorithmDescriptor, ArtifactManifest, EvidenceClass, Frame, FrameSequence, IntegerScale,
        MeasurementParameters, NormalizationParameters, NormalizedSequence, OutputHash, Parameters,
        PixelDimensions, PixelFormat, ProcessingLimits, Rgb8, StoryboardTileLimit, Timestamp,
        normalize_sequence, select_storyboard_frames,
    };
    use uuid::Uuid;

    type TestSequence = FrameSequence<FrameId, krometrail_core::ArtifactMarkerId, GapId, Box<[u8]>>;

    fn session() -> SessionId {
        SessionId::from_uuid(Uuid::from_u128(1))
    }
    fn target() -> TargetId {
        TargetId::from_uuid(Uuid::from_u128(2))
    }

    /// Builds a frame with the given pixel value at every pixel.
    fn frame(id: u128, timestamp: u64, value: u8) -> Frame<FrameId, Box<[u8]>> {
        let dimensions = PixelDimensions::new(1, 1).unwrap();
        Frame::new(
            FrameId::from_uuid(Uuid::from_u128(id)),
            Timestamp::from_nanos(timestamp),
            dimensions,
            PixelFormat::Rgba8SrgbStraight,
            vec![value, value, value, 255].into_boxed_slice(),
        )
        .unwrap()
    }

    /// Builds a one-epoch frame sequence with a baseline (black) frame at t=0
    /// and a changed frame at `change_time`. Returns the sequence and its
    /// normalized form.
    fn changed_sequence(
        change_time: u64,
        change_value: u8,
    ) -> (TestSequence, NormalizedSequence<FrameId>) {
        let frames = vec![frame(100, 0, 0), frame(101, change_time, change_value)];
        let source = FrameSequence::new(frames, vec![], vec![], None, None).unwrap();
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

    /// Builds an unchanged sequence (two identical black frames).
    fn unchanged_sequence() -> (TestSequence, NormalizedSequence<FrameId>) {
        let frames = vec![frame(100, 0, 0), frame(101, 100, 0)];
        let source = FrameSequence::new(frames, vec![], vec![], None, None).unwrap();
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

    /// Produces a storyboard manifest from the given sequence with a real
    /// selection computed by `select_storyboard_frames`.
    fn storyboard_manifest(
        artifact_id: u128,
        sequence: &TestSequence,
        normalized: &NormalizedSequence<FrameId>,
        anchor: Timestamp,
    ) -> ArtifactManifest<ArtifactId, FrameId, krometrail_core::ArtifactMarkerId, GapId> {
        let selection = select_storyboard_frames(
            sequence,
            normalized,
            anchor,
            StoryboardTileLimit::new(3).unwrap(),
            MeasurementParameters::new(0),
        )
        .unwrap();
        let dimensions = normalized.dimensions();
        ArtifactManifest::from_storyboard_sequence(
            ArtifactId::from_uuid(Uuid::from_u128(artifact_id)),
            ArtifactKind::Storyboard,
            EvidenceClass::SourceDerived,
            AlgorithmDescriptor::new("test-storyboard", "1.1.0").unwrap(),
            sequence,
            selection
                .selected_frames()
                .iter()
                .map(|frame| *frame.frame_id())
                .collect(),
            selection,
            vec![],
            Parameters::default(),
            dimensions,
            OutputHash::from_bytes([0_u8; 32]),
        )
        .unwrap()
    }

    fn storyboard_outcome(
        _artifact_id: u128,
        manifest: ArtifactManifest<ArtifactId, FrameId, krometrail_core::ArtifactMarkerId, GapId>,
    ) -> ArtifactOutcome {
        ArtifactOutcome::Available {
            epoch_index: 0,
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

    fn storyboard_outcome_with_epoch(
        _artifact_id: u128,
        manifest: ArtifactManifest<ArtifactId, FrameId, krometrail_core::ArtifactMarkerId, GapId>,
        epoch_index: u32,
    ) -> ArtifactOutcome {
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

    fn difference_map_outcome(epoch_index: u32) -> ArtifactOutcome {
        ArtifactOutcome::Unavailable {
            epoch_index,
            generator_index: 1,
            artifact_kind: ArtifactKind::DifferenceMap,
            error: krometrail_core::KrometrailError::new(
                krometrail_core::ErrorCode::ArtifactGenerationFailed,
                NonEmptyText::new("fixture difference map unavailable").unwrap(),
            ),
        }
    }

    fn unavailable_storyboard_outcome(epoch_index: u32) -> ArtifactOutcome {
        ArtifactOutcome::Unavailable {
            epoch_index,
            generator_index: 0,
            artifact_kind: ArtifactKind::Storyboard,
            error: krometrail_core::KrometrailError::new(
                krometrail_core::ErrorCode::ArtifactGenerationFailed,
                NonEmptyText::new("storyboard unavailable").unwrap(),
            ),
        }
    }

    #[test]
    fn empty_unavailable_or_unchanged_storyboards_produce_no_focus_times() {
        // No outcomes.
        assert!(extract_focus_times(&[]).is_empty());

        // Unavailable storyboard + unavailable difference map.
        let outcomes = vec![unavailable_storyboard_outcome(0), difference_map_outcome(0)];
        assert!(extract_focus_times(&outcomes).is_empty());

        // Available storyboard with no measured change (identical frames).
        let (sequence, normalized) = unchanged_sequence();
        let manifest = storyboard_manifest(1, &sequence, &normalized, Timestamp::from_nanos(0));
        let outcomes = vec![storyboard_outcome(1, manifest)];
        let focus = extract_focus_times(&outcomes);
        assert!(
            focus.is_empty(),
            "unchanged storyboard must produce no focus times"
        );
    }

    #[test]
    fn changed_storyboard_produces_focus_times_from_summary_and_selected_frames() {
        let (sequence, normalized) = changed_sequence(100, 255);
        let manifest = storyboard_manifest(1, &sequence, &normalized, Timestamp::from_nanos(0));
        let outcomes = vec![storyboard_outcome(1, manifest)];
        let focus = extract_focus_times(&outcomes);
        // The change is at t=100; the selector assigns FirstChange and
        // PeakBaselineChange to that frame, so t=100 is a focus time.
        assert!(focus.contains(&SessionTime::from_nanos(100)));
        // Focus times are sorted and unique.
        assert!(focus.windows(2).all(|pair| pair[0] < pair[1]));
    }

    #[test]
    fn duplicate_session_times_deduplicate_keeping_earliest_rank() {
        // First-change moment and the selected frame carrying FirstChange are
        // at the same session time; they collapse to one focus time.
        let (sequence, normalized) = changed_sequence(200, 255);
        let manifest = storyboard_manifest(1, &sequence, &normalized, Timestamp::from_nanos(0));
        let outcomes = vec![storyboard_outcome(1, manifest)];
        let focus = extract_focus_times(&outcomes);
        let count_at_200 = focus.iter().filter(|time| time.as_nanos() == 200).count();
        assert_eq!(count_at_200, 1, "duplicate session time must deduplicate");
    }

    #[test]
    fn multi_epoch_ties_break_by_epoch_then_frame_index() {
        // Two epochs, each with a change at the same timestamp. The earliest
        // epoch wins the dedup tie-break.
        let (seq0, norm0) = changed_sequence(500, 64);
        let (seq1, norm1) = changed_sequence(500, 128);
        let manifest0 = storyboard_manifest(1, &seq0, &norm0, Timestamp::from_nanos(0));
        let manifest1 = storyboard_manifest(2, &seq1, &norm1, Timestamp::from_nanos(0));
        let outcomes = vec![
            storyboard_outcome_with_epoch(1, manifest0, 0),
            storyboard_outcome_with_epoch(2, manifest1, 1),
        ];
        let focus = extract_focus_times(&outcomes);
        // One focus time at t=500 (deduplicated across epochs).
        assert_eq!(focus, vec![SessionTime::from_nanos(500)]);
    }

    #[test]
    fn focus_cap_keeps_highest_priority_candidates() {
        // Build 20 storyboard outcomes, each with a change at a distinct time.
        let mut outcomes = Vec::new();
        for index in 0..20u32 {
            let (sequence, normalized) = changed_sequence(1000 + index as u64, 255);
            let manifest = storyboard_manifest(
                index as u128 + 1,
                &sequence,
                &normalized,
                Timestamp::from_nanos(0),
            );
            outcomes.push(storyboard_outcome_with_epoch(
                index as u128 + 1,
                manifest,
                index,
            ));
        }
        let focus = extract_focus_times(&outcomes);
        assert_eq!(focus.len(), MAX_FOCUS_TIMES);
        // Sorted chronologically.
        assert!(focus.windows(2).all(|pair| pair[0] < pair[1]));
        // The 16 earliest first-change times are kept (epochs 0..16).
        for (index, time) in focus.iter().enumerate() {
            assert_eq!(time.as_nanos(), 1000 + index as u64);
        }
    }

    #[test]
    fn difference_map_and_orientation_outcomes_are_not_read_as_focus_sources() {
        // A difference-map-only outcome set produces no focus. Orientation
        // (BeforeDuringAfter) manifests are not constructed here, but the
        // extractor filters by ArtifactKind::Storyboard only.
        let outcomes = vec![difference_map_outcome(0)];
        assert!(extract_focus_times(&outcomes).is_empty());
    }

    #[test]
    fn storyboard_manifest_cannot_be_constructed_without_a_trace_post_1_1_0() {
        // Documents the manifest contract: from_sequence (no trace) rejects
        // storyboard kinds. The extractor never sees a trace-less storyboard
        // manifest in practice; this test confirms the contract holds.
        let (sequence, _normalized) = unchanged_sequence();
        let result = ArtifactManifest::from_sequence(
            ArtifactId::from_uuid(Uuid::from_u128(99)),
            ArtifactKind::Storyboard,
            EvidenceClass::SourceDerived,
            AlgorithmDescriptor::new("test", "1.1.0").unwrap(),
            &sequence,
            vec![FrameId::from_uuid(Uuid::from_u128(100))],
            vec![],
            Parameters::default(),
            PixelDimensions::new(1, 1).unwrap(),
            OutputHash::from_bytes([0_u8; 32]),
        );
        assert!(
            result.is_err(),
            "storyboard manifests cannot be constructed without a trace post-1.1.0"
        );
    }

    #[test]
    fn extract_focus_times_accepts_full_result_outcomes() {
        let (sequence, normalized) = changed_sequence(700, 200);
        let manifest = storyboard_manifest(1, &sequence, &normalized, Timestamp::from_nanos(0));
        let range = ResolvedRange::new(
            session(),
            target(),
            TemporalRangeAnchorKind::SessionTime,
            SessionRange::new(SessionTime::ZERO, SessionTime::from_nanos(1_000)).unwrap(),
            SessionRange::new(SessionTime::ZERO, SessionTime::from_nanos(1_000)).unwrap(),
            vec![FrameId::from_uuid(Uuid::from_u128(100))],
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
            RangeResolutionOptions {
                retention: RetentionPolicy::AllowPartial,
                capture_gaps: CaptureGapPolicy::Include,
                ..RangeResolutionOptions::DEFAULT
            },
        )
        .unwrap();
        let result = ArtifactGenerationResult {
            range,
            epochs: vec![VisualEpoch {
                index: 0,
                frame_ids: vec![FrameId::from_uuid(Uuid::from_u128(100))],
                image: krometrail_core::PixelDimensions::new(1, 1).unwrap(),
                viewport: krometrail_core::PixelDimensions::new(1, 1).unwrap(),
                device_scale_factor: DeviceScaleFactor::new(1.0).unwrap(),
            }],
            outcomes: vec![storyboard_outcome(1, manifest)],
            artifact_grace_overridden: false,
        };
        let focus = extract_focus_times(&result.outcomes);
        assert!(focus.contains(&SessionTime::from_nanos(700)));
    }
}
