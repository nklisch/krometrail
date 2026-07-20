use std::{
    collections::HashSet,
    time::{Duration, SystemTime},
};

use serde::{Deserialize, Serialize};

use crate::{
    CaptureGap, FrameId, InteractionAnchor, InteractionId, MarkerId, NavigationId, ObservationKind,
    Result, SessionId, SessionRange, SessionTime, TargetId, TimelineObservation,
    error::{
        ErrorCode, ErrorContext, KrometrailError, NonEmptyText, RetryAdvice, invalid, invalid_time,
    },
    validation::{delegate_json_schema, deserialize_validated},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub enum TemporalRangeAnchorKind {
    #[serde(rename = "session_time")]
    SessionTime,
    #[serde(rename = "wall_clock")]
    WallClock,
    #[serde(rename = "interaction")]
    Interaction,
    #[serde(rename = "latest_interaction")]
    LatestInteraction,
    #[serde(rename = "navigation")]
    Navigation,
    #[serde(rename = "marker")]
    Marker,
    #[serde(rename = "source_frame")]
    SourceFrame,
}

impl TemporalRangeAnchorKind {
    pub const ALL: &'static [Self] = &[
        Self::SessionTime,
        Self::WallClock,
        Self::Interaction,
        Self::LatestInteraction,
        Self::Navigation,
        Self::Marker,
        Self::SourceFrame,
    ];
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SessionTime => "session_time",
            Self::WallClock => "wall_clock",
            Self::Interaction => "interaction",
            Self::LatestInteraction => "latest_interaction",
            Self::Navigation => "navigation",
            Self::Marker => "marker",
            Self::SourceFrame => "source_frame",
        }
    }
    pub fn from_stable_name(value: &str) -> Option<Self> {
        Self::ALL
            .iter()
            .copied()
            .find(|kind| kind.as_str() == value)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AnchorScope {
    pub session_id: Option<SessionId>,
    pub target_id: Option<TargetId>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct IntervalAnchorScope {
    pub session_id: SessionId,
    pub target_id: TargetId,
}

impl IntervalAnchorScope {
    pub const fn new(session_id: SessionId, target_id: TargetId) -> Self {
        Self {
            session_id,
            target_id,
        }
    }
}
impl AnchorScope {
    pub const fn new(session_id: Option<SessionId>, target_id: Option<TargetId>) -> Self {
        Self {
            session_id,
            target_id,
        }
    }
}

pub const MAX_NATURAL_ANCHOR_WINDOW: Duration = Duration::from_secs(120);

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, schemars::JsonSchema)]
pub struct InteractionWindow {
    #[serde(rename = "before_ms")]
    before_millis: u64,
    #[serde(rename = "after_ms")]
    after_millis: u64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct InteractionWindowWire {
    before_ms: u64,
    after_ms: u64,
}

impl InteractionWindow {
    pub fn new(before: Duration, after: Duration) -> Result<Self> {
        fn whole_millis(value: Duration, side: &str) -> Result<u64> {
            if value > MAX_NATURAL_ANCHOR_WINDOW {
                return Err(invalid(format!(
                    "interaction window {side} duration exceeds 120 seconds"
                )));
            }
            if value.subsec_nanos() % 1_000_000 != 0 {
                return Err(invalid(format!(
                    "interaction window {side} duration must use whole milliseconds"
                )));
            }
            u64::try_from(value.as_millis())
                .map_err(|_| invalid_time("interaction window exceeds supported time"))
        }

        Ok(Self {
            before_millis: whole_millis(before, "before")?,
            after_millis: whole_millis(after, "after")?,
        })
    }

    pub const fn before(self) -> Duration {
        Duration::from_millis(self.before_millis)
    }

    pub const fn after(self) -> Duration {
        Duration::from_millis(self.after_millis)
    }

    pub fn as_nanos(self) -> Result<(u64, u64)> {
        let millis_to_nanos = |value: u64| {
            value
                .checked_mul(1_000_000)
                .ok_or_else(|| invalid_time("interaction window exceeds session time"))
        };
        Ok((
            millis_to_nanos(self.before_millis)?,
            millis_to_nanos(self.after_millis)?,
        ))
    }
}

impl<'de> Deserialize<'de> for InteractionWindow {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        deserialize_validated(d, |wire: InteractionWindowWire| {
            Self::new(
                Duration::from_millis(wire.before_ms),
                Duration::from_millis(wire.after_ms),
            )
        })
    }
}

temporal_vision::stable_registry! {
    pub enum RetentionPolicy {
        RequireComplete => "require_complete",
        AllowPartial => "allow_partial",
    }
}

temporal_vision::stable_registry! {
    pub enum CaptureGapPolicy {
        Include => "include",
        Reject => "reject",
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RangeResolutionOptions {
    pub retention: RetentionPolicy,
    pub capture_gaps: CaptureGapPolicy,
    pub implicit_interaction_window: InteractionWindow,
}
impl RangeResolutionOptions {
    pub const DEFAULT_PRE_INTERACTION_CONTEXT: Duration = Duration::from_millis(150);
    pub const DEFAULT_POST_INTERACTION_CONTEXT: Duration = Duration::from_millis(250);
    pub const DEFAULT: Self = Self {
        retention: RetentionPolicy::RequireComplete,
        capture_gaps: CaptureGapPolicy::Include,
        implicit_interaction_window: InteractionWindow {
            before_millis: 150,
            after_millis: 250,
        },
    };
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, schemars::JsonSchema)]
#[serde(tag = "anchor", rename_all = "snake_case")]
pub enum TemporalRangeAnchor {
    SessionTime {
        scope: IntervalAnchorScope,
        range: SessionRange,
    },
    WallClock {
        scope: IntervalAnchorScope,
        start: SystemTime,
        end: SystemTime,
    },
    Interaction {
        scope: AnchorScope,
        interaction_id: InteractionId,
        window: Option<InteractionWindow>,
    },
    LatestInteraction {
        session_id: SessionId,
        target_id: TargetId,
        window: Option<InteractionWindow>,
    },
    Navigation {
        scope: AnchorScope,
        navigation_id: NavigationId,
        window: Option<InteractionWindow>,
    },
    Marker {
        scope: AnchorScope,
        marker_id: MarkerId,
        window: Option<InteractionWindow>,
    },
    SourceFrame {
        scope: AnchorScope,
        start_frame_id: FrameId,
        end_frame_id: FrameId,
    },
}
#[derive(Deserialize)]
#[serde(tag = "anchor", rename_all = "snake_case", deny_unknown_fields)]
enum TemporalRangeAnchorWire {
    SessionTime {
        scope: IntervalAnchorScope,
        range: SessionRange,
    },
    WallClock {
        scope: IntervalAnchorScope,
        start: SystemTime,
        end: SystemTime,
    },
    Interaction {
        scope: AnchorScope,
        interaction_id: InteractionId,
        window: Option<InteractionWindow>,
    },
    LatestInteraction {
        session_id: SessionId,
        target_id: TargetId,
        window: Option<InteractionWindow>,
    },
    Navigation {
        scope: AnchorScope,
        navigation_id: NavigationId,
        window: Option<InteractionWindow>,
    },
    Marker {
        scope: AnchorScope,
        marker_id: MarkerId,
        window: Option<InteractionWindow>,
    },
    SourceFrame {
        scope: AnchorScope,
        start_frame_id: FrameId,
        end_frame_id: FrameId,
    },
}

impl TemporalRangeAnchor {
    pub const fn kind(&self) -> TemporalRangeAnchorKind {
        match self {
            Self::SessionTime { .. } => TemporalRangeAnchorKind::SessionTime,
            Self::WallClock { .. } => TemporalRangeAnchorKind::WallClock,
            Self::Interaction { .. } => TemporalRangeAnchorKind::Interaction,
            Self::LatestInteraction { .. } => TemporalRangeAnchorKind::LatestInteraction,
            Self::Navigation { .. } => TemporalRangeAnchorKind::Navigation,
            Self::Marker { .. } => TemporalRangeAnchorKind::Marker,
            Self::SourceFrame { .. } => TemporalRangeAnchorKind::SourceFrame,
        }
    }

    pub fn validate(&self) -> Result<()> {
        match self {
            Self::SessionTime { .. } => {}
            Self::WallClock { scope, start, end } => {
                if start > end {
                    return Err(invalid_with_context(
                        "wall-clock range start must not exceed its end",
                        interval_scope_context(*scope),
                    ));
                }
            }
            Self::Interaction { window, .. }
            | Self::LatestInteraction { window, .. }
            | Self::Navigation { window, .. }
            | Self::Marker { window, .. } => {
                if let Some(window) = window {
                    // Re-run the public constructor so values created by future internal code
                    // cannot bypass the same whole-millisecond boundary as Serde.
                    InteractionWindow::new(window.before(), window.after())?;
                }
            }
            Self::SourceFrame {
                start_frame_id,
                end_frame_id,
                ..
            } => {
                if start_frame_id.as_uuid().is_nil() || end_frame_id.as_uuid().is_nil() {
                    return Err(invalid("source-frame range endpoints must be non-nil"));
                }
            }
        }
        Ok(())
    }
}

impl<'de> Deserialize<'de> for TemporalRangeAnchor {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        deserialize_validated(d, |wire: TemporalRangeAnchorWire| {
            let value = match wire {
                TemporalRangeAnchorWire::SessionTime { scope, range } => {
                    Self::SessionTime { scope, range }
                }
                TemporalRangeAnchorWire::WallClock { scope, start, end } => {
                    Self::WallClock { scope, start, end }
                }
                TemporalRangeAnchorWire::Interaction {
                    scope,
                    interaction_id,
                    window,
                } => Self::Interaction {
                    scope,
                    interaction_id,
                    window,
                },
                TemporalRangeAnchorWire::LatestInteraction {
                    session_id,
                    target_id,
                    window,
                } => Self::LatestInteraction {
                    session_id,
                    target_id,
                    window,
                },
                TemporalRangeAnchorWire::Navigation {
                    scope,
                    navigation_id,
                    window,
                } => Self::Navigation {
                    scope,
                    navigation_id,
                    window,
                },
                TemporalRangeAnchorWire::Marker {
                    scope,
                    marker_id,
                    window,
                } => Self::Marker {
                    scope,
                    marker_id,
                    window,
                },
                TemporalRangeAnchorWire::SourceFrame {
                    scope,
                    start_frame_id,
                    end_frame_id,
                } => Self::SourceFrame {
                    scope,
                    start_frame_id,
                    end_frame_id,
                },
            };
            value.validate()?;
            Ok(value)
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FrameAvailability {
    pub retained_bounds: Option<SessionRange>,
    pub evicted_ranges: Vec<SessionRange>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FrameAvailabilityWire {
    retained_bounds: Option<SessionRange>,
    evicted_ranges: Vec<SessionRange>,
}

impl FrameAvailability {
    pub fn new(
        retained_bounds: Option<SessionRange>,
        evicted_ranges: Vec<SessionRange>,
    ) -> Result<Self> {
        for ranges in evicted_ranges.windows(2) {
            if ranges[0].start() > ranges[1].start()
                || ranges[0].end().as_nanos().saturating_add(1) >= ranges[1].start().as_nanos()
            {
                return Err(invalid(
                    "evicted frame ranges must be sorted, disjoint, and coalesced",
                ));
            }
        }
        Ok(Self {
            retained_bounds,
            evicted_ranges,
        })
    }
}

impl<'de> Deserialize<'de> for FrameAvailability {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        deserialize_validated(d, |wire: FrameAvailabilityWire| {
            Self::new(wire.retained_bounds, wire.evicted_ranges)
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RetentionWarning {
    RequestedStartBeforeOldestRetained {
        requested: SessionTime,
        oldest_retained: SessionTime,
    },
    RequestedEndAfterNewestRetained {
        requested: SessionTime,
        newest_retained: SessionTime,
    },
    PartiallyEvicted {
        requested: SessionRange,
        retained: SessionRange,
    },
    PartiallyCaptured {
        requested: SessionRange,
        retained: SessionRange,
    },
    EvictedRanges {
        ranges: Vec<SessionRange>,
    },
}

/// Exact semantic anchor chosen by the resolver before retention is applied.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, schemars::JsonSchema)]
#[serde(tag = "source", rename_all = "snake_case")]
pub enum ResolvedAnchorReference {
    Interval,
    Interaction {
        interaction_id: InteractionId,
    },
    Navigation {
        navigation_id: NavigationId,
    },
    Marker {
        marker_id: MarkerId,
    },
    SourceFrames {
        start_frame_id: FrameId,
        end_frame_id: FrameId,
    },
}

impl ResolvedAnchorReference {
    fn validate(&self) -> Result<()> {
        let non_nil = match self {
            Self::Interval => return Ok(()),
            Self::Interaction { interaction_id } => !interaction_id.as_uuid().is_nil(),
            Self::Navigation { navigation_id } => !navigation_id.as_uuid().is_nil(),
            Self::Marker { marker_id } => !marker_id.as_uuid().is_nil(),
            Self::SourceFrames {
                start_frame_id,
                end_frame_id,
            } => !start_frame_id.as_uuid().is_nil() && !end_frame_id.as_uuid().is_nil(),
        };
        if non_nil {
            Ok(())
        } else {
            Err(invalid("resolved anchor identifiers must be non-nil"))
        }
    }
}

impl<'de> Deserialize<'de> for ResolvedAnchorReference {
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(tag = "source", rename_all = "snake_case", deny_unknown_fields)]
        enum Wire {
            Interval,
            Interaction {
                interaction_id: InteractionId,
            },
            Navigation {
                navigation_id: NavigationId,
            },
            Marker {
                marker_id: MarkerId,
            },
            SourceFrames {
                start_frame_id: FrameId,
                end_frame_id: FrameId,
            },
        }
        let value = match Wire::deserialize(deserializer)? {
            Wire::Interval => Self::Interval,
            Wire::Interaction { interaction_id } => Self::Interaction { interaction_id },
            Wire::Navigation { navigation_id } => Self::Navigation { navigation_id },
            Wire::Marker { marker_id } => Self::Marker { marker_id },
            Wire::SourceFrames {
                start_frame_id,
                end_frame_id,
            } => Self::SourceFrames {
                start_frame_id,
                end_frame_id,
            },
        };
        value.validate().map_err(serde::de::Error::custom)?;
        Ok(value)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ResolvedAnchor {
    pub reference: ResolvedAnchorReference,
    pub requested_time: SessionTime,
    pub effective_time: SessionTime,
}

impl ResolvedAnchor {
    pub fn new(
        reference: ResolvedAnchorReference,
        requested_time: SessionTime,
        effective_time: SessionTime,
    ) -> Result<Self> {
        reference.validate()?;
        Ok(Self {
            reference,
            requested_time,
            effective_time,
        })
    }
}

impl<'de> Deserialize<'de> for ResolvedAnchor {
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Wire {
            reference: ResolvedAnchorReference,
            requested_time: SessionTime,
            effective_time: SessionTime,
        }
        let wire = Wire::deserialize(deserializer)?;
        Self::new(wire.reference, wire.requested_time, wire.effective_time)
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ResolvedRange {
    pub session_id: SessionId,
    pub target_id: TargetId,
    pub anchor_kind: TemporalRangeAnchorKind,
    pub resolved_anchor: ResolvedAnchor,
    pub requested_range: SessionRange,
    pub resolved_range: SessionRange,
    pub frame_ids: Vec<FrameId>,
    pub interaction_ids: Vec<InteractionId>,
    pub navigation_ids: Vec<NavigationId>,
    pub marker_ids: Vec<MarkerId>,
    pub gaps: Vec<CaptureGap>,
    pub retention_warnings: Vec<RetentionWarning>,
    pub options: RangeResolutionOptions,
}

/// A resolved range carries the resolver-selected kind, not the request-only
/// `latest_interaction` anchor that has already collapsed to `interaction`.
#[derive(Deserialize, schemars::JsonSchema)]
#[schemars(rename = "ResolvedRangeAnchorKind")]
#[serde(rename_all = "snake_case")]
enum ResolvedRangeAnchorKindWire {
    SessionTime,
    WallClock,
    Interaction,
    Navigation,
    Marker,
    SourceFrame,
}

impl From<ResolvedRangeAnchorKindWire> for TemporalRangeAnchorKind {
    fn from(value: ResolvedRangeAnchorKindWire) -> Self {
        match value {
            ResolvedRangeAnchorKindWire::SessionTime => Self::SessionTime,
            ResolvedRangeAnchorKindWire::WallClock => Self::WallClock,
            ResolvedRangeAnchorKindWire::Interaction => Self::Interaction,
            ResolvedRangeAnchorKindWire::Navigation => Self::Navigation,
            ResolvedRangeAnchorKindWire::Marker => Self::Marker,
            ResolvedRangeAnchorKindWire::SourceFrame => Self::SourceFrame,
        }
    }
}

#[derive(Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
struct ResolvedRangeWire {
    session_id: SessionId,
    target_id: TargetId,
    anchor_kind: ResolvedRangeAnchorKindWire,
    resolved_anchor: ResolvedAnchor,
    requested_range: SessionRange,
    resolved_range: SessionRange,
    frame_ids: Vec<FrameId>,
    interaction_ids: Vec<InteractionId>,
    navigation_ids: Vec<NavigationId>,
    marker_ids: Vec<MarkerId>,
    gaps: Vec<CaptureGap>,
    retention_warnings: Vec<RetentionWarning>,
    options: RangeResolutionOptions,
}

delegate_json_schema!(ResolvedRange => ResolvedRangeWire);

impl ResolvedRange {
    /// Compatibility constructor for explicit ranges used by internal callers.
    /// Natural anchors must use `new_with_anchor` because their semantic time
    /// cannot be reconstructed from a resolved interval.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        session_id: SessionId,
        target_id: TargetId,
        anchor_kind: TemporalRangeAnchorKind,
        requested_range: SessionRange,
        resolved_range: SessionRange,
        frame_ids: Vec<FrameId>,
        interaction_ids: Vec<InteractionId>,
        navigation_ids: Vec<NavigationId>,
        marker_ids: Vec<MarkerId>,
        gaps: Vec<CaptureGap>,
        retention_warnings: Vec<RetentionWarning>,
        options: RangeResolutionOptions,
    ) -> Result<Self> {
        let reference = match anchor_kind {
            TemporalRangeAnchorKind::SessionTime | TemporalRangeAnchorKind::WallClock => {
                ResolvedAnchorReference::Interval
            }
            _ => {
                return Err(invalid(
                    "non-interval resolved ranges require their exact resolver-selected anchor",
                ));
            }
        };
        let requested_time = range_midpoint(requested_range);
        let resolved_anchor = ResolvedAnchor::new(
            reference,
            requested_time,
            clamp_time(requested_time, resolved_range),
        )?;
        Self::new_with_anchor(
            session_id,
            target_id,
            anchor_kind,
            resolved_anchor,
            requested_range,
            resolved_range,
            frame_ids,
            interaction_ids,
            navigation_ids,
            marker_ids,
            gaps,
            retention_warnings,
            options,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn new_with_anchor(
        session_id: SessionId,
        target_id: TargetId,
        anchor_kind: TemporalRangeAnchorKind,
        resolved_anchor: ResolvedAnchor,
        requested_range: SessionRange,
        resolved_range: SessionRange,
        frame_ids: Vec<FrameId>,
        interaction_ids: Vec<InteractionId>,
        navigation_ids: Vec<NavigationId>,
        marker_ids: Vec<MarkerId>,
        gaps: Vec<CaptureGap>,
        retention_warnings: Vec<RetentionWarning>,
        options: RangeResolutionOptions,
    ) -> Result<Self> {
        let value = Self {
            session_id,
            target_id,
            anchor_kind,
            resolved_anchor,
            requested_range,
            resolved_range,
            frame_ids,
            interaction_ids,
            navigation_ids,
            marker_ids,
            gaps,
            retention_warnings,
            options,
        };
        value.validate()?;
        Ok(value)
    }

    pub fn validate(&self) -> Result<()> {
        if self.session_id.as_uuid().is_nil() || self.target_id.as_uuid().is_nil() {
            return Err(invalid("resolved range scope identifiers must be non-nil"));
        }
        self.resolved_anchor.reference.validate()?;
        if self.frame_ids.is_empty() {
            return Err(invalid(
                "resolved range must contain at least one retained frame",
            ));
        }
        if self.frame_ids.iter().any(|id| id.as_uuid().is_nil())
            || self.interaction_ids.iter().any(|id| id.as_uuid().is_nil())
            || self.navigation_ids.iter().any(|id| id.as_uuid().is_nil())
            || self.marker_ids.iter().any(|id| id.as_uuid().is_nil())
            || has_duplicates(&self.frame_ids)
            || has_duplicates(&self.interaction_ids)
            || has_duplicates(&self.navigation_ids)
            || has_duplicates(&self.marker_ids)
        {
            return Err(invalid(
                "resolved range identifiers must be non-nil and unique",
            ));
        }
        if self.resolved_range.start() < self.requested_range.start()
            || self.resolved_range.end() > self.requested_range.end()
        {
            return Err(invalid(
                "resolved range must be contained in its requested range",
            ));
        }
        if !self
            .requested_range
            .contains(self.resolved_anchor.requested_time)
            || !self
                .resolved_range
                .contains(self.resolved_anchor.effective_time)
            || self.resolved_anchor.effective_time
                != clamp_time(self.resolved_anchor.requested_time, self.resolved_range)
        {
            return Err(invalid(
                "resolved anchor times must match requested and retained ranges",
            ));
        }
        if self.gaps.iter().any(|gap| {
            gap.session_id() != self.session_id
                || gap.target_id() != self.target_id
                || !ranges_intersect(gap.range(), self.resolved_range)
        }) {
            return Err(invalid(
                "resolved capture gaps must match and intersect the resolved scope",
            ));
        }
        validate_anchor_kind(self.anchor_kind, &self.resolved_anchor.reference)?;
        if let ResolvedAnchorReference::SourceFrames {
            start_frame_id,
            end_frame_id,
        } = &self.resolved_anchor.reference
            && (self.frame_ids.first() != Some(start_frame_id)
                || self.frame_ids.last() != Some(end_frame_id))
        {
            return Err(invalid(
                "source-frame anchor endpoints must match resolved source order",
            ));
        }
        if matches!(
            self.anchor_kind,
            TemporalRangeAnchorKind::SessionTime
                | TemporalRangeAnchorKind::WallClock
                | TemporalRangeAnchorKind::SourceFrame
        ) && self.resolved_anchor.requested_time != range_midpoint(self.requested_range)
        {
            return Err(invalid(
                "interval and source-frame anchors must use the requested midpoint",
            ));
        }
        let partial = self.resolved_range != self.requested_range;
        if partial && self.options.retention == RetentionPolicy::RequireComplete {
            return Err(invalid("complete retention cannot return a partial range"));
        }
        if partial && self.retention_warnings.is_empty() {
            return Err(invalid("partial retention requires retention warnings"));
        }
        if !partial && !self.retention_warnings.is_empty() {
            return Err(invalid(
                "retention warnings require a partial resolved range",
            ));
        }
        Ok(())
    }
}

impl<'de> Deserialize<'de> for ResolvedRange {
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Self, D::Error> {
        let wire = ResolvedRangeWire::deserialize(deserializer)?;
        Self::new_with_anchor(
            wire.session_id,
            wire.target_id,
            wire.anchor_kind.into(),
            wire.resolved_anchor,
            wire.requested_range,
            wire.resolved_range,
            wire.frame_ids,
            wire.interaction_ids,
            wire.navigation_ids,
            wire.marker_ids,
            wire.gaps,
            wire.retention_warnings,
            wire.options,
        )
        .map_err(serde::de::Error::custom)
    }
}

fn validate_anchor_kind(
    kind: TemporalRangeAnchorKind,
    reference: &ResolvedAnchorReference,
) -> Result<()> {
    let compatible = matches!(
        (kind, reference),
        (
            TemporalRangeAnchorKind::SessionTime | TemporalRangeAnchorKind::WallClock,
            ResolvedAnchorReference::Interval
        ) | (
            TemporalRangeAnchorKind::Interaction,
            ResolvedAnchorReference::Interaction { .. }
        ) | (
            TemporalRangeAnchorKind::Navigation,
            ResolvedAnchorReference::Navigation { .. }
        ) | (
            TemporalRangeAnchorKind::Marker,
            ResolvedAnchorReference::Marker { .. }
        ) | (
            TemporalRangeAnchorKind::SourceFrame,
            ResolvedAnchorReference::SourceFrames { .. }
        )
    );
    if !compatible {
        return Err(invalid(
            "resolved anchor reference does not match its anchor kind",
        ));
    }
    Ok(())
}

const fn range_midpoint(range: SessionRange) -> SessionTime {
    let start = range.start().as_nanos();
    let distance = range.end().as_nanos() - start;
    SessionTime::from_nanos(start + distance / 2)
}

const fn clamp_time(value: SessionTime, range: SessionRange) -> SessionTime {
    if value.as_nanos() < range.start().as_nanos() {
        range.start()
    } else if value.as_nanos() > range.end().as_nanos() {
        range.end()
    } else {
        value
    }
}

fn has_duplicates<T: Eq + std::hash::Hash>(values: &[T]) -> bool {
    values.iter().collect::<HashSet<_>>().len() != values.len()
}

pub(crate) fn not_found(message: impl Into<String>, context: ErrorContext) -> KrometrailError {
    KrometrailError::new(
        ErrorCode::NotFound,
        NonEmptyText::new(message).expect("range errors must have a non-empty message"),
    )
    .with_context(context)
}
pub(crate) fn invalid_with_context(
    message: impl Into<String>,
    context: ErrorContext,
) -> KrometrailError {
    invalid(message).with_context(context)
}
pub(crate) fn interaction_range(
    anchor: &InteractionAnchor,
    window: InteractionWindow,
) -> Result<SessionRange> {
    let (before, after) = window.as_nanos()?;
    let start = anchor.timing.started_at.as_nanos().saturating_sub(before);
    let end_base = anchor
        .timing
        .observed_at
        .unwrap_or(anchor.timing.completed_at)
        .as_nanos();
    let end = end_base
        .checked_add(after)
        .ok_or_else(|| invalid_time("interaction window exceeds session time"))?;
    SessionRange::new(SessionTime::from_nanos(start), SessionTime::from_nanos(end))
}
pub(crate) fn point_range(
    at: SessionTime,
    window: Option<InteractionWindow>,
) -> Result<SessionRange> {
    let window = window.unwrap_or_else(|| {
        InteractionWindow::new(Duration::ZERO, Duration::ZERO)
            .expect("zero natural-anchor window is valid")
    });
    let (before, after) = window.as_nanos()?;
    let start = at.as_nanos().saturating_sub(before);
    let end = at
        .as_nanos()
        .checked_add(after)
        .ok_or_else(|| invalid_time("anchor window exceeds session time"))?;
    SessionRange::new(SessionTime::from_nanos(start), SessionTime::from_nanos(end))
}
pub(crate) fn scope_context(scope: AnchorScope) -> ErrorContext {
    ErrorContext {
        session_id: scope.session_id,
        target_id: scope.target_id,
        ..ErrorContext::default()
    }
}

fn interval_scope_context(scope: IntervalAnchorScope) -> ErrorContext {
    ErrorContext {
        session_id: Some(scope.session_id),
        target_id: Some(scope.target_id),
        ..ErrorContext::default()
    }
}

use crate::{
    CaptureGapStore, CapturedFrame, FrameSource, InteractionAnchorSource, ObservationPayloadRef,
    PortFuture, RecordingCatalog, TimelineAnchorSource, TimelineStore,
};

/// Resolves every temporal request through the same storage authorities before a
/// consumer can read source frames or generate an artifact.
pub struct TemporalRangeResolver<C, F, G, T, I> {
    catalog: C,
    frames: F,
    gaps: G,
    timeline: T,
    interactions: I,
}

impl<C, F, G, T, I> TemporalRangeResolver<C, F, G, T, I> {
    pub const fn new(catalog: C, frames: F, gaps: G, timeline: T, interactions: I) -> Self {
        Self {
            catalog,
            frames,
            gaps,
            timeline,
            interactions,
        }
    }
}

struct RangeSeed {
    session_id: SessionId,
    target_id: TargetId,
    requested_range: SessionRange,
    anchor_kind: TemporalRangeAnchorKind,
    anchor_reference: ResolvedAnchorReference,
    requested_anchor_time: SessionTime,
    preloaded_frames: Option<Vec<CapturedFrame>>,
}

fn seed_from_interaction(
    interaction: InteractionAnchor,
    window: InteractionWindow,
    anchor_kind: TemporalRangeAnchorKind,
) -> Result<RangeSeed> {
    debug_assert!(matches!(
        anchor_kind,
        TemporalRangeAnchorKind::Interaction | TemporalRangeAnchorKind::LatestInteraction
    ));
    Ok(RangeSeed {
        session_id: interaction.session_id,
        target_id: interaction.target_id,
        requested_range: interaction_range(&interaction, window)?,
        anchor_kind,
        anchor_reference: ResolvedAnchorReference::Interaction {
            interaction_id: interaction.interaction_id,
        },
        requested_anchor_time: interaction.timing.dispatched_at,
        preloaded_frames: None,
    })
}

impl<C, F, G, T, I> TemporalRangeResolver<C, F, G, T, I>
where
    C: RecordingCatalog,
    F: FrameSource,
    G: CaptureGapStore,
    T: TimelineStore + TimelineAnchorSource,
    I: InteractionAnchorSource,
{
    pub fn resolve(
        &self,
        anchor: TemporalRangeAnchor,
        options: RangeResolutionOptions,
    ) -> PortFuture<'_, Result<ResolvedRange>> {
        Box::pin(async move {
            anchor.validate()?;
            let seed = self.seed(anchor, options).await?;
            self.finalize(seed, options).await
        })
    }

    async fn seed(
        &self,
        anchor: TemporalRangeAnchor,
        options: RangeResolutionOptions,
    ) -> Result<RangeSeed> {
        match anchor {
            TemporalRangeAnchor::SessionTime { scope, range } => {
                let (session_id, target_id) = (scope.session_id, scope.target_id);
                self.validate_catalog_scope(session_id, target_id).await?;
                Ok(RangeSeed {
                    session_id,
                    target_id,
                    requested_range: range,
                    anchor_kind: TemporalRangeAnchorKind::SessionTime,
                    anchor_reference: ResolvedAnchorReference::Interval,
                    requested_anchor_time: range_midpoint(range),
                    preloaded_frames: None,
                })
            }
            TemporalRangeAnchor::WallClock { scope, start, end } => {
                let (session_id, target_id) = (scope.session_id, scope.target_id);
                if start > end {
                    return Err(invalid_with_context(
                        "wall-clock range start must not exceed its end",
                        interval_scope_context(scope),
                    ));
                }
                let session = self.catalog.session(session_id).await?.ok_or_else(|| {
                    not_found(
                        "wall-clock resolution requires complete session metadata",
                        interval_scope_context(scope),
                    )
                })?;
                let start = wall_clock_offset(start, session.started_at(), scope)?;
                let end = wall_clock_offset(end, session.started_at(), scope)?;
                let requested_range = SessionRange::new(start, end)?;
                self.validate_catalog_scope(session_id, target_id).await?;
                Ok(RangeSeed {
                    session_id,
                    target_id,
                    requested_range,
                    anchor_kind: TemporalRangeAnchorKind::WallClock,
                    anchor_reference: ResolvedAnchorReference::Interval,
                    requested_anchor_time: range_midpoint(requested_range),
                    preloaded_frames: None,
                })
            }
            TemporalRangeAnchor::SourceFrame {
                scope,
                start_frame_id,
                end_frame_id,
            } => {
                let metadata = self
                    .frames
                    .frame_metadata_by_id(vec![start_frame_id, end_frame_id])
                    .await?;
                if metadata.len() != 2 {
                    return Err(not_found(
                        "source-frame range endpoint is not retained",
                        scope_context(scope),
                    ));
                }
                let start = &metadata[0];
                let end = &metadata[1];
                if start.session_id() != end.session_id() || start.target_id() != end.target_id() {
                    return Err(invalid_with_context(
                        "source-frame range endpoints belong to different sessions or targets",
                        scope_context(scope),
                    ));
                }
                validate_scope_match(scope, start.session_id(), start.target_id())?;
                if start.capture_ordinal() > end.capture_ordinal() {
                    return Err(invalid_with_context(
                        "source-frame range start must not follow its end",
                        scope_context(scope),
                    ));
                }
                let requested_range = SessionRange::new(start.session_time(), end.session_time())?;
                let frames = self
                    .frames
                    .frame_metadata_in_ordinal_range(
                        start.session_id(),
                        start.target_id(),
                        start.capture_ordinal(),
                        end.capture_ordinal(),
                    )
                    .await?;
                Ok(RangeSeed {
                    session_id: start.session_id(),
                    target_id: start.target_id(),
                    requested_range,
                    anchor_kind: TemporalRangeAnchorKind::SourceFrame,
                    anchor_reference: ResolvedAnchorReference::SourceFrames {
                        start_frame_id,
                        end_frame_id,
                    },
                    requested_anchor_time: range_midpoint(requested_range),
                    preloaded_frames: Some(frames),
                })
            }
            TemporalRangeAnchor::Interaction {
                scope,
                interaction_id,
                window,
            } => {
                let interaction = self
                    .interactions
                    .interaction_anchor(interaction_id)
                    .await?
                    .ok_or_else(|| {
                        not_found(
                            "no durable interaction anchor exists for this interaction",
                            scope_context(scope).with_interaction(interaction_id),
                        )
                    })?;
                validate_scope_match(scope, interaction.session_id, interaction.target_id)?;
                let window = window.unwrap_or(options.implicit_interaction_window);
                seed_from_interaction(interaction, window, TemporalRangeAnchorKind::Interaction)
            }
            TemporalRangeAnchor::LatestInteraction {
                session_id,
                target_id,
                window,
            } => {
                let interaction = self
                    .interactions
                    .latest_interaction_anchor(session_id, target_id)
                    .await?
                    .ok_or_else(|| {
                        not_found(
                            "no durable interaction anchor exists for this session and target",
                            ErrorContext {
                                session_id: Some(session_id),
                                target_id: Some(target_id),
                                ..ErrorContext::default()
                            },
                        )
                    })?;
                if interaction.session_id != session_id || interaction.target_id != target_id {
                    return Err(invalid_with_context(
                        "latest interaction anchor has the wrong session or target",
                        ErrorContext {
                            session_id: Some(session_id),
                            target_id: Some(target_id),
                            ..ErrorContext::default()
                        },
                    ));
                }
                let window = window.unwrap_or(options.implicit_interaction_window);
                seed_from_interaction(interaction, window, TemporalRangeAnchorKind::Interaction)
            }
            TemporalRangeAnchor::Navigation {
                scope,
                navigation_id,
                window,
            } => {
                let observation = self
                    .timeline
                    .observation_for_payload(
                        scope,
                        ObservationKind::Navigation,
                        ObservationPayloadRef::Navigation(navigation_id),
                    )
                    .await?
                    .ok_or_else(|| {
                        not_found("navigation anchor was not found", scope_context(scope))
                    })?;
                seed_from_observation(
                    observation,
                    scope,
                    window,
                    TemporalRangeAnchorKind::Navigation,
                    ResolvedAnchorReference::Navigation { navigation_id },
                )
            }
            TemporalRangeAnchor::Marker {
                scope,
                marker_id,
                window,
            } => {
                let observation = self
                    .timeline
                    .observation_for_payload(
                        scope,
                        ObservationKind::Marker,
                        ObservationPayloadRef::Marker(marker_id),
                    )
                    .await?
                    .ok_or_else(|| {
                        not_found("marker anchor was not found", scope_context(scope))
                    })?;
                seed_from_observation(
                    observation,
                    scope,
                    window,
                    TemporalRangeAnchorKind::Marker,
                    ResolvedAnchorReference::Marker { marker_id },
                )
            }
        }
    }

    async fn validate_catalog_scope(
        &self,
        session_id: SessionId,
        target_id: TargetId,
    ) -> Result<()> {
        if let Some(session) = self.catalog.session(session_id).await? {
            if session.id() != session_id {
                return Err(invalid(
                    "session catalog identity does not match its lookup",
                ));
            }
        }
        if let Some(target) = self.catalog.target(session_id, target_id).await? {
            if target.id() != target_id {
                return Err(invalid("target catalog identity does not match its lookup"));
            }
        }
        Ok(())
    }

    async fn finalize(
        &self,
        mut seed: RangeSeed,
        options: RangeResolutionOptions,
    ) -> Result<ResolvedRange> {
        let availability = self
            .frames
            .frame_availability(seed.session_id, seed.target_id)
            .await?;
        let frames = match seed.preloaded_frames.take() {
            Some(frames) => frames,
            None => {
                self.frames
                    .frame_metadata_in_range(seed.session_id, seed.target_id, seed.requested_range)
                    .await?
            }
        };
        validate_frame_metadata(&seed, &frames, &availability.evicted_ranges)?;
        if frames.is_empty() {
            let intersects_eviction = availability
                .evicted_ranges
                .iter()
                .any(|range| ranges_intersect(*range, seed.requested_range));
            let (message, captured_bounds) = if intersects_eviction {
                ("requested interval source frames were evicted", None)
            } else {
                (
                    "requested interval has no captured source frames",
                    availability.retained_bounds,
                )
            };
            return Err(range_not_found(
                message,
                &seed,
                seed.requested_range,
                captured_bounds,
            ));
        }

        let (resolved_range, retention_warnings) =
            classify_retention(&seed, &frames, &availability, options)?;
        let retained_frames: Vec<_> = frames
            .into_iter()
            .filter(|frame| resolved_range.contains(frame.session_time()))
            .collect();
        if retained_frames.is_empty() {
            return Err(range_not_found(
                "requested interval source frames were evicted",
                &seed,
                resolved_range,
                None,
            ));
        }

        let gaps = self
            .gaps
            .gaps(seed.session_id, seed.target_id, resolved_range)
            .await?;
        let gaps = clip_capture_gaps(gaps, resolved_range)?;
        if options.capture_gaps == CaptureGapPolicy::Reject && !gaps.is_empty() {
            return Err(range_not_found(
                "requested range contains known capture gaps",
                &seed,
                resolved_range,
                None,
            ));
        }
        let observations = self
            .timeline
            .range(seed.session_id, seed.target_id, resolved_range)
            .await?;
        let mut interaction_ids = Vec::new();
        let mut navigation_ids = Vec::new();
        let mut marker_ids = Vec::new();
        for observation in observations {
            match observation.payload() {
                ObservationPayloadRef::Interaction(id) => push_unique(&mut interaction_ids, *id),
                ObservationPayloadRef::Navigation(id) => push_unique(&mut navigation_ids, *id),
                ObservationPayloadRef::Marker(id) => push_unique(&mut marker_ids, *id),
                _ => {}
            }
        }
        let frame_ids = retained_frames
            .into_iter()
            .map(|frame| frame.id())
            .collect();
        let resolved_anchor = ResolvedAnchor::new(
            seed.anchor_reference,
            seed.requested_anchor_time,
            clamp_time(seed.requested_anchor_time, resolved_range),
        )?;
        ResolvedRange::new_with_anchor(
            seed.session_id,
            seed.target_id,
            seed.anchor_kind,
            resolved_anchor,
            seed.requested_range,
            resolved_range,
            frame_ids,
            interaction_ids,
            navigation_ids,
            marker_ids,
            gaps,
            retention_warnings,
            options,
        )
    }
}

fn validate_frame_metadata(
    seed: &RangeSeed,
    frames: &[CapturedFrame],
    evicted_ranges: &[SessionRange],
) -> Result<()> {
    let mut ids = HashSet::new();
    for frame in frames {
        if frame.session_id() != seed.session_id || frame.target_id() != seed.target_id {
            return Err(persistence_range_error(
                "frame source returned metadata for the wrong session or target",
            ));
        }
        if !seed.requested_range.contains(frame.session_time()) {
            return Err(persistence_range_error(
                "frame source returned metadata outside the requested range",
            ));
        }
        if !ids.insert(frame.id()) {
            return Err(persistence_range_error(
                "frame source returned duplicate frame metadata",
            ));
        }
        if evicted_ranges
            .iter()
            .any(|range| range.contains(frame.session_time()))
        {
            return Err(persistence_range_error(
                "retained frame metadata overlaps durable eviction truth",
            ));
        }
    }
    if frames
        .windows(2)
        .any(|pair| pair[0].capture_ordinal() >= pair[1].capture_ordinal())
    {
        return Err(persistence_range_error(
            "frame metadata is not in capture-ordinal order",
        ));
    }
    Ok(())
}

fn classify_retention(
    seed: &RangeSeed,
    frames: &[CapturedFrame],
    availability: &FrameAvailability,
    options: RangeResolutionOptions,
) -> Result<(SessionRange, Vec<RetentionWarning>)> {
    let retained = availability.retained_bounds.ok_or_else(|| {
        persistence_range_error("frame availability omitted bounds for retained metadata")
    })?;
    let intersecting: Vec<_> = availability
        .evicted_ranges
        .iter()
        .copied()
        .filter(|range| ranges_intersect(*range, seed.requested_range))
        .map(|range| intersection(range, seed.requested_range))
        .collect();

    let has_evictions = !intersecting.is_empty();
    if has_evictions && options.retention == RetentionPolicy::RequireComplete {
        return Err(range_not_found(
            "requested range is not completely retained",
            seed,
            seed.requested_range,
            None,
        ));
    }

    let left_evicted = intersecting
        .iter()
        .any(|range| range.contains(seed.requested_range.start()));
    let right_evicted = intersecting
        .iter()
        .any(|range| range.contains(seed.requested_range.end()));
    if intersecting.iter().any(|range| {
        !range.contains(seed.requested_range.start()) && !range.contains(seed.requested_range.end())
    }) {
        return Err(range_not_found(
            "requested range contains an internal eviction hole",
            seed,
            seed.requested_range,
            None,
        ));
    }

    let candidate = SessionRange::new(
        if left_evicted {
            frames
                .first()
                .expect("non-empty frame metadata")
                .session_time()
        } else {
            seed.requested_range.start()
        },
        if right_evicted {
            frames
                .last()
                .expect("non-empty frame metadata")
                .session_time()
        } else {
            seed.requested_range.end()
        },
    )
    .map_err(|_| {
        range_not_found(
            "requested interval source frames were fully evicted",
            seed,
            seed.requested_range,
            None,
        )
    })?;
    let resolved = if retained.start() > candidate.start() || retained.end() < candidate.end() {
        clamp_natural_interaction_range(seed, candidate, retained, options)?.ok_or_else(|| {
            range_not_found(
                if has_evictions {
                    "requested interval includes uncaptured evidence beyond an evicted edge"
                } else {
                    "requested interval extends beyond captured source-frame bounds"
                },
                seed,
                seed.requested_range,
                (!has_evictions).then_some(retained),
            )
        })?
    } else {
        candidate
    };
    if intersecting
        .iter()
        .any(|range| ranges_intersect(*range, resolved))
    {
        return Err(range_not_found(
            "requested range contains an internal eviction hole",
            seed,
            seed.requested_range,
            None,
        ));
    }

    let mut warnings = Vec::new();
    if resolved.start() > seed.requested_range.start() {
        warnings.push(RetentionWarning::RequestedStartBeforeOldestRetained {
            requested: seed.requested_range.start(),
            oldest_retained: resolved.start(),
        });
    }
    if resolved.end() < seed.requested_range.end() {
        warnings.push(RetentionWarning::RequestedEndAfterNewestRetained {
            requested: seed.requested_range.end(),
            newest_retained: resolved.end(),
        });
    }
    if has_evictions {
        warnings.push(RetentionWarning::PartiallyEvicted {
            requested: seed.requested_range,
            retained: resolved,
        });
    }
    if resolved != candidate {
        warnings.push(RetentionWarning::PartiallyCaptured {
            requested: seed.requested_range,
            retained: resolved,
        });
    }
    if has_evictions {
        warnings.push(RetentionWarning::EvictedRanges {
            ranges: intersecting,
        });
    }
    Ok((resolved, warnings))
}

fn clamp_natural_interaction_range(
    seed: &RangeSeed,
    requested: SessionRange,
    retained: SessionRange,
    options: RangeResolutionOptions,
) -> Result<Option<SessionRange>> {
    if options.retention != RetentionPolicy::AllowPartial
        || !matches!(
            seed.anchor_kind,
            TemporalRangeAnchorKind::Interaction | TemporalRangeAnchorKind::LatestInteraction
        )
        || !ranges_intersect(requested, retained)
    {
        return Ok(None);
    }
    SessionRange::new(
        requested.start().max(retained.start()),
        requested.end().min(retained.end()),
    )
    .map(Some)
}

fn ranges_intersect(left: SessionRange, right: SessionRange) -> bool {
    left.start() <= right.end() && right.start() <= left.end()
}

fn clip_capture_gaps(gaps: Vec<CaptureGap>, range: SessionRange) -> Result<Vec<CaptureGap>> {
    gaps.into_iter()
        .filter_map(|gap| {
            if !ranges_intersect(gap.range(), range) {
                return if gap.estimated_missing_frames().is_some() {
                    Some(Err(persistence_range_error(
                        "capture-gap source returned estimated loss outside the resolved range",
                    )))
                } else {
                    None
                };
            }
            let clipped_range = SessionRange::new(
                SessionTime::from_nanos(
                    gap.range().start().as_nanos().max(range.start().as_nanos()),
                ),
                SessionTime::from_nanos(gap.range().end().as_nanos().min(range.end().as_nanos())),
            )
            .expect("intersecting capture gaps form a valid range");
            Some(
                CaptureGap::new(
                    gap.id(),
                    gap.session_id(),
                    gap.target_id(),
                    clipped_range,
                    gap.observed_time(),
                    *gap.reason(),
                    gap.estimated_missing_frames(),
                    gap.detail().map(str::to_owned),
                )
                .map_err(|_| persistence_range_error("capture gap could not be clipped")),
            )
        })
        .collect()
}

fn intersection(left: SessionRange, right: SessionRange) -> SessionRange {
    SessionRange::new(left.start().max(right.start()), left.end().min(right.end()))
        .expect("intersecting ranges form a valid intersection")
}

fn range_not_found(
    message: impl Into<String>,
    seed: &RangeSeed,
    range: SessionRange,
    retained_bounds: Option<SessionRange>,
) -> KrometrailError {
    let mut error = not_found(
        message,
        ErrorContext {
            session_id: Some(seed.session_id),
            target_id: Some(seed.target_id),
            range: Some(range),
            ..ErrorContext::default()
        },
    );
    if let Some(bounds) = retained_bounds
        && (range.start() < bounds.start() || range.end() > bounds.end())
    {
        error = error
            .with_retry(RetryAdvice::AfterRecovery)
            .with_recovery(
                NonEmptyText::new(format!(
                    "retry with a range contained by captured bounds: start_session_nanos={}, end_session_nanos={}",
                    bounds.start().as_nanos(),
                    bounds.end().as_nanos(),
                ))
                .expect("captured-bound recovery is non-empty"),
            );
    }
    error
}

fn validate_scope_match(
    scope: AnchorScope,
    session_id: SessionId,
    target_id: TargetId,
) -> Result<()> {
    if scope.session_id.is_some_and(|value| value != session_id)
        || scope.target_id.is_some_and(|value| value != target_id)
    {
        return Err(invalid_with_context(
            "range anchor belongs to another session or target",
            scope_context(scope),
        ));
    }
    Ok(())
}

fn wall_clock_offset(
    value: SystemTime,
    started_at: SystemTime,
    scope: IntervalAnchorScope,
) -> Result<SessionTime> {
    let duration = value.duration_since(started_at).map_err(|_| {
        not_found(
            "wall-clock timestamp precedes the recording session",
            interval_scope_context(scope),
        )
    })?;
    let nanos = u64::try_from(duration.as_nanos())
        .map_err(|_| invalid_time("wall-clock offset exceeds session time"))?;
    Ok(SessionTime::from_nanos(nanos))
}

fn seed_from_observation(
    observation: TimelineObservation,
    scope: AnchorScope,
    window: Option<InteractionWindow>,
    kind: TemporalRangeAnchorKind,
    anchor_reference: ResolvedAnchorReference,
) -> Result<RangeSeed> {
    validate_scope_match(scope, observation.session_id(), observation.target_id())?;
    Ok(RangeSeed {
        session_id: observation.session_id(),
        target_id: observation.target_id(),
        requested_range: point_range(observation.session_time(), window)?,
        anchor_kind: kind,
        anchor_reference,
        requested_anchor_time: observation.session_time(),
        preloaded_frames: None,
    })
}

fn push_unique<T: Eq>(values: &mut Vec<T>, value: T) {
    if !values.contains(&value) {
        values.push(value);
    }
}

fn persistence_range_error(message: impl Into<String>) -> KrometrailError {
    KrometrailError::new(
        ErrorCode::PersistenceFailed,
        NonEmptyText::new(message).expect("range persistence errors must have a message"),
    )
}

trait ErrorContextInteraction {
    fn with_interaction(self, interaction_id: InteractionId) -> Self;
}
impl ErrorContextInteraction for ErrorContext {
    fn with_interaction(mut self, interaction_id: InteractionId) -> Self {
        self.interaction_id = Some(interaction_id);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CaptureGapReason, InteractionTiming, IntervalAnchorScope};
    use std::num::NonZeroU64;
    fn ids() -> (
        SessionId,
        TargetId,
        FrameId,
        InteractionId,
        NavigationId,
        MarkerId,
    ) {
        let id = uuid::Uuid::from_u128(1);
        (
            SessionId::from_uuid(id),
            TargetId::from_uuid(id),
            FrameId::from_uuid(id),
            InteractionId::from_uuid(id),
            NavigationId::from_uuid(id),
            MarkerId::from_uuid(id),
        )
    }
    fn resolved(
        requested: SessionRange,
        retained: SessionRange,
        warnings: Vec<RetentionWarning>,
        options: RangeResolutionOptions,
    ) -> Result<ResolvedRange> {
        let (session, target, frame, interaction, navigation, marker) = ids();
        ResolvedRange::new(
            session,
            target,
            TemporalRangeAnchorKind::SessionTime,
            requested,
            retained,
            vec![frame],
            vec![interaction],
            vec![navigation],
            vec![marker],
            vec![],
            warnings,
            options,
        )
    }
    #[test]
    fn registry_names_and_serde_round_trip_from_one_registry() {
        assert_eq!(TemporalRangeAnchorKind::ALL.len(), 7);
        for kind in TemporalRangeAnchorKind::ALL {
            assert_eq!(
                TemporalRangeAnchorKind::from_stable_name(kind.as_str()),
                Some(*kind)
            );
            assert_eq!(
                serde_json::to_string(kind).unwrap(),
                format!("\"{}\"", kind.as_str())
            );
        }
        assert_eq!(TemporalRangeAnchorKind::from_stable_name("future"), None);
        let (session, target, _, _, _, _) = ids();
        let anchor = TemporalRangeAnchor::SessionTime {
            scope: IntervalAnchorScope::new(session, target),
            range: SessionRange::new(SessionTime::ZERO, SessionTime::ZERO).unwrap(),
        };
        assert_eq!(
            serde_json::from_str::<TemporalRangeAnchor>(&serde_json::to_string(&anchor).unwrap())
                .unwrap(),
            anchor
        );
    }
    #[test]
    fn every_resolved_anchor_kind_preserves_exact_identity_time_and_validated_serde() {
        let (session, target, frame, interaction, navigation, marker) = ids();
        let requested = SessionRange::new(SessionTime::ZERO, SessionTime::from_nanos(10)).unwrap();
        let cases = [
            (
                TemporalRangeAnchorKind::SessionTime,
                ResolvedAnchorReference::Interval,
                SessionTime::from_nanos(5),
            ),
            (
                TemporalRangeAnchorKind::WallClock,
                ResolvedAnchorReference::Interval,
                SessionTime::from_nanos(5),
            ),
            (
                TemporalRangeAnchorKind::Interaction,
                ResolvedAnchorReference::Interaction {
                    interaction_id: interaction,
                },
                SessionTime::from_nanos(7),
            ),
            (
                TemporalRangeAnchorKind::Navigation,
                ResolvedAnchorReference::Navigation {
                    navigation_id: navigation,
                },
                SessionTime::from_nanos(7),
            ),
            (
                TemporalRangeAnchorKind::Marker,
                ResolvedAnchorReference::Marker { marker_id: marker },
                SessionTime::from_nanos(7),
            ),
            (
                TemporalRangeAnchorKind::SourceFrame,
                ResolvedAnchorReference::SourceFrames {
                    start_frame_id: frame,
                    end_frame_id: frame,
                },
                SessionTime::from_nanos(5),
            ),
        ];
        for (kind, reference, requested_time) in cases {
            let value = ResolvedRange::new_with_anchor(
                session,
                target,
                kind,
                ResolvedAnchor::new(reference.clone(), requested_time, requested_time).unwrap(),
                requested,
                requested,
                vec![frame],
                vec![interaction],
                vec![navigation],
                vec![marker],
                vec![],
                vec![],
                RangeResolutionOptions::DEFAULT,
            )
            .unwrap();
            assert_eq!(value.resolved_anchor.reference, reference);
            assert_eq!(value.resolved_anchor.requested_time, requested_time);
            let encoded = serde_json::to_string(&value).unwrap();
            assert_eq!(
                serde_json::from_str::<ResolvedRange>(&encoded).unwrap(),
                value
            );
        }
        let valid = ResolvedRange::new(
            session,
            target,
            TemporalRangeAnchorKind::SessionTime,
            requested,
            requested,
            vec![frame],
            vec![interaction],
            vec![navigation],
            vec![marker],
            vec![],
            vec![],
            RangeResolutionOptions::DEFAULT,
        )
        .unwrap();
        let mut invalid_wire = serde_json::to_value(valid).unwrap();
        invalid_wire["anchor_kind"] = serde_json::json!("latest_interaction");
        assert!(serde_json::from_value::<ResolvedRange>(invalid_wire).is_err());
    }

    #[test]
    fn partial_retention_clamps_only_effective_anchor_and_midpoint_is_overflow_safe() {
        let (session, target, frame, _, _, _) = ids();
        let requested = SessionRange::new(
            SessionTime::from_nanos(u64::MAX - 10),
            SessionTime::from_nanos(u64::MAX),
        )
        .unwrap();
        let retained = SessionRange::new(
            SessionTime::from_nanos(u64::MAX - 2),
            SessionTime::from_nanos(u64::MAX),
        )
        .unwrap();
        let warning = RetentionWarning::PartiallyEvicted {
            requested,
            retained,
        };
        let value = ResolvedRange::new_with_anchor(
            session,
            target,
            TemporalRangeAnchorKind::SessionTime,
            ResolvedAnchor::new(
                ResolvedAnchorReference::Interval,
                SessionTime::from_nanos(u64::MAX - 5),
                SessionTime::from_nanos(u64::MAX - 2),
            )
            .unwrap(),
            requested,
            retained,
            vec![frame],
            vec![],
            vec![],
            vec![],
            vec![],
            vec![warning],
            RangeResolutionOptions {
                retention: RetentionPolicy::AllowPartial,
                ..RangeResolutionOptions::DEFAULT
            },
        )
        .unwrap();
        assert_eq!(
            value.resolved_anchor.requested_time,
            SessionTime::from_nanos(u64::MAX - 5)
        );
        assert_eq!(
            value.resolved_anchor.effective_time,
            SessionTime::from_nanos(u64::MAX - 2)
        );

        let mut malformed = serde_json::to_value(value).unwrap();
        malformed["resolved_anchor"]["effective_time"] = serde_json::json!(u64::MAX - 1);
        assert!(serde_json::from_value::<ResolvedRange>(malformed).is_err());
    }

    #[test]
    fn resolved_range_enforces_nonempty_unique_and_contained_contracts() {
        let requested = SessionRange::new(SessionTime::ZERO, SessionTime::from_nanos(10)).unwrap();
        let retained =
            SessionRange::new(SessionTime::from_nanos(2), SessionTime::from_nanos(8)).unwrap();
        let options = RangeResolutionOptions {
            retention: RetentionPolicy::AllowPartial,
            ..RangeResolutionOptions::DEFAULT
        };
        let warning = RetentionWarning::PartiallyEvicted {
            requested,
            retained,
        };
        assert!(resolved(requested, retained, vec![warning.clone()], options).is_ok());
        let (_, _, frame, interaction, navigation, marker) = ids();
        assert!(
            ResolvedRange::new(
                ids().0,
                ids().1,
                TemporalRangeAnchorKind::SessionTime,
                requested,
                retained,
                vec![frame, frame],
                vec![interaction],
                vec![navigation],
                vec![marker],
                vec![],
                vec![warning.clone()],
                options
            )
            .is_err()
        );
        assert!(
            ResolvedRange::new(
                ids().0,
                ids().1,
                TemporalRangeAnchorKind::SessionTime,
                requested,
                retained,
                vec![],
                vec![],
                vec![],
                vec![],
                vec![],
                vec![warning.clone()],
                options
            )
            .is_err()
        );
        assert!(
            ResolvedRange::new(
                ids().0,
                ids().1,
                TemporalRangeAnchorKind::SessionTime,
                requested,
                SessionRange::new(SessionTime::from_nanos(11), SessionTime::from_nanos(11))
                    .unwrap(),
                vec![frame],
                vec![],
                vec![],
                vec![],
                vec![],
                vec![],
                options
            )
            .is_err()
        );
    }
    #[test]
    fn zero_length_ranges_and_checked_windows_are_supported() {
        let range = SessionRange::new(SessionTime::ZERO, SessionTime::ZERO).unwrap();
        let options = RangeResolutionOptions {
            retention: RetentionPolicy::AllowPartial,
            ..RangeResolutionOptions::DEFAULT
        };
        assert!(resolved(range, range, vec![], options).is_ok());
        assert!(InteractionWindow::new(Duration::MAX, Duration::ZERO).is_err());
        let window =
            InteractionWindow::new(Duration::from_millis(1), Duration::from_millis(2)).unwrap();
        assert_eq!(window.as_nanos().unwrap(), (1_000_000, 2_000_000));
        assert_eq!(
            serde_json::to_value(window).unwrap(),
            serde_json::json!({"before_ms": 1, "after_ms": 2})
        );
        assert!(InteractionWindow::new(Duration::from_nanos(1), Duration::ZERO).is_err());
    }
    #[test]
    fn default_interaction_window_saturates_before_zero_and_checks_overflow() {
        let (session, target, _, interaction, _, _) = ids();
        let anchor = InteractionAnchor::new(
            interaction,
            session,
            target,
            crate::BrowserOperationKind::NavigatePage,
            InteractionTiming::new(
                SessionTime::from_nanos(10),
                SessionTime::from_nanos(10),
                SessionTime::from_nanos(20),
                Some(SessionTime::from_nanos(30)),
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(
            interaction_range(
                &anchor,
                InteractionWindow::new(Duration::from_millis(1), Duration::ZERO).unwrap()
            )
            .unwrap()
            .start(),
            SessionTime::ZERO
        );
        let latest = seed_from_interaction(
            anchor,
            InteractionWindow::new(Duration::ZERO, Duration::ZERO).unwrap(),
            TemporalRangeAnchorKind::LatestInteraction,
        )
        .unwrap();
        assert_eq!(latest.requested_anchor_time, SessionTime::from_nanos(10));
        assert_eq!(
            latest.anchor_reference,
            ResolvedAnchorReference::Interaction {
                interaction_id: interaction,
            }
        );
        assert!(InteractionWindow::new(Duration::ZERO, Duration::MAX).is_err());
    }
    #[test]
    fn capture_gap_values_remain_available_to_the_response_contract() {
        let (session, target, _, _, _, _) = ids();
        let gap = CaptureGap::new(
            crate::GapId::from_uuid(uuid::Uuid::from_u128(2)),
            session,
            target,
            SessionRange::new(SessionTime::ZERO, SessionTime::ZERO).unwrap(),
            crate::ObservedTime::from_nanos(1),
            CaptureGapReason::CaptureStopped,
            NonZeroU64::new(1),
            None,
        )
        .unwrap();
        assert_eq!(gap.target_id(), target);
    }

    #[test]
    fn resolver_gap_clipping_preserves_estimated_loss() {
        let (session, target, _, _, _, _) = ids();
        let gap = CaptureGap::new(
            crate::GapId::from_uuid(uuid::Uuid::from_u128(3)),
            session,
            target,
            SessionRange::new(SessionTime::from_nanos(1), SessionTime::from_nanos(20)).unwrap(),
            crate::ObservedTime::from_nanos(20),
            CaptureGapReason::CaptureStopped,
            NonZeroU64::new(7),
            Some("capture tail".into()),
        )
        .unwrap();
        let resolved =
            SessionRange::new(SessionTime::from_nanos(5), SessionTime::from_nanos(10)).unwrap();
        let clipped = clip_capture_gaps(vec![gap], resolved).unwrap();
        assert_eq!(clipped.len(), 1);
        assert_eq!(clipped[0].range(), resolved);
        assert_eq!(clipped[0].estimated_missing_frames(), NonZeroU64::new(7));
        assert_eq!(clipped[0].detail(), Some("capture tail"));

        let outside = CaptureGap::new(
            crate::GapId::from_uuid(uuid::Uuid::from_u128(4)),
            session,
            target,
            SessionRange::new(SessionTime::from_nanos(30), SessionTime::from_nanos(40)).unwrap(),
            crate::ObservedTime::from_nanos(40),
            CaptureGapReason::CaptureStopped,
            None,
            None,
        )
        .unwrap();
        assert!(
            clip_capture_gaps(vec![outside], resolved)
                .unwrap()
                .is_empty()
        );
    }
}
