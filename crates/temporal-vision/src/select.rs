use std::cmp::Ordering;

use serde::Serialize;

use crate::{
    ComparisonOutcome, ErrorCode, FrameSequence, MeasurementParameters, MeasurementVector,
    NormalizedSequence, Result, Timestamp, VisionError, measure_adjacent, measure_pair,
};

const MIN_TILES: u8 = 3;
const MAX_TILES: u8 = 12;

stable_registry! {
    /// Stable descriptive reason why a source frame appears in a storyboard.
    pub enum SelectionReason {
        PreAnchor => "pre_anchor",
        PostAnchor => "post_anchor",
        FirstChange => "first_change",
        PeakBaselineChange => "peak_baseline_change",
        FinalFrame => "final_frame",
        MarkerBoundary => "marker_boundary",
        GapBoundary => "gap_boundary",
        LocalChangePeak => "local_change_peak",
        ChangeTrend => "change_trend",
        ChangedRegionTransition => "changed_region_transition",
        InformationGain => "information_gain",
        TemporalCoverage => "temporal_coverage",
    }
}

/// Hard maximum number of storyboard source panels.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub struct StoryboardTileLimit(u8);

impl StoryboardTileLimit {
    pub const DEFAULT: Self = Self(8);

    pub fn new(value: u8) -> Result<Self> {
        if !(MIN_TILES..=MAX_TILES).contains(&value) {
            return Err(VisionError::new(
                ErrorCode::InvalidParameter,
                "storyboard tile limit must be between three and twelve",
            ));
        }
        Ok(Self(value))
    }

    pub const fn get(self) -> u8 {
        self.0
    }
}

impl Default for StoryboardTileLimit {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// One selected source frame and every unique reason assigned to it.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct SelectedFrame<FrameId> {
    frame_id: FrameId,
    frame_index: usize,
    timestamp: Timestamp,
    reasons: Box<[SelectionReason]>,
}

impl<F> SelectedFrame<F> {
    pub fn frame_id(&self) -> &F {
        &self.frame_id
    }

    pub const fn frame_index(&self) -> usize {
        self.frame_index
    }

    pub const fn timestamp(&self) -> Timestamp {
        self.timestamp
    }

    pub fn reasons(&self) -> &[SelectionReason] {
        &self.reasons
    }
}

/// An available required or boundary anchor displaced by the hard tile budget.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct OmittedAnchor {
    frame_index: usize,
    reason: SelectionReason,
}

impl OmittedAnchor {
    pub const fn frame_index(&self) -> usize {
        self.frame_index
    }

    pub const fn reason(&self) -> SelectionReason {
        self.reason
    }
}

/// Reusable deterministic source-frame plan shared by storyboard renderers.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct StoryboardSelection<FrameId> {
    selected_frames: Box<[SelectedFrame<FrameId>]>,
    omitted_anchors: Box<[OmittedAnchor]>,
    before_index: usize,
    during_index: usize,
    after_index: usize,
    continuity_segment_count: usize,
}

impl<F> StoryboardSelection<F> {
    pub fn selected_frames(&self) -> &[SelectedFrame<F>] {
        &self.selected_frames
    }

    pub fn omitted_anchors(&self) -> &[OmittedAnchor] {
        &self.omitted_anchors
    }

    /// Source-sequence index used by the orientation composite's BEFORE panel.
    pub const fn before_index(&self) -> usize {
        self.before_index
    }

    /// Source-sequence index used by the orientation composite's DURING panel.
    pub const fn during_index(&self) -> usize {
        self.during_index
    }

    /// Source-sequence index used by the orientation composite's AFTER panel.
    pub const fn after_index(&self) -> usize {
        self.after_index
    }

    pub const fn continuity_segment_count(&self) -> usize {
        self.continuity_segment_count
    }
}

/// Select representative source frames using the temporal-storyboard 1.0.0 rules.
pub fn select_storyboard_frames<F: Clone + Eq, M: Eq, G: Eq, P: AsRef<[u8]>>(
    source: &FrameSequence<F, M, G, P>,
    normalized: &NormalizedSequence<F>,
    anchor: Timestamp,
    tile_limit: StoryboardTileLimit,
    measurement: MeasurementParameters,
) -> Result<StoryboardSelection<F>> {
    validate_alignment(source, normalized)?;
    if !source.range().contains(anchor) {
        return Err(VisionError::new(
            ErrorCode::InvalidParameter,
            "storyboard anchor must lie inside the source frame range",
        ));
    }

    let frame_count = source.frames().len();
    let adjacent = measure_adjacent(normalized, measurement)?;
    let analysis = SelectionAnalysis::new(&adjacent, source, frame_count)?;
    let baseline = source
        .frames()
        .iter()
        .rposition(|frame| frame.timestamp() < anchor)
        .unwrap_or(0);
    let post_anchor = source
        .frames()
        .iter()
        .position(|frame| frame.timestamp() > anchor);
    let first_change = first_change_index(&adjacent, normalized, anchor);
    let peak = peak_baseline_index(normalized, baseline, &analysis, measurement)?;
    let final_frame = frame_count - 1;

    let mut selected = vec![false; frame_count];
    let mut reasons = vec![Vec::<SelectionReason>::new(); frame_count];
    let mut omitted = Vec::new();
    let limit = usize::from(tile_limit.get());

    // The priority makes a three-tile request preserve the orientation spine.
    admit_anchor(
        baseline,
        SelectionReason::PreAnchor,
        limit,
        &mut selected,
        &mut reasons,
        &mut omitted,
    );
    if let Some(index) = peak {
        admit_anchor(
            index,
            SelectionReason::PeakBaselineChange,
            limit,
            &mut selected,
            &mut reasons,
            &mut omitted,
        );
    }
    admit_anchor(
        final_frame,
        SelectionReason::FinalFrame,
        limit,
        &mut selected,
        &mut reasons,
        &mut omitted,
    );
    if let Some(index) = first_change {
        admit_anchor(
            index,
            SelectionReason::FirstChange,
            limit,
            &mut selected,
            &mut reasons,
            &mut omitted,
        );
    }
    if let Some(index) = post_anchor {
        admit_anchor(
            index,
            SelectionReason::PostAnchor,
            limit,
            &mut selected,
            &mut reasons,
            &mut omitted,
        );
    }

    let mut boundary_roles = vec![Vec::<SelectionReason>::new(); frame_count];
    for marker in source.markers() {
        let index = source
            .frames()
            .iter()
            .position(|frame| frame.timestamp() >= marker.timestamp())
            .unwrap_or(final_frame);
        boundary_roles[index].push(SelectionReason::MarkerBoundary);
    }
    for gap in source.gaps() {
        if let Some(index) = source
            .frames()
            .iter()
            .rposition(|frame| frame.timestamp() < gap.range().start())
        {
            boundary_roles[index].push(SelectionReason::GapBoundary);
        }
        if let Some(index) = source
            .frames()
            .iter()
            .position(|frame| frame.timestamp() > gap.range().end())
        {
            boundary_roles[index].push(SelectionReason::GapBoundary);
        }
    }

    // Supplementary anchors are admitted in source declaration order. Repeated
    // boundaries on one source frame merge into one panel and one visible reason.
    for index in 0..frame_count {
        for reason in boundary_roles[index].iter().copied() {
            admit_anchor(
                index,
                reason,
                limit,
                &mut selected,
                &mut reasons,
                &mut omitted,
            );
        }
    }

    while selected.iter().filter(|value| **value).count() < limit
        && selected.iter().any(|value| !*value)
    {
        let index = best_fill_candidate(source, &analysis, &selected, &boundary_roles)?;
        selected[index] = true;
        let candidate_reasons = fill_reasons(index, &analysis, &selected, &boundary_roles);
        for reason in candidate_reasons {
            push_unique(&mut reasons[index], reason);
        }
    }

    let selected_frames = source
        .frames()
        .iter()
        .enumerate()
        .filter(|(index, _)| selected[*index])
        .map(|(index, frame)| SelectedFrame {
            frame_id: frame.id().clone(),
            frame_index: index,
            timestamp: frame.timestamp(),
            reasons: std::mem::take(&mut reasons[index]).into_boxed_slice(),
        })
        .collect::<Vec<_>>()
        .into_boxed_slice();

    Ok(StoryboardSelection {
        selected_frames,
        omitted_anchors: omitted.into_boxed_slice(),
        before_index: baseline,
        during_index: peak.or(post_anchor).unwrap_or(baseline),
        after_index: final_frame,
        continuity_segment_count: analysis.segment_count,
    })
}

fn validate_alignment<F: Eq, M: Eq, G: Eq, P: AsRef<[u8]>>(
    source: &FrameSequence<F, M, G, P>,
    normalized: &NormalizedSequence<F>,
) -> Result<()> {
    if source.frames().len() != normalized.frames().len()
        || source
            .frames()
            .iter()
            .zip(normalized.frames())
            .any(|(left, right)| {
                left.id() != right.id()
                    || left.timestamp() != right.timestamp()
                    || right.dimensions() != normalized.dimensions()
            })
    {
        return Err(VisionError::new(
            ErrorCode::InvalidParameter,
            "source and normalized sequences must have aligned frame identity, time, and geometry",
        ));
    }
    Ok(())
}

fn first_change_index<F>(
    adjacent: &[crate::FrameComparison],
    normalized: &NormalizedSequence<F>,
    anchor: Timestamp,
) -> Option<usize> {
    let changed = |comparison: &&crate::FrameComparison| {
        matches!(
            comparison.outcome(),
            ComparisonOutcome::Measured(vector)
                if vector.changed_pixel_proportion().changed() > 0
        )
    };
    adjacent
        .iter()
        .filter(changed)
        .find(|comparison| {
            normalized.frames()[comparison.later_frame_index()].timestamp() >= anchor
        })
        .or_else(|| adjacent.iter().find(changed))
        .map(crate::FrameComparison::later_frame_index)
}

fn peak_baseline_index<F>(
    normalized: &NormalizedSequence<F>,
    baseline: usize,
    analysis: &SelectionAnalysis,
    measurement: MeasurementParameters,
) -> Result<Option<usize>> {
    let segment = analysis.segments[baseline];
    let mut best = None;
    let mut best_metrics = None::<PeakMetrics>;
    for index in (baseline + 1)..normalized.frames().len() {
        if analysis.segments[index] != segment {
            break;
        }
        let comparison = measure_pair(normalized, baseline, index, measurement)?;
        let ComparisonOutcome::Measured(vector) = comparison.outcome() else {
            break;
        };
        let metrics = PeakMetrics::from_vector(vector, index);
        if metrics.distance == 0 && metrics.changed == 0 {
            continue;
        }
        if best_metrics
            .as_ref()
            .is_none_or(|current| metrics > *current)
        {
            best = Some(index);
            best_metrics = Some(metrics);
        }
    }
    Ok(best)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PeakMetrics {
    distance: u16,
    changed: u64,
    compared: u64,
    absolute: u64,
    reverse_index: usize,
}

impl PeakMetrics {
    fn from_vector(vector: &MeasurementVector, index: usize) -> Self {
        let proportion = vector.changed_pixel_proportion();
        Self {
            distance: vector.perceptual_frame_distance(),
            changed: proportion.changed(),
            compared: proportion.compared(),
            absolute: vector.absolute_pixel_difference(),
            reverse_index: usize::MAX - index,
        }
    }
}

impl Ord for PeakMetrics {
    fn cmp(&self, other: &Self) -> Ordering {
        self.distance
            .cmp(&other.distance)
            .then_with(|| {
                u128::from(self.changed)
                    .saturating_mul(u128::from(other.compared))
                    .cmp(&u128::from(other.changed).saturating_mul(u128::from(self.compared)))
            })
            .then(self.changed.cmp(&other.changed))
            .then(self.absolute.cmp(&other.absolute))
            .then(self.reverse_index.cmp(&other.reverse_index))
    }
}

impl PartialOrd for PeakMetrics {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

fn admit_anchor(
    index: usize,
    reason: SelectionReason,
    limit: usize,
    selected: &mut [bool],
    reasons: &mut [Vec<SelectionReason>],
    omitted: &mut Vec<OmittedAnchor>,
) {
    if selected[index] {
        push_unique(&mut reasons[index], reason);
    } else if selected.iter().filter(|value| **value).count() < limit {
        selected[index] = true;
        reasons[index].push(reason);
    } else {
        omitted.push(OmittedAnchor {
            frame_index: index,
            reason,
        });
    }
}

fn push_unique(reasons: &mut Vec<SelectionReason>, reason: SelectionReason) {
    if !reasons.contains(&reason) {
        reasons.push(reason);
    }
}

#[derive(Debug)]
struct SelectionAnalysis {
    segments: Vec<usize>,
    cumulative_change: Vec<u64>,
    incoming_change: Vec<u16>,
    local_peak: Vec<u16>,
    trend_delta: Vec<u16>,
    region_transition: Vec<bool>,
    segment_count: usize,
}

impl SelectionAnalysis {
    fn new<F: Eq, M: Eq, G: Eq, P: AsRef<[u8]>>(
        adjacent: &[crate::FrameComparison],
        _source: &FrameSequence<F, M, G, P>,
        frame_count: usize,
    ) -> Result<Self> {
        let mut segments = vec![0; frame_count];
        let mut cumulative_change = vec![0_u64; frame_count];
        let mut incoming_change = vec![0_u16; frame_count];
        let mut changed_region = vec![false; frame_count];
        let mut segment = 0_usize;
        for comparison in adjacent {
            let later = comparison.later_frame_index();
            match comparison.outcome() {
                ComparisonOutcome::GapBoundary { .. } => {
                    segment = segment.checked_add(1).ok_or_else(score_overflow)?;
                    segments[later] = segment;
                    cumulative_change[later] = 0;
                }
                ComparisonOutcome::Measured(vector) => {
                    segments[later] = segment;
                    incoming_change[later] = vector.perceptual_frame_distance();
                    changed_region[later] = vector.changed_region_bounds().is_some();
                    cumulative_change[later] = cumulative_change[later - 1]
                        .checked_add(u64::from(vector.perceptual_frame_distance()))
                        .ok_or_else(score_overflow)?;
                }
            }
        }
        let mut local_peak = vec![0; frame_count];
        let mut trend_delta = vec![0; frame_count];
        let mut region_transition = vec![false; frame_count];
        for index in 1..frame_count {
            if segments[index] != segments[index - 1] {
                continue;
            }
            let current = incoming_change[index];
            let previous = if index > 1 && segments[index - 1] == segments[index - 2] {
                incoming_change[index - 1]
            } else {
                0
            };
            let next = if index + 1 < frame_count && segments[index + 1] == segments[index] {
                incoming_change[index + 1]
            } else {
                0
            };
            if current >= previous && current > next {
                local_peak[index] = current;
            }
            trend_delta[index] = current.abs_diff(previous).max(current.abs_diff(next));
            region_transition[index] = changed_region[index] != changed_region[index - 1];
        }
        Ok(Self {
            segments,
            cumulative_change,
            incoming_change,
            local_peak,
            trend_delta,
            region_transition,
            segment_count: segment + 1,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct FillScore {
    unrepresented_segment: bool,
    supplementary_roles: usize,
    information_gain: u64,
    local_peak: u16,
    trend_delta: u16,
    region_transition: bool,
    temporal_coverage: u64,
    reverse_index: usize,
}

fn best_fill_candidate<F: Eq, M: Eq, G: Eq, P: AsRef<[u8]>>(
    source: &FrameSequence<F, M, G, P>,
    analysis: &SelectionAnalysis,
    selected: &[bool],
    boundary_roles: &[Vec<SelectionReason>],
) -> Result<usize> {
    let represented = selected
        .iter()
        .enumerate()
        .filter(|(_, value)| **value)
        .map(|(index, _)| analysis.segments[index])
        .collect::<Vec<_>>();
    (0..selected.len())
        .filter(|index| !selected[*index])
        .map(|index| {
            score_candidate(
                source,
                analysis,
                selected,
                boundary_roles,
                &represented,
                index,
            )
            .map(|score| (score, index))
        })
        .collect::<Result<Vec<_>>>()?
        .into_iter()
        .max_by_key(|(score, _)| *score)
        .map(|(_, index)| index)
        .ok_or_else(score_overflow)
}

fn score_candidate<F: Eq, M: Eq, G: Eq, P: AsRef<[u8]>>(
    source: &FrameSequence<F, M, G, P>,
    analysis: &SelectionAnalysis,
    selected: &[bool],
    boundary_roles: &[Vec<SelectionReason>],
    represented: &[usize],
    index: usize,
) -> Result<FillScore> {
    Ok(FillScore {
        unrepresented_segment: !represented.contains(&analysis.segments[index]),
        supplementary_roles: boundary_roles[index].len(),
        information_gain: nearest_selected_distance(index, selected, |left, right| {
            if analysis.segments[left] != analysis.segments[right] {
                None
            } else {
                Some(analysis.cumulative_change[left].abs_diff(analysis.cumulative_change[right]))
            }
        }),
        local_peak: analysis.local_peak[index],
        trend_delta: analysis.trend_delta[index],
        region_transition: analysis.region_transition[index],
        temporal_coverage: nearest_selected_distance(index, selected, |left, right| {
            Some(
                source.frames()[left]
                    .timestamp()
                    .as_nanos()
                    .abs_diff(source.frames()[right].timestamp().as_nanos()),
            )
        }),
        reverse_index: usize::MAX - index,
    })
}

fn nearest_selected_distance(
    index: usize,
    selected: &[bool],
    distance: impl Fn(usize, usize) -> Option<u64>,
) -> u64 {
    let predecessor = (0..index)
        .rev()
        .find(|candidate| selected[*candidate] && distance(index, *candidate).is_some());
    let successor = ((index + 1)..selected.len())
        .find(|candidate| selected[*candidate] && distance(index, *candidate).is_some());
    match (predecessor, successor) {
        (Some(left), Some(right)) => distance(index, left)
            .expect("filtered")
            .min(distance(index, right).expect("filtered")),
        (Some(left), None) => distance(index, left).expect("filtered"),
        (None, Some(right)) => distance(index, right).expect("filtered"),
        (None, None) => 0,
    }
}

fn fill_reasons(
    index: usize,
    analysis: &SelectionAnalysis,
    selected_after: &[bool],
    boundary_roles: &[Vec<SelectionReason>],
) -> Vec<SelectionReason> {
    let mut reasons = boundary_roles[index].clone();
    reasons.sort_by_key(|reason| {
        SelectionReason::ALL
            .iter()
            .position(|value| value == reason)
    });
    reasons.dedup();
    if analysis.local_peak[index] > 0 {
        reasons.push(SelectionReason::LocalChangePeak);
    }
    if analysis.trend_delta[index] > 0 {
        reasons.push(SelectionReason::ChangeTrend);
    }
    if analysis.region_transition[index] {
        reasons.push(SelectionReason::ChangedRegionTransition);
    }
    if analysis.incoming_change[index] > 0 {
        reasons.push(SelectionReason::InformationGain);
    }
    if reasons.is_empty() || selected_after.iter().filter(|value| **value).count() > 1 {
        reasons.push(SelectionReason::TemporalCoverage);
    }
    reasons
}

fn score_overflow() -> VisionError {
    VisionError::new(
        ErrorCode::ResourceLimitExceeded,
        "storyboard selection score exceeds the supported integer representation",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        DeclaredGap, Frame, IntegerScale, Marker, NormalizationParameters, PixelDimensions,
        PixelFormat, ProcessingLimits, Rgb8, normalize_sequence,
    };

    fn sequence() -> (FrameSequence<u8, u8, u8, Box<[u8]>>, NormalizedSequence<u8>) {
        let dimensions = PixelDimensions::new(1, 1).unwrap();
        let frames = [0_u8, 0, 255, 64, 255, 0]
            .into_iter()
            .enumerate()
            .map(|(index, value)| {
                Frame::new(
                    index as u8,
                    Timestamp::from_nanos(index as u64),
                    dimensions,
                    PixelFormat::Rgba8SrgbStraight,
                    vec![value, value, value, 255].into_boxed_slice(),
                )
                .unwrap()
            })
            .collect();
        let source = FrameSequence::new(
            frames,
            vec![Marker::new(1, Timestamp::from_nanos(3), "action", "marker").unwrap()],
            Vec::<DeclaredGap<u8>>::new(),
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
        (source, normalized)
    }

    #[test]
    fn limits_and_anchor_priority_are_deterministic() {
        assert_eq!(StoryboardTileLimit::default().get(), 8);
        assert!(StoryboardTileLimit::new(2).is_err());
        assert!(StoryboardTileLimit::new(13).is_err());
        let (source, normalized) = sequence();
        let selection = select_storyboard_frames(
            &source,
            &normalized,
            Timestamp::from_nanos(2),
            StoryboardTileLimit::new(3).unwrap(),
            MeasurementParameters::new(0),
        )
        .unwrap();
        assert_eq!(selection.selected_frames().len(), 3);
        assert_eq!(selection.before_index(), 1);
        assert_eq!(selection.after_index(), 5);
        assert!(!selection.omitted_anchors().is_empty());
        assert_eq!(
            serde_json::to_vec(&selection).unwrap(),
            serde_json::to_vec(&selection).unwrap()
        );
    }

    #[test]
    fn reason_registry_has_stable_names() {
        for reason in SelectionReason::ALL {
            let json = serde_json::to_string(reason).unwrap();
            assert_eq!(
                serde_json::from_str::<SelectionReason>(&json).unwrap(),
                *reason
            );
            assert_eq!(reason.to_string(), reason.as_str());
        }
    }
}
