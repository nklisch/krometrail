use serde::{Deserialize, Serialize};

use crate::{
    error::{Result, invalid},
    ids::{FrameId, GapId, InteractionId, MarkerId, NavigationId, SessionId, TargetId},
    time::{ObservedTime, SessionTime, SourceTime},
    validation::deserialize_validated,
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

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct TimelineObservation {
    session_id: SessionId,
    target_id: TargetId,
    session_time: SessionTime,
    source_time: Option<SourceTime>,
    observed_time: ObservedTime,
    kind: ObservationKind,
    payload: ObservationPayloadRef,
}

#[derive(Deserialize)]
struct TimelineObservationWire {
    session_id: SessionId,
    target_id: TargetId,
    session_time: SessionTime,
    source_time: Option<SourceTime>,
    observed_time: ObservedTime,
    kind: ObservationKind,
    payload: ObservationPayloadRef,
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
        let observation = Self {
            session_id,
            target_id,
            session_time,
            source_time,
            observed_time,
            kind,
            payload,
        };
        observation.validate()?;
        Ok(observation)
    }

    pub const fn session_id(&self) -> SessionId {
        self.session_id
    }
    pub const fn target_id(&self) -> TargetId {
        self.target_id
    }
    pub const fn session_time(&self) -> SessionTime {
        self.session_time
    }
    pub const fn source_time(&self) -> Option<SourceTime> {
        self.source_time
    }
    pub const fn observed_time(&self) -> ObservedTime {
        self.observed_time
    }
    pub const fn kind(&self) -> ObservationKind {
        self.kind
    }
    pub fn payload(&self) -> &ObservationPayloadRef {
        &self.payload
    }

    pub fn validate(&self) -> Result<()> {
        let matches_kind = matches!(
            (&self.kind, &self.payload),
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
                    ObservationPayloadRef::External(_)
                ),
        );
        if !matches_kind {
            return Err(invalid(format!(
                "payload does not match observation kind {:?}",
                self.kind
            )));
        }
        if matches!(&self.payload, ObservationPayloadRef::External(value) if value.trim().is_empty())
        {
            return Err(invalid("external observation payload must not be empty"));
        }
        if self.session_time.as_nanos() > self.observed_time.as_nanos() {
            return Err(invalid(
                "observation session time must not exceed observed time",
            ));
        }
        Ok(())
    }
}

impl<'de> Deserialize<'de> for TimelineObservation {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserialize_validated(deserializer, |wire: TimelineObservationWire| {
            Self::new(
                wire.session_id,
                wire.target_id,
                wire.session_time,
                wire.source_time,
                wire.observed_time,
                wire.kind,
                wire.payload,
            )
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
                ObservationPayloadRef::Frame(frame)
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
                ObservationPayloadRef::Marker(marker)
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
                ObservationPayloadRef::External("console-1".into())
            )
            .is_ok()
        );
    }

    #[test]
    fn rejects_malformed_serialized_observation_pairs_and_times() {
        let value = serde_json::json!({
            "session_id": UUID, "target_id": UUID, "session_time": 0,
            "source_time": null, "observed_time": 1, "kind": "capture_gap",
            "payload": {"kind": "frame", "id": UUID}
        });
        assert!(serde_json::from_value::<TimelineObservation>(value).is_err());
        let value = serde_json::json!({
            "session_id": UUID, "target_id": UUID, "session_time": 2,
            "source_time": null, "observed_time": 1, "kind": "console_message",
            "payload": {"kind": "external", "id": "console-1"}
        });
        assert!(serde_json::from_value::<TimelineObservation>(value).is_err());
        let valid = TimelineObservation::new(
            SessionId::from_uuid(UUID.parse().unwrap()),
            TargetId::from_uuid(UUID.parse().unwrap()),
            SessionTime::ZERO,
            None,
            ObservedTime::from_nanos(1),
            ObservationKind::ConsoleMessage,
            ObservationPayloadRef::External("console-1".into()),
        )
        .unwrap();
        let encoded = serde_json::to_string(&valid).unwrap();
        assert_eq!(
            serde_json::from_str::<TimelineObservation>(&encoded).unwrap(),
            valid
        );
    }
}
