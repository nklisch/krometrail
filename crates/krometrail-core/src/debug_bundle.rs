//! Validated application contracts for one temporal debug-bundle request.
//!
//! The boundary deliberately owns a natural `TemporalQueryRequest`. It reuses
//! resolved ranges, artifact results, temporal context, and generic artifact
//! provenance rather than projecting those authorities into bundle-specific copies.

use std::{collections::HashSet, sync::Arc, time::Instant};

use serde::{Deserialize, Deserializer, Serialize};

use crate::{
    ArtifactFailurePolicy, ArtifactGenerationRequest, ArtifactGenerationResult,
    ArtifactGeneratorRequest, ArtifactMarker, ArtifactMarkerId, BrowserEventFilter,
    BrowserEventSelection, CancellationSignal, CapabilityId, FrameId, KrometrailError,
    NonEmptyText, OperationMutability, PortFuture, ResolvedAnchorReference, ResolvedRange, Result,
    SessionTime, TemporalContext, TemporalContextRequest, TemporalQueryRequest,
    TemporalRangeAnchor, TemporalRangeResolution, error::invalid, timeline::MAX_FOCUS_TIMES,
    validation::deserialize_validated,
};

pub const MAX_BUNDLE_CALLER_MARKERS: usize = 64;
pub const MAX_BUNDLE_ARTIFACT_MARKERS: usize = 256;
pub const MAX_BUNDLE_TIMELINE_ROWS: u16 = 1_024;
pub const MAX_BUNDLE_HEADER_BYTES: usize = 512;

#[derive(
    Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum OrientationPolicy {
    #[default]
    Include,
    Omit,
}

#[derive(
    Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum BundleEpochScope {
    #[default]
    Anchor,
    All,
}

/// The sole bundle request. Natural anchors are resolved by the bundle service
/// exactly once; no sibling request accepts an already-resolved range.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TemporalDebugBundleRequest {
    query: TemporalQueryRequest,
    caller_markers: Vec<ArtifactMarker>,
    orientation: OrientationPolicy,
    epochs: BundleEpochScope,
}

#[derive(Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
struct TemporalDebugBundleRequestWire {
    query: TemporalQueryRequest,
    caller_markers: Vec<ArtifactMarker>,
    orientation: OrientationPolicy,
    #[serde(default)]
    epochs: BundleEpochScope,
}

impl TemporalDebugBundleRequest {
    pub fn new(
        query: TemporalQueryRequest,
        caller_markers: Vec<ArtifactMarker>,
        orientation: OrientationPolicy,
        epochs: BundleEpochScope,
    ) -> Result<Self> {
        // Public fields on the nested request support ergonomic construction;
        // rebuild it here so this boundary cannot inherit an unchecked value.
        let query = TemporalQueryRequest::new(query.anchor, query.retention, query.capture_gaps)?;
        validate_caller_markers(&caller_markers)?;
        Ok(Self {
            query,
            caller_markers,
            orientation,
            epochs,
        })
    }

    pub fn default_policy(query: TemporalQueryRequest) -> Result<Self> {
        Self::new(
            query,
            Vec::new(),
            OrientationPolicy::Include,
            BundleEpochScope::Anchor,
        )
    }

    pub const fn query(&self) -> &TemporalQueryRequest {
        &self.query
    }

    pub fn caller_markers(&self) -> &[ArtifactMarker] {
        &self.caller_markers
    }

    pub const fn orientation(&self) -> OrientationPolicy {
        self.orientation
    }

    pub const fn epochs(&self) -> BundleEpochScope {
        self.epochs
    }

    pub fn into_parts(
        self,
    ) -> (
        TemporalQueryRequest,
        Vec<ArtifactMarker>,
        OrientationPolicy,
        BundleEpochScope,
    ) {
        (
            self.query,
            self.caller_markers,
            self.orientation,
            self.epochs,
        )
    }
}

impl<'de> Deserialize<'de> for TemporalDebugBundleRequest {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        deserialize_validated(deserializer, |wire: TemporalDebugBundleRequestWire| {
            Self::new(
                wire.query,
                wire.caller_markers,
                wire.orientation,
                wire.epochs,
            )
        })
    }
}

crate::validation::delegate_json_schema!(TemporalDebugBundleRequest => TemporalDebugBundleRequestWire);

/// Metadata for the single natural-anchor temporal entry point.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TemporalDebugBundleOperationDefinition {
    pub stable_name: &'static str,
    pub description: &'static str,
    pub capability: CapabilityId,
    pub mutability: OperationMutability,
}

pub const TEMPORAL_DEBUG_BUNDLE_OPERATION: TemporalDebugBundleOperationDefinition =
    TemporalDebugBundleOperationDefinition {
        stable_name: "temporal_debug_bundle",
        description: "Inspect a resolved interaction or temporal range as a compact evidence bundle.",
        capability: CapabilityId::TemporalVision,
        mutability: OperationMutability::ReadOnly,
    };

fn validate_caller_markers(markers: &[ArtifactMarker]) -> Result<()> {
    if markers.len() > MAX_BUNDLE_CALLER_MARKERS {
        return Err(invalid("bundle caller marker count exceeds sixty-four"));
    }
    let mut ids = HashSet::with_capacity(markers.len());
    for marker in markers {
        if artifact_marker_id_is_nil(marker.id()) || !ids.insert(marker.id().clone()) {
            return Err(invalid(
                "bundle caller marker identifiers must be non-nil and unique",
            ));
        }
        if marker.kind().as_str().len() > 64 || marker.label().as_str().len() > 160 {
            return Err(invalid(
                "bundle caller marker kind or label exceeds its UTF-8 byte limit",
            ));
        }
    }
    Ok(())
}

/// True when a typed marker identifier is a nil UUID. Caller-supplied text
/// identifiers cannot be nil and are always valid at this layer.
fn artifact_marker_id_is_nil(id: &ArtifactMarkerId) -> bool {
    match id {
        ArtifactMarkerId::Interaction(id) => id.as_uuid().is_nil(),
        ArtifactMarkerId::Navigation(id) => id.as_uuid().is_nil(),
        ArtifactMarkerId::Marker(id) => id.as_uuid().is_nil(),
        ArtifactMarkerId::Caller(_) => false,
    }
}

#[derive(Clone, Default)]
pub struct TemporalDebugBundleContext {
    pub deadline: Option<Instant>,
    pub cancellation: Option<Arc<dyn CancellationSignal>>,
}

impl TemporalDebugBundleContext {
    pub fn is_cancelled(&self) -> bool {
        self.cancellation
            .as_ref()
            .is_some_and(|signal| signal.is_cancelled())
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EffectiveBundlePolicy {
    pub artifact_anchor: SessionTime,
    pub epoch_scope: BundleEpochScope,
    pub artifact_generators: Vec<ArtifactGeneratorRequest>,
    pub artifact_failure_policy: ArtifactFailurePolicy,
    pub event_filter: BrowserEventFilter,
    pub event_selection: BrowserEventSelection,
    pub focus_times: Vec<SessionTime>,
}

impl EffectiveBundlePolicy {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        artifact_anchor: SessionTime,
        epoch_scope: BundleEpochScope,
        artifact_generators: Vec<ArtifactGeneratorRequest>,
        artifact_failure_policy: ArtifactFailurePolicy,
        event_filter: BrowserEventFilter,
        event_selection: BrowserEventSelection,
        focus_times: Vec<SessionTime>,
    ) -> Result<Self> {
        if artifact_generators.is_empty() {
            return Err(invalid(
                "effective bundle policy must contain artifact generators",
            ));
        }
        if focus_times.len() > MAX_FOCUS_TIMES
            || focus_times.windows(2).any(|pair| pair[0] >= pair[1])
        {
            return Err(invalid(
                "effective bundle focus times must be unique, ordered, and bounded",
            ));
        }
        Ok(Self {
            artifact_anchor,
            epoch_scope,
            artifact_generators,
            artifact_failure_policy,
            event_filter,
            event_selection,
            focus_times,
        })
    }
}

impl<'de> Deserialize<'de> for EffectiveBundlePolicy {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Wire {
            artifact_anchor: SessionTime,
            epoch_scope: BundleEpochScope,
            artifact_generators: Vec<ArtifactGeneratorRequest>,
            artifact_failure_policy: ArtifactFailurePolicy,
            event_filter: BrowserEventFilter,
            event_selection: BrowserEventSelection,
            #[serde(default)]
            focus_times: Vec<SessionTime>,
        }
        deserialize_validated(deserializer, |wire: Wire| {
            Self::new(
                wire.artifact_anchor,
                wire.epoch_scope,
                wire.artifact_generators,
                wire.artifact_failure_policy,
                wire.event_filter,
                wire.event_selection,
                wire.focus_times,
            )
        })
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
// The application contract deliberately carries the existing result value exactly;
// boxing it here would create a second, bundle-specific ownership shape.
#[allow(clippy::large_enum_variant)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
pub enum BundleArtifactEvidence {
    Available(ArtifactGenerationResult),
    Unavailable { error: KrometrailError },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
// Preserve the exact temporal-context contract rather than introducing a wrapper DTO.
#[allow(clippy::large_enum_variant)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
pub enum BundleContextEvidence {
    Available(TemporalContext),
    Unavailable { error: KrometrailError },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidencePosture {
    ObservedChangeAndTemporalProximityOnly,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BundleEpochVisualSummary {
    pub epoch_index: u32,
    pub summary: temporal_vision::StoryboardVisualSummary<FrameId>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TemporalDebugHeader {
    pub summary: NonEmptyText,
    pub posture: EvidencePosture,
    pub visual_summaries: Vec<BundleEpochVisualSummary>,
}

impl TemporalDebugHeader {
    pub fn new(
        summary: NonEmptyText,
        visual_summaries: Vec<BundleEpochVisualSummary>,
    ) -> Result<Self> {
        if summary.as_str().len() > MAX_BUNDLE_HEADER_BYTES {
            return Err(invalid("temporal debug header exceeds 512 UTF-8 bytes"));
        }
        if visual_summaries
            .windows(2)
            .any(|pair| pair[0].epoch_index >= pair[1].epoch_index)
        {
            return Err(invalid(
                "bundle epoch visual summaries must be unique and ordered",
            ));
        }
        Ok(Self {
            summary,
            posture: EvidencePosture::ObservedChangeAndTemporalProximityOnly,
            visual_summaries,
        })
    }
}

impl<'de> Deserialize<'de> for TemporalDebugHeader {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Wire {
            summary: NonEmptyText,
            posture: EvidencePosture,
            visual_summaries: Vec<BundleEpochVisualSummary>,
        }
        deserialize_validated(deserializer, |wire: Wire| {
            if wire.posture != EvidencePosture::ObservedChangeAndTemporalProximityOnly {
                return Err(invalid("unsupported temporal debug evidence posture"));
            }
            Self::new(wire.summary, wire.visual_summaries)
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "warning", rename_all = "snake_case", deny_unknown_fields)]
pub enum BundleWarning {
    AnchorAdjustedForRetention {
        requested: SessionTime,
        effective: SessionTime,
    },
    TimelineMarkerEvidenceTruncated {
        matched_count: u64,
        returned_count: u64,
        limit: u16,
    },
    MarkersTruncated {
        matched_count: u64,
        returned_count: u64,
        limit: u16,
    },
    MarkerLabelUnavailable {
        marker_id: ArtifactMarkerId,
    },
    NoMajorVisualChangeFocus,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "component", rename_all = "snake_case", deny_unknown_fields)]
pub enum BundleDegradation {
    MarkerContextUnavailable { error: KrometrailError },
    ArtifactRequestUnavailable,
    ArtifactOutcomesUnavailable { unavailable: u16, total: u16 },
    ContextUnavailable,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TemporalDebugBundle {
    pub requested_query: TemporalQueryRequest,
    pub range: ResolvedRange,
    pub effective: EffectiveBundlePolicy,
    pub header: TemporalDebugHeader,
    pub markers: Vec<ArtifactMarker>,
    pub artifacts: BundleArtifactEvidence,
    pub context: BundleContextEvidence,
    pub warnings: Vec<BundleWarning>,
    pub degradations: Vec<BundleDegradation>,
}

impl TemporalDebugBundle {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        requested_query: TemporalQueryRequest,
        range: ResolvedRange,
        effective: EffectiveBundlePolicy,
        header: TemporalDebugHeader,
        markers: Vec<ArtifactMarker>,
        artifacts: BundleArtifactEvidence,
        context: BundleContextEvidence,
        warnings: Vec<BundleWarning>,
        degradations: Vec<BundleDegradation>,
    ) -> Result<Self> {
        let requested_query = TemporalQueryRequest::new(
            requested_query.anchor,
            requested_query.retention,
            requested_query.capture_gaps,
        )?;
        let effective = EffectiveBundlePolicy::new(
            effective.artifact_anchor,
            effective.epoch_scope,
            effective.artifact_generators,
            effective.artifact_failure_policy,
            effective.event_filter,
            effective.event_selection,
            effective.focus_times,
        )?;
        if header.posture != EvidencePosture::ObservedChangeAndTemporalProximityOnly {
            return Err(invalid("unsupported temporal debug evidence posture"));
        }
        let header = TemporalDebugHeader::new(header.summary, header.visual_summaries)?;
        range.validate()?;
        validate_query_resolution(&requested_query, &range)?;
        if effective.artifact_anchor != range.resolved_anchor.effective_time
            || effective
                .focus_times
                .iter()
                .any(|time| !range.resolved_range.contains(*time))
        {
            return Err(invalid(
                "effective bundle policy must use the exact resolved anchor and range",
            ));
        }
        if markers.len() > MAX_BUNDLE_ARTIFACT_MARKERS {
            return Err(invalid("bundle artifact marker count exceeds 256"));
        }
        ArtifactGenerationRequest::new(
            range.clone(),
            markers.clone(),
            effective.artifact_generators.clone(),
            effective.artifact_failure_policy,
        )?;
        TemporalContextRequest::new(
            range.clone(),
            None,
            effective.event_filter.clone(),
            effective.event_selection.clone(),
            effective.focus_times.clone(),
        )?;
        if matches!(
            &artifacts,
            BundleArtifactEvidence::Available(result) if result.range != range
        ) || matches!(
            &context,
            BundleContextEvidence::Available(result) if result.range != range
        ) {
            return Err(invalid(
                "bundle components must preserve the exact resolved range",
            ));
        }
        warnings.iter().try_for_each(validate_warning)?;
        degradations.iter().try_for_each(validate_degradation)?;
        let adjusted = range.resolved_anchor.requested_time != range.resolved_anchor.effective_time;
        let adjustment_warnings = warnings
            .iter()
            .filter(|warning| matches!(warning, BundleWarning::AnchorAdjustedForRetention { .. }))
            .collect::<Vec<_>>();
        if adjustment_warnings.len() != usize::from(adjusted)
            || adjustment_warnings.iter().any(|warning| {
                !matches!(
                    warning,
                    BundleWarning::AnchorAdjustedForRetention { requested, effective }
                        if *requested == range.resolved_anchor.requested_time
                            && *effective == range.resolved_anchor.effective_time
                )
            })
        {
            return Err(invalid(
                "bundle anchor-adjustment warning must match resolved retention truth",
            ));
        }
        Ok(Self {
            requested_query,
            range,
            effective,
            header,
            markers,
            artifacts,
            context,
            warnings,
            degradations,
        })
    }
}

impl<'de> Deserialize<'de> for TemporalDebugBundle {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Wire {
            requested_query: TemporalQueryRequest,
            range: ResolvedRange,
            effective: EffectiveBundlePolicy,
            header: TemporalDebugHeader,
            markers: Vec<ArtifactMarker>,
            artifacts: BundleArtifactEvidence,
            context: BundleContextEvidence,
            warnings: Vec<BundleWarning>,
            degradations: Vec<BundleDegradation>,
        }
        deserialize_validated(deserializer, |wire: Wire| {
            Self::new(
                wire.requested_query,
                wire.range,
                wire.effective,
                wire.header,
                wire.markers,
                wire.artifacts,
                wire.context,
                wire.warnings,
                wire.degradations,
            )
        })
    }
}

fn validate_warning(warning: &BundleWarning) -> Result<()> {
    match warning {
        BundleWarning::TimelineMarkerEvidenceTruncated {
            matched_count,
            returned_count,
            limit,
        }
        | BundleWarning::MarkersTruncated {
            matched_count,
            returned_count,
            limit,
        } => {
            if *limit == 0
                || returned_count > matched_count
                || *returned_count > u64::from(*limit)
                || matched_count == returned_count
            {
                return Err(invalid(
                    "bundle truncation warnings must report exact bounded counts",
                ));
            }
        }
        BundleWarning::MarkerLabelUnavailable { marker_id }
            if artifact_marker_id_is_nil(marker_id) =>
        {
            return Err(invalid(
                "bundle marker-label warning requires a non-nil marker id",
            ));
        }
        BundleWarning::AnchorAdjustedForRetention { .. }
        | BundleWarning::MarkerLabelUnavailable { .. }
        | BundleWarning::NoMajorVisualChangeFocus => {}
    }
    Ok(())
}

fn validate_degradation(degradation: &BundleDegradation) -> Result<()> {
    if let BundleDegradation::ArtifactOutcomesUnavailable { unavailable, total } = degradation
        && (*unavailable == 0 || unavailable > total)
    {
        return Err(invalid(
            "artifact outcome degradation must report a nonzero bounded count",
        ));
    }
    Ok(())
}

fn validate_query_resolution(request: &TemporalQueryRequest, range: &ResolvedRange) -> Result<()> {
    if request.anchor.kind() != range.anchor_kind || request.options() != range.options {
        return Err(invalid(
            "resolved range must preserve the exact temporal query options",
        ));
    }
    let compatible = match (&request.anchor, &range.resolved_anchor.reference) {
        (
            TemporalRangeAnchor::SessionTime {
                range: requested, ..
            },
            ResolvedAnchorReference::Interval,
        ) => *requested == range.requested_range,
        (TemporalRangeAnchor::WallClock { .. }, ResolvedAnchorReference::Interval) => true,
        (
            TemporalRangeAnchor::Interaction { interaction_id, .. },
            ResolvedAnchorReference::Interaction {
                interaction_id: resolved,
            },
        ) => interaction_id == resolved,
        (
            TemporalRangeAnchor::LatestInteraction { .. },
            ResolvedAnchorReference::Interaction { .. },
        ) => true,
        (
            TemporalRangeAnchor::Navigation { navigation_id, .. },
            ResolvedAnchorReference::Navigation {
                navigation_id: resolved,
            },
        ) => navigation_id == resolved,
        (
            TemporalRangeAnchor::Marker { marker_id, .. },
            ResolvedAnchorReference::Marker {
                marker_id: resolved,
            },
        ) => marker_id == resolved,
        (
            TemporalRangeAnchor::SourceFrame {
                start_frame_id,
                end_frame_id,
                ..
            },
            ResolvedAnchorReference::SourceFrames {
                start_frame_id: resolved_start,
                end_frame_id: resolved_end,
            },
        ) => start_frame_id == resolved_start && end_frame_id == resolved_end,
        _ => false,
    };
    if compatible {
        Ok(())
    } else {
        Err(invalid(
            "resolved anchor identity must match the exact temporal query",
        ))
    }
}

pub trait TemporalDebugBundles: Send + Sync {
    fn bundle(
        &self,
        request: TemporalDebugBundleRequest,
        context: TemporalDebugBundleContext,
    ) -> PortFuture<'_, Result<TemporalDebugBundle>>;

    fn resolve(
        &self,
        _request: TemporalQueryRequest,
        _context: TemporalDebugBundleContext,
    ) -> PortFuture<'_, Result<TemporalRangeResolution>> {
        Box::pin(std::future::ready(Err(KrometrailError::new(
            crate::ErrorCode::Unsupported,
            NonEmptyText::new("temporal range resolution is unavailable")
                .expect("static unsupported message is non-empty"),
        ))))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        CaptureGapPolicy, InteractionId, IntervalAnchorScope, RetentionPolicy, SessionId,
        SessionRange, TargetId,
    };
    use uuid::Uuid;

    fn query() -> TemporalQueryRequest {
        TemporalQueryRequest::new(
            TemporalRangeAnchor::SessionTime {
                scope: IntervalAnchorScope::new(
                    SessionId::from_uuid(Uuid::from_u128(1)),
                    TargetId::from_uuid(Uuid::from_u128(2)),
                ),
                range: SessionRange::new(SessionTime::ZERO, SessionTime::from_nanos(10)).unwrap(),
            },
            RetentionPolicy::AllowPartial,
            CaptureGapPolicy::Include,
        )
        .unwrap()
    }

    fn marker(id: ArtifactMarkerId, kind: &str, label: &str) -> ArtifactMarker {
        ArtifactMarker::new(
            id,
            SessionTime::from_nanos(5),
            NonEmptyText::new(kind).unwrap(),
            NonEmptyText::new(label).unwrap(),
        )
    }

    #[test]
    fn request_owns_one_validated_query_and_bounded_private_markers() {
        let object_safe_port: Option<&dyn TemporalDebugBundles> = None;
        assert!(object_safe_port.is_none());
        let request = TemporalDebugBundleRequest::new(
            query(),
            vec![marker(
                ArtifactMarkerId::Caller(NonEmptyText::new("caller-1").unwrap()),
                "caller",
                "exact caller label",
            )],
            OrientationPolicy::Omit,
            BundleEpochScope::All,
        )
        .unwrap();
        let encoded = serde_json::to_string(&request).unwrap();
        assert!(!encoded.contains("resolved_range"));
        assert!(!encoded.contains("bytes"));
        assert!(!encoded.contains("path"));
        assert!(!encoded.contains("uri"));
        assert_eq!(
            serde_json::from_str::<TemporalDebugBundleRequest>(&encoded).unwrap(),
            request
        );

        let duplicate = marker(
            ArtifactMarkerId::Caller(NonEmptyText::new("same").unwrap()),
            "caller",
            "label",
        );
        assert!(
            TemporalDebugBundleRequest::new(
                query(),
                vec![duplicate.clone(), duplicate],
                OrientationPolicy::Include,
                BundleEpochScope::Anchor,
            )
            .is_err()
        );
        assert!(
            TemporalDebugBundleRequest::new(
                query(),
                vec![marker(
                    ArtifactMarkerId::Interaction(InteractionId::from_uuid(Uuid::nil())),
                    "caller",
                    "label",
                )],
                OrientationPolicy::Include,
                BundleEpochScope::Anchor,
            )
            .is_err()
        );
        assert!(
            TemporalDebugBundleRequest::new(
                query(),
                vec![marker(
                    ArtifactMarkerId::Caller(NonEmptyText::new("long").unwrap()),
                    &"k".repeat(65),
                    "label",
                )],
                OrientationPolicy::Include,
                BundleEpochScope::Anchor,
            )
            .is_err()
        );
    }

    #[test]
    fn request_deserialization_revalidates_nested_query_and_rejects_unknown_fields() {
        let request = TemporalDebugBundleRequest::default_policy(query()).unwrap();
        assert_eq!(request.epochs(), BundleEpochScope::Anchor);
        let mut defaulted = serde_json::to_value(&request).unwrap();
        defaulted.as_object_mut().unwrap().remove("epochs");
        assert_eq!(
            serde_json::from_value::<TemporalDebugBundleRequest>(defaulted)
                .unwrap()
                .epochs(),
            BundleEpochScope::Anchor
        );
        let mut value = serde_json::to_value(request).unwrap();
        value["query"]["anchor"]["scope"]["target_id"] = serde_json::Value::Null;
        assert!(serde_json::from_value::<TemporalDebugBundleRequest>(value).is_err());

        let mut value =
            serde_json::to_value(TemporalDebugBundleRequest::default_policy(query()).unwrap())
                .unwrap();
        value["resolved_range"] = serde_json::json!({});
        assert!(serde_json::from_value::<TemporalDebugBundleRequest>(value).is_err());
    }
}
