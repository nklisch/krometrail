//! Bounded, privacy-safe marker assembly for the temporal debug bundle.
//!
//! The bundle preserves caller markers and the resolved natural-anchor marker
//! exactly, then selects remaining candidates from the bounded, kind-filtered
//! generic timeline by absolute session-time distance to the effective anchor.
//! Labels contain only typed IDs and operation stable names; persisted secrets,
//! locators, page text, URLs, filenames, keys, and parameters never appear.

use std::collections::{BTreeMap, BTreeSet, HashSet};

use krometrail_core::{
    ArtifactMarker, ArtifactMarkerId, BundleWarning, ErrorCode, InteractionAnchor, InteractionId,
    KrometrailError, MAX_BUNDLE_ARTIFACT_MARKERS, MAX_BUNDLE_TIMELINE_ROWS, MarkerId, NavigationId,
    NonEmptyText, ResolvedAnchorReference, ResolvedRange, Result, SessionTime, TimelineRangeSlice,
};

/// Loaded interaction evidence needed to label interaction markers exactly.
///
/// The service loads one anchor per interaction identifier discovered in the
/// bounded timeline read; marker assembly never performs I/O of its own.
pub(crate) struct MarkerEvidence<'a> {
    pub range: &'a ResolvedRange,
    pub caller_markers: &'a [ArtifactMarker],
    pub timeline: &'a TimelineRangeSlice,
    pub interactions: &'a BTreeMap<InteractionId, InteractionAnchor>,
}

/// The assembled marker list and the composition warnings it produced.
pub(crate) struct AssembledMarkers {
    pub markers: Vec<ArtifactMarker>,
    pub warnings: Vec<BundleWarning>,
}

/// Assembles the bounded, ordered, privacy-safe marker list.
///
/// Mandatory markers (caller markers + the resolved natural-anchor marker, when
/// the anchor carries a typed ID) are always preserved. Remaining interaction,
/// navigation, and generic markers from the bounded timeline read are ranked by
/// absolute session-time distance to the effective anchor, then class, session
/// time, and stable ID bytes. The combined list is capped at 256 markers and
/// presented in `(session_time, class, stable ID)` order.
///
/// Truncation warnings are exact: `TimelineMarkerEvidenceTruncated` reports the
/// 1024-row source cap, and `MarkersTruncated` reports the 256-marker result cap.
/// `MarkerLabelUnavailable` is emitted once per generic timeline marker that has
/// no caller-supplied presentation metadata.
pub(crate) fn assemble_markers(evidence: MarkerEvidence<'_>) -> Result<AssembledMarkers> {
    let effective_anchor = evidence.range.resolved_anchor.effective_time;
    let mut warnings = Vec::new();

    // Source-cap warning: the bounded timeline read returned fewer rows than
    // matched the filter.
    if evidence.timeline.truncated {
        warnings.push(BundleWarning::TimelineMarkerEvidenceTruncated {
            matched_count: evidence.timeline.matched_count,
            returned_count: evidence.timeline.observations.len() as u64,
            limit: MAX_BUNDLE_TIMELINE_ROWS,
        });
    }

    // Caller markers are mandatory and preserved exactly as supplied.
    let mut mandatory: Vec<ArtifactMarker> = evidence.caller_markers.to_vec();

    // The resolved natural-anchor marker is mandatory when the anchor carries a
    // typed ID. It is produced from the effective time so it always lies inside
    // the resolved range, even when partial retention clamped the requested time.
    let anchor_marker = anchor_marker(evidence.range, evidence.interactions)?;
    let anchor_id = anchor_marker.as_ref().map(|marker| marker.id().clone());
    if let Some(marker) = anchor_marker {
        mandatory.push(marker);
    }

    // Build candidate markers from the bounded timeline read, skipping any
    // marker whose ID is already mandatory (the anchor or a caller marker).
    let mandatory_ids: HashSet<ArtifactMarkerId> =
        mandatory.iter().map(|marker| marker.id().clone()).collect();
    let mut candidates: Vec<ArtifactMarker> = Vec::new();
    let mut seen_interactions: BTreeSet<InteractionId> = BTreeSet::new();
    let mut seen_navigations: BTreeSet<NavigationId> = BTreeSet::new();
    let mut seen_markers: BTreeSet<MarkerId> = BTreeSet::new();

    for observation in &evidence.timeline.observations {
        match observation.payload() {
            krometrail_core::ObservationPayloadRef::Interaction(interaction_id) => {
                if !seen_interactions.insert(*interaction_id) {
                    continue;
                }
                if anchor_marker_matches_interaction(&anchor_id, interaction_id) {
                    continue;
                }
                if mandatory_ids.contains(&ArtifactMarkerId::Interaction(*interaction_id)) {
                    continue;
                }
                let Some(anchor) = evidence.interactions.get(interaction_id) else {
                    // Without the authoritative anchor the marker cannot carry
                    // the exact dispatch time or operation identity; skip rather
                    // than invent a label.
                    continue;
                };
                candidates.push(interaction_marker(anchor)?);
            }
            krometrail_core::ObservationPayloadRef::Navigation(navigation_id) => {
                if !seen_navigations.insert(*navigation_id) {
                    continue;
                }
                if anchor_marker_matches_navigation(&anchor_id, navigation_id) {
                    continue;
                }
                if mandatory_ids.contains(&ArtifactMarkerId::Navigation(*navigation_id)) {
                    continue;
                }
                candidates.push(navigation_marker(
                    *navigation_id,
                    observation.session_time(),
                )?);
            }
            krometrail_core::ObservationPayloadRef::Marker(marker_id) => {
                if !seen_markers.insert(*marker_id) {
                    continue;
                }
                if anchor_marker_matches_marker(&anchor_id, marker_id) {
                    continue;
                }
                if mandatory_ids.contains(&ArtifactMarkerId::Marker(*marker_id)) {
                    // A caller marker already provides the exact presentation
                    // for this generic timeline marker.
                    continue;
                }
                warnings.push(BundleWarning::MarkerLabelUnavailable {
                    marker_id: *marker_id,
                });
                candidates.push(generic_marker(*marker_id, observation.session_time())?);
            }
            _ => {}
        }
    }

    // Rank candidates by (distance to anchor, class, session time, stable ID).
    candidates.sort_by(|left, right| {
        candidate_rank(left, effective_anchor).cmp(&candidate_rank(right, effective_anchor))
    });

    let total = mandatory.len() + candidates.len();
    let mut markers = mandatory;
    if total > MAX_BUNDLE_ARTIFACT_MARKERS {
        let remaining = MAX_BUNDLE_ARTIFACT_MARKERS.saturating_sub(markers.len());
        markers.extend(candidates.into_iter().take(remaining));
        warnings.push(BundleWarning::MarkersTruncated {
            matched_count: total as u64,
            returned_count: markers.len() as u64,
            limit: MAX_BUNDLE_ARTIFACT_MARKERS as u16,
        });
    } else {
        markers.extend(candidates);
    }

    // Final presentation order: (session_time, class, stable ID).
    markers.sort_by_key(presentation_order);

    Ok(AssembledMarkers { markers, warnings })
}

/// Returns the mandatory anchor marker when the resolved anchor carries a typed
/// ID. Interval and source-frame anchors have no natural marker.
fn anchor_marker(
    range: &ResolvedRange,
    interactions: &BTreeMap<InteractionId, InteractionAnchor>,
) -> Result<Option<ArtifactMarker>> {
    let effective_time = range.resolved_anchor.effective_time;
    Ok(match &range.resolved_anchor.reference {
        ResolvedAnchorReference::Interaction { interaction_id } => {
            let label = interaction_label(interaction_id, interactions)?;
            Some(ArtifactMarker::new(
                ArtifactMarkerId::Interaction(*interaction_id),
                effective_time,
                NonEmptyText::new("interaction").expect("interaction kind is non-empty"),
                label,
            ))
        }
        ResolvedAnchorReference::Navigation { navigation_id } => Some(ArtifactMarker::new(
            ArtifactMarkerId::Navigation(*navigation_id),
            effective_time,
            NonEmptyText::new("navigation").expect("navigation kind is non-empty"),
            navigation_label(navigation_id),
        )),
        ResolvedAnchorReference::Marker { marker_id } => Some(ArtifactMarker::new(
            ArtifactMarkerId::Marker(*marker_id),
            effective_time,
            NonEmptyText::new("marker").expect("marker kind is non-empty"),
            marker_label(marker_id),
        )),
        ResolvedAnchorReference::Interval | ResolvedAnchorReference::SourceFrames { .. } => None,
    })
}

fn interaction_marker(anchor: &InteractionAnchor) -> Result<ArtifactMarker> {
    let label = format!(
        "Interaction {}: {}",
        anchor.interaction_id.as_uuid(),
        anchor.operation.stable_name()
    );
    Ok(ArtifactMarker::new(
        ArtifactMarkerId::Interaction(anchor.interaction_id),
        anchor.timing.dispatched_at,
        NonEmptyText::new("interaction").expect("interaction kind is non-empty"),
        NonEmptyText::new(label).map_err(|_| {
            KrometrailError::new(
                ErrorCode::InvalidInput,
                NonEmptyText::new("interaction marker label must not be empty")
                    .expect("marker label error is non-empty"),
            )
        })?,
    ))
}

fn navigation_marker(navigation_id: NavigationId, time: SessionTime) -> Result<ArtifactMarker> {
    Ok(ArtifactMarker::new(
        ArtifactMarkerId::Navigation(navigation_id),
        time,
        NonEmptyText::new("navigation").expect("navigation kind is non-empty"),
        navigation_label(&navigation_id),
    ))
}

fn generic_marker(marker_id: MarkerId, time: SessionTime) -> Result<ArtifactMarker> {
    Ok(ArtifactMarker::new(
        ArtifactMarkerId::Marker(marker_id),
        time,
        NonEmptyText::new("marker").expect("marker kind is non-empty"),
        marker_label(&marker_id),
    ))
}

fn interaction_label(
    interaction_id: &InteractionId,
    interactions: &BTreeMap<InteractionId, InteractionAnchor>,
) -> Result<NonEmptyText> {
    if let Some(anchor) = interactions.get(interaction_id) {
        let label = format!(
            "Interaction {}: {}",
            interaction_id.as_uuid(),
            anchor.operation.stable_name()
        );
        NonEmptyText::new(label).map_err(|_| {
            KrometrailError::new(
                ErrorCode::InvalidInput,
                NonEmptyText::new("interaction anchor marker label must not be empty")
                    .expect("marker label error is non-empty"),
            )
        })
    } else {
        // The anchor's interaction evidence is no longer retained; fall back to
        // the typed ID only. The anchor marker remains mandatory and explicit.
        NonEmptyText::new(format!("Interaction {}", interaction_id.as_uuid())).map_err(|_| {
            KrometrailError::new(
                ErrorCode::InvalidInput,
                NonEmptyText::new("interaction fallback label must not be empty")
                    .expect("marker label error is non-empty"),
            )
        })
    }
}

fn navigation_label(navigation_id: &NavigationId) -> NonEmptyText {
    NonEmptyText::new(format!("Navigation {}", navigation_id.as_uuid()))
        .expect("navigation label is non-empty")
}

fn marker_label(marker_id: &MarkerId) -> NonEmptyText {
    NonEmptyText::new(format!("Marker {}", marker_id.as_uuid())).expect("marker label is non-empty")
}

fn anchor_marker_matches_interaction(
    anchor_id: &Option<ArtifactMarkerId>,
    interaction_id: &InteractionId,
) -> bool {
    matches!(
        anchor_id,
        Some(ArtifactMarkerId::Interaction(id)) if id == interaction_id
    )
}

fn anchor_marker_matches_navigation(
    anchor_id: &Option<ArtifactMarkerId>,
    navigation_id: &NavigationId,
) -> bool {
    matches!(
        anchor_id,
        Some(ArtifactMarkerId::Navigation(id)) if id == navigation_id
    )
}

fn anchor_marker_matches_marker(
    anchor_id: &Option<ArtifactMarkerId>,
    marker_id: &MarkerId,
) -> bool {
    matches!(
        anchor_id,
        Some(ArtifactMarkerId::Marker(id)) if id == marker_id
    )
}

/// Class rank for candidate selection and presentation order.
///
/// Interaction sorts before navigation, which sorts before marker; caller
/// markers (mandatory only) sort last at equal times.
fn class_rank(id: &ArtifactMarkerId) -> u8 {
    match id {
        ArtifactMarkerId::Interaction(_) => 0,
        ArtifactMarkerId::Navigation(_) => 1,
        ArtifactMarkerId::Marker(_) => 2,
        ArtifactMarkerId::Caller(_) => 3,
    }
}

/// Stable, deterministic byte comparison key for an `ArtifactMarkerId`.
fn stable_id_bytes(id: &ArtifactMarkerId) -> Vec<u8> {
    match id {
        ArtifactMarkerId::Interaction(id) => id.as_uuid().as_bytes().to_vec(),
        ArtifactMarkerId::Navigation(id) => id.as_uuid().as_bytes().to_vec(),
        ArtifactMarkerId::Marker(id) => id.as_uuid().as_bytes().to_vec(),
        ArtifactMarkerId::Caller(text) => text.as_str().as_bytes().to_vec(),
    }
}

/// Ranking key for candidate selection: closer to the anchor first, then class,
/// then session time, then stable ID bytes.
fn candidate_rank(marker: &ArtifactMarker, anchor: SessionTime) -> (u64, u8, u64, Vec<u8>) {
    (
        marker.session_time().as_nanos().abs_diff(anchor.as_nanos()),
        class_rank(marker.id()),
        marker.session_time().as_nanos(),
        stable_id_bytes(marker.id()),
    )
}

/// Presentation order: chronological, then class, then stable ID bytes.
fn presentation_order(marker: &ArtifactMarker) -> (u64, u8, Vec<u8>) {
    (
        marker.session_time().as_nanos(),
        class_rank(marker.id()),
        stable_id_bytes(marker.id()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use krometrail_core::{
        BrowserOperationKind, CaptureGapPolicy, InteractionId, InteractionTiming,
        MAX_BUNDLE_ARTIFACT_MARKERS, MarkerId, NavigationId, ObservationKind,
        ObservationPayloadRef, ObservedTime, RangeResolutionOptions, ResolvedAnchor,
        ResolvedAnchorReference, RetentionPolicy, SessionId, SessionRange, SessionTime, TargetId,
        TemporalRangeAnchorKind, TimelineObservation,
    };
    use uuid::Uuid;

    fn session() -> SessionId {
        SessionId::from_uuid(Uuid::from_u128(1))
    }
    fn target() -> TargetId {
        TargetId::from_uuid(Uuid::from_u128(2))
    }

    fn interval_range(start: u64, end: u64) -> ResolvedRange {
        let range = SessionRange::new(SessionTime::from_nanos(start), SessionTime::from_nanos(end))
            .unwrap();
        ResolvedRange::new(
            session(),
            target(),
            TemporalRangeAnchorKind::SessionTime,
            range,
            range,
            vec![krometrail_core::FrameId::from_uuid(Uuid::from_u128(99))],
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

    fn interaction_range(interaction_id: InteractionId, dispatch: u64) -> ResolvedRange {
        let _anchor = krometrail_core::InteractionAnchor::new(
            interaction_id,
            session(),
            target(),
            BrowserOperationKind::Click,
            InteractionTiming::new(
                SessionTime::from_nanos(dispatch.saturating_sub(50)),
                SessionTime::from_nanos(dispatch),
                SessionTime::from_nanos(dispatch + 100),
                Some(SessionTime::from_nanos(dispatch + 100)),
            )
            .unwrap(),
        )
        .unwrap();
        let requested = SessionRange::new(
            SessionTime::from_nanos(dispatch.saturating_sub(150)),
            SessionTime::from_nanos(dispatch + 250),
        )
        .unwrap();
        ResolvedRange::new_with_anchor(
            session(),
            target(),
            TemporalRangeAnchorKind::Interaction,
            ResolvedAnchor::new(
                ResolvedAnchorReference::Interaction { interaction_id },
                SessionTime::from_nanos(dispatch),
                SessionTime::from_nanos(dispatch),
            )
            .unwrap(),
            requested,
            requested,
            vec![krometrail_core::FrameId::from_uuid(Uuid::from_u128(99))],
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

    fn marker(id: ArtifactMarkerId, time: u64, kind: &str, label: &str) -> ArtifactMarker {
        ArtifactMarker::new(
            id,
            SessionTime::from_nanos(time),
            NonEmptyText::new(kind).unwrap(),
            NonEmptyText::new(label).unwrap(),
        )
    }

    fn timeline_observation(
        time: u64,
        kind: ObservationKind,
        payload: ObservationPayloadRef,
    ) -> TimelineObservation {
        TimelineObservation::new(
            session(),
            target(),
            SessionTime::from_nanos(time),
            None,
            ObservedTime::from_nanos(time + 1),
            kind,
            payload,
        )
        .unwrap()
    }

    fn slice(observations: Vec<TimelineObservation>, matched_count: u64) -> TimelineRangeSlice {
        let truncated = matched_count > observations.len() as u64;
        TimelineRangeSlice {
            matched_count,
            observations,
            truncated,
        }
    }

    fn interaction_anchor(id: u128, dispatch: u64) -> (InteractionId, InteractionAnchor) {
        let interaction_id = InteractionId::from_uuid(Uuid::from_u128(id));
        (
            interaction_id,
            krometrail_core::InteractionAnchor::new(
                interaction_id,
                session(),
                target(),
                BrowserOperationKind::Click,
                InteractionTiming::new(
                    SessionTime::from_nanos(dispatch.saturating_sub(50)),
                    SessionTime::from_nanos(dispatch),
                    SessionTime::from_nanos(dispatch + 100),
                    Some(SessionTime::from_nanos(dispatch + 100)),
                )
                .unwrap(),
            )
            .unwrap(),
        )
    }

    #[test]
    fn caller_markers_and_anchor_marker_are_mandatory_and_chronological() {
        let interaction_id = InteractionId::from_uuid(Uuid::from_u128(7));
        let range = interaction_range(interaction_id, 500);
        let caller = marker(
            ArtifactMarkerId::Caller(NonEmptyText::new("caller-1").unwrap()),
            100,
            "caller",
            "exact caller label",
        );
        let timeline = slice(vec![], 0);
        let interactions = std::iter::once(interaction_anchor(7, 500)).collect();
        let assembled = assemble_markers(MarkerEvidence {
            range: &range,
            caller_markers: std::slice::from_ref(&caller),
            timeline: &timeline,
            interactions: &interactions,
        })
        .unwrap();
        assert_eq!(assembled.markers.len(), 2);
        // Chronological: caller at 100, anchor at 500.
        assert_eq!(
            assembled.markers[0].session_time(),
            SessionTime::from_nanos(100)
        );
        assert_eq!(
            assembled.markers[1].session_time(),
            SessionTime::from_nanos(500)
        );
        assert_eq!(
            assembled.markers[1].id(),
            &ArtifactMarkerId::Interaction(interaction_id)
        );
        assert_eq!(assembled.markers[1].kind().as_str(), "interaction");
        assert_eq!(
            assembled.markers[1].label().as_str(),
            "Interaction 00000000-0000-0000-0000-000000000007: click"
        );
        assert!(assembled.warnings.is_empty());
    }

    #[test]
    fn interaction_navigation_and_generic_markers_use_exact_identity_and_privacy() {
        let range = interval_range(0, 1_000);
        let interaction_id = InteractionId::from_uuid(Uuid::from_u128(10));
        let navigation_id = NavigationId::from_uuid(Uuid::from_u128(11));
        let marker_id = MarkerId::from_uuid(Uuid::from_u128(12));
        let timeline = slice(
            vec![
                timeline_observation(
                    200,
                    ObservationKind::InteractionBoundary,
                    ObservationPayloadRef::Interaction(interaction_id),
                ),
                timeline_observation(
                    300,
                    ObservationKind::Navigation,
                    ObservationPayloadRef::Navigation(navigation_id),
                ),
                timeline_observation(
                    400,
                    ObservationKind::Marker,
                    ObservationPayloadRef::Marker(marker_id),
                ),
            ],
            3,
        );
        let interactions = std::iter::once(interaction_anchor(10, 200)).collect();
        let assembled = assemble_markers(MarkerEvidence {
            range: &range,
            caller_markers: &[],
            timeline: &timeline,
            interactions: &interactions,
        })
        .unwrap();
        assert_eq!(assembled.markers.len(), 3);
        // Interaction marker at dispatch time (200), not boundary observation time.
        assert_eq!(
            assembled.markers[0].id(),
            &ArtifactMarkerId::Interaction(interaction_id)
        );
        assert_eq!(
            assembled.markers[0].session_time(),
            SessionTime::from_nanos(200)
        );
        assert_eq!(
            assembled.markers[0].label().as_str(),
            "Interaction 00000000-0000-0000-0000-00000000000a: click"
        );
        assert_eq!(
            assembled.markers[1].id(),
            &ArtifactMarkerId::Navigation(navigation_id)
        );
        assert_eq!(
            assembled.markers[1].session_time(),
            SessionTime::from_nanos(300)
        );
        assert_eq!(
            assembled.markers[1].label().as_str(),
            "Navigation 00000000-0000-0000-0000-00000000000b"
        );
        assert_eq!(
            assembled.markers[2].id(),
            &ArtifactMarkerId::Marker(marker_id)
        );
        assert_eq!(
            assembled.markers[2].session_time(),
            SessionTime::from_nanos(400)
        );
        assert_eq!(
            assembled.markers[2].label().as_str(),
            "Marker 00000000-0000-0000-0000-00000000000c"
        );
        // Generic marker without caller presentation emits the label warning.
        assert_eq!(assembled.warnings.len(), 1);
        assert!(matches!(
            assembled.warnings[0],
            BundleWarning::MarkerLabelUnavailable { marker_id } if marker_id == MarkerId::from_uuid(Uuid::from_u128(12))
        ));
    }

    #[test]
    fn caller_marker_provides_presentation_for_matching_generic_timeline_marker() {
        let range = interval_range(0, 1_000);
        let marker_id = MarkerId::from_uuid(Uuid::from_u128(12));
        let caller = marker(
            ArtifactMarkerId::Marker(marker_id),
            400,
            "caller-kind",
            "exact caller text",
        );
        let timeline = slice(
            vec![timeline_observation(
                400,
                ObservationKind::Marker,
                ObservationPayloadRef::Marker(marker_id),
            )],
            1,
        );
        let interactions = BTreeMap::new();
        let assembled = assemble_markers(MarkerEvidence {
            range: &range,
            caller_markers: std::slice::from_ref(&caller),
            timeline: &timeline,
            interactions: &interactions,
        })
        .unwrap();
        // One marker total: the caller marker (matching the generic timeline marker).
        assert_eq!(assembled.markers.len(), 1);
        assert_eq!(assembled.markers[0].kind().as_str(), "caller-kind");
        assert_eq!(assembled.markers[0].label().as_str(), "exact caller text");
        // No label-unavailable warning because the caller supplied presentation.
        assert!(assembled.warnings.is_empty());
    }

    #[test]
    fn candidates_rank_by_distance_class_time_then_id_and_truncate_at_256() {
        let range = interval_range(0, 10_000);
        let anchor_time = 5_000;
        let mut observations = Vec::new();
        let mut interactions = BTreeMap::new();
        // 300 interactions at 5_000 + i (i = 0..300), so 300 candidates + 0 mandatory.
        for index in 0..300u128 {
            let interaction_id = InteractionId::from_uuid(Uuid::from_u128(100 + index));
            let time = anchor_time + index as u64;
            observations.push(timeline_observation(
                time,
                ObservationKind::InteractionBoundary,
                ObservationPayloadRef::Interaction(interaction_id),
            ));
            let (id, anchor) = interaction_anchor(100 + index, time);
            interactions.insert(id, anchor);
        }
        let timeline = slice(observations, 300);
        let assembled = assemble_markers(MarkerEvidence {
            range: &range,
            caller_markers: &[],
            timeline: &timeline,
            interactions: &interactions,
        })
        .unwrap();
        assert_eq!(assembled.markers.len(), MAX_BUNDLE_ARTIFACT_MARKERS);
        assert_eq!(assembled.warnings.len(), 1);
        let BundleWarning::MarkersTruncated {
            matched_count,
            returned_count,
            limit,
        } = &assembled.warnings[0]
        else {
            panic!("expected markers truncated warning");
        };
        assert_eq!(*matched_count, 300);
        assert_eq!(*returned_count, MAX_BUNDLE_ARTIFACT_MARKERS as u64);
        assert_eq!(*limit, MAX_BUNDLE_ARTIFACT_MARKERS as u16);
        // The 256 closest interactions to the anchor (times 5_000..5_255) are kept.
        for marker in &assembled.markers {
            let time = marker.session_time().as_nanos();
            assert!((anchor_time..anchor_time + 256).contains(&time));
        }
        // Presentation order is chronological.
        assert!(
            assembled
                .markers
                .windows(2)
                .all(|pair| pair[0].session_time() <= pair[1].session_time())
        );
    }

    #[test]
    fn timeline_source_truncation_emits_exact_warning() {
        let range = interval_range(0, 10_000);
        // matched_count (2000) exceeds the returned observations (2).
        let timeline = TimelineRangeSlice {
            matched_count: 2000,
            observations: vec![timeline_observation(
                100,
                ObservationKind::Marker,
                ObservationPayloadRef::Marker(MarkerId::from_uuid(Uuid::from_u128(5))),
            )],
            truncated: true,
        };
        let assembled = assemble_markers(MarkerEvidence {
            range: &range,
            caller_markers: &[],
            timeline: &timeline,
            interactions: &BTreeMap::new(),
        })
        .unwrap();
        assert_eq!(assembled.warnings.len(), 2);
        assert!(matches!(
            assembled.warnings[0],
            BundleWarning::TimelineMarkerEvidenceTruncated {
                matched_count: 2000,
                returned_count: 1,
                limit: MAX_BUNDLE_TIMELINE_ROWS,
            }
        ));
        assert!(matches!(
            assembled.warnings[1],
            BundleWarning::MarkerLabelUnavailable { .. }
        ));
    }

    #[test]
    fn equal_time_markers_order_by_class_then_id() {
        let range = interval_range(0, 1_000);
        let interaction_id = InteractionId::from_uuid(Uuid::from_u128(20));
        let navigation_id = NavigationId::from_uuid(Uuid::from_u128(21));
        let marker_id = MarkerId::from_uuid(Uuid::from_u128(22));
        // All at the same time; interaction < navigation < marker.
        let timeline = slice(
            vec![
                timeline_observation(
                    500,
                    ObservationKind::Marker,
                    ObservationPayloadRef::Marker(marker_id),
                ),
                timeline_observation(
                    500,
                    ObservationKind::Navigation,
                    ObservationPayloadRef::Navigation(navigation_id),
                ),
                timeline_observation(
                    500,
                    ObservationKind::InteractionBoundary,
                    ObservationPayloadRef::Interaction(interaction_id),
                ),
            ],
            3,
        );
        let interactions = std::iter::once(interaction_anchor(20, 500)).collect();
        let assembled = assemble_markers(MarkerEvidence {
            range: &range,
            caller_markers: &[],
            timeline: &timeline,
            interactions: &interactions,
        })
        .unwrap();
        assert_eq!(assembled.markers.len(), 3);
        assert!(matches!(
            assembled.markers[0].id(),
            ArtifactMarkerId::Interaction(_)
        ));
        assert!(matches!(
            assembled.markers[1].id(),
            ArtifactMarkerId::Navigation(_)
        ));
        assert!(matches!(
            assembled.markers[2].id(),
            ArtifactMarkerId::Marker(_)
        ));
    }

    #[test]
    fn interval_anchor_has_no_mandatory_anchor_marker() {
        let range = interval_range(0, 1_000);
        let timeline = slice(vec![], 0);
        let assembled = assemble_markers(MarkerEvidence {
            range: &range,
            caller_markers: &[],
            timeline: &timeline,
            interactions: &BTreeMap::new(),
        })
        .unwrap();
        assert!(assembled.markers.is_empty());
        assert!(assembled.warnings.is_empty());
    }

    #[test]
    fn labels_contain_no_persisted_secrets_or_locator_values() {
        let range = interval_range(0, 1_000);
        let interaction_id = InteractionId::from_uuid(Uuid::from_u128(30));
        let timeline = slice(
            vec![timeline_observation(
                200,
                ObservationKind::InteractionBoundary,
                ObservationPayloadRef::Interaction(interaction_id),
            )],
            1,
        );
        let interactions = std::iter::once(interaction_anchor(30, 200)).collect();
        let assembled = assemble_markers(MarkerEvidence {
            range: &range,
            caller_markers: &[],
            timeline: &timeline,
            interactions: &interactions,
        })
        .unwrap();
        let encoded = serde_json::to_string(&assembled.markers).unwrap();
        for forbidden in [
            "selector", "locator", "url", "http", "cookie", "token", "password", "param", "body",
            "text",
        ] {
            assert!(
                !encoded.to_lowercase().contains(forbidden),
                "marker payload leaked forbidden term: {forbidden}"
            );
        }
    }
}
