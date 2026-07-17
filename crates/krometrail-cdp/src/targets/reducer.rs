//! Deterministic, single-writer target/session reducer.
//!
//! No async work, clocks, random sources, or transport handles appear here. Effects are a
//! complete description of the adapter work that may happen after a reduction commits.

use std::collections::HashSet;

use krometrail_core::{
    BrowserSessionEvent, BrowserSessionState, ErrorCode, KrometrailError, NonEmptyText, Result,
    TargetLifecycle, TargetVisibility,
};

use super::model::{
    CaptureBinding, CaptureEffectContext, ReconnectedSnapshot, ReconnectedTarget, Reduction,
    ShutdownCause, SupervisorEffect, SupervisorInput, SupervisorState, SupervisorTargetState,
    TransportTargetInfo, ViewportEffectContext, cancelled_error, close_event, close_reason,
    make_target, process_error, reconnect_error, target_changed_event, target_discovered_event,
    target_error,
};

/// Apply one serialized input. Callers must execute this function from one task/owner; sharing the
/// state between multiple writers would invalidate revision ordering and target identity.
pub fn reduce(mut state: SupervisorState, input: SupervisorInput) -> Result<Reduction> {
    if let SupervisorInput::ForConnectionGeneration { generation, input } = input {
        // A transport task can finish an event read just as its connection is being replaced.
        // Generation is the only reliable discriminator: target URLs and titles are not identity.
        if generation != state.connection_generation {
            return Ok(Reduction {
                state,
                effects: Vec::new(),
            });
        }
        return reduce(state, *input);
    }

    if state.session_state == BrowserSessionState::Reconnecting
        && matches!(
            input,
            SupervisorInput::InitialTargets(_)
                | SupervisorInput::TargetCreated(_)
                | SupervisorInput::TargetInfoChanged(_)
                | SupervisorInput::Attached { .. }
                | SupervisorInput::TargetAttachFailed { .. }
                | SupervisorInput::Detached { .. }
                | SupervisorInput::TargetDestroyed { .. }
                | SupervisorInput::VisibilityChanged { .. }
                | SupervisorInput::CaptureVisibilityChanged { .. }
                | SupervisorInput::SelectTarget { .. }
                | SupervisorInput::ViewportOverrideApplied { .. }
        )
    {
        // Events from the disconnected transport can still be queued in another task. They are
        // not part of the new connection's snapshot and must not mutate suspended state.
        return Ok(Reduction {
            state,
            effects: Vec::new(),
        });
    }

    let mut effects = Vec::new();
    match input {
        SupervisorInput::InitialTargets(infos) => {
            reconcile_initial(&mut state, infos, &mut effects)?
        }
        SupervisorInput::InitialReconciliationCompleted => {
            if state.targets_by_key.values().any(|target| {
                !matches!(
                    target.target.lifecycle,
                    TargetLifecycle::Closed | TargetLifecycle::Failed
                ) && target.target.visibility == TargetVisibility::Unknown
            }) {
                return Err(invalid_state(
                    "initial reconciliation cannot become ready with unresolved target visibility",
                ));
            }
            reconcile_selection(&mut state, &mut effects);
            set_session_state(&mut state, BrowserSessionState::Ready, &mut effects)?;
        }
        SupervisorInput::TargetCreated(info) => {
            reconcile_one(&mut state, info, true, &mut effects)?
        }
        SupervisorInput::TargetInfoChanged(info) => {
            reconcile_one(&mut state, info, false, &mut effects)?
        }
        SupervisorInput::Attached {
            target_key,
            session,
        } => attach(&mut state, target_key, session, &mut effects, true)?,
        SupervisorInput::TargetAttachFailed { target_key } => {
            let failed_target_id = if let Some(target) = state.targets_by_key.get_mut(&target_key) {
                if !matches!(
                    target.target.lifecycle,
                    TargetLifecycle::Closed | TargetLifecycle::Failed
                ) {
                    target.target.lifecycle = target
                        .target
                        .lifecycle
                        .transition(TargetLifecycle::Failed)?;
                    Some(target.target.target.id())
                } else {
                    None
                }
            } else {
                None
            };
            if let Some(target_id) = failed_target_id {
                publish(
                    &mut state,
                    BrowserSessionEvent::TargetFailed {
                        target_id,
                        error: target_error(),
                    },
                    &mut effects,
                );
                reconcile_selection(&mut state, &mut effects);
            }
        }
        SupervisorInput::CaptureStartFailed { target_key } => {
            capture_start_failed(&mut state, &target_key, &mut effects)?;
        }
        SupervisorInput::Detached { session, reason: _ } => {
            detach_failed(&mut state, session, &mut effects)?
        }
        SupervisorInput::TargetDestroyed { target_key } => {
            destroy(&mut state, &target_key, &mut effects)?
        }
        SupervisorInput::VisibilityChanged {
            target_key,
            visibility,
        } => visibility_changed(&mut state, &target_key, visibility, &mut effects)?,
        SupervisorInput::InitialVisibilityProbeFailed { target_key } => {
            initial_visibility_probe_failed(&mut state, &target_key, &mut effects)?;
        }
        SupervisorInput::CaptureVisibilityChanged {
            target_id,
            visibility,
        } => {
            if let Some(target_key) = state
                .targets_by_key
                .iter()
                .find(|(_, target)| target.target.target.id() == target_id)
                .map(|(key, _)| key.clone())
            {
                visibility_changed(&mut state, &target_key, visibility, &mut effects)?;
            }
        }
        SupervisorInput::SelectTarget { target_key } => {
            select_target(&mut state, &target_key, &mut effects)?;
        }
        SupervisorInput::ViewportOverrideApplied {
            target_key,
            viewport,
        } => {
            let target = state
                .targets_by_key
                .get_mut(&target_key)
                .ok_or_else(target_error)?;
            if target.transport_session.is_none()
                || matches!(
                    target.target.lifecycle,
                    TargetLifecycle::Closed | TargetLifecycle::Failed | TargetLifecycle::Suspended
                )
            {
                return Err(target_error());
            }
            target.viewport_override = viewport;
        }
        SupervisorInput::ConnectionLost(close) => {
            if matches!(
                state.session_state,
                BrowserSessionState::Ended | BrowserSessionState::Stopping
            ) {
                return Ok(Reduction { state, effects });
            }
            set_session_state(&mut state, BrowserSessionState::Reconnecting, &mut effects)?;
            for target in state.targets_by_key.values_mut() {
                if matches!(
                    target.target.lifecycle,
                    TargetLifecycle::Closed | TargetLifecycle::Failed
                ) {
                    continue;
                }
                if target.target.lifecycle != TargetLifecycle::Suspended {
                    target.prior_to_suspension = Some(target.target.lifecycle);
                    target.target.lifecycle = target
                        .target
                        .lifecycle
                        .transition(TargetLifecycle::Suspended)?;
                    effects.push(SupervisorEffect::Publish(
                        BrowserSessionEvent::TargetChanged {
                            target: target.target.clone(),
                        },
                    ));
                }
                target.transport_session = None;
            }
            state.target_key_by_session.clear();
            effects.push(SupervisorEffect::BeginReconnect);
            tracing::debug!(
                reason = close_reason(&close),
                connection_generation = state.connection_generation,
                "browser.session.connection_lost"
            );
        }
        SupervisorInput::BrowserProcessTerminated { exit: _ } => {
            if state.session_state == BrowserSessionState::Ended {
                return Ok(Reduction { state, effects });
            }
            let error = process_error();
            publish(
                &mut state,
                BrowserSessionEvent::SessionFailed { error },
                &mut effects,
            );
            begin_shutdown(
                &mut state,
                ShutdownCause::BrowserProcessTerminated,
                &mut effects,
            )?;
        }
        SupervisorInput::Reconnected(snapshot) => reconnect(&mut state, snapshot, &mut effects)?,
        SupervisorInput::ReconnectExhausted => {
            let error = reconnect_error();
            publish(
                &mut state,
                BrowserSessionEvent::SessionFailed { error },
                &mut effects,
            );
            begin_shutdown(&mut state, ShutdownCause::ReconnectExhausted, &mut effects)?;
        }
        SupervisorInput::StopRequested => {
            begin_shutdown(&mut state, ShutdownCause::StopRequested, &mut effects)?;
        }
        SupervisorInput::Cancelled => {
            let error = cancelled_error();
            publish(
                &mut state,
                BrowserSessionEvent::SessionFailed { error },
                &mut effects,
            );
            begin_shutdown(&mut state, ShutdownCause::Cancelled, &mut effects)?;
        }
        SupervisorInput::ForConnectionGeneration { .. } => {
            unreachable!("generation-guarded input is handled before reduction")
        }
    }
    reconcile_capture_bindings(&mut state, &mut effects)?;
    Ok(Reduction { state, effects })
}

fn reconcile_initial(
    state: &mut SupervisorState,
    infos: Vec<TransportTargetInfo>,
    effects: &mut Vec<SupervisorEffect>,
) -> Result<()> {
    let recordable = infos
        .into_iter()
        .filter(TransportTargetInfo::is_recordable)
        .collect::<Vec<_>>();
    let keys = recordable
        .iter()
        .map(|info| info.target_key.as_str())
        .collect::<HashSet<_>>();
    let old_keys = state.targets_by_key.keys().cloned().collect::<Vec<_>>();
    for key in old_keys {
        if !keys.contains(key.as_str()) {
            destroy(state, &key, effects)?;
        }
    }
    for info in recordable {
        reconcile_one(state, info, true, effects)?;
    }
    Ok(())
}

fn reconcile_one(
    state: &mut SupervisorState,
    info: TransportTargetInfo,
    creation_event: bool,
    effects: &mut Vec<SupervisorEffect>,
) -> Result<()> {
    if !info.is_recordable() {
        if state.targets_by_key.contains_key(&info.target_key) {
            destroy(state, &info.target_key, effects)?;
        }
        return Ok(());
    }
    let key = info.target_key.clone();
    if let Some(existing) = state.targets_by_key.get_mut(&key) {
        if matches!(
            existing.target.lifecycle,
            TargetLifecycle::Closed | TargetLifecycle::Failed
        ) {
            // Terminal targets never transition back. A late info/detach race must not resurrect
            // them; only a fresh creation/snapshot observation can establish a new logical target.
            if !creation_event {
                return Ok(());
            }
            state.targets_by_key.remove(&key);
        } else {
            let changed = existing.target.target.url() != info.url
                || existing.target.target.title() != info.title;
            if changed {
                existing.target.target = krometrail_core::PageTarget::new(
                    existing.target.target.id(),
                    key.clone(),
                    info.url,
                    info.title,
                )?;
                effects.push(target_changed_event(existing));
            }
            return Ok(());
        }
    }
    let id = allocate_target_id(state, &key);
    let target = make_target(
        id,
        &info,
        TargetLifecycle::Discovered,
        TargetVisibility::Unknown,
        0,
    )?;
    let target_state = SupervisorTargetState {
        target,
        transport_session: None,
        prior_to_suspension: None,
        capture_binding: CaptureBinding::Inactive,
        viewport_override: None,
    };
    if creation_event {
        effects.push(target_discovered_event(&target_state));
    }
    state.targets_by_key.insert(key.clone(), target_state);
    effects.push(SupervisorEffect::Attach { target_key: key });
    Ok(())
}

fn attach(
    state: &mut SupervisorState,
    key: String,
    session: crate::transport::TransportSessionId,
    effects: &mut Vec<SupervisorEffect>,
    probe_visibility: bool,
) -> Result<()> {
    let Some(target) = state.targets_by_key.get_mut(&key) else {
        return Ok(());
    };
    if matches!(
        target.target.lifecycle,
        TargetLifecycle::Closed | TargetLifecycle::Failed
    ) {
        return Ok(());
    }
    if target.transport_session.as_ref() == Some(&session) {
        return Ok(());
    }
    if let Some(previous) = target.transport_session.replace(session.clone()) {
        state.target_key_by_session.remove(&previous);
        effects.push(SupervisorEffect::Detach { session: previous });
    }
    state
        .target_key_by_session
        .insert(session.clone(), key.clone());
    target.target.attachment_generation = target
        .target
        .attachment_generation
        .checked_add(1)
        .ok_or_else(|| invalid_state("target attachment generation overflow"))?;
    let next = target
        .prior_to_suspension
        .take()
        .unwrap_or(TargetLifecycle::Attached);
    target.target.lifecycle = target.target.lifecycle.transition(next)?;
    if target.target.lifecycle == TargetLifecycle::Discovered {
        target.target.lifecycle = target
            .target
            .lifecycle
            .transition(TargetLifecycle::Attached)?;
    }
    effects.push(target_changed_event(target));
    if let Some(viewport) = target.viewport_override {
        effects.push(SupervisorEffect::RestoreViewport {
            context: ViewportEffectContext {
                target_id: target.target.target.id(),
                target_key: key.clone(),
                connection_generation: state.connection_generation,
                attachment_generation: target.target.attachment_generation,
                transport_session: session.clone(),
            },
            viewport,
        });
    }
    if probe_visibility {
        effects.push(SupervisorEffect::RestoreSessionDomains {
            target_key: key,
            session,
        });
    }
    Ok(())
}

fn detach_failed(
    state: &mut SupervisorState,
    session: crate::transport::TransportSessionId,
    effects: &mut Vec<SupervisorEffect>,
) -> Result<()> {
    let Some(key) = state.target_key_by_session.remove(&session) else {
        return Ok(());
    };
    let target_id = {
        let Some(target) = state.targets_by_key.get_mut(&key) else {
            return Ok(());
        };
        target.transport_session = None;
        if matches!(
            target.target.lifecycle,
            TargetLifecycle::Closed | TargetLifecycle::Failed
        ) {
            return Ok(());
        }
        target.prior_to_suspension = None;
        target.target.lifecycle = target
            .target
            .lifecycle
            .transition(TargetLifecycle::Failed)?;
        target.target.target.id()
    };
    publish(
        state,
        BrowserSessionEvent::TargetFailed {
            target_id,
            error: target_error(),
        },
        effects,
    );
    reconcile_selection(state, effects);
    Ok(())
}

fn initial_visibility_probe_failed(
    state: &mut SupervisorState,
    key: &str,
    effects: &mut Vec<SupervisorEffect>,
) -> Result<()> {
    let (session, target_id) = {
        let Some(target) = state.targets_by_key.get_mut(key) else {
            return Ok(());
        };
        if matches!(
            target.target.lifecycle,
            TargetLifecycle::Closed | TargetLifecycle::Failed
        ) {
            return Ok(());
        }
        let session = target.transport_session.take();
        if let Some(session) = session.as_ref() {
            state.target_key_by_session.remove(session);
        }
        target.prior_to_suspension = None;
        target.target.lifecycle = target
            .target
            .lifecycle
            .transition(TargetLifecycle::Failed)?;
        target.capture_binding = CaptureBinding::Terminal;
        (session, target.target.target.id())
    };
    if let Some(session) = session {
        // Keep the exact flat session in the effect until apply_effects performs the detach.
        effects.push(SupervisorEffect::Detach { session });
    }
    publish(
        state,
        BrowserSessionEvent::TargetFailed {
            target_id,
            error: target_error(),
        },
        effects,
    );
    reconcile_selection(state, effects);
    Ok(())
}

fn destroy(
    state: &mut SupervisorState,
    key: &str,
    effects: &mut Vec<SupervisorEffect>,
) -> Result<()> {
    let Some(target) = state.targets_by_key.get_mut(key) else {
        return Ok(());
    };
    if target.target.lifecycle == TargetLifecycle::Closed {
        return Ok(());
    }
    if let Some(session) = target.transport_session.take() {
        state.target_key_by_session.remove(&session);
        effects.push(SupervisorEffect::Detach { session });
    }
    if target.target.lifecycle != TargetLifecycle::Failed {
        target.target.lifecycle = target
            .target
            .lifecycle
            .transition(TargetLifecycle::Closed)?;
    }
    effects.push(close_event(target));
    reconcile_selection(state, effects);
    Ok(())
}

fn selection_candidate(state: &SupervisorState, key: &str) -> bool {
    state.targets_by_key.get(key).is_some_and(|target| {
        target.transport_session.is_some()
            && !matches!(
                target.target.lifecycle,
                TargetLifecycle::Closed | TargetLifecycle::Failed | TargetLifecycle::Suspended
            )
    })
}

fn selected_id(state: &SupervisorState, key: Option<&str>) -> Option<krometrail_core::TargetId> {
    key.and_then(|key| state.targets_by_key.get(key))
        .map(|target| target.target.target.id())
}

fn set_selection(
    state: &mut SupervisorState,
    selected: Option<String>,
    effects: &mut Vec<SupervisorEffect>,
) {
    if state.selected_target_key == selected {
        return;
    }
    let previous = selected_id(state, state.selected_target_key.as_deref());
    let next = selected_id(state, selected.as_deref());
    state.selected_target_key = selected;
    publish(
        state,
        BrowserSessionEvent::SelectedTargetChanged {
            previous,
            selected: next,
        },
        effects,
    );
}

fn reconcile_selection(state: &mut SupervisorState, effects: &mut Vec<SupervisorEffect>) {
    if state
        .selected_target_key
        .as_deref()
        .is_some_and(|key| selection_candidate(state, key))
    {
        return;
    }
    let selected = state
        .targets_by_key
        .keys()
        .filter(|key| selection_candidate(state, key))
        .min()
        .cloned();
    set_selection(state, selected, effects);
}

fn select_target(
    state: &mut SupervisorState,
    target_key: &str,
    effects: &mut Vec<SupervisorEffect>,
) -> Result<()> {
    if !selection_candidate(state, target_key) {
        return Err(target_error());
    }
    set_selection(state, Some(target_key.to_owned()), effects);
    Ok(())
}

fn visibility_changed(
    state: &mut SupervisorState,
    key: &str,
    visibility: TargetVisibility,
    effects: &mut Vec<SupervisorEffect>,
) -> Result<()> {
    let Some(target) = state.targets_by_key.get_mut(key) else {
        return Ok(());
    };
    if matches!(
        target.target.lifecycle,
        TargetLifecycle::Closed | TargetLifecycle::Failed
    ) {
        return Ok(());
    }
    if target.target.visibility == visibility {
        return Ok(());
    }
    let next_lifecycle = match (target.target.lifecycle, visibility) {
        (TargetLifecycle::Attached, TargetVisibility::Hidden)
        | (TargetLifecycle::Recording, TargetVisibility::Hidden) => TargetLifecycle::Hidden,
        (TargetLifecycle::Hidden, TargetVisibility::Visible) => TargetLifecycle::Recording,
        (lifecycle, _) => lifecycle,
    };
    if next_lifecycle != target.target.lifecycle {
        target.target.lifecycle = target.target.lifecycle.transition(next_lifecycle)?;
    }
    target.target.visibility = visibility;
    effects.push(target_changed_event(target));
    Ok(())
}

fn reconnect(
    state: &mut SupervisorState,
    snapshot: ReconnectedSnapshot,
    effects: &mut Vec<SupervisorEffect>,
) -> Result<()> {
    if matches!(
        state.session_state,
        BrowserSessionState::Stopping | BrowserSessionState::Ended
    ) {
        return Ok(());
    }
    if snapshot.connection_generation <= state.connection_generation {
        return Ok(());
    }
    state.connection_generation = snapshot.connection_generation;
    state.compatibility = snapshot.compatibility;
    let recordable = snapshot
        .targets
        .into_iter()
        .filter(|target| target.info.is_recordable())
        .collect::<Vec<_>>();
    let seen = recordable
        .iter()
        .map(|target| target.info.target_key.as_str())
        .collect::<HashSet<_>>();
    let old_keys = state.targets_by_key.keys().cloned().collect::<Vec<_>>();
    for key in old_keys {
        if !seen.contains(key.as_str()) {
            destroy(state, &key, effects)?;
        }
    }
    state.target_key_by_session.clear();
    for restored in recordable {
        reconcile_restored(state, restored, effects)?;
    }
    reconcile_selection(state, effects);
    set_session_state(state, BrowserSessionState::Ready, effects)?;
    Ok(())
}

fn reconcile_restored(
    state: &mut SupervisorState,
    reconnected: ReconnectedTarget,
    effects: &mut Vec<SupervisorEffect>,
) -> Result<()> {
    let key = reconnected.info.target_key.clone();
    if !state.targets_by_key.contains_key(&key) {
        let id = allocate_target_id(state, &key);
        let target = make_target(
            id,
            &reconnected.info,
            TargetLifecycle::Discovered,
            reconnected.visibility,
            0,
        )?;
        state.targets_by_key.insert(
            key.clone(),
            SupervisorTargetState {
                target,
                transport_session: None,
                prior_to_suspension: None,
                capture_binding: CaptureBinding::Inactive,
                viewport_override: None,
            },
        );
        if let Some(session) = reconnected.session {
            attach(state, key, session, effects, false)?;
        } else {
            effects.push(SupervisorEffect::Attach { target_key: key });
        }
        return Ok(());
    }
    let (changed, needs_attachment) = {
        let target = state.targets_by_key.get_mut(&key).expect("key checked");
        let changed = target.target.target.url() != reconnected.info.url
            || target.target.target.title() != reconnected.info.title;
        if changed {
            target.target.target = krometrail_core::PageTarget::new(
                target.target.target.id(),
                key.clone(),
                reconnected.info.url.clone(),
                reconnected.info.title.clone(),
            )?;
        }
        target.target.visibility = reconnected.visibility;
        let needs_attachment = target.transport_session.is_none();
        (changed, needs_attachment)
    };
    if let Some(session) = reconnected.session {
        attach(state, key, session, effects, false)?;
    } else if needs_attachment {
        // A snapshot may omit the flat session when auto-attach raced discovery. Re-requesting the
        // exact target key is idempotent and is safer than treating a missing session as closure.
        if changed {
            let target = state.targets_by_key.get(&key).expect("key remains");
            effects.push(target_changed_event(target));
        }
        effects.push(SupervisorEffect::Attach { target_key: key });
    } else if changed {
        let target = state.targets_by_key.get(&key).expect("key remains");
        effects.push(target_changed_event(target));
    }
    Ok(())
}

fn reconcile_capture_bindings(
    state: &mut SupervisorState,
    effects: &mut Vec<SupervisorEffect>,
) -> Result<()> {
    let keys = state.targets_by_key.keys().cloned().collect::<Vec<_>>();
    for key in keys {
        let Some(target) = state.targets_by_key.get(&key) else {
            continue;
        };
        let target_id = target.target.target.id();
        let terminal = matches!(
            target.target.lifecycle,
            TargetLifecycle::Closed | TargetLifecycle::Failed
        );
        let eligible = state.session_state == BrowserSessionState::Ready
            && target.target.visibility == TargetVisibility::Visible
            && matches!(
                target.target.lifecycle,
                TargetLifecycle::Attached | TargetLifecycle::Recording
            )
            && target.transport_session.is_some();
        let current_context =
            target
                .transport_session
                .clone()
                .map(|transport_session| CaptureEffectContext {
                    target_id,
                    connection_generation: state.connection_generation,
                    attachment_generation: target.target.attachment_generation,
                    transport_session,
                });
        let binding = target.capture_binding.clone();
        let next = match binding {
            CaptureBinding::Inactive => {
                if terminal {
                    CaptureBinding::Terminal
                } else if eligible {
                    let context = current_context.expect("eligible target has a session");
                    effects.push(SupervisorEffect::StartCapture {
                        context: context.clone(),
                    });
                    CaptureBinding::Active(context)
                } else {
                    CaptureBinding::Inactive
                }
            }
            CaptureBinding::Active(previous) => {
                if terminal || state.session_state == BrowserSessionState::Stopping {
                    queue_capture_teardown(
                        effects,
                        SupervisorEffect::StopCapture { context: previous },
                    );
                    CaptureBinding::Terminal
                } else if state.session_state == BrowserSessionState::Reconnecting
                    || target.transport_session.is_none()
                {
                    queue_capture_teardown(
                        effects,
                        SupervisorEffect::SuspendCapture {
                            context: previous.clone(),
                        },
                    );
                    CaptureBinding::Suspended(previous)
                } else if current_context.as_ref() != Some(&previous) {
                    queue_capture_teardown(
                        effects,
                        SupervisorEffect::StopCapture { context: previous },
                    );
                    if eligible {
                        let context = current_context.expect("eligible target has a session");
                        effects.push(SupervisorEffect::StartCapture {
                            context: context.clone(),
                        });
                        CaptureBinding::Active(context)
                    } else {
                        CaptureBinding::Inactive
                    }
                } else {
                    CaptureBinding::Active(previous)
                }
            }
            CaptureBinding::Suspended(previous) => {
                if terminal || state.session_state == BrowserSessionState::Stopping {
                    queue_capture_teardown(
                        effects,
                        SupervisorEffect::StopCapture { context: previous },
                    );
                    CaptureBinding::Terminal
                } else if eligible
                    && current_context.as_ref().is_some_and(|current| {
                        current.attachment_generation > previous.attachment_generation
                    })
                {
                    let context = current_context.expect("eligible target has a session");
                    effects.push(SupervisorEffect::ResumeCapture {
                        context: context.clone(),
                    });
                    CaptureBinding::Active(context)
                } else {
                    CaptureBinding::Suspended(previous)
                }
            }
            CaptureBinding::Terminal => CaptureBinding::Terminal,
        };
        if let Some(target) = state.targets_by_key.get_mut(&key) {
            target.capture_binding = next;
        }
    }
    Ok(())
}

fn queue_capture_teardown(effects: &mut Vec<SupervisorEffect>, effect: SupervisorEffect) {
    let Some(session) = (match &effect {
        SupervisorEffect::StopCapture { context }
        | SupervisorEffect::SuspendCapture { context } => Some(&context.transport_session),
        _ => None,
    }) else {
        effects.push(effect);
        return;
    };
    if let Some(index) = effects.iter().position(|existing| match existing {
        SupervisorEffect::Detach { session: detached } => detached == session,
        SupervisorEffect::Shutdown { .. } | SupervisorEffect::BeginReconnect => true,
        _ => false,
    }) {
        effects.insert(index, effect);
    } else {
        effects.push(effect);
    }
}

fn capture_start_failed(
    state: &mut SupervisorState,
    key: &str,
    effects: &mut Vec<SupervisorEffect>,
) -> Result<()> {
    let Some(target) = state.targets_by_key.get_mut(key) else {
        return Ok(());
    };
    if matches!(
        target.target.lifecycle,
        TargetLifecycle::Closed | TargetLifecycle::Failed
    ) {
        return Ok(());
    }
    if let Some(session) = target.transport_session.take() {
        state.target_key_by_session.remove(&session);
        // A failed start can happen after flat-session attachment (for example when the
        // coordinator's active-stream cap rejects this target). The reducer owns the mapping,
        // but the adapter still has to release the exact session before it is forgotten.
        effects.push(SupervisorEffect::Detach { session });
    }
    target.target.lifecycle = target
        .target
        .lifecycle
        .transition(TargetLifecycle::Failed)?;
    target.capture_binding = CaptureBinding::Terminal;
    let target_id = target.target.target.id();
    publish(
        state,
        BrowserSessionEvent::TargetFailed {
            target_id,
            error: target_error(),
        },
        effects,
    );
    reconcile_selection(state, effects);
    Ok(())
}

fn begin_shutdown(
    state: &mut SupervisorState,
    cause: ShutdownCause,
    effects: &mut Vec<SupervisorEffect>,
) -> Result<()> {
    if state.session_state == BrowserSessionState::Ended {
        return Ok(());
    }
    if state.session_state != BrowserSessionState::Stopping {
        set_session_state(state, BrowserSessionState::Stopping, effects)?;
        effects.push(SupervisorEffect::Shutdown { cause });
    }
    Ok(())
}

fn set_session_state(
    state: &mut SupervisorState,
    next: BrowserSessionState,
    effects: &mut Vec<SupervisorEffect>,
) -> Result<()> {
    if state.session_state == next {
        return Ok(());
    }
    let allowed = matches!(
        (state.session_state, next),
        (BrowserSessionState::Connecting, BrowserSessionState::Ready)
            | (
                BrowserSessionState::Connecting,
                BrowserSessionState::Reconnecting
            )
            | (
                BrowserSessionState::Connecting,
                BrowserSessionState::Stopping
            )
            | (
                BrowserSessionState::Ready,
                BrowserSessionState::Reconnecting
            )
            | (BrowserSessionState::Ready, BrowserSessionState::Stopping)
            | (
                BrowserSessionState::Reconnecting,
                BrowserSessionState::Ready
            )
            | (
                BrowserSessionState::Reconnecting,
                BrowserSessionState::Stopping
            )
            | (BrowserSessionState::Stopping, BrowserSessionState::Ended)
            | (BrowserSessionState::Connecting, BrowserSessionState::Ended)
    );
    if !allowed {
        return Err(invalid_state("invalid browser session state transition"));
    }
    let previous = state.session_state;
    state.session_state = next;
    tracing::info!(
        previous_state = previous.as_str(),
        next_state = next.as_str(),
        connection_generation = state.connection_generation,
        "browser.session.state_changed"
    );
    publish(
        state,
        BrowserSessionEvent::SessionStateChanged { state: next },
        effects,
    );
    Ok(())
}

fn publish(
    state: &mut SupervisorState,
    event: BrowserSessionEvent,
    effects: &mut Vec<SupervisorEffect>,
) {
    state.revision = state.revision.saturating_add(1);
    match &event {
        BrowserSessionEvent::SessionFailed { error } => {
            tracing::warn!(code = error.code.as_str(), "browser.session.failed")
        }
        BrowserSessionEvent::TargetFailed { target_id, error } => tracing::warn!(
            target_id = %target_id,
            code = error.code.as_str(),
            "browser.target.failed"
        ),
        BrowserSessionEvent::SessionStateChanged { .. }
        | BrowserSessionEvent::TargetDiscovered { .. }
        | BrowserSessionEvent::TargetChanged { .. }
        | BrowserSessionEvent::TargetClosed { .. }
        | BrowserSessionEvent::SelectedTargetChanged { .. }
        | BrowserSessionEvent::CaptureStateChanged { .. }
        | BrowserSessionEvent::CaptureGapDeclared { .. } => {}
    }
    effects.push(SupervisorEffect::Publish(event));
}

fn allocate_target_id(state: &mut SupervisorState, key: &str) -> krometrail_core::TargetId {
    state.revision = state.revision.saturating_add(1);
    let mut bytes = [0_u8; 16];
    let mut first = 0xcbf29ce484222325_u64 ^ state.revision;
    let mut second = 0x9e3779b185ebca87_u64 ^ state.connection_generation;
    for (index, byte) in key.bytes().enumerate() {
        first ^= u64::from(byte);
        first = first.wrapping_mul(0x100000001b3);
        second ^= first.rotate_left((index % 63) as u32);
        second = second.wrapping_mul(0x100000001b3);
    }
    bytes[..8].copy_from_slice(&first.to_be_bytes());
    bytes[8..].copy_from_slice(&second.to_be_bytes());
    // Keep generated IDs in the same UUID-shaped space as all other core identities.
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    krometrail_core::TargetId::from_uuid(uuid::Uuid::from_bytes(bytes))
}

fn invalid_state(message: &'static str) -> KrometrailError {
    KrometrailError::new(
        ErrorCode::InvalidLifecycleTransition,
        NonEmptyText::new(message).unwrap(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use krometrail_core::{
        BrowserCompatibility, BrowserProduct, BrowserProductVersion, BrowserVersion,
        CapabilitySupport, RendererCapability, TargetLifecycle,
    };

    fn compatibility() -> BrowserCompatibility {
        BrowserCompatibility::new(
            BrowserVersion::new(
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

    fn info(key: &str, url: &str) -> TransportTargetInfo {
        TransportTargetInfo::new(key, "page", url, key, false, None).unwrap()
    }

    #[test]
    fn initial_snapshot_and_duplicates_are_idempotent() {
        let state = SupervisorState::new(compatibility());
        let first = reduce(
            state,
            SupervisorInput::InitialTargets(vec![info("a", "https://a")]),
        )
        .unwrap();
        let second = reduce(
            first.state.clone(),
            SupervisorInput::InitialTargets(vec![info("a", "https://a")]),
        )
        .unwrap();
        assert_eq!(second.state.targets_by_key.len(), 1);
        assert!(second.effects.is_empty());
        assert_eq!(
            first.state.targets_by_key["a"].target.target.id(),
            second.state.targets_by_key["a"].target.target.id()
        );
    }

    #[test]
    fn selection_is_initial_explicit_fallback_and_reconnect_stable_by_exact_key() {
        let mut state = reduce(
            SupervisorState::new(compatibility()),
            SupervisorInput::InitialTargets(vec![
                info("b", "https://same.test"),
                info("a", "https://same.test"),
            ]),
        )
        .unwrap()
        .state;
        for key in ["b", "a"] {
            state = reduce(
                state,
                SupervisorInput::Attached {
                    target_key: key.into(),
                    session: crate::transport::TransportSessionId::new(format!("session-{key}"))
                        .unwrap(),
                },
            )
            .unwrap()
            .state;
            state = reduce(
                state,
                SupervisorInput::VisibilityChanged {
                    target_key: key.into(),
                    visibility: TargetVisibility::Visible,
                },
            )
            .unwrap()
            .state;
        }
        state = reduce(state, SupervisorInput::InitialReconciliationCompleted)
            .unwrap()
            .state;
        assert_eq!(state.selected_target_key.as_deref(), Some("a"));
        let a_id = state.targets_by_key["a"].target.target.id();
        let b_id = state.targets_by_key["b"].target.target.id();

        let selected = reduce(
            state,
            SupervisorInput::SelectTarget {
                target_key: "b".into(),
            },
        )
        .unwrap();
        assert_eq!(selected.state.selected_target_key.as_deref(), Some("b"));
        assert!(selected.effects.iter().any(|effect| matches!(
            effect,
            SupervisorEffect::Publish(BrowserSessionEvent::SelectedTargetChanged {
                previous: Some(previous),
                selected: Some(next),
            }) if *previous == a_id && *next == b_id
        )));

        let closed = reduce(
            selected.state,
            SupervisorInput::TargetDestroyed {
                target_key: "b".into(),
            },
        )
        .unwrap();
        assert_eq!(closed.state.selected_target_key.as_deref(), Some("a"));
        assert_eq!(
            closed.state.selected_target().unwrap().target.target.id(),
            a_id
        );

        let disconnected = reduce(
            closed.state,
            SupervisorInput::ConnectionLost(crate::transport::TransportClose {
                reason: krometrail_core::NonEmptyText::new("remote").unwrap(),
            }),
        )
        .unwrap()
        .state;
        assert_eq!(disconnected.selected_target_key.as_deref(), Some("a"));
        let restored = reduce(
            disconnected,
            SupervisorInput::Reconnected(ReconnectedSnapshot {
                connection_generation: 1,
                compatibility: compatibility(),
                targets: vec![ReconnectedTarget {
                    info: info("a", "https://changed.test"),
                    session: Some(crate::transport::TransportSessionId::new("new-a").unwrap()),
                    visibility: TargetVisibility::Visible,
                }],
            }),
        )
        .unwrap();
        assert_eq!(restored.state.selected_target_key.as_deref(), Some("a"));
        assert_eq!(
            restored.state.selected_target().unwrap().target.target.id(),
            a_id
        );
        assert!(!restored.effects.iter().any(|effect| matches!(
            effect,
            SupervisorEffect::Publish(BrowserSessionEvent::SelectedTargetChanged { .. })
        )));
    }

    #[test]
    fn unsupported_and_internal_targets_are_ignored() {
        let result = reduce(
            SupervisorState::new(compatibility()),
            SupervisorInput::InitialTargets(vec![
                TransportTargetInfo::new("worker", "worker", "https://a", "", false, None).unwrap(),
                info("devtools", "devtools://devtools/bundled/inspector.html"),
            ]),
        )
        .unwrap();
        assert!(result.state.targets_by_key.is_empty());
    }

    #[test]
    fn disconnect_suspends_targets_and_reconnect_restores_exact_keys() {
        let state = reduce(
            SupervisorState::new(compatibility()),
            SupervisorInput::InitialTargets(vec![info("a", "https://old")]),
        )
        .unwrap()
        .state;
        let state = reduce(
            state,
            SupervisorInput::Attached {
                target_key: "a".into(),
                session: crate::transport::TransportSessionId::new("s1").unwrap(),
            },
        )
        .unwrap()
        .state;
        let state = reduce(
            state,
            SupervisorInput::ConnectionLost(crate::transport::TransportClose {
                reason: krometrail_core::NonEmptyText::new("remote").unwrap(),
            }),
        )
        .unwrap()
        .state;
        assert_eq!(
            state.targets_by_key["a"].target.lifecycle,
            TargetLifecycle::Suspended
        );
        let old_id = state.targets_by_key["a"].target.target.id();
        let restored = reduce(
            state,
            SupervisorInput::Reconnected(ReconnectedSnapshot {
                connection_generation: 1,
                compatibility: compatibility(),
                targets: vec![ReconnectedTarget {
                    info: info("a", "https://new"),
                    session: Some(crate::transport::TransportSessionId::new("s2").unwrap()),
                    visibility: TargetVisibility::Visible,
                }],
            }),
        )
        .unwrap();
        assert_eq!(
            restored.state.targets_by_key["a"].target.target.id(),
            old_id
        );
        assert_eq!(
            restored.state.targets_by_key["a"].target.target.url(),
            "https://new"
        );
        assert_eq!(
            restored.state.targets_by_key["a"].target.lifecycle,
            TargetLifecycle::Attached
        );
    }

    #[test]
    fn stale_generation_events_are_ignored_after_reconnect() {
        let state = reduce(
            SupervisorState::new(compatibility()),
            SupervisorInput::InitialTargets(vec![info("a", "https://a")]),
        )
        .unwrap()
        .state;
        let state = reduce(
            state,
            SupervisorInput::ConnectionLost(crate::transport::TransportClose {
                reason: krometrail_core::NonEmptyText::new("remote").unwrap(),
            }),
        )
        .unwrap()
        .state;
        let state = reduce(
            state,
            SupervisorInput::Reconnected(ReconnectedSnapshot {
                connection_generation: 1,
                compatibility: compatibility(),
                targets: vec![ReconnectedTarget {
                    info: info("a", "https://a"),
                    session: Some(crate::transport::TransportSessionId::new("new").unwrap()),
                    visibility: TargetVisibility::Visible,
                }],
            }),
        )
        .unwrap()
        .state;
        let reduction = reduce(
            state.clone(),
            SupervisorInput::ForConnectionGeneration {
                generation: 0,
                input: Box::new(SupervisorInput::Detached {
                    session: crate::transport::TransportSessionId::new("new").unwrap(),
                    reason: None,
                }),
            },
        )
        .unwrap();
        assert_eq!(reduction.state, state);
        assert!(reduction.effects.is_empty());
    }

    #[test]
    fn reconnect_snapshot_without_session_retries_exact_key_attachment() {
        let state = reduce(
            SupervisorState::new(compatibility()),
            SupervisorInput::InitialTargets(vec![info("a", "https://a")]),
        )
        .unwrap()
        .state;
        let state = reduce(
            state,
            SupervisorInput::ConnectionLost(crate::transport::TransportClose {
                reason: krometrail_core::NonEmptyText::new("remote").unwrap(),
            }),
        )
        .unwrap()
        .state;
        let reduction = reduce(
            state,
            SupervisorInput::Reconnected(ReconnectedSnapshot {
                connection_generation: 1,
                compatibility: compatibility(),
                targets: vec![ReconnectedTarget {
                    info: info("a", "https://a"),
                    session: None,
                    visibility: TargetVisibility::Unknown,
                }],
            }),
        )
        .unwrap();
        assert!(reduction.effects.iter().any(|effect| matches!(
            effect,
            SupervisorEffect::Attach { target_key } if target_key == "a"
        )));
    }

    #[test]
    fn initial_ready_rejects_unresolved_visibility() {
        let state = reduce(
            SupervisorState::new(compatibility()),
            SupervisorInput::InitialTargets(vec![info("a", "https://a")]),
        )
        .unwrap()
        .state;
        let error = reduce(state, SupervisorInput::InitialReconciliationCompleted).unwrap_err();
        assert_eq!(error.code, ErrorCode::InvalidLifecycleTransition);
    }

    #[test]
    fn failed_initial_visibility_probe_detaches_and_fails_only_that_target() {
        let state = reduce(
            SupervisorState::new(compatibility()),
            SupervisorInput::InitialTargets(vec![info("a", "https://a")]),
        )
        .unwrap()
        .state;
        let state = reduce(
            state,
            SupervisorInput::Attached {
                target_key: "a".into(),
                session: crate::transport::TransportSessionId::new("session-a").unwrap(),
            },
        )
        .unwrap()
        .state;
        let failed = reduce(
            state,
            SupervisorInput::InitialVisibilityProbeFailed {
                target_key: "a".into(),
            },
        )
        .unwrap();
        assert!(failed.effects.iter().any(|effect| matches!(
            effect,
            SupervisorEffect::Detach { session } if session.as_str() == "session-a"
        )));
        assert_eq!(
            failed.state.targets_by_key["a"].target.lifecycle,
            TargetLifecycle::Failed
        );
        assert!(
            !failed
                .state
                .target_key_by_session
                .contains_key(&crate::transport::TransportSessionId::new("session-a").unwrap())
        );
    }

    #[test]
    fn capture_starts_only_after_ready_and_visible_and_is_idempotent() {
        let state = reduce(
            SupervisorState::new(compatibility()),
            SupervisorInput::InitialTargets(vec![info("a", "https://a")]),
        )
        .unwrap()
        .state;
        let state = reduce(
            state,
            SupervisorInput::Attached {
                target_key: "a".into(),
                session: crate::transport::TransportSessionId::new("session-a").unwrap(),
            },
        )
        .unwrap()
        .state;
        let state = reduce(
            state,
            SupervisorInput::VisibilityChanged {
                target_key: "a".into(),
                visibility: TargetVisibility::Visible,
            },
        )
        .unwrap()
        .state;
        let ready = reduce(state, SupervisorInput::InitialReconciliationCompleted).unwrap();
        assert!(ready.effects.iter().any(|effect| matches!(
            effect,
            SupervisorEffect::StartCapture { context }
                if context.target_id == ready.state.targets_by_key["a"].target.target.id()
                    && context.connection_generation == 0
                    && context.attachment_generation == 1
                    && context.transport_session.as_str() == "session-a"
        )));
        let again = reduce(
            ready.state.clone(),
            SupervisorInput::InitialReconciliationCompleted,
        )
        .unwrap();
        assert!(again.effects.is_empty());
    }

    #[test]
    fn capture_suspends_on_disconnect_and_resumes_exact_key_on_new_generation() {
        let state = reduce(
            SupervisorState::new(compatibility()),
            SupervisorInput::InitialTargets(vec![info("a", "https://a")]),
        )
        .unwrap()
        .state;
        let state = reduce(
            state,
            SupervisorInput::Attached {
                target_key: "a".into(),
                session: crate::transport::TransportSessionId::new("old").unwrap(),
            },
        )
        .unwrap()
        .state;
        let state = reduce(
            state,
            SupervisorInput::VisibilityChanged {
                target_key: "a".into(),
                visibility: TargetVisibility::Visible,
            },
        )
        .unwrap()
        .state;
        let state = reduce(state, SupervisorInput::InitialReconciliationCompleted)
            .unwrap()
            .state;
        let disconnected = reduce(
            state,
            SupervisorInput::ConnectionLost(crate::transport::TransportClose {
                reason: krometrail_core::NonEmptyText::new("remote").unwrap(),
            }),
        )
        .unwrap();
        assert!(
            disconnected
                .effects
                .iter()
                .any(|effect| matches!(effect, SupervisorEffect::SuspendCapture { .. }))
        );
        let old_id = disconnected.state.targets_by_key["a"].target.target.id();
        let restored = reduce(
            disconnected.state,
            SupervisorInput::Reconnected(ReconnectedSnapshot {
                connection_generation: 1,
                compatibility: compatibility(),
                targets: vec![ReconnectedTarget {
                    info: info("a", "https://changed"),
                    session: Some(crate::transport::TransportSessionId::new("new").unwrap()),
                    visibility: TargetVisibility::Visible,
                }],
            }),
        )
        .unwrap();
        assert!(restored.effects.iter().any(|effect| matches!(
            effect,
            SupervisorEffect::ResumeCapture { context }
                if context.target_id == old_id
                    && context.connection_generation == 1
                    && context.attachment_generation == 2
                    && context.transport_session.as_str() == "new"
        )));
        assert_eq!(
            restored.state.targets_by_key["a"].target.target.id(),
            old_id
        );
    }

    #[test]
    fn capture_stops_on_target_close_and_failure_without_affecting_other_targets() {
        let state = reduce(
            SupervisorState::new(compatibility()),
            SupervisorInput::InitialTargets(vec![info("a", "https://a"), info("b", "https://b")]),
        )
        .unwrap()
        .state;
        let mut state = state;
        for key in ["a", "b"] {
            state = reduce(
                state,
                SupervisorInput::Attached {
                    target_key: key.into(),
                    session: crate::transport::TransportSessionId::new(key).unwrap(),
                },
            )
            .unwrap()
            .state;
            state = reduce(
                state,
                SupervisorInput::VisibilityChanged {
                    target_key: key.into(),
                    visibility: TargetVisibility::Visible,
                },
            )
            .unwrap()
            .state;
        }
        let ready = reduce(state, SupervisorInput::InitialReconciliationCompleted).unwrap();
        let state = ready.state;
        let closed = reduce(
            state,
            SupervisorInput::TargetDestroyed {
                target_key: "a".into(),
            },
        )
        .unwrap();
        assert!(closed
            .effects
            .iter()
            .any(|effect| matches!(effect, SupervisorEffect::StopCapture { context } if context.transport_session.as_str() == "a")));
        assert!(matches!(
            closed.state.targets_by_key["b"].capture_binding,
            CaptureBinding::Active(_)
        ));
    }

    #[test]
    fn process_death_does_not_emit_reconnect() {
        let result = reduce(
            SupervisorState::new(compatibility()),
            SupervisorInput::BrowserProcessTerminated {
                exit: crate::launcher::SanitizedProcessExit::Code(1),
            },
        )
        .unwrap();
        assert!(result.effects.iter().any(|effect| matches!(
            effect,
            SupervisorEffect::Publish(BrowserSessionEvent::SessionFailed { error })
            if error.code == ErrorCode::BrowserProcessTerminated
        )));
        assert!(
            !result
                .effects
                .iter()
                .any(|effect| matches!(effect, SupervisorEffect::BeginReconnect))
        );
    }
}
