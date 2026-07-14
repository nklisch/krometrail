//! Temporal debug bundle policy, marker assembly, and focus extraction.
//!
//! This module owns the pure, versioned `temporal-debug-bundle-v1` evidence
//! policy: the exact artifact generator requests, bounded privacy-safe marker
//! assembly, and deterministic major-change focus extraction. It produces the
//! inputs the bundle service (Unit 3) composes into one `TemporalDebugBundle`;
//! it does not perform generation, event correlation, or orchestration.
//!
//! The functions and types here are `pub(crate)` for the upcoming
//! `TemporalDebugBundleService` (Unit 3 — bounded composition). They are
//! exercised by focused tests until the service wires them into `RuntimeDependencies`.

#![allow(dead_code)]

mod focus;
mod markers;
mod policy;

use std::collections::BTreeMap;

use krometrail_core::{
    ArtifactFailurePolicy, ArtifactMarker, ArtifactOutcome, BrowserEventFilter,
    BrowserEventSelection, EffectiveBundlePolicy, InteractionAnchor, InteractionAnchorSource,
    InteractionId, InteractionRecordSource, OrientationPolicy, ResolvedRange, Result, SessionTime,
    TemporalDebugBundleRequest, TimelineRangeSlice, TimelineStore,
};
use policy::policy_version;

// Re-exported for the upcoming `TemporalDebugBundleService` (Unit 3). They are
// exercised by focused tests until the service wires them into composition.
#[allow(unused_imports)]
pub(crate) use focus::extract_focus_times;
#[allow(unused_imports)]
pub(crate) use markers::{AssembledMarkers, MarkerEvidence, assemble_markers};
#[allow(unused_imports)]
pub(crate) use policy::{default_artifact_request, default_generators};

/// The zero-method intersection of the three evidence-reading ports the bundle
/// service projects over one concrete store. It introduces no facade methods;
/// any type implementing `TimelineStore + InteractionAnchorSource +
/// InteractionRecordSource` implements it automatically.
pub(crate) trait TemporalDebugEvidenceStore:
    TimelineStore + InteractionAnchorSource + InteractionRecordSource
{
}

impl<T> TemporalDebugEvidenceStore for T where
    T: TimelineStore + InteractionAnchorSource + InteractionRecordSource
{
}

/// The bounded marker-evidence inputs the bundle service loads before visual
/// work begins. Marker assembly is pure given these inputs.
pub(crate) struct BundleMarkerEvidence<'a> {
    pub range: &'a ResolvedRange,
    pub caller_markers: &'a [ArtifactMarker],
    pub timeline: &'a TimelineRangeSlice,
    pub interactions: &'a BTreeMap<InteractionId, InteractionAnchor>,
}

/// Builds the effective bundle policy from the resolved range, caller
/// orientation choice, and extracted focus times.
///
/// The effective policy carries the versioned identifier, the exact resolved
/// anchor time, the two v1 generator requests, `AllowPartial` failure policy,
/// the default all-class/debug event filter, the default compact event
/// selection (limit 24), and the focus times. It is the observable contract
/// between the bundle request and the artifact/context services.
pub(crate) fn build_effective_policy(
    range: &ResolvedRange,
    orientation: OrientationPolicy,
    focus_times: Vec<SessionTime>,
) -> Result<EffectiveBundlePolicy> {
    EffectiveBundlePolicy::new(
        policy_version(),
        range.resolved_anchor.effective_time,
        default_generators(range, orientation),
        ArtifactFailurePolicy::AllowPartial,
        BrowserEventFilter::default(),
        BrowserEventSelection::compact_default(),
        focus_times,
    )
}

/// Convenience: extract focus times from artifact outcomes and build the
/// effective policy in one step. The bundle service calls this after artifact
/// generation completes.
pub(crate) fn effective_policy_from_outcomes(
    range: &ResolvedRange,
    request: &TemporalDebugBundleRequest,
    outcomes: &[ArtifactOutcome],
) -> Result<EffectiveBundlePolicy> {
    let focus_times = extract_focus_times(outcomes);
    build_effective_policy(range, request.orientation(), focus_times)
}

#[cfg(test)]
mod tests {
    use super::*;
    use krometrail_core::{
        CaptureGapPolicy, InteractionId, InteractionTiming, RangeResolutionOptions, ResolvedAnchor,
        ResolvedAnchorReference, RetentionPolicy, SessionId, SessionRange, SessionTime,
        TEMPORAL_DEBUG_BUNDLE_POLICY_VERSION, TargetId, TemporalQueryRequest, TemporalRangeAnchor,
        TemporalRangeAnchorKind,
    };
    use uuid::Uuid;

    fn session() -> SessionId {
        SessionId::from_uuid(Uuid::from_u128(1))
    }
    fn target() -> TargetId {
        TargetId::from_uuid(Uuid::from_u128(2))
    }

    fn interaction_range(interaction_id: InteractionId, dispatch: u64) -> ResolvedRange {
        let _anchor = krometrail_core::InteractionAnchor::new(
            interaction_id,
            session(),
            target(),
            krometrail_core::BrowserOperationKind::Click,
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

    #[test]
    fn build_effective_policy_carries_exact_v1_values() {
        let interaction_id = InteractionId::from_uuid(Uuid::from_u128(7));
        let range = interaction_range(interaction_id, 500);
        let request = TemporalDebugBundleRequest::default_policy(
            TemporalQueryRequest::strict(TemporalRangeAnchor::Interaction {
                scope: krometrail_core::AnchorScope::new(Some(session()), Some(target())),
                interaction_id,
                window: None,
            })
            .unwrap(),
        )
        .unwrap();
        let effective = build_effective_policy(&range, request.orientation(), vec![]).unwrap();
        assert_eq!(
            effective.version.as_str(),
            TEMPORAL_DEBUG_BUNDLE_POLICY_VERSION
        );
        assert_eq!(
            effective.artifact_anchor,
            range.resolved_anchor.effective_time
        );
        assert_eq!(effective.artifact_generators.len(), 2);
        assert_eq!(
            effective.artifact_failure_policy,
            ArtifactFailurePolicy::AllowPartial
        );
        assert!(effective.focus_times.is_empty());
        // Default event filter is all-class/debug.
        assert!(effective.event_filter.classes().is_empty());
        // Default event selection is compact with limit 24.
        assert!(matches!(
            effective.event_selection,
            BrowserEventSelection::Compact { .. }
        ));
    }

    #[test]
    fn build_effective_policy_validates_focus_time_count_and_ordering() {
        let interaction_id = InteractionId::from_uuid(Uuid::from_u128(7));
        let range = interaction_range(interaction_id, 500);
        // Duplicate focus times are rejected by EffectiveBundlePolicy::new.
        let dup = build_effective_policy(
            &range,
            OrientationPolicy::Include,
            vec![SessionTime::from_nanos(400), SessionTime::from_nanos(400)],
        );
        assert!(dup.is_err());
        // Unsorted focus times are rejected.
        let unsorted = build_effective_policy(
            &range,
            OrientationPolicy::Include,
            vec![SessionTime::from_nanos(600), SessionTime::from_nanos(400)],
        );
        assert!(unsorted.is_err());
        // Range containment of focus times is validated by TemporalDebugBundle::new
        // (Unit 3), not by build_effective_policy; the effective policy is a pure
        // value that the bundle constructor cross-checks against the range.
        let ok = build_effective_policy(
            &range,
            OrientationPolicy::Include,
            vec![SessionTime::from_nanos(400), SessionTime::from_nanos(600)],
        );
        assert!(ok.is_ok());
    }

    #[test]
    fn trait_alias_accepts_any_type_implementing_the_three_ports() {
        // Static check: TemporalDebugEvidenceStore is auto-implemented for any
        // type implementing TimelineStore + InteractionAnchorSource +
        // InteractionRecordSource. The trait object form compiles only if the
        // supertrait intersection is object-safe, which it is by construction.
        let _: Option<Box<dyn TemporalDebugEvidenceStore>> = None;
        fn accepts<T: TemporalDebugEvidenceStore>(_: &T) {}
        // The concrete RecordingStore satisfies this after Unit 2 adds the
        // interaction-source delegations. Verify the blanket impl applies.
        let _ = accepts as fn(&krometrail_store::RecordingStore);
    }
}
