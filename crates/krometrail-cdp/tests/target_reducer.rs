use krometrail_cdp::{
    CommandScope, ReconnectedSnapshot, ReconnectedTarget, SupervisorEffect, SupervisorInput,
    SupervisorState, TransportClose, TransportSessionId, TransportTargetInfo, reduce,
};
use krometrail_core::{
    BrowserCompatibility, BrowserProduct, BrowserProductVersion, BrowserSessionEvent,
    BrowserSessionState, BrowserVersion, CapabilitySupport, ErrorCode, NonEmptyText,
    RendererCapability, TargetLifecycle, TargetVisibility, ViewportMetrics,
    browser::MAX_KNOWN_PAGE_TARGETS,
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
    assert_eq!(inventory.cursor.get(), 4);
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
fn empty_inventory_cursor_is_first_page_wait_safe_and_closed_pages_do_not_rewind_it() {
    let state = SupervisorState::new(compatibility());
    assert_eq!(state.page_contexts().unwrap().cursor.get(), 1);
    let state = reduce(
        state,
        SupervisorInput::TargetCreated(page("first", "https://first.test")),
    )
    .unwrap()
    .state;
    let first = state.page_contexts().unwrap();
    assert!(first.pages[0].sequence > krometrail_core::PageSequence::new(1).unwrap());
    let high_water = first.cursor;
    let state = reduce(
        state,
        SupervisorInput::TargetDestroyed {
            target_key: "first".into(),
        },
    )
    .unwrap()
    .state;
    assert_eq!(state.page_contexts().unwrap().cursor, high_water);
}

#[test]
fn terminal_page_churn_keeps_target_state_bounded_and_cursor_monotonic() {
    let mut state = SupervisorState::new(compatibility());
    let mut previous_cursor = state.page_contexts().unwrap().cursor;
    for index in 0..10_001 {
        let key = format!("churn-{index}");
        state = reduce(
            state,
            SupervisorInput::TargetCreated(page(&key, &format!("https://{key}.test"))),
        )
        .unwrap()
        .state;
        let cursor = state.page_contexts().unwrap().cursor;
        assert!(cursor > previous_cursor);
        previous_cursor = cursor;
        state = reduce(state, SupervisorInput::TargetDestroyed { target_key: key })
            .unwrap()
            .state;
        assert!(state.targets_by_key.len() <= MAX_KNOWN_PAGE_TARGETS);
        assert_eq!(state.page_contexts().unwrap().cursor, previous_cursor);
    }
}

#[test]
fn the_129th_live_page_fails_atomically_with_the_stable_limit_error() {
    let infos = (0..MAX_KNOWN_PAGE_TARGETS)
        .map(|index| {
            let key = format!("live-{index}");
            page(&key, &format!("https://{key}.test"))
        })
        .collect();
    let state = reduce(
        SupervisorState::new(compatibility()),
        SupervisorInput::InitialTargets(infos),
    )
    .unwrap()
    .state;
    let before = state.clone();
    let error = reduce(
        state,
        SupervisorInput::TargetCreated(page("overflow", "https://overflow.test")),
    )
    .unwrap_err();

    assert_eq!(error.code, ErrorCode::ResourceLimitExceeded);
    assert_eq!(before.targets_by_key.len(), MAX_KNOWN_PAGE_TARGETS);
    assert!(!before.targets_by_key.contains_key("overflow"));
    assert_eq!(
        before.page_contexts().unwrap().cursor.get(),
        u64::try_from(MAX_KNOWN_PAGE_TARGETS).unwrap() + 1
    );
}

#[test]
fn pruning_terminal_openers_does_not_rebind_popups_when_a_key_is_reused() {
    let mut state = reduce(
        SupervisorState::new(compatibility()),
        SupervisorInput::InitialTargets(vec![page("opener", "https://old.test")]),
    )
    .unwrap()
    .state;
    let old_opener_id = state.targets_by_key["opener"].target.target.id();
    state = reduce(
        state,
        SupervisorInput::TargetCreated(popup("popup", "opener")),
    )
    .unwrap()
    .state;
    state = reduce(
        state,
        SupervisorInput::TargetDestroyed {
            target_key: "opener".into(),
        },
    )
    .unwrap()
    .state;
    for index in 0..=MAX_KNOWN_PAGE_TARGETS {
        let key = format!("prune-{index}");
        state = reduce(
            state,
            SupervisorInput::TargetCreated(page(&key, &format!("https://{key}.test"))),
        )
        .unwrap()
        .state;
        state = reduce(state, SupervisorInput::TargetDestroyed { target_key: key })
            .unwrap()
            .state;
    }
    assert!(!state.targets_by_key.contains_key("opener"));
    let before_reuse = state.page_contexts().unwrap().cursor;
    state = reduce(
        state,
        SupervisorInput::TargetCreated(page("opener", "https://new.test")),
    )
    .unwrap()
    .state;

    assert!(state.page_contexts().unwrap().cursor > before_reuse);
    assert_ne!(
        state.targets_by_key["opener"].target.target.id(),
        old_opener_id
    );
    let popup = state
        .page_contexts()
        .unwrap()
        .pages
        .into_iter()
        .find(|page| page.page.target.target.browser_target_key() == "popup")
        .unwrap();
    assert_eq!(popup.opener_target_id, None);
    assert!(state.targets_by_key.len() <= MAX_KNOWN_PAGE_TARGETS);
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
fn reconnect_target_info_cannot_rebind_an_existing_popup_opener() {
    let state = reduce(
        SupervisorState::new(compatibility()),
        SupervisorInput::InitialTargets(vec![
            page("opener", "https://opener.test"),
            popup("popup", "opener"),
            page("other", "https://other.test"),
        ]),
    )
    .unwrap()
    .state;
    let original = state
        .page_contexts()
        .unwrap()
        .pages
        .into_iter()
        .find(|page| page.page.target.target.browser_target_key() == "popup")
        .unwrap()
        .opener_target_id;
    let state = reduce(
        state,
        SupervisorInput::ConnectionLost(TransportClose {
            reason: NonEmptyText::new("test reconnect").unwrap(),
        }),
    )
    .unwrap()
    .state;
    let reconnected = reduce(
        state,
        SupervisorInput::Reconnected(ReconnectedSnapshot {
            connection_generation: 1,
            compatibility: compatibility(),
            targets: vec![
                ReconnectedTarget {
                    info: page("opener", "https://opener.test"),
                    session: None,
                    visibility: TargetVisibility::Visible,
                },
                ReconnectedTarget {
                    info: popup("popup", "other"),
                    session: None,
                    visibility: TargetVisibility::Visible,
                },
                ReconnectedTarget {
                    info: page("other", "https://other.test"),
                    session: None,
                    visibility: TargetVisibility::Visible,
                },
            ],
        }),
    )
    .unwrap()
    .state;
    let popup = reconnected
        .page_contexts()
        .unwrap()
        .pages
        .into_iter()
        .find(|page| page.page.target.target.browser_target_key() == "popup")
        .unwrap();
    assert_eq!(popup.opener_target_id, original);
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
