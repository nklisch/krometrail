use crate::{
    AnchorScope, InteractionAnchor, InteractionId, ObservationKind, ObservationPayloadRef,
    PortFuture, Result, SessionId, TargetId, TimelineObservation,
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
