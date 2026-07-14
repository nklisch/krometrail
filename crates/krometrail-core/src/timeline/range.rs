use std::{
    collections::HashSet,
    time::{Duration, SystemTime},
};

use serde::{Deserialize, Serialize};

use crate::{
    CaptureGap, FrameId, InteractionAnchor, InteractionId, MarkerId, NavigationId, ObservationKind,
    Result, SessionId, SessionRange, SessionTime, TargetId, TimelineObservation,
    error::{ErrorCode, ErrorContext, KrometrailError, NonEmptyText, invalid, invalid_time},
    validation::deserialize_validated,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
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

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnchorScope {
    pub session_id: Option<SessionId>,
    pub target_id: Option<TargetId>,
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

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
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

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum RetentionPolicy {
    RequireComplete,
    AllowPartial,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum CaptureGapPolicy {
    Include,
    Reject,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
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

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "anchor", rename_all = "snake_case")]
pub enum TemporalRangeAnchor {
    SessionTime {
        scope: AnchorScope,
        range: SessionRange,
    },
    WallClock {
        scope: AnchorScope,
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
        scope: AnchorScope,
        range: SessionRange,
    },
    WallClock {
        scope: AnchorScope,
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
            Self::SessionTime { scope, .. } => {
                required_scope(*scope)?;
            }
            Self::WallClock { scope, start, end } => {
                required_scope(*scope)?;
                if start > end {
                    return Err(invalid_with_context(
                        "wall-clock range start must not exceed its end",
                        scope_context(*scope),
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

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
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
            if ranges[0].start() > ranges[1].start() || ranges[0].end() >= ranges[1].start() {
                return Err(invalid(
                    "evicted frame ranges must be sorted and non-overlapping",
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

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
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
    EvictedRanges {
        ranges: Vec<SessionRange>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ResolvedRange {
    pub session_id: SessionId,
    pub target_id: TargetId,
    pub anchor_kind: TemporalRangeAnchorKind,
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
impl ResolvedRange {
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
        if frame_ids.is_empty() {
            return Err(invalid(
                "resolved range must contain at least one retained frame",
            ));
        }
        if has_duplicates(&frame_ids)
            || has_duplicates(&interaction_ids)
            || has_duplicates(&navigation_ids)
            || has_duplicates(&marker_ids)
        {
            return Err(invalid("resolved range identifiers must be unique"));
        }
        if resolved_range.start() < requested_range.start()
            || resolved_range.end() > requested_range.end()
        {
            return Err(invalid(
                "resolved range must be contained in its requested range",
            ));
        }
        let partial = resolved_range != requested_range;
        if partial && options.retention == RetentionPolicy::RequireComplete {
            return Err(invalid("complete retention cannot return a partial range"));
        }
        if partial && retention_warnings.is_empty() {
            return Err(invalid("partial retention requires retention warnings"));
        }
        if !partial && !retention_warnings.is_empty() {
            return Err(invalid(
                "retention warnings require a partial resolved range",
            ));
        }
        Ok(Self {
            session_id,
            target_id,
            anchor_kind,
            requested_range,
            resolved_range,
            frame_ids,
            interaction_ids,
            navigation_ids,
            marker_ids,
            gaps,
            retention_warnings,
            options,
        })
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
    preloaded_frames: Option<Vec<CapturedFrame>>,
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
                let (session_id, target_id) = required_scope(scope)?;
                self.validate_catalog_scope(session_id, target_id).await?;
                Ok(RangeSeed {
                    session_id,
                    target_id,
                    requested_range: range,
                    anchor_kind: TemporalRangeAnchorKind::SessionTime,
                    preloaded_frames: None,
                })
            }
            TemporalRangeAnchor::WallClock { scope, start, end } => {
                let (session_id, target_id) = required_scope(scope)?;
                if start > end {
                    return Err(invalid_with_context(
                        "wall-clock range start must not exceed its end",
                        scope_context(scope),
                    ));
                }
                let session = self.catalog.session(session_id).await?.ok_or_else(|| {
                    not_found(
                        "wall-clock resolution requires complete session metadata",
                        scope_context(scope),
                    )
                })?;
                let start = wall_clock_offset(start, session.started_at(), scope)?;
                let end = wall_clock_offset(end, session.started_at(), scope)?;
                self.validate_catalog_scope(session_id, target_id).await?;
                Ok(RangeSeed {
                    session_id,
                    target_id,
                    requested_range: SessionRange::new(start, end)?,
                    anchor_kind: TemporalRangeAnchorKind::WallClock,
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
                Ok(RangeSeed {
                    session_id: interaction.session_id,
                    target_id: interaction.target_id,
                    requested_range: interaction_range(&interaction, window)?,
                    anchor_kind: TemporalRangeAnchorKind::Interaction,
                    preloaded_frames: None,
                })
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
                Ok(RangeSeed {
                    session_id,
                    target_id,
                    requested_range: interaction_range(&interaction, window)?,
                    anchor_kind: TemporalRangeAnchorKind::LatestInteraction,
                    preloaded_frames: None,
                })
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
                seed_from_observation(observation, scope, window, TemporalRangeAnchorKind::Marker)
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
            let message = if availability
                .evicted_ranges
                .iter()
                .any(|range| ranges_intersect(*range, seed.requested_range))
            {
                "requested interval source frames were evicted"
            } else {
                "requested interval has no captured source frames"
            };
            return Err(range_not_found(message, &seed, seed.requested_range));
        }

        let (resolved_range, retention_warnings) =
            classify_retention(&seed, &frames, &availability, options.retention)?;
        let retained_frames: Vec<_> = frames
            .into_iter()
            .filter(|frame| resolved_range.contains(frame.session_time()))
            .collect();
        if retained_frames.is_empty() {
            return Err(range_not_found(
                "requested interval source frames were evicted",
                &seed,
                resolved_range,
            ));
        }

        let gaps = self
            .gaps
            .gaps(seed.session_id, seed.target_id, resolved_range)
            .await?;
        if options.capture_gaps == CaptureGapPolicy::Reject && !gaps.is_empty() {
            return Err(range_not_found(
                "requested range contains known capture gaps",
                &seed,
                resolved_range,
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
        ResolvedRange::new(
            seed.session_id,
            seed.target_id,
            seed.anchor_kind,
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
    policy: RetentionPolicy,
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

    if intersecting.is_empty() {
        if retained.start() > seed.requested_range.start()
            || retained.end() < seed.requested_range.end()
        {
            return Err(range_not_found(
                "requested interval extends beyond captured source-frame bounds",
                seed,
                seed.requested_range,
            ));
        }
        return Ok((seed.requested_range, Vec::new()));
    }
    if policy == RetentionPolicy::RequireComplete {
        return Err(range_not_found(
            "requested range is not completely retained",
            seed,
            seed.requested_range,
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
        ));
    }

    let first = frames
        .first()
        .expect("non-empty frame metadata")
        .session_time();
    let last = frames
        .last()
        .expect("non-empty frame metadata")
        .session_time();
    let start = if left_evicted {
        first
    } else {
        seed.requested_range.start()
    };
    let end = if right_evicted {
        last
    } else {
        seed.requested_range.end()
    };
    let resolved = SessionRange::new(start, end).map_err(|_| {
        range_not_found(
            "requested interval source frames were fully evicted",
            seed,
            seed.requested_range,
        )
    })?;
    if retained.start() > resolved.start() || retained.end() < resolved.end() {
        return Err(range_not_found(
            "requested interval includes uncaptured evidence beyond an evicted edge",
            seed,
            seed.requested_range,
        ));
    }
    if intersecting
        .iter()
        .any(|range| ranges_intersect(*range, resolved))
    {
        return Err(range_not_found(
            "requested range contains an internal eviction hole",
            seed,
            seed.requested_range,
        ));
    }

    let mut warnings = Vec::new();
    if left_evicted {
        warnings.push(RetentionWarning::RequestedStartBeforeOldestRetained {
            requested: seed.requested_range.start(),
            oldest_retained: resolved.start(),
        });
    }
    if right_evicted {
        warnings.push(RetentionWarning::RequestedEndAfterNewestRetained {
            requested: seed.requested_range.end(),
            newest_retained: resolved.end(),
        });
    }
    warnings.push(RetentionWarning::PartiallyEvicted {
        requested: seed.requested_range,
        retained: resolved,
    });
    warnings.push(RetentionWarning::EvictedRanges {
        ranges: intersecting,
    });
    Ok((resolved, warnings))
}

fn ranges_intersect(left: SessionRange, right: SessionRange) -> bool {
    left.start() <= right.end() && right.start() <= left.end()
}

fn intersection(left: SessionRange, right: SessionRange) -> SessionRange {
    SessionRange::new(left.start().max(right.start()), left.end().min(right.end()))
        .expect("intersecting ranges form a valid intersection")
}

fn range_not_found(
    message: &'static str,
    seed: &RangeSeed,
    range: SessionRange,
) -> KrometrailError {
    not_found(
        message,
        ErrorContext {
            session_id: Some(seed.session_id),
            target_id: Some(seed.target_id),
            range: Some(range),
            ..ErrorContext::default()
        },
    )
}

fn required_scope(scope: AnchorScope) -> Result<(SessionId, TargetId)> {
    match (scope.session_id, scope.target_id) {
        (Some(session_id), Some(target_id)) => Ok((session_id, target_id)),
        _ => Err(invalid_with_context(
            "range anchor requires both session and target scope",
            scope_context(scope),
        )),
    }
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
    scope: AnchorScope,
) -> Result<SessionTime> {
    let duration = value.duration_since(started_at).map_err(|_| {
        not_found(
            "wall-clock timestamp precedes the recording session",
            scope_context(scope),
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
) -> Result<RangeSeed> {
    validate_scope_match(scope, observation.session_id(), observation.target_id())?;
    Ok(RangeSeed {
        session_id: observation.session_id(),
        target_id: observation.target_id(),
        requested_range: point_range(observation.session_time(), window)?,
        anchor_kind: kind,
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
    use crate::{CaptureGapReason, InteractionTiming};
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
            scope: AnchorScope::new(Some(session), Some(target)),
            range: SessionRange::new(SessionTime::ZERO, SessionTime::ZERO).unwrap(),
        };
        assert_eq!(
            serde_json::from_str::<TemporalRangeAnchor>(&serde_json::to_string(&anchor).unwrap())
                .unwrap(),
            anchor
        );
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
}
