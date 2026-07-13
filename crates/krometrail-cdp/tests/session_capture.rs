#![cfg(feature = "cdpkit-transport")]

use std::num::NonZeroU64;

use krometrail_cdp::{
    ReconnectedSnapshot, ReconnectedTarget, SupervisorEffect, SupervisorInput, SupervisorState,
    TransportSessionId, TransportTargetInfo, reduce,
};
use krometrail_core::{
    BrowserCompatibility, BrowserProduct, BrowserProductVersion, BrowserSessionEvent,
    BrowserSessionState, CapabilitySupport, CaptureGap, CaptureGapReason, CaptureStatistics,
    CaptureStreamState, CaptureTimingSummary, GapId, RendererCapability, SessionId, SessionRange,
    SessionTime, TargetCaptureStatus, TargetId, TargetLifecycle, TargetVisibility,
};

const SESSION_UUID: &str = "123e4567-e89b-12d3-a456-426614174000";
const TARGET_UUID: &str = "123e4567-e89b-12d3-a456-426614174001";

fn compatibility() -> BrowserCompatibility {
    BrowserCompatibility::new(
        krometrail_core::BrowserVersion::new(
            BrowserProduct::Chrome,
            BrowserProductVersion::new("128").unwrap(),
            "revision",
            "1.3",
            "Chrome/128",
            "12",
        )
        .unwrap(),
        RendererCapability::ALL
            .iter()
            .map(|capability| CapabilitySupport::new(*capability, true, true, None).unwrap())
            .collect(),
    )
    .unwrap()
}

fn target(key: &str) -> TransportTargetInfo {
    TransportTargetInfo::new(key, "page", format!("https://{key}.test"), key, false, None).unwrap()
}

fn attached_visible_state() -> SupervisorState {
    let state = reduce(
        SupervisorState::new(compatibility()),
        SupervisorInput::InitialTargets(vec![target("page-a")]),
    )
    .unwrap()
    .state;
    let state = reduce(
        state,
        SupervisorInput::Attached {
            target_key: "page-a".into(),
            session: TransportSessionId::new("transport-a").unwrap(),
        },
    )
    .unwrap()
    .state;
    let state = reduce(
        state,
        SupervisorInput::VisibilityChanged {
            target_key: "page-a".into(),
            visibility: TargetVisibility::Visible,
        },
    )
    .unwrap()
    .state;
    reduce(state, SupervisorInput::InitialReconciliationCompleted)
        .unwrap()
        .state
}

#[test]
fn capture_effects_are_reducer_owned_and_exactly_scoped() {
    let state = attached_visible_state();
    assert!(matches!(state.session_state, BrowserSessionState::Ready));
    let target_state = &state.targets_by_key["page-a"];
    assert_eq!(target_state.target.lifecycle, TargetLifecycle::Attached);
    assert!(matches!(
        target_state.capture_binding,
        krometrail_cdp::CaptureBinding::Active(_)
    ));

    let disconnect = reduce(
        state,
        SupervisorInput::ConnectionLost(krometrail_cdp::TransportClose {
            reason: krometrail_core::NonEmptyText::new("transport lost").unwrap(),
        }),
    )
    .unwrap();
    assert!(
        disconnect
            .effects
            .iter()
            .any(|effect| matches!(effect, SupervisorEffect::SuspendCapture { .. }))
    );
    let target_id = disconnect.state.targets_by_key["page-a"].target.target.id();

    let restored = reduce(
        disconnect.state,
        SupervisorInput::Reconnected(ReconnectedSnapshot {
            connection_generation: 1,
            compatibility: compatibility(),
            targets: vec![ReconnectedTarget {
                info: target("page-a"),
                session: Some(TransportSessionId::new("transport-b").unwrap()),
                visibility: TargetVisibility::Visible,
            }],
        }),
    )
    .unwrap();
    assert!(restored.effects.iter().any(|effect| matches!(
        effect,
        SupervisorEffect::ResumeCapture { context }
            if context.target_id == target_id
                && context.connection_generation == 1
                && context.attachment_generation == 2
                && context.transport_session.as_str() == "transport-b"
    )));
}

#[test]
fn visibility_and_target_failure_are_local_reducer_inputs() {
    let state = attached_visible_state();
    let target_id = state.targets_by_key["page-a"].target.target.id();
    let hidden = reduce(
        state,
        SupervisorInput::CaptureVisibilityChanged {
            target_id,
            visibility: TargetVisibility::Hidden,
        },
    )
    .unwrap();
    assert_eq!(
        hidden.state.targets_by_key["page-a"].target.visibility,
        TargetVisibility::Hidden
    );
    assert!(matches!(
        hidden.state.targets_by_key["page-a"].capture_binding,
        krometrail_cdp::CaptureBinding::Active(_)
    ));

    let failed = reduce(
        attached_visible_state(),
        SupervisorInput::CaptureStartFailed {
            target_key: "page-a".into(),
        },
    )
    .unwrap();
    assert!(failed.effects.iter().any(|effect| matches!(
        effect,
        SupervisorEffect::Publish(BrowserSessionEvent::TargetFailed { .. })
    )));
}

#[test]
fn capture_events_expose_no_transport_or_page_privacy_fields() {
    let target_id = TargetId::from_uuid(TARGET_UUID.parse().unwrap());
    let status = TargetCaptureStatus::new(
        target_id,
        1,
        CaptureStreamState::Capturing,
        CaptureStatistics::default(),
        4,
        0,
        None,
        CaptureTimingSummary::empty(),
        CaptureTimingSummary::empty(),
    )
    .unwrap();
    let gap = CaptureGap::new(
        GapId::from_uuid(SESSION_UUID.parse().unwrap()),
        SessionId::from_uuid(SESSION_UUID.parse().unwrap()),
        target_id,
        SessionRange::new(SessionTime::ZERO, SessionTime::ZERO).unwrap(),
        CaptureGapReason::BrowserDisconnected,
        NonZeroU64::new(1),
        Some("transport suspended".into()),
    )
    .unwrap();
    let encoded = serde_json::to_string(&[
        BrowserSessionEvent::CaptureStateChanged { status },
        BrowserSessionEvent::CaptureGapDeclared { gap },
    ])
    .unwrap();
    for private in [
        "browser-target-key",
        "https://private.test",
        "page title",
        "session-opaque",
        "raw params",
        "base64 payload",
    ] {
        assert!(
            !encoded.contains(private),
            "private field leaked: {private}"
        );
    }
}

#[test]
fn shutdown_effect_is_reducer_owned_after_acceptance_fence() {
    let reduction = reduce(attached_visible_state(), SupervisorInput::StopRequested).unwrap();
    assert!(
        reduction
            .effects
            .iter()
            .any(|effect| matches!(effect, SupervisorEffect::StopCapture { .. }))
    );
    assert!(
        reduction
            .effects
            .iter()
            .any(|effect| matches!(effect, SupervisorEffect::Shutdown { .. }))
    );
}

#[allow(dead_code)]
fn _range_id_contracts_remain_typed() {
    let _ = SessionRange::new(SessionTime::ZERO, SessionTime::ZERO).unwrap();
    let _ = TargetId::from_uuid(TARGET_UUID.parse().unwrap());
}
