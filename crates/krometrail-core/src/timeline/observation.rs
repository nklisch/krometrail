use serde::{Deserialize, Serialize};

use crate::{
    error::{Result, invalid},
    ids::{FrameId, GapId, InteractionId, MarkerId, NavigationId, SessionId, TargetId},
    time::{ObservedTime, SessionTime, SourceTime},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ObservationKind {
    Frame,
    InteractionBoundary,
    Navigation,
    TargetLifecycle,
    VisibilityChange,
    CaptureGap,
    ConsoleMessage,
    JavascriptException,
    NetworkLifecycle,
    Marker,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "id", rename_all = "snake_case")]
pub enum ObservationPayloadRef {
    Frame(FrameId),
    Interaction(InteractionId),
    Navigation(NavigationId),
    Gap(GapId),
    Marker(MarkerId),
    External(String),
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct TimelineObservation {
    pub session_id: SessionId,
    pub target_id: TargetId,
    pub session_time: SessionTime,
    pub source_time: Option<SourceTime>,
    pub observed_time: ObservedTime,
    pub kind: ObservationKind,
    pub payload: ObservationPayloadRef,
}

impl TimelineObservation {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        session_id: SessionId,
        target_id: TargetId,
        session_time: SessionTime,
        source_time: Option<SourceTime>,
        observed_time: ObservedTime,
        kind: ObservationKind,
        payload: ObservationPayloadRef,
    ) -> Result<Self> {
        let matches_kind = matches!(
            (&kind, &payload),
            (ObservationKind::Frame, ObservationPayloadRef::Frame(_))
                | (
                    ObservationKind::InteractionBoundary,
                    ObservationPayloadRef::Interaction(_)
                )
                | (
                    ObservationKind::Navigation,
                    ObservationPayloadRef::Navigation(_)
                )
                | (ObservationKind::CaptureGap, ObservationPayloadRef::Gap(_))
                | (ObservationKind::Marker, ObservationPayloadRef::Marker(_))
                | (
                    ObservationKind::TargetLifecycle
                        | ObservationKind::VisibilityChange
                        | ObservationKind::ConsoleMessage
                        | ObservationKind::JavascriptException
                        | ObservationKind::NetworkLifecycle,
                    ObservationPayloadRef::External(_),
                )
        );
        if !matches_kind {
            return Err(invalid(format!(
                "payload does not match observation kind {kind:?}"
            )));
        }
        if matches!(&payload, ObservationPayloadRef::External(value) if value.trim().is_empty()) {
            return Err(invalid("external observation payload must not be empty"));
        }
        Ok(Self {
            session_id,
            target_id,
            session_time,
            source_time,
            observed_time,
            kind,
            payload,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const UUID: &str = "123e4567-e89b-12d3-a456-426614174000";

    #[test]
    fn rejects_payload_kind_mismatches() {
        let session = SessionId::from_uuid(UUID.parse().unwrap());
        let target = TargetId::from_uuid(UUID.parse().unwrap());
        let frame = FrameId::from_uuid(UUID.parse().unwrap());
        assert!(
            TimelineObservation::new(
                session,
                target,
                SessionTime::ZERO,
                None,
                ObservedTime::from_nanos(1),
                ObservationKind::CaptureGap,
                ObservationPayloadRef::Frame(frame),
            )
            .is_err()
        );
    }

    #[test]
    fn accepts_matching_payloads_and_external_evidence() {
        let session = SessionId::from_uuid(UUID.parse().unwrap());
        let target = TargetId::from_uuid(UUID.parse().unwrap());
        let marker = MarkerId::from_uuid(UUID.parse().unwrap());
        assert!(
            TimelineObservation::new(
                session,
                target,
                SessionTime::ZERO,
                Some(SourceTime::from_nanos(2)),
                ObservedTime::from_nanos(3),
                ObservationKind::Marker,
                ObservationPayloadRef::Marker(marker),
            )
            .is_ok()
        );
        assert!(
            TimelineObservation::new(
                session,
                target,
                SessionTime::ZERO,
                None,
                ObservedTime::from_nanos(3),
                ObservationKind::ConsoleMessage,
                ObservationPayloadRef::External("console-1".into()),
            )
            .is_ok()
        );
    }
}
