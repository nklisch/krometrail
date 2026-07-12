use serde::{Deserialize, Serialize};

use crate::{
    error::{Result, invalid},
    ids::{FrameId, GapId, InteractionId, MarkerId, NavigationId, SessionId, TargetId},
    time::{ObservedTime, SessionTime, SourceTime},
    validation::deserialize_validated,
};

macro_rules! define_observation_contract {
    ($( $kind:ident => $payload_spec:tt ),+ $(,)?) => {
        define_observation_contract!(@collect [] [] [] [] ; $( $kind => $payload_spec ),+);
    };

    (@collect
        [$($kind:ident,)*]
        [$($payload_variant:tt)*]
        [$($payload_pattern:tt)*]
        [$($test_payload:tt)*]
        ;
    ) => {
        #[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
        #[serde(rename_all = "snake_case")]
        pub enum ObservationKind {
            $($kind,)*
        }

        #[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
        #[serde(tag = "kind", content = "id", rename_all = "snake_case")]
        pub enum ObservationPayloadRef {
            $($payload_variant)*
            External(String),
        }

        impl ObservationKind {
            fn matches_payload(self, payload: &ObservationPayloadRef) -> bool {
                match (self, payload) {
                    $($payload_pattern)*
                    _ => false,
                }
            }

            #[cfg(test)]
            const ALL: &'static [Self] = &[$(Self::$kind),*];
        }

        #[cfg(test)]
        mod generated_contract_tests {
            use super::*;

            const UUID: &str = "123e4567-e89b-12d3-a456-426614174000";

            fn payloads() -> Vec<ObservationPayloadRef> {
                let mut payloads: Vec<_> = [$($test_payload)*]
                    .into_iter()
                    .flatten()
                    .collect();
                payloads.push(ObservationPayloadRef::External("external".into()));
                payloads
            }

            #[test]
            fn all_registered_observation_pairs_are_compatible() {
                let payloads = payloads();
                for kind in ObservationKind::ALL {
                    let compatible = payloads
                        .iter()
                        .filter(|payload| kind.matches_payload(payload))
                        .count();
                    assert_eq!(compatible, 1, "unexpected payload contract for {kind:?}");
                }
            }
        }
    };

    (@collect
        [$($kind:ident,)*]
        [$($payload_variant:tt)*]
        [$($payload_pattern:tt)*]
        [$($test_payload:tt)*]
        ;
        $next_kind:ident => (typed($payload:ident, $payload_type:ty))
        $(, $rest_kind:ident => $rest_spec:tt)*
    ) => {
        define_observation_contract!(@collect
            [$($kind,)* $next_kind,]
            [$($payload_variant)* $payload($payload_type),]
            [$($payload_pattern)* (ObservationKind::$next_kind, ObservationPayloadRef::$payload(_)) => true,]
            [$($test_payload)* Some(ObservationPayloadRef::$payload(<$payload_type>::from_uuid(UUID.parse().unwrap()))),]
            ; $( $rest_kind => $rest_spec ),*
        );
    };

    (@collect
        [$($kind:ident,)*]
        [$($payload_variant:tt)*]
        [$($payload_pattern:tt)*]
        [$($test_payload:tt)*]
        ;
        $next_kind:ident => (external)
        $(, $rest_kind:ident => $rest_spec:tt)*
    ) => {
        define_observation_contract!(@collect
            [$($kind,)* $next_kind,]
            [$($payload_variant)*]
            [$($payload_pattern)* (ObservationKind::$next_kind, ObservationPayloadRef::External(_)) => true,]
            [$($test_payload)* None,]
            ; $( $rest_kind => $rest_spec ),*
        );
    };
}

// Keep the public observation taxonomy and its payload compatibility contract in
// one declaration. External observations intentionally share one payload variant.
define_observation_contract! {
    Frame => (typed(Frame, FrameId)),
    InteractionBoundary => (typed(Interaction, InteractionId)),
    Navigation => (typed(Navigation, NavigationId)),
    TargetLifecycle => (external),
    VisibilityChange => (external),
    CaptureGap => (typed(Gap, GapId)),
    ConsoleMessage => (external),
    JavascriptException => (external),
    NetworkLifecycle => (external),
    Marker => (typed(Marker, MarkerId)),
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
        if !self.kind.matches_payload(&self.payload) {
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
