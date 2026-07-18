use krometrail_cdp::{
    CommandScope, ReconnectedSnapshot, ReconnectedTarget, SupervisorEffect, SupervisorInput,
    SupervisorState, TransportClose, TransportSessionId, TransportTargetInfo, reduce,
};
use krometrail_core::{
    BrowserCompatibility, BrowserProduct, BrowserProductVersion, BrowserSessionEvent,
    BrowserSessionState, BrowserVersion, CapabilitySupport, ErrorCode, NonEmptyText,
    RendererCapability, TargetLifecycle, TargetVisibility, ViewportMetrics,
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

fn popup(key: &str, opener: &str) -> TransportTargetInfo {
    page(key, &format!("https://{key}.test")).with_opener_target_key(Some(opener.to_owned()))
}

#[test]
fn page_contexts_use_monotonic_sequences_and_resolve_only_known_openers() {
    let state = reduce(
        SupervisorState::new(compatibility()),
        SupervisorInput::InitialTargets(vec![page("opener", "https://opener.test")]),
    )
    .unwrap()
    .state;
    let opener_id = state.targets_by_key["opener"].target.target.id();
    let state = reduce(
        state,
        SupervisorInput::TargetCreated(popup("popup", "opener")),
    )
    .unwrap()
    .state;
    let state = reduce(
        state,
        SupervisorInput::TargetCreated(popup("orphan", "raw-missing-key")),
    )
    .unwrap()
    .state;

    let inventory = state.page_contexts().unwrap();
    assert_eq!(inventory.cursor.get(), 3);
    assert_eq!(inventory.pages.len(), 3);
    assert!(
        inventory
            .pages
            .windows(2)
            .all(|pages| pages[0].sequence < pages[1].sequence)
    );
    let popup = inventory
        .pages
        .iter()
        .find(|page| page.page.target.target.browser_target_key() == "popup")
        .unwrap();
    assert_eq!(popup.opener_target_id, Some(opener_id));
    let orphan = inventory
        .pages
        .iter()
        .find(|page| page.page.target.target.browser_target_key() == "orphan")
        .unwrap();
    assert_eq!(orphan.opener_target_id, None);
}

#[test]
fn popup_relationship_does_not_rebind_when_an_opener_key_is_reused() {
    let state = reduce(
        SupervisorState::new(compatibility()),
        SupervisorInput::InitialTargets(vec![page("opener", "https://old.test")]),
    )
    .unwrap()
    .state;
    let state = reduce(
        state,
        SupervisorInput::TargetCreated(popup("popup", "opener")),
    )
    .unwrap()
    .state;
    let state = reduce(
        state,
        SupervisorInput::TargetDestroyed {
            target_key: "opener".into(),
        },
    )
    .unwrap()
    .state;
    let state = reduce(
        state,
        SupervisorInput::TargetCreated(page("opener", "https://new.test")),
    )
    .unwrap()
    .state;
    let popup = state
        .page_contexts()
        .unwrap()
        .pages
        .into_iter()
        .find(|page| page.page.target.target.browser_target_key() == "popup")
        .unwrap();
    assert_eq!(popup.opener_target_id, None);
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

#[test]
fn exact_target_reconnect_restores_viewport_before_capture_resume() {
    let state = reduce(
        SupervisorState::new(compatibility()),
        SupervisorInput::InitialTargets(vec![page("one", "https://one.test")]),
    )
    .unwrap()
    .state;
    let state = reduce(
        state,
        SupervisorInput::Attached {
            target_key: "one".into(),
            session: TransportSessionId::new("old-session").unwrap(),
        },
    )
    .unwrap()
    .state;
    let state = reduce(
        state,
        SupervisorInput::VisibilityChanged {
            target_key: "one".into(),
            visibility: TargetVisibility::Visible,
        },
    )
    .unwrap()
    .state;
    let state = reduce(state, SupervisorInput::InitialReconciliationCompleted)
        .unwrap()
        .state;
    let metrics = ViewportMetrics::new(390, 844, 3.0, true, true).unwrap();
    let state = reduce(
        state,
        SupervisorInput::ViewportOverrideApplied {
            target_key: "one".into(),
            viewport: Some(metrics),
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
    let restored = reduce(
        state,
        SupervisorInput::Reconnected(ReconnectedSnapshot {
            connection_generation: 1,
            compatibility: compatibility(),
            targets: vec![ReconnectedTarget {
                info: page("one", "https://one.test"),
                session: Some(TransportSessionId::new("new-session").unwrap()),
                visibility: TargetVisibility::Visible,
            }],
        }),
    )
    .unwrap();
    let restore_index = restored
        .effects
        .iter()
        .position(|effect| {
            matches!(effect,
                SupervisorEffect::RestoreViewport { viewport, .. } if *viewport == metrics
            )
        })
        .unwrap();
    let resume_index = restored
        .effects
        .iter()
        .position(|effect| matches!(effect, SupervisorEffect::ResumeCapture { .. }))
        .unwrap();
    assert!(restore_index < resume_index);
    assert_eq!(
        restored.state.targets_by_key["one"].viewport_override,
        Some(metrics)
    );
}

#[test]
fn cleared_and_new_targets_do_not_restore_an_old_viewport() {
    let state = reduce(
        SupervisorState::new(compatibility()),
        SupervisorInput::InitialTargets(vec![page("one", "https://one.test")]),
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
        SupervisorInput::ViewportOverrideApplied {
            target_key: "one".into(),
            viewport: Some(ViewportMetrics::new(800, 600, 1.0, false, false).unwrap()),
        },
    )
    .unwrap()
    .state;
    let state = reduce(
        state,
        SupervisorInput::ViewportOverrideApplied {
            target_key: "one".into(),
            viewport: None,
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
    let restored = reduce(
        state,
        SupervisorInput::Reconnected(ReconnectedSnapshot {
            connection_generation: 1,
            compatibility: compatibility(),
            targets: vec![ReconnectedTarget {
                info: page("new", "https://new.test"),
                session: Some(TransportSessionId::new("new-session").unwrap()),
                visibility: TargetVisibility::Visible,
            }],
        }),
    )
    .unwrap();
    assert!(
        !restored
            .effects
            .iter()
            .any(|effect| matches!(effect, SupervisorEffect::RestoreViewport { .. }))
    );
    assert_eq!(restored.state.targets_by_key["new"].viewport_override, None);
}
