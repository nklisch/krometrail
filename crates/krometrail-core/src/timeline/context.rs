use std::{collections::BTreeMap, num::NonZeroU8};

use serde::{Deserialize, Serialize};

use crate::{
    BrowserEvent, BrowserEventClass, BrowserEventCollectionGap, BrowserEventCursor,
    BrowserEventPayload, BrowserEventSelector, BrowserEventSeverity, BrowserEventSource,
    BrowserEventUnavailableRange, CapabilityId, CaptureGap, CaptureStatusSamples,
    CaptureStreamState, CaptureWarning, CapturedFrame, DeviceScaleFactor, ErrorCode, ErrorContext,
    EventCandidateLimit, EventPageLimit, FrameId, FrameSource, KrometrailError, NonEmptyText,
    OperationMutability, PixelDimensions, PortFuture, ResolvedRange, Result, RetentionWarning,
    RetryAdvice, SessionRange, SessionTime, TargetCaptureStatus, validation::deserialize_validated,
};

pub const MAX_CAPTURE_QUALITY_FRAMES: usize = 20_000;
pub const MAX_FOCUS_TIMES: usize = 16;
pub const DEFAULT_COMPACT_EVENT_LIMIT: u8 = 24;
pub const MAX_COMPACT_EVENT_LIMIT: u8 = 64;
pub const DEFAULT_CHRONOLOGICAL_EVENT_LIMIT: u16 = crate::DEFAULT_EVENT_PAGE_ROWS;
const OPERATIONAL_EVENT_SCAN_LIMIT: u16 = crate::MAX_EVENT_PAGE_ROWS;

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BrowserEventFilter {
    classes: Vec<BrowserEventClass>,
    minimum_severity: BrowserEventSeverity,
}

#[derive(Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
struct BrowserEventFilterWire {
    classes: Vec<BrowserEventClass>,
    minimum_severity: BrowserEventSeverity,
}

impl BrowserEventFilter {
    pub fn new(
        mut classes: Vec<BrowserEventClass>,
        minimum_severity: BrowserEventSeverity,
    ) -> Result<Self> {
        classes.sort_unstable();
        if classes.windows(2).any(|pair| pair[0] == pair[1]) {
            return Err(invalid_error("browser event filter classes must be unique"));
        }
        Ok(Self {
            classes,
            minimum_severity,
        })
    }

    pub fn classes(&self) -> &[BrowserEventClass] {
        &self.classes
    }

    pub const fn minimum_severity(&self) -> BrowserEventSeverity {
        self.minimum_severity
    }

    fn selector(
        &self,
        range: &ResolvedRange,
        effective: SessionRange,
    ) -> Result<BrowserEventSelector> {
        BrowserEventSelector::new(
            range.session_id,
            range.target_id,
            effective,
            self.classes.clone(),
            self.minimum_severity,
        )
    }
}

impl Default for BrowserEventFilter {
    fn default() -> Self {
        Self {
            classes: Vec::new(),
            minimum_severity: BrowserEventSeverity::Debug,
        }
    }
}

impl<'de> Deserialize<'de> for BrowserEventFilter {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserialize_validated(deserializer, |wire: BrowserEventFilterWire| {
            Self::new(wire.classes, wire.minimum_severity)
        })
    }
}

crate::validation::delegate_json_schema!(BrowserEventFilter => BrowserEventFilterWire);

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, schemars::JsonSchema)]
#[serde(transparent)]
pub struct EventCompactLimit(NonZeroU8);

impl EventCompactLimit {
    pub fn new(value: u8) -> Result<Self> {
        NonZeroU8::new(value)
            .filter(|value| value.get() <= MAX_COMPACT_EVENT_LIMIT)
            .map(Self)
            .ok_or_else(|| invalid_error("compact browser event limit is out of range"))
    }

    pub const fn get(self) -> u8 {
        self.0.get()
    }
}

impl Default for EventCompactLimit {
    fn default() -> Self {
        Self::new(DEFAULT_COMPACT_EVENT_LIMIT).expect("default compact event limit is valid")
    }
}

impl<'de> Deserialize<'de> for EventCompactLimit {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserialize_validated(deserializer, Self::new)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "mode", rename_all = "snake_case", deny_unknown_fields)]
pub enum BrowserEventSelection {
    Compact {
        #[serde(default)]
        limit: EventCompactLimit,
    },
    Chronological {
        #[serde(default)]
        limit: EventPageLimit,
        #[serde(default)]
        cursor: Option<BrowserEventCursor>,
    },
}

impl BrowserEventSelection {
    pub fn compact(limit: u8) -> Result<Self> {
        Ok(Self::Compact {
            limit: EventCompactLimit::new(limit)?,
        })
    }

    pub fn chronological(limit: u16, cursor: Option<BrowserEventCursor>) -> Result<Self> {
        Ok(Self::Chronological {
            limit: EventPageLimit::new(limit)?,
            cursor,
        })
    }

    pub fn compact_default() -> Self {
        Self::Compact {
            limit: EventCompactLimit::default(),
        }
    }

    pub fn chronological_default() -> Self {
        Self::Chronological {
            limit: EventPageLimit::new(DEFAULT_CHRONOLOGICAL_EVENT_LIMIT)
                .expect("default chronological event limit is valid"),
            cursor: None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TemporalContextRequest {
    range: ResolvedRange,
    clip: Option<SessionRange>,
    filter: BrowserEventFilter,
    selection: BrowserEventSelection,
    focus_times: Vec<SessionTime>,
}

#[derive(Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
struct TemporalContextRequestWire {
    range: ResolvedRange,
    clip: Option<SessionRange>,
    filter: BrowserEventFilter,
    selection: BrowserEventSelection,
    #[serde(default)]
    focus_times: Vec<SessionTime>,
}

/// The external wire shape for verbose browser-event detail. This intentionally
/// has no compact variant: the same core wire type drives deserialization and
/// the generated public schema, so they cannot drift apart.
#[derive(Deserialize, schemars::JsonSchema)]
#[serde(tag = "mode", rename_all = "snake_case", deny_unknown_fields)]
enum BrowserEventDetailSelectionWire {
    Chronological {
        #[serde(default)]
        limit: EventPageLimit,
        #[serde(default)]
        cursor: Option<BrowserEventCursor>,
    },
}

#[derive(Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
struct BrowserEventDetailRequestWire {
    range: ResolvedRange,
    clip: Option<SessionRange>,
    filter: BrowserEventFilter,
    selection: BrowserEventDetailSelectionWire,
    #[serde(default)]
    focus_times: Vec<SessionTime>,
}

impl BrowserEventDetailRequestWire {
    fn into_request(self) -> Result<BrowserEventDetailRequest> {
        let BrowserEventDetailSelectionWire::Chronological { limit, cursor } = self.selection;
        BrowserEventDetailRequest::new(
            self.range,
            self.clip,
            self.filter,
            limit.get(),
            cursor,
            self.focus_times,
        )
    }
}

impl TemporalContextRequest {
    pub fn compact(range: ResolvedRange, focus_times: Vec<SessionTime>) -> Result<Self> {
        Self::new(
            range,
            None,
            BrowserEventFilter::default(),
            BrowserEventSelection::compact_default(),
            focus_times,
        )
    }

    pub fn new(
        range: ResolvedRange,
        clip: Option<SessionRange>,
        filter: BrowserEventFilter,
        selection: BrowserEventSelection,
        focus_times: Vec<SessionTime>,
    ) -> Result<Self> {
        validate_resolved_range(&range)?;
        if range.frame_ids.len() > MAX_CAPTURE_QUALITY_FRAMES {
            return Err(context_error(
                ErrorCode::ResourceLimitExceeded,
                "temporal context frame count exceeds its limit",
                &range,
                None,
            ));
        }
        let effective = match clip {
            Some(clip)
                if clip.start() >= range.resolved_range.start()
                    && clip.end() <= range.resolved_range.end() =>
            {
                clip
            }
            Some(_) => {
                return Err(context_error(
                    ErrorCode::InvalidInput,
                    "temporal context clip must be contained in the resolved range",
                    &range,
                    None,
                ));
            }
            None => range.resolved_range,
        };
        if focus_times.len() > MAX_FOCUS_TIMES
            || focus_times.iter().any(|time| !effective.contains(*time))
        {
            return Err(context_error(
                ErrorCode::InvalidInput,
                "temporal context focus times are outside their bounded range",
                &range,
                None,
            ));
        }
        let selector = filter.selector(&range, effective)?;
        if let BrowserEventSelection::Chronological {
            cursor: Some(cursor),
            ..
        } = &selection
            && cursor.selector() != &selector
        {
            return Err(context_error(
                ErrorCode::InvalidInput,
                "browser event cursor does not match the temporal context request",
                &range,
                None,
            ));
        }
        Ok(Self {
            range,
            clip,
            filter,
            selection,
            focus_times,
        })
    }

    pub const fn range(&self) -> &ResolvedRange {
        &self.range
    }

    pub const fn clip(&self) -> Option<SessionRange> {
        self.clip
    }

    pub const fn filter(&self) -> &BrowserEventFilter {
        &self.filter
    }

    pub const fn selection(&self) -> &BrowserEventSelection {
        &self.selection
    }

    pub fn focus_times(&self) -> &[SessionTime] {
        &self.focus_times
    }

    pub fn effective_range(&self) -> Result<SessionRange> {
        Ok(self.clip.unwrap_or(self.range.resolved_range))
    }
}

impl<'de> Deserialize<'de> for TemporalContextRequest {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserialize_validated(deserializer, |wire: TemporalContextRequestWire| {
            Self::new(
                wire.range,
                wire.clip,
                wire.filter,
                wire.selection,
                wire.focus_times,
            )
        })
    }
}

/// Validated verbose browser-event detail. Unlike the broader context request,
/// this public operation intentionally cannot select compact correlation output.
#[derive(Clone, Debug, PartialEq)]
pub struct BrowserEventDetailRequest(TemporalContextRequest);

impl BrowserEventDetailRequest {
    pub fn new(
        range: ResolvedRange,
        clip: Option<SessionRange>,
        filter: BrowserEventFilter,
        limit: u16,
        cursor: Option<BrowserEventCursor>,
        focus_times: Vec<SessionTime>,
    ) -> Result<Self> {
        let selection = BrowserEventSelection::chronological(limit, cursor)?;
        Ok(Self(TemporalContextRequest::new(
            range,
            clip,
            filter,
            selection,
            focus_times,
        )?))
    }

    pub fn into_context_request(self) -> TemporalContextRequest {
        self.0
    }

    pub const fn context_request(&self) -> &TemporalContextRequest {
        &self.0
    }
}

impl Serialize for BrowserEventDetailRequest {
    fn serialize<S: serde::Serializer>(
        &self,
        serializer: S,
    ) -> std::result::Result<S::Ok, S::Error> {
        self.0.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for BrowserEventDetailRequest {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserialize_validated(deserializer, BrowserEventDetailRequestWire::into_request)
    }
}

crate::validation::delegate_json_schema!(BrowserEventDetailRequest => BrowserEventDetailRequestWire);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TemporalContextOperationKind {
    QueryBrowserEvents,
}

impl TemporalContextOperationKind {
    pub const ALL: &'static [Self] = &[Self::QueryBrowserEvents];

    pub const fn stable_name(self) -> &'static str {
        match self {
            Self::QueryBrowserEvents => "query_browser_events",
        }
    }

    pub fn input_schema(self) -> schemars::Schema {
        match self {
            Self::QueryBrowserEvents => schemars::schema_for!(BrowserEventDetailRequest),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TemporalContextOperationDefinition {
    pub kind: TemporalContextOperationKind,
    pub stable_name: &'static str,
    pub description: &'static str,
    pub capability: CapabilityId,
    pub mutability: OperationMutability,
}

pub static TEMPORAL_CONTEXT_OPERATION_REGISTRY: &[TemporalContextOperationDefinition] =
    &[TemporalContextOperationDefinition {
        kind: TemporalContextOperationKind::QueryBrowserEvents,
        stable_name: "query_browser_events",
        description: "Read chronological browser-event detail for a resolved temporal range.",
        capability: CapabilityId::BrowserEvents,
        mutability: OperationMutability::ReadOnly,
    }];

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct FramePoint {
    pub frame_id: FrameId,
    pub capture_ordinal: crate::CaptureOrdinal,
    pub session_time: SessionTime,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CadenceSummary {
    pub interval_count: u64,
    pub min_nanos: u64,
    pub median_nanos: u64,
    pub p95_nanos: u64,
    pub max_nanos: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CaptureWarningSummary {
    pub warning: CaptureWarning,
    pub frame_count: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CaptureGapSummary {
    pub gap_count: u64,
    pub covered_duration_nanos: u64,
    pub known_missing_frames: u64,
    pub has_unknown_missing_estimate: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CaptureStatusPoint {
    pub session_time: SessionTime,
    pub attachment_generation: u64,
    pub state: CaptureStreamState,
    pub status: TargetCaptureStatus,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CaptureStatusBound {
    pub state: CaptureStreamState,
    pub established_at: SessionTime,
    pub attachment_generation: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CaptureStatusEvidence {
    pub at_range_start: Option<CaptureStatusBound>,
    pub at_range_end: Option<CaptureStatusBound>,
    pub transitions: Vec<CaptureStatusPoint>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CaptureQualityWarning {
    CaptureStatusMissing,
    CaptureStatusUnavailable,
    CaptureStatusTruncated,
}

/// One contiguous run of frames sharing a visual epoch (identical image
/// dimensions, viewport dimensions, and device scale factor). Metadata-only:
/// computed from already-loaded frame metadata without decoding pixels.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct EpochSummary {
    pub epoch_index: u32,
    pub range: SessionRange,
    pub frame_count: u64,
    pub first_frame: FramePoint,
    pub last_frame: FramePoint,
    pub image: PixelDimensions,
    pub viewport: PixelDimensions,
    pub device_scale_factor: DeviceScaleFactor,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CaptureQuality {
    pub requested_range: SessionRange,
    pub retained_range: SessionRange,
    pub frame_count: u64,
    pub first_frame: FramePoint,
    pub last_frame: FramePoint,
    pub cadence: Option<CadenceSummary>,
    pub frame_warnings: Vec<CaptureWarningSummary>,
    pub gaps: Vec<CaptureGap>,
    pub gap_summary: CaptureGapSummary,
    pub retention_warnings: Vec<RetentionWarning>,
    /// Exact visual-epoch runs over the resolved frame metadata. The domain
    /// vector is exact; bounded presentation is the response projector's job.
    pub epochs: Vec<EpochSummary>,
    pub capture_status: CaptureStatusEvidence,
    pub warnings: Vec<CaptureQualityWarning>,
}

/// Deterministic inclusion metadata. Focus distance is temporal correlation only
/// and never attributes a browser event as the cause of visual change.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "selection", rename_all = "snake_case")]
pub enum BrowserEventSelectionReason {
    CompactCorrelation {
        compact_priority: u8,
        nearest_focus_distance_nanos: Option<u64>,
    },
    Chronological,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SelectedBrowserEvent {
    pub event: BrowserEvent,
    pub reason: BrowserEventSelectionReason,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventQueryWarning {
    Truncated {
        matched_count: u64,
        returned_count: u64,
    },
    CollectionGapsPresent {
        gap_count: u64,
    },
    CollectionEvidenceTruncated {
        matched_operational_count: u64,
        scanned_count: u64,
    },
    UnavailableRangesPresent {
        range_count: u64,
    },
    UnavailableRangesTruncated,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct BrowserEventContext {
    pub effective_range: SessionRange,
    pub matched_count: u64,
    pub returned_count: u64,
    pub events: Vec<SelectedBrowserEvent>,
    pub next_cursor: Option<BrowserEventCursor>,
    pub collection_gaps: Vec<BrowserEventCollectionGap>,
    pub unavailable_ranges: Vec<BrowserEventUnavailableRange>,
    pub warnings: Vec<EventQueryWarning>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct TemporalContext {
    pub range: ResolvedRange,
    pub capture_quality: CaptureQuality,
    pub browser_events: BrowserEventContext,
}

pub trait TemporalContextQuery: Send + Sync {
    fn context(&self, request: TemporalContextRequest) -> PortFuture<'_, Result<TemporalContext>>;

    /// Read only capture quality without loading browser-event rows.
    fn capture_quality(&self, _range: ResolvedRange) -> PortFuture<'_, Result<CaptureQuality>> {
        Box::pin(std::future::ready(Err(KrometrailError::new(
            ErrorCode::Unsupported,
            NonEmptyText::new("capture-quality-only queries are unavailable")
                .expect("static unsupported message is non-empty"),
        ))))
    }
}

pub struct TemporalContextService<F, E> {
    frames: F,
    events: E,
}

impl<F, E> TemporalContextService<F, E> {
    pub const fn new(frames: F, events: E) -> Self {
        Self { frames, events }
    }
}

impl<F, E> TemporalContextQuery for TemporalContextService<F, E>
where
    F: FrameSource,
    E: BrowserEventSource,
{
    fn context(&self, request: TemporalContextRequest) -> PortFuture<'_, Result<TemporalContext>> {
        Box::pin(async move {
            let effective = request.effective_range()?;
            let metadata = self
                .frames
                .frame_metadata_by_id(request.range.frame_ids.clone())
                .await
                .map_err(|error| {
                    map_source_error(
                        error,
                        "resolved frame metadata could not be read",
                        &request.range,
                    )
                })?;
            validate_frame_metadata(&request.range, &metadata)?;

            let unavailable_ranges = self
                .events
                .unavailable_ranges(
                    request.range.session_id,
                    request.range.target_id,
                    effective,
                    crate::MAX_EVENT_UNAVAILABLE_RANGES,
                )
                .await
                .map_err(|error| {
                    map_source_error(
                        error,
                        "browser event availability could not be read",
                        &request.range,
                    )
                })?;
            let status_samples = self
                .events
                .capture_status_samples(
                    request.range.session_id,
                    request.range.target_id,
                    effective,
                    crate::MAX_CAPTURE_STATUS_SAMPLES,
                )
                .await
                .map_err(|error| {
                    map_source_error(
                        error,
                        "capture status evidence could not be read",
                        &request.range,
                    )
                })?;
            let (capture_status, capture_warnings) = capture_status_evidence(
                &request.range,
                effective,
                status_samples,
                &unavailable_ranges,
            )?;
            let capture_quality =
                capture_quality(&request.range, &metadata, capture_status, capture_warnings)?;
            let browser_events = self
                .browser_event_context(&request, effective, unavailable_ranges)
                .await?;
            Ok(TemporalContext {
                range: request.range,
                capture_quality,
                browser_events,
            })
        })
    }

    fn capture_quality(&self, range: ResolvedRange) -> PortFuture<'_, Result<CaptureQuality>> {
        Box::pin(async move {
            let metadata = self
                .frames
                .frame_metadata_by_id(range.frame_ids.clone())
                .await
                .map_err(|error| {
                    map_source_error(error, "resolved frame metadata could not be read", &range)
                })?;
            validate_frame_metadata(&range, &metadata)?;
            let unavailable_ranges = self
                .events
                .unavailable_ranges(
                    range.session_id,
                    range.target_id,
                    range.resolved_range,
                    crate::MAX_EVENT_UNAVAILABLE_RANGES,
                )
                .await
                .map_err(|error| {
                    map_source_error(
                        error,
                        "capture quality availability could not be read",
                        &range,
                    )
                })?;
            let status_samples = self
                .events
                .capture_status_samples(
                    range.session_id,
                    range.target_id,
                    range.resolved_range,
                    crate::MAX_CAPTURE_STATUS_SAMPLES,
                )
                .await
                .map_err(|error| {
                    map_source_error(error, "capture status evidence could not be read", &range)
                })?;
            let (capture_status, capture_warnings) = capture_status_evidence(
                &range,
                range.resolved_range,
                status_samples,
                &unavailable_ranges,
            )?;
            capture_quality(&range, &metadata, capture_status, capture_warnings)
        })
    }
}

impl<F, E> TemporalContextService<F, E>
where
    F: FrameSource,
    E: BrowserEventSource,
{
    async fn browser_event_context(
        &self,
        request: &TemporalContextRequest,
        effective: SessionRange,
        unavailable_ranges: Vec<BrowserEventUnavailableRange>,
    ) -> Result<BrowserEventContext> {
        let selector = request.filter.selector(&request.range, effective)?;
        let matched_count = self
            .events
            .count_events(selector.clone())
            .await
            .map_err(|error| {
                map_source_error(
                    error,
                    "browser event count could not be read",
                    &request.range,
                )
            })?;
        let (events, next_cursor) = match &request.selection {
            BrowserEventSelection::Compact { limit } => {
                (self.compact_events(request, selector, *limit).await?, None)
            }
            BrowserEventSelection::Chronological { limit, cursor } => {
                self.chronological_events(&request.range, selector, cursor.clone(), *limit)
                    .await?
            }
        };

        let operational_selector = BrowserEventSelector::new(
            request.range.session_id,
            request.range.target_id,
            effective,
            vec![BrowserEventClass::Operational],
            BrowserEventSeverity::Debug,
        )?;
        let operational_count = self
            .events
            .count_events(operational_selector.clone())
            .await
            .map_err(|error| {
                map_source_error(
                    error,
                    "collection evidence count could not be read",
                    &request.range,
                )
            })?;
        let operational = self
            .events
            .chronological_events(
                operational_selector,
                None,
                EventPageLimit::new(OPERATIONAL_EVENT_SCAN_LIMIT)?,
            )
            .await
            .map_err(|error| {
                map_source_error(
                    error,
                    "collection evidence could not be read",
                    &request.range,
                )
            })?;
        let scanned_count = u64::try_from(operational.len()).unwrap_or(u64::MAX);
        let collection_gaps = operational
            .into_iter()
            .filter_map(|event| match event.payload() {
                BrowserEventPayload::CollectionGap(gap) => Some(gap.clone()),
                _ => None,
            })
            .collect::<Vec<_>>();

        let mut warnings = Vec::new();
        let returned_count = u64::try_from(events.len()).unwrap_or(u64::MAX);
        if matched_count > returned_count {
            warnings.push(EventQueryWarning::Truncated {
                matched_count,
                returned_count,
            });
        }
        if !collection_gaps.is_empty() {
            warnings.push(EventQueryWarning::CollectionGapsPresent {
                gap_count: u64::try_from(collection_gaps.len()).unwrap_or(u64::MAX),
            });
        }
        if operational_count > scanned_count {
            warnings.push(EventQueryWarning::CollectionEvidenceTruncated {
                matched_operational_count: operational_count,
                scanned_count,
            });
        }
        if !unavailable_ranges.is_empty() {
            warnings.push(EventQueryWarning::UnavailableRangesPresent {
                range_count: u64::try_from(unavailable_ranges.len()).unwrap_or(u64::MAX),
            });
        }
        if unavailable_ranges.len() == usize::from(crate::MAX_EVENT_UNAVAILABLE_RANGES) {
            warnings.push(EventQueryWarning::UnavailableRangesTruncated);
        }
        Ok(BrowserEventContext {
            effective_range: effective,
            matched_count,
            returned_count,
            events,
            next_cursor,
            collection_gaps,
            unavailable_ranges,
            warnings,
        })
    }

    async fn compact_events(
        &self,
        request: &TemporalContextRequest,
        selector: BrowserEventSelector,
        limit: EventCompactLimit,
    ) -> Result<Vec<SelectedBrowserEvent>> {
        let candidate_limit = u16::from(limit.get()) * 4;
        let mut candidates = self
            .events
            .priority_candidates(selector.clone(), EventCandidateLimit::new(candidate_limit)?)
            .await
            .map_err(|error| {
                map_source_error(
                    error,
                    "priority browser events could not be read",
                    &request.range,
                )
            })?;
        if !request.focus_times.is_empty() {
            candidates.extend(
                self.events
                    .nearest_candidates(selector, request.focus_times.clone(), 2)
                    .await
                    .map_err(|error| {
                        map_source_error(
                            error,
                            "nearby browser events could not be read",
                            &request.range,
                        )
                    })?,
            );
        }
        let mut unique = BTreeMap::new();
        for event in candidates {
            if !matches!(event.payload(), BrowserEventPayload::CollectionGap(_)) {
                unique.entry(event.id()).or_insert(event);
            }
        }
        let mut ranked = unique
            .into_values()
            .map(|event| {
                let distance = nearest_distance(event.session_time(), &request.focus_times);
                (event, distance)
            })
            .collect::<Vec<_>>();
        ranked.sort_by_key(|(event, distance)| {
            (
                event.compact_priority(),
                distance.unwrap_or(u64::MAX),
                event.session_time(),
                event.ordinal(),
                event.id(),
            )
        });
        ranked.truncate(usize::from(limit.get()));
        let mut selected = ranked
            .into_iter()
            .map(|(event, distance)| SelectedBrowserEvent {
                reason: BrowserEventSelectionReason::CompactCorrelation {
                    compact_priority: event.compact_priority(),
                    nearest_focus_distance_nanos: distance,
                },
                event,
            })
            .collect::<Vec<_>>();
        selected.sort_by_key(|selected| {
            (
                selected.event.session_time(),
                selected.event.ordinal(),
                selected.event.id(),
            )
        });
        Ok(selected)
    }

    async fn chronological_events(
        &self,
        range: &ResolvedRange,
        selector: BrowserEventSelector,
        cursor: Option<BrowserEventCursor>,
        limit: EventPageLimit,
    ) -> Result<(Vec<SelectedBrowserEvent>, Option<BrowserEventCursor>)> {
        let page = self
            .events
            .chronological_events(selector.clone(), cursor, limit)
            .await
            .map_err(|error| {
                map_source_error(
                    error,
                    "chronological browser events could not be read",
                    range,
                )
            })?;
        let page_cursor = page
            .last()
            .map(|event| {
                BrowserEventCursor::new(
                    selector.clone(),
                    event.session_time(),
                    event.ordinal(),
                    event.id(),
                )
            })
            .transpose()?;
        let next_cursor = if let Some(page_cursor) = page_cursor {
            let has_next = !self
                .events
                .chronological_events(selector, Some(page_cursor.clone()), EventPageLimit::new(1)?)
                .await
                .map_err(|error| {
                    map_source_error(
                        error,
                        "browser event page continuation could not be read",
                        range,
                    )
                })?
                .is_empty();
            has_next.then_some(page_cursor)
        } else {
            None
        };
        Ok((
            page.into_iter()
                .filter(|event| !matches!(event.payload(), BrowserEventPayload::CollectionGap(_)))
                .map(|event| SelectedBrowserEvent {
                    event,
                    reason: BrowserEventSelectionReason::Chronological,
                })
                .collect(),
            next_cursor,
        ))
    }
}

fn validate_resolved_range(range: &ResolvedRange) -> Result<()> {
    if range.session_id.as_uuid().is_nil()
        || range.target_id.as_uuid().is_nil()
        || range.frame_ids.iter().any(|id| id.as_uuid().is_nil())
    {
        return Err(context_error(
            ErrorCode::InvalidInput,
            "temporal context resolved scope contains a nil identity",
            range,
            None,
        ));
    }
    // Validate the resolved range in place rather than reconstructing it via
    // `ResolvedRange::new`, which only accepts interval anchors. `validate()`
    // checks the same invariants for every anchor kind, including interaction,
    // navigation, marker, and source-frame anchors produced by the resolver.
    range.validate().map_err(|_| {
        context_error(
            ErrorCode::InvalidInput,
            "temporal context resolved range is invalid",
            range,
            None,
        )
    })?;
    if range.gaps.iter().any(|gap| {
        gap.session_id() != range.session_id
            || gap.target_id() != range.target_id
            || intersection(gap.range(), range.requested_range).is_none()
    }) {
        return Err(context_error(
            ErrorCode::InvalidInput,
            "temporal context gap scope is inconsistent",
            range,
            None,
        ));
    }
    Ok(())
}

fn validate_frame_metadata(range: &ResolvedRange, metadata: &[CapturedFrame]) -> Result<()> {
    if metadata.len() != range.frame_ids.len() {
        return Err(context_error(
            ErrorCode::NotFound,
            "resolved source frame metadata is no longer complete",
            range,
            Some("resolve the temporal range again before requesting context"),
        ));
    }
    let mut preceding: Option<&CapturedFrame> = None;
    for (expected_id, frame) in range.frame_ids.iter().zip(metadata) {
        if frame.id() != *expected_id
            || frame.session_id() != range.session_id
            || frame.target_id() != range.target_id
            || !range.resolved_range.contains(frame.session_time())
            || preceding.is_some_and(|previous| {
                frame.capture_ordinal() <= previous.capture_ordinal()
                    || frame.session_time() < previous.session_time()
            })
        {
            return Err(context_error(
                ErrorCode::PersistenceFailed,
                "resolved source frame metadata is inconsistent",
                range,
                Some(
                    "resolve the temporal range again; if this repeats, inspect recording storage",
                ),
            ));
        }
        preceding = Some(frame);
    }
    Ok(())
}

fn capture_quality(
    range: &ResolvedRange,
    metadata: &[CapturedFrame],
    capture_status: CaptureStatusEvidence,
    warnings: Vec<CaptureQualityWarning>,
) -> Result<CaptureQuality> {
    let first = metadata.first().ok_or_else(|| {
        context_error(
            ErrorCode::NotFound,
            "resolved source frame metadata is empty",
            range,
            Some("resolve the temporal range again before requesting context"),
        )
    })?;
    let last = metadata.last().expect("validated metadata is non-empty");
    Ok(CaptureQuality {
        requested_range: range.requested_range,
        retained_range: range.resolved_range,
        frame_count: u64::try_from(metadata.len()).unwrap_or(u64::MAX),
        first_frame: frame_point(first),
        last_frame: frame_point(last),
        cadence: cadence(metadata),
        frame_warnings: warning_summary(metadata),
        gaps: range.gaps.clone(),
        gap_summary: gap_summary(&range.gaps, range.resolved_range),
        retention_warnings: range.retention_warnings.clone(),
        epochs: epoch_summaries(metadata)?,
        capture_status,
        warnings,
    })
}

/// One O(n) pass splitting the ordered frame metadata into contiguous
/// visual-epoch runs on `CapturedFrame::same_visual_epoch`. Exact by design:
/// adversarial per-frame geometry churn yields epochs == frames, which is
/// truthful and cheap; the projector bounds presentation.
fn epoch_summaries(metadata: &[CapturedFrame]) -> Result<Vec<EpochSummary>> {
    let mut epochs: Vec<EpochSummary> = Vec::new();
    let mut previous: Option<&CapturedFrame> = None;
    for frame in metadata {
        let same_epoch = previous.is_some_and(|prev| frame.same_visual_epoch(prev));
        previous = Some(frame);
        if same_epoch && let Some(current) = epochs.last_mut() {
            current.frame_count = current.frame_count.saturating_add(1);
            current.last_frame = frame_point(frame);
            current.range = SessionRange::new(current.range.start(), frame.session_time())
                .map_err(|_| {
                    KrometrailError::new(
                        ErrorCode::PersistenceFailed,
                        NonEmptyText::new("epoch frame metadata is not in session-time order")
                            .expect("static epoch error is non-empty"),
                    )
                })?;
            continue;
        }
        let index = u32::try_from(epochs.len()).map_err(|_| {
            KrometrailError::new(
                ErrorCode::ResourceLimitExceeded,
                NonEmptyText::new("epoch count exceeds the summary format")
                    .expect("static epoch error is non-empty"),
            )
        })?;
        epochs.push(EpochSummary {
            epoch_index: index,
            range: SessionRange::new(frame.session_time(), frame.session_time())
                .expect("a single frame forms a valid range"),
            frame_count: 1,
            first_frame: frame_point(frame),
            last_frame: frame_point(frame),
            image: frame.image(),
            viewport: frame.viewport(),
            device_scale_factor: frame.device_scale_factor(),
        });
    }
    Ok(epochs)
}

fn frame_point(frame: &CapturedFrame) -> FramePoint {
    FramePoint {
        frame_id: frame.id(),
        capture_ordinal: frame.capture_ordinal(),
        session_time: frame.session_time(),
    }
}

fn cadence(metadata: &[CapturedFrame]) -> Option<CadenceSummary> {
    let mut deltas = metadata
        .windows(2)
        .map(|pair| {
            pair[1]
                .session_time()
                .as_nanos()
                .saturating_sub(pair[0].session_time().as_nanos())
        })
        .collect::<Vec<_>>();
    if deltas.is_empty() {
        return None;
    }
    deltas.sort_unstable();
    Some(CadenceSummary {
        interval_count: u64::try_from(deltas.len()).unwrap_or(u64::MAX),
        min_nanos: deltas[0],
        median_nanos: nearest_rank(&deltas, 50),
        p95_nanos: nearest_rank(&deltas, 95),
        max_nanos: *deltas.last().expect("non-empty cadence has a maximum"),
    })
}

fn nearest_rank(sorted: &[u64], percentile: usize) -> u64 {
    let rank = percentile
        .saturating_mul(sorted.len())
        .div_ceil(100)
        .clamp(1, sorted.len());
    sorted[rank - 1]
}

fn warning_summary(metadata: &[CapturedFrame]) -> Vec<CaptureWarningSummary> {
    let mut counts = [0_u64; 3];
    for frame in metadata {
        let mut present = [false; 3];
        for warning in frame.warnings() {
            present[match warning {
                CaptureWarning::MissingSourceTime => 0,
                CaptureWarning::SourceTimestampRounded => 1,
                CaptureWarning::ViewportMetadataIncomplete => 2,
            }] = true;
        }
        for (count, present) in counts.iter_mut().zip(present) {
            if present {
                *count = count.saturating_add(1);
            }
        }
    }
    [
        CaptureWarning::MissingSourceTime,
        CaptureWarning::SourceTimestampRounded,
        CaptureWarning::ViewportMetadataIncomplete,
    ]
    .into_iter()
    .zip(counts)
    .filter_map(|(warning, frame_count)| {
        (frame_count > 0).then_some(CaptureWarningSummary {
            warning,
            frame_count,
        })
    })
    .collect()
}

fn gap_summary(gaps: &[CaptureGap], range: SessionRange) -> CaptureGapSummary {
    let mut clipped = Vec::new();
    let mut known_missing_frames = 0_u64;
    let mut has_unknown_missing_estimate = false;
    for gap in gaps {
        if let Some(intersection) = intersection(gap.range(), range) {
            clipped.push(intersection);
            match gap.estimated_missing_frames() {
                Some(count) => {
                    known_missing_frames = known_missing_frames.saturating_add(count.get())
                }
                None => has_unknown_missing_estimate = true,
            }
        }
    }
    clipped.sort_by_key(|range| (range.start(), range.end()));
    let mut merged: Vec<SessionRange> = Vec::new();
    for current in clipped {
        if let Some(last) = merged.last_mut()
            && current.start() <= last.end()
        {
            *last = SessionRange::new(last.start(), last.end().max(current.end()))
                .expect("merged gap range is ordered");
        } else {
            merged.push(current);
        }
    }
    let covered_duration_nanos = merged.into_iter().fold(0_u64, |total, range| {
        total.saturating_add(
            range
                .end()
                .as_nanos()
                .saturating_sub(range.start().as_nanos()),
        )
    });
    CaptureGapSummary {
        gap_count: u64::try_from(
            gaps.iter()
                .filter(|gap| intersection(gap.range(), range).is_some())
                .count(),
        )
        .unwrap_or(u64::MAX),
        covered_duration_nanos,
        known_missing_frames,
        has_unknown_missing_estimate,
    }
}

fn intersection(left: SessionRange, right: SessionRange) -> Option<SessionRange> {
    let start = left.start().max(right.start());
    let end = left.end().min(right.end());
    (start <= end).then(|| {
        SessionRange::new(start, end).expect("intersecting session ranges form a valid range")
    })
}

fn capture_status_evidence(
    range: &ResolvedRange,
    effective: SessionRange,
    samples: CaptureStatusSamples,
    unavailable: &[BrowserEventUnavailableRange],
) -> Result<(CaptureStatusEvidence, Vec<CaptureQualityWarning>)> {
    if samples
        .at_or_before()
        .is_some_and(|event| event.session_time() > effective.start())
        || samples
            .in_range()
            .iter()
            .any(|event| !effective.contains(event.session_time()))
        || samples.in_range().windows(2).any(|pair| {
            (pair[0].session_time(), pair[0].ordinal(), pair[0].id())
                > (pair[1].session_time(), pair[1].ordinal(), pair[1].id())
        })
    {
        return Err(context_error(
            ErrorCode::PersistenceFailed,
            "capture status ordering is inconsistent",
            range,
            Some("inspect recording storage before retrying the context query"),
        ));
    }
    let at_range_start = samples
        .at_or_before()
        .map(|event| capture_status_point(event, range))
        .transpose()?
        .as_ref()
        .map(capture_status_bound);
    let transitions = samples
        .in_range()
        .iter()
        .map(|event| capture_status_point(event, range))
        .collect::<Result<Vec<_>>>()?;
    let at_range_end = transitions
        .last()
        .map(capture_status_bound)
        .or_else(|| at_range_start.clone());
    let mut warnings = Vec::new();
    if !unavailable.is_empty() {
        // Tombstones intentionally do not claim an event kind, so any overlap can
        // include a missing status transition and must remain explicit.
        warnings.push(CaptureQualityWarning::CaptureStatusUnavailable);
    } else if at_range_start.is_none() {
        warnings.push(CaptureQualityWarning::CaptureStatusMissing);
    }
    if samples.in_range().len() == usize::from(crate::MAX_CAPTURE_STATUS_SAMPLES) {
        warnings.push(CaptureQualityWarning::CaptureStatusTruncated);
    }
    Ok((
        CaptureStatusEvidence {
            at_range_start,
            at_range_end,
            transitions,
        },
        warnings,
    ))
}

fn capture_status_bound(point: &CaptureStatusPoint) -> CaptureStatusBound {
    CaptureStatusBound {
        state: point.state,
        established_at: point.session_time,
        attachment_generation: point.attachment_generation,
    }
}

fn capture_status_point(event: &BrowserEvent, range: &ResolvedRange) -> Result<CaptureStatusPoint> {
    let BrowserEventPayload::CaptureStatusChanged(status) = event.payload() else {
        return Err(context_error(
            ErrorCode::PersistenceFailed,
            "capture status source returned a non-status event",
            range,
            Some("inspect recording storage before retrying the context query"),
        ));
    };
    if event.session_id() != range.session_id || event.target_id() != range.target_id {
        return Err(context_error(
            ErrorCode::PersistenceFailed,
            "capture status scope is inconsistent",
            range,
            Some("inspect recording storage before retrying the context query"),
        ));
    }
    Ok(CaptureStatusPoint {
        session_time: event.session_time(),
        attachment_generation: event.attachment_generation(),
        state: status.state(),
        status: status.clone(),
    })
}

fn nearest_distance(time: SessionTime, focus_times: &[SessionTime]) -> Option<u64> {
    focus_times
        .iter()
        .map(|focus| time.as_nanos().abs_diff(focus.as_nanos()))
        .min()
}

fn map_source_error(
    error: KrometrailError,
    message: &'static str,
    range: &ResolvedRange,
) -> KrometrailError {
    let recovery = match error.code {
        ErrorCode::NotFound => Some("resolve the temporal range again before requesting context"),
        ErrorCode::PersistenceFailed => {
            Some("inspect recording storage before retrying the context query")
        }
        _ => None,
    };
    context_error(error.code, message, range, recovery)
}

fn context_error(
    code: ErrorCode,
    message: &'static str,
    range: &ResolvedRange,
    recovery: Option<&'static str>,
) -> KrometrailError {
    let mut error = KrometrailError::new(
        code,
        NonEmptyText::new(message).expect("static temporal context error is non-empty"),
    )
    .with_context(ErrorContext {
        session_id: Some(range.session_id),
        target_id: Some(range.target_id),
        range: Some(range.resolved_range),
        ..ErrorContext::default()
    });
    if let Some(recovery) = recovery {
        error = error.with_retry(RetryAdvice::AfterRecovery).with_recovery(
            NonEmptyText::new(recovery).expect("static temporal context recovery is non-empty"),
        );
    }
    error
}

fn invalid_error(message: &'static str) -> KrometrailError {
    KrometrailError::new(
        ErrorCode::InvalidInput,
        NonEmptyText::new(message).expect("static temporal context validation is non-empty"),
    )
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU64;

    use super::*;
    use crate::{
        BrowserEventId, CaptureGapPolicy, RangeResolutionOptions, RetentionPolicy,
        TemporalRangeAnchorKind,
    };
    use uuid::Uuid;

    fn resolved(frame_count: usize) -> ResolvedRange {
        ResolvedRange::new(
            crate::SessionId::from_uuid(Uuid::from_u128(1)),
            crate::TargetId::from_uuid(Uuid::from_u128(2)),
            TemporalRangeAnchorKind::SessionTime,
            SessionRange::new(SessionTime::ZERO, SessionTime::from_nanos(100)).unwrap(),
            SessionRange::new(SessionTime::ZERO, SessionTime::from_nanos(100)).unwrap(),
            (0..frame_count)
                .map(|index| crate::FrameId::from_uuid(Uuid::from_u128(index as u128 + 10)))
                .collect(),
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
            RangeResolutionOptions {
                retention: RetentionPolicy::RequireComplete,
                capture_gaps: CaptureGapPolicy::Include,
                ..RangeResolutionOptions::DEFAULT
            },
        )
        .unwrap()
    }

    fn epoch_frame(ordinal: u64, at: u64, width: u32) -> CapturedFrame {
        CapturedFrame::new(
            crate::FrameId::from_uuid(Uuid::from_u128(ordinal as u128 + 100)),
            crate::SessionId::from_uuid(Uuid::from_u128(1)),
            crate::TargetId::from_uuid(Uuid::from_u128(2)),
            crate::CaptureOrdinal::new(ordinal).unwrap(),
            None,
            crate::ObservedTime::from_nanos(at),
            SessionTime::from_nanos(at),
            crate::ImageFormat::Png,
            PixelDimensions::new(width, 10).unwrap(),
            PixelDimensions::new(width, 10).unwrap(),
            DeviceScaleFactor::new(1.0).unwrap(),
            vec![],
        )
        .unwrap()
    }

    #[test]
    fn uniform_geometry_yields_one_exact_epoch() {
        let frames = vec![
            epoch_frame(1, 10, 20),
            epoch_frame(2, 20, 20),
            epoch_frame(3, 30, 20),
        ];
        let epochs = epoch_summaries(&frames).unwrap();
        assert_eq!(epochs.len(), 1);
        let epoch = &epochs[0];
        assert_eq!(epoch.epoch_index, 0);
        assert_eq!(epoch.frame_count, 3);
        assert_eq!(
            epoch.range,
            SessionRange::new(SessionTime::from_nanos(10), SessionTime::from_nanos(30)).unwrap()
        );
        assert_eq!(epoch.first_frame.frame_id, frames[0].id());
        assert_eq!(epoch.last_frame.frame_id, frames[2].id());
        assert_eq!(epoch.image, PixelDimensions::new(20, 10).unwrap());
    }

    #[test]
    fn one_mid_range_geometry_change_yields_exactly_two_epochs() {
        let frames = vec![
            epoch_frame(1, 10, 20),
            epoch_frame(2, 20, 20),
            epoch_frame(3, 30, 40),
            epoch_frame(4, 40, 40),
            epoch_frame(5, 50, 40),
        ];
        let epochs = epoch_summaries(&frames).unwrap();
        assert_eq!(epochs.len(), 2);
        assert_eq!(epochs[0].epoch_index, 0);
        assert_eq!(epochs[0].frame_count, 2);
        assert_eq!(
            epochs[0].range,
            SessionRange::new(SessionTime::from_nanos(10), SessionTime::from_nanos(20)).unwrap()
        );
        assert_eq!(epochs[0].first_frame.frame_id, frames[0].id());
        assert_eq!(epochs[0].last_frame.frame_id, frames[1].id());
        assert_eq!(epochs[1].epoch_index, 1);
        assert_eq!(epochs[1].frame_count, 3);
        assert_eq!(
            epochs[1].range,
            SessionRange::new(SessionTime::from_nanos(30), SessionTime::from_nanos(50)).unwrap()
        );
        assert_eq!(epochs[1].first_frame.frame_id, frames[2].id());
        assert_eq!(epochs[1].last_frame.frame_id, frames[4].id());
        assert_eq!(epochs[1].image, PixelDimensions::new(40, 10).unwrap());
        assert!(epochs[0].image != epochs[1].image);
    }

    #[test]
    fn request_and_selection_boundaries_round_trip_through_validated_serde() {
        let range = resolved(1);
        let filter = BrowserEventFilter::new(
            vec![BrowserEventClass::Network, BrowserEventClass::Console],
            BrowserEventSeverity::Info,
        )
        .unwrap();
        let request = TemporalContextRequest::new(
            range,
            Some(
                SessionRange::new(SessionTime::from_nanos(10), SessionTime::from_nanos(90))
                    .unwrap(),
            ),
            filter,
            BrowserEventSelection::chronological_default(),
            vec![SessionTime::from_nanos(50)],
        )
        .unwrap();
        let encoded = serde_json::to_string(&request).unwrap();
        assert_eq!(
            serde_json::from_str::<TemporalContextRequest>(&encoded).unwrap(),
            request
        );
        let mut omitted_focus_times = serde_json::to_value(&request).unwrap();
        omitted_focus_times
            .as_object_mut()
            .unwrap()
            .remove("focus_times");
        let defaulted =
            serde_json::from_value::<TemporalContextRequest>(omitted_focus_times).unwrap();
        assert!(defaulted.focus_times().is_empty());
        assert!(EventCompactLimit::new(0).is_err());
        assert!(EventCompactLimit::new(MAX_COMPACT_EVENT_LIMIT + 1).is_err());
    }

    #[test]
    fn chronological_detail_wrapper_rejects_compact_invalid_limits_focus_and_unknown_fields() {
        let range = resolved(1);
        let request = BrowserEventDetailRequest::new(
            range.clone(),
            None,
            BrowserEventFilter::default(),
            DEFAULT_CHRONOLOGICAL_EVENT_LIMIT,
            None,
            vec![SessionTime::from_nanos(50)],
        )
        .unwrap();
        let mut encoded = serde_json::to_value(&request).unwrap();
        assert_eq!(encoded["selection"]["mode"], "chronological");
        assert_eq!(
            serde_json::from_value::<BrowserEventDetailRequest>(encoded.clone()).unwrap(),
            request
        );
        encoded.as_object_mut().unwrap().remove("focus_times");
        let defaulted =
            serde_json::from_value::<BrowserEventDetailRequest>(encoded.clone()).unwrap();
        assert!(defaulted.context_request().focus_times().is_empty());
        assert_eq!(
            TEMPORAL_CONTEXT_OPERATION_REGISTRY,
            &[TemporalContextOperationDefinition {
                kind: TemporalContextOperationKind::QueryBrowserEvents,
                stable_name: "query_browser_events",
                description: "Read chronological browser-event detail for a resolved temporal range.",
                capability: CapabilityId::BrowserEvents,
                mutability: OperationMutability::ReadOnly,
            }]
        );
        let schema =
            serde_json::to_value(TemporalContextOperationKind::QueryBrowserEvents.input_schema())
                .unwrap();
        assert_eq!(schema["type"], "object");
        let selection_schema = schema["properties"]["selection"].clone();
        let selection_schema = selection_schema
            .get("$ref")
            .and_then(serde_json::Value::as_str)
            .and_then(|reference| reference.strip_prefix("#/$defs/"))
            .map(|name| schema["$defs"][name].clone())
            .unwrap_or(selection_schema);
        let modes: Vec<_> = selection_schema["oneOf"]
            .as_array()
            .expect("detail selection schema is a tagged union")
            .iter()
            .map(|branch| branch["properties"]["mode"]["const"].as_str().unwrap())
            .collect();
        assert_eq!(modes, ["chronological"]);

        let compact =
            serde_json::to_value(TemporalContextRequest::compact(range.clone(), vec![]).unwrap())
                .unwrap();
        assert!(serde_json::from_value::<BrowserEventDetailRequest>(compact).is_err());
        let mut invalid_limit = encoded.clone();
        invalid_limit["selection"]["limit"] = serde_json::json!(0);
        assert!(serde_json::from_value::<BrowserEventDetailRequest>(invalid_limit).is_err());
        let mut unknown = encoded;
        unknown["unexpected"] = serde_json::json!(true);
        assert!(serde_json::from_value::<BrowserEventDetailRequest>(unknown).is_err());
        assert!(
            BrowserEventDetailRequest::new(
                range,
                None,
                BrowserEventFilter::default(),
                DEFAULT_CHRONOLOGICAL_EVENT_LIMIT,
                None,
                vec![SessionTime::from_nanos(101)],
            )
            .is_err()
        );
    }

    #[test]
    fn request_rejects_clip_focus_filter_cursor_and_frame_limit_mismatches() {
        let range = resolved(1);
        assert!(
            TemporalContextRequest::new(
                range.clone(),
                Some(SessionRange::new(SessionTime::ZERO, SessionTime::from_nanos(101)).unwrap()),
                BrowserEventFilter::default(),
                BrowserEventSelection::compact_default(),
                vec![],
            )
            .is_err()
        );
        assert!(
            TemporalContextRequest::compact(range.clone(), vec![SessionTime::from_nanos(101)])
                .is_err()
        );
        assert!(
            TemporalContextRequest::compact(
                range.clone(),
                vec![SessionTime::from_nanos(1); MAX_FOCUS_TIMES + 1]
            )
            .is_err()
        );
        assert!(
            BrowserEventFilter::new(
                vec![BrowserEventClass::Console, BrowserEventClass::Console],
                BrowserEventSeverity::Debug,
            )
            .is_err()
        );

        let wrong_selector = BrowserEventSelector::new(
            range.session_id,
            range.target_id,
            range.resolved_range,
            vec![BrowserEventClass::Network],
            BrowserEventSeverity::Debug,
        )
        .unwrap();
        let cursor = BrowserEventCursor::new(
            wrong_selector,
            SessionTime::ZERO,
            crate::BrowserEventOrdinal::new(1).unwrap(),
            BrowserEventId::from_uuid(Uuid::from_u128(9)),
        )
        .unwrap();
        assert_eq!(
            serde_json::from_str::<BrowserEventCursor>(&serde_json::to_string(&cursor).unwrap())
                .unwrap(),
            cursor
        );
        assert!(
            TemporalContextRequest::new(
                range,
                None,
                BrowserEventFilter::default(),
                BrowserEventSelection::chronological(
                    DEFAULT_CHRONOLOGICAL_EVENT_LIMIT,
                    Some(cursor)
                )
                .unwrap(),
                vec![],
            )
            .is_err()
        );

        let mut oversized = resolved(1);
        oversized.frame_ids = (0..=MAX_CAPTURE_QUALITY_FRAMES)
            .map(|index| FrameId::from_uuid(Uuid::from_u128(index as u128 + 100)))
            .collect();
        assert_eq!(
            TemporalContextRequest::compact(oversized, vec![])
                .unwrap_err()
                .code,
            ErrorCode::ResourceLimitExceeded
        );

        let mut empty_wire =
            serde_json::to_value(TemporalContextRequest::compact(resolved(1), vec![]).unwrap())
                .unwrap();
        empty_wire["range"]["frame_ids"] = serde_json::json!([]);
        assert!(serde_json::from_value::<TemporalContextRequest>(empty_wire).is_err());
    }

    #[test]
    fn cadence_uses_nearest_rank_and_gap_union_keeps_point_gaps_zero_duration() {
        assert_eq!(nearest_rank(&[0, 10, 20, 100], 50), 10);
        assert_eq!(nearest_rank(&[0, 10, 20, 100], 95), 100);
        let range = SessionRange::new(SessionTime::ZERO, SessionTime::from_nanos(20)).unwrap();
        let make_gap = |id, start, end, estimate| {
            CaptureGap::new(
                crate::GapId::from_uuid(Uuid::from_u128(id)),
                crate::SessionId::from_uuid(Uuid::from_u128(1)),
                crate::TargetId::from_uuid(Uuid::from_u128(2)),
                SessionRange::new(SessionTime::from_nanos(start), SessionTime::from_nanos(end))
                    .unwrap(),
                crate::ObservedTime::from_nanos(30),
                crate::CaptureGapReason::CaptureStopped,
                NonZeroU64::new(estimate),
                None,
            )
            .unwrap()
        };
        let gaps = vec![
            make_gap(1, 2, 8, 2),
            make_gap(2, 5, 10, 3),
            make_gap(3, 15, 15, 1),
        ];
        let summary = gap_summary(&gaps, range);
        assert_eq!(summary.covered_duration_nanos, 8);
        assert_eq!(summary.known_missing_frames, 6);
        assert!(!summary.has_unknown_missing_estimate);
    }
}
