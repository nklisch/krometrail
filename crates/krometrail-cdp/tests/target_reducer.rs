use krometrail_cdp::{
    CommandScope, ReconnectedSnapshot, ReconnectedTarget, SupervisorEffect, SupervisorInput,
    SupervisorState, TransportClose, TransportSessionId, TransportTargetInfo, reduce,
};
use krometrail_core::{
    BrowserCompatibility, BrowserProduct, BrowserProductVersion, BrowserSessionEvent,
    BrowserSessionState, BrowserVersion, CapabilitySupport, ErrorCode, NonEmptyText,
    RendererCapability, TargetLifecycle, TargetVisibility,
};

fn compatibility() -> BrowserCompatibility {
    BrowserCompatibility::new(
        BrowserVersion::new(
            BrowserProduct::Chrome,
            BrowserProductVersion::new("149").unwrap(),
            "revision",
            "1.3",
            "Chrome/149",
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

fn page(key: &str, url: &str) -> TransportTargetInfo {
    TransportTargetInfo::new(key, "page", url, key, false, Some("context".into())).unwrap()
}

#[test]
fn subscription_before_enable_contract_is_represented_by_attach_effects() {
    let result = reduce(
        SupervisorState::new(compatibility()),
        SupervisorInput::InitialTargets(vec![
            page("one", "https://one.test"),
            page("two", "https://two.test"),
        ]),
    )
    .unwrap();
    assert_eq!(result.state.targets_by_key.len(), 2);
    assert_eq!(
        result
            .effects
            .iter()
            .filter(|effect| matches!(effect, SupervisorEffect::Attach { .. }))
            .count(),
        2
    );
}

#[test]
fn exact_key_reconnect_restores_identity_but_changed_key_creates_new_target() {
    let state = reduce(
        SupervisorState::new(compatibility()),
        SupervisorInput::InitialTargets(vec![page("one", "https://old.test")]),
    )
    .unwrap()
    .state;
    let state = reduce(
        state,
        SupervisorInput::Attached {
            target_key: "one".into(),
            session: TransportSessionId::new("session-one").unwrap(),
        },
    )
    .unwrap()
    .state;
    let state = reduce(
        state,
        SupervisorInput::ConnectionLost(TransportClose {
            reason: NonEmptyText::new("remote").unwrap(),
        }),
    )
    .unwrap()
    .state;
    let old_id = state.targets_by_key["one"].target.target.id();
    let result = reduce(
        state,
        SupervisorInput::Reconnected(ReconnectedSnapshot {
            connection_generation: 1,
            compatibility: compatibility(),
            targets: vec![ReconnectedTarget {
                info: page("new-key", "https://old.test"),
                session: Some(TransportSessionId::new("new-session").unwrap()),
                visibility: TargetVisibility::Visible,
            }],
        }),
    )
    .unwrap();
    assert_eq!(
        result.state.targets_by_key["one"].target.lifecycle,
        TargetLifecycle::Closed
    );
    assert_ne!(
        result.state.targets_by_key["new-key"].target.target.id(),
        old_id
    );
    assert_eq!(result.state.session_state, BrowserSessionState::Ready);
}

#[test]
fn target_failure_is_local_and_slow_observers_do_not_change_reducer_state() {
    let state = reduce(
        SupervisorState::new(compatibility()),
        SupervisorInput::InitialTargets(vec![
            page("one", "https://one.test"),
            page("two", "https://two.test"),
        ]),
    )
    .unwrap()
    .state;
    let state = reduce(
        state,
        SupervisorInput::Attached {
            target_key: "one".into(),
            session: TransportSessionId::new("session-one").unwrap(),
        },
    )
    .unwrap()
    .state;
    let result = reduce(
        state,
        SupervisorInput::Detached {
            session: TransportSessionId::new("session-one").unwrap(),
            reason: Some("renderer closed".into()),
        },
    )
    .unwrap();
    assert_eq!(
        result.state.targets_by_key["one"].target.lifecycle,
        TargetLifecycle::Failed
    );
    assert_eq!(
        result.state.targets_by_key["two"].target.lifecycle,
        TargetLifecycle::Discovered
    );
    assert!(result.effects.iter().any(|effect| matches!(
        effect,
        SupervisorEffect::Publish(BrowserSessionEvent::TargetFailed { error, .. })
        if error.code == ErrorCode::TargetFailed
    )));
}

#[test]
fn transport_scope_value_remains_opaque_and_session_specific() {
    let scope = CommandScope::session("session-a").unwrap();
    assert!(matches!(scope, CommandScope::Session(id) if id.as_str() == "session-a"));
}
