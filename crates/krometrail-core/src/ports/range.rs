use std::sync::Arc;

use crate::{
    AnchorScope, InteractionAnchor, InteractionId, InteractionRecord, NavigationId,
    ObservationKind, ObservationPayloadRef, ObservedTime, PortFuture, Result, SessionId, TargetId,
    TimelineObservation,
};

/// Finds typed timeline anchors without exposing the index representation.
pub trait TimelineAnchorSource: Send + Sync {
    fn observation_for_payload(
        &self,
        scope: AnchorScope,
        kind: ObservationKind,
        payload: ObservationPayloadRef,
    ) -> PortFuture<'_, Result<Option<TimelineObservation>>>;

    fn latest_observation(
        &self,
        session_id: SessionId,
        target_id: TargetId,
        kind: ObservationKind,
    ) -> PortFuture<'_, Result<Option<TimelineObservation>>>;
}

/// Reads durable interaction timing projections. Implementations must not infer
/// anchors from live operation results or unrelated timeline observations.
pub trait InteractionAnchorSource: Send + Sync {
    fn interaction_anchor(
        &self,
        interaction_id: InteractionId,
    ) -> PortFuture<'_, Result<Option<InteractionAnchor>>>;

    fn latest_interaction_anchor(
        &self,
        session_id: SessionId,
        target_id: TargetId,
    ) -> PortFuture<'_, Result<Option<InteractionAnchor>>>;
}

/// Persists browser-produced operation evidence before a state-changing result is published.
pub trait InteractionEvidenceSink: Send + Sync {
    fn append_operation_evidence(
        &self,
        anchor: InteractionAnchor,
        record: Option<InteractionRecord>,
        persisted_at: ObservedTime,
        navigation_id: Option<NavigationId>,
    ) -> PortFuture<'_, Result<()>>;
}

/// Reads the exact optional browser-produced action record for an interaction.
pub trait InteractionRecordSource: Send + Sync {
    fn interaction_record(
        &self,
        interaction_id: InteractionId,
    ) -> PortFuture<'_, Result<Option<InteractionRecord>>>;
}

impl<T: TimelineAnchorSource + ?Sized> TimelineAnchorSource for Arc<T> {
    fn observation_for_payload(
        &self,
        scope: AnchorScope,
        kind: ObservationKind,
        payload: ObservationPayloadRef,
    ) -> PortFuture<'_, Result<Option<TimelineObservation>>> {
        (**self).observation_for_payload(scope, kind, payload)
    }
    fn latest_observation(
        &self,
        session_id: SessionId,
        target_id: TargetId,
        kind: ObservationKind,
    ) -> PortFuture<'_, Result<Option<TimelineObservation>>> {
        (**self).latest_observation(session_id, target_id, kind)
    }
}

impl<T: InteractionEvidenceSink + ?Sized> InteractionEvidenceSink for Arc<T> {
    fn append_operation_evidence(
        &self,
        anchor: InteractionAnchor,
        record: Option<InteractionRecord>,
        persisted_at: ObservedTime,
        navigation_id: Option<NavigationId>,
    ) -> PortFuture<'_, Result<()>> {
        (**self).append_operation_evidence(anchor, record, persisted_at, navigation_id)
    }
}

impl<T: InteractionRecordSource + ?Sized> InteractionRecordSource for Arc<T> {
    fn interaction_record(
        &self,
        interaction_id: InteractionId,
    ) -> PortFuture<'_, Result<Option<InteractionRecord>>> {
        (**self).interaction_record(interaction_id)
    }
}

impl<T: InteractionAnchorSource + ?Sized> InteractionAnchorSource for Arc<T> {
    fn interaction_anchor(
        &self,
        interaction_id: InteractionId,
    ) -> PortFuture<'_, Result<Option<InteractionAnchor>>> {
        (**self).interaction_anchor(interaction_id)
    }
    fn latest_interaction_anchor(
        &self,
        session_id: SessionId,
        target_id: TargetId,
    ) -> PortFuture<'_, Result<Option<InteractionAnchor>>> {
        (**self).latest_interaction_anchor(session_id, target_id)
    }
}
