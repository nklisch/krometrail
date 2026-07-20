use serde::{Deserialize, Serialize};

use crate::{
    CapabilityId, CaptureGapPolicy, CaptureGapStore, CaptureQuality, FrameSource,
    InteractionAnchorSource, OperationMutability, PortFuture, RangeResolutionOptions,
    RecordingCatalog, ResolvedRange, Result, RetentionPolicy, TemporalRangeAnchor,
    TemporalRangeResolver, TimelineAnchorSource, TimelineStore, validation::deserialize_validated,
};

/// The single application-facing request for resolving temporal evidence.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TemporalQueryRequest {
    pub anchor: TemporalRangeAnchor,
    pub retention: RetentionPolicy,
    pub capture_gaps: CaptureGapPolicy,
}

#[derive(Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
struct TemporalQueryRequestWire {
    anchor: TemporalRangeAnchor,
    retention: RetentionPolicy,
    capture_gaps: CaptureGapPolicy,
}

impl TemporalQueryRequest {
    pub fn new(
        anchor: TemporalRangeAnchor,
        retention: RetentionPolicy,
        capture_gaps: CaptureGapPolicy,
    ) -> Result<Self> {
        anchor.validate()?;
        Ok(Self {
            anchor,
            retention,
            capture_gaps,
        })
    }

    pub fn strict(anchor: TemporalRangeAnchor) -> Result<Self> {
        Self::new(
            anchor,
            RetentionPolicy::RequireComplete,
            CaptureGapPolicy::Include,
        )
    }

    pub const fn options(&self) -> RangeResolutionOptions {
        RangeResolutionOptions {
            retention: self.retention,
            capture_gaps: self.capture_gaps,
            implicit_interaction_window: RangeResolutionOptions::DEFAULT
                .implicit_interaction_window,
        }
    }
}

impl<'de> Deserialize<'de> for TemporalQueryRequest {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        deserialize_validated(d, |wire: TemporalQueryRequestWire| {
            Self::new(wire.anchor, wire.retention, wire.capture_gaps)
        })
    }
}

crate::validation::delegate_json_schema!(TemporalQueryRequest => TemporalQueryRequestWire);

/// Resolves validated temporal requests without exposing persistence details.
pub trait TemporalQuery: Send + Sync {
    fn resolve_range(&self, request: TemporalQueryRequest)
    -> PortFuture<'_, Result<ResolvedRange>>;
}

/// The lightweight result returned by the natural-anchor range resolver.
/// Artifact generation and browser-event correlation remain separate operations.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct TemporalRangeResolution {
    pub range: ResolvedRange,
    pub capture_quality: CaptureQuality,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TemporalRangeResolutionOperationDefinition {
    pub stable_name: &'static str,
    pub description: &'static str,
    pub capability: CapabilityId,
    pub mutability: OperationMutability,
}

pub const TEMPORAL_RANGE_RESOLUTION_OPERATION: TemporalRangeResolutionOperationDefinition =
    TemporalRangeResolutionOperationDefinition {
        stable_name: "resolve_temporal_range",
        description: "Resolve a natural temporal anchor into a range handle and capture quality.",
        capability: CapabilityId::TemporalVision,
        mutability: OperationMutability::ReadOnly,
    };

/// Thin application service that preserves the resolver as the sole range authority.
pub struct TemporalQueryService<C, F, G, T, I> {
    resolver: TemporalRangeResolver<C, F, G, T, I>,
}

impl<C, F, G, T, I> TemporalQueryService<C, F, G, T, I> {
    pub const fn new(resolver: TemporalRangeResolver<C, F, G, T, I>) -> Self {
        Self { resolver }
    }
}

impl<C, F, G, T, I> TemporalQuery for TemporalQueryService<C, F, G, T, I>
where
    C: RecordingCatalog,
    F: FrameSource,
    G: CaptureGapStore,
    T: TimelineStore + TimelineAnchorSource,
    I: InteractionAnchorSource,
{
    fn resolve_range(
        &self,
        request: TemporalQueryRequest,
    ) -> PortFuture<'_, Result<ResolvedRange>> {
        let options = request.options();
        self.resolver.resolve(request.anchor, options)
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, SystemTime};

    use uuid::Uuid;

    use super::*;
    use crate::{
        AnchorScope, FrameId, InteractionId, InteractionWindow, IntervalAnchorScope, MarkerId,
        NavigationId, SessionId, SessionRange, SessionTime, TargetId,
    };

    fn session() -> SessionId {
        SessionId::from_uuid(Uuid::from_u128(1))
    }
    fn target() -> TargetId {
        TargetId::from_uuid(Uuid::from_u128(2))
    }
    fn scope() -> AnchorScope {
        AnchorScope::new(Some(session()), Some(target()))
    }

    fn interval_scope() -> IntervalAnchorScope {
        IntervalAnchorScope::new(session(), target())
    }

    fn anchors() -> Vec<TemporalRangeAnchor> {
        let window = Some(
            InteractionWindow::new(Duration::from_millis(1), Duration::from_millis(2)).unwrap(),
        );
        vec![
            TemporalRangeAnchor::SessionTime {
                scope: interval_scope(),
                range: SessionRange::new(SessionTime::ZERO, SessionTime::from_nanos(1)).unwrap(),
            },
            TemporalRangeAnchor::WallClock {
                scope: interval_scope(),
                start: SystemTime::UNIX_EPOCH,
                end: SystemTime::UNIX_EPOCH + Duration::from_nanos(1),
            },
            TemporalRangeAnchor::Interaction {
                scope: scope(),
                interaction_id: InteractionId::from_uuid(Uuid::from_u128(3)),
                window,
            },
            TemporalRangeAnchor::LatestInteraction {
                session_id: session(),
                target_id: target(),
                window,
            },
            TemporalRangeAnchor::Navigation {
                scope: scope(),
                navigation_id: NavigationId::from_uuid(Uuid::from_u128(4)),
                window,
            },
            TemporalRangeAnchor::Marker {
                scope: scope(),
                marker_id: MarkerId::from_uuid(Uuid::from_u128(5)),
                window,
            },
            TemporalRangeAnchor::SourceFrame {
                scope: scope(),
                start_frame_id: FrameId::from_uuid(Uuid::from_u128(6)),
                end_frame_id: FrameId::from_uuid(Uuid::from_u128(7)),
            },
        ]
    }

    #[test]
    fn every_anchor_round_trips_through_the_validated_request() {
        for anchor in anchors() {
            let request = TemporalQueryRequest::strict(anchor).unwrap();
            let json = serde_json::to_string(&request).unwrap();
            assert_eq!(
                serde_json::from_str::<TemporalQueryRequest>(&json).unwrap(),
                request
            );
        }
    }

    #[test]
    fn request_rejects_unknown_fields_invalid_scope_and_nil_source_frames() {
        let missing_scope = serde_json::json!({
            "anchor": "session_time",
            "scope": {"session_id": session()},
            "range": {"start": 0, "end": 0},
            "retention": "require_complete",
            "capture_gaps": "include"
        });
        assert!(serde_json::from_value::<TemporalQueryRequest>(missing_scope).is_err());

        let mut value =
            serde_json::to_value(TemporalQueryRequest::strict(anchors().remove(0)).unwrap())
                .unwrap();
        value["unexpected"] = serde_json::json!(true);
        assert!(serde_json::from_value::<TemporalQueryRequest>(value).is_err());

        assert!(
            TemporalQueryRequest::strict(TemporalRangeAnchor::SourceFrame {
                scope: scope(),
                start_frame_id: FrameId::from_uuid(Uuid::nil()),
                end_frame_id: FrameId::from_uuid(Uuid::from_u128(7)),
            })
            .is_err()
        );
    }

    #[test]
    fn strict_interval_scopes_deserialize_required_ids_while_other_anchors_remain_tolerant() {
        for anchor in [
            TemporalRangeAnchor::SessionTime {
                scope: interval_scope(),
                range: SessionRange::new(SessionTime::ZERO, SessionTime::ZERO).unwrap(),
            },
            TemporalRangeAnchor::WallClock {
                scope: interval_scope(),
                start: SystemTime::UNIX_EPOCH,
                end: SystemTime::UNIX_EPOCH,
            },
        ] {
            let strict =
                serde_json::to_value(TemporalQueryRequest::strict(anchor).unwrap()).unwrap();
            for field in ["session_id", "target_id"] {
                let mut missing = strict.clone();
                missing["anchor"]["scope"]
                    .as_object_mut()
                    .unwrap()
                    .remove(field);
                let error = serde_json::from_value::<TemporalQueryRequest>(missing).unwrap_err();
                assert!(error.to_string().contains("missing field"));
            }
        }

        for anchor in [
            TemporalRangeAnchor::Interaction {
                scope: AnchorScope::new(None, None),
                interaction_id: InteractionId::from_uuid(Uuid::from_u128(3)),
                window: None,
            },
            TemporalRangeAnchor::Navigation {
                scope: AnchorScope::new(None, None),
                navigation_id: NavigationId::from_uuid(Uuid::from_u128(4)),
                window: None,
            },
            TemporalRangeAnchor::Marker {
                scope: AnchorScope::new(None, None),
                marker_id: MarkerId::from_uuid(Uuid::from_u128(5)),
                window: None,
            },
            TemporalRangeAnchor::SourceFrame {
                scope: AnchorScope::new(None, None),
                start_frame_id: FrameId::from_uuid(Uuid::from_u128(6)),
                end_frame_id: FrameId::from_uuid(Uuid::from_u128(7)),
            },
            TemporalRangeAnchor::LatestInteraction {
                session_id: session(),
                target_id: target(),
                window: None,
            },
        ] {
            assert!(TemporalQueryRequest::strict(anchor).is_ok());
        }
    }

    #[test]
    fn natural_anchor_windows_accept_only_bounded_whole_milliseconds() {
        assert!(InteractionWindow::new(Duration::from_secs(120), Duration::from_secs(120)).is_ok());
        assert!(
            InteractionWindow::new(
                Duration::from_secs(120) + Duration::from_millis(1),
                Duration::ZERO,
            )
            .is_err()
        );
        assert!(InteractionWindow::new(Duration::from_micros(1), Duration::ZERO).is_err());
        for malformed in [
            serde_json::json!({"before_ms": 1.5, "after_ms": 0}),
            serde_json::json!({"before_ms": 0, "after_ms": 120_001}),
            serde_json::json!({"before_ms": 0, "after_ms": 0, "extra": true}),
        ] {
            assert!(serde_json::from_value::<InteractionWindow>(malformed).is_err());
        }

        let mut anchor = serde_json::to_value(anchors().remove(4)).unwrap();
        anchor["extra"] = serde_json::json!(true);
        assert!(serde_json::from_value::<TemporalRangeAnchor>(anchor).is_err());

        let permissive = TemporalQueryRequest::new(
            anchors().remove(0),
            RetentionPolicy::AllowPartial,
            CaptureGapPolicy::Reject,
        )
        .unwrap();
        assert_eq!(
            serde_json::from_str::<TemporalQueryRequest>(
                &serde_json::to_string(&permissive).unwrap()
            )
            .unwrap(),
            permissive
        );
    }

    #[test]
    fn strict_options_preserve_the_exact_implicit_interaction_window() {
        let request = TemporalQueryRequest::strict(anchors().remove(2)).unwrap();
        let options = request.options();
        assert_eq!(options.retention, RetentionPolicy::RequireComplete);
        assert_eq!(options.capture_gaps, CaptureGapPolicy::Include);
        assert_eq!(
            options.implicit_interaction_window.before(),
            Duration::from_millis(150)
        );
        assert_eq!(
            options.implicit_interaction_window.after(),
            Duration::from_millis(250)
        );
    }
}
