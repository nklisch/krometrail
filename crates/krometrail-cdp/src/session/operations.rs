use super::*;
use crate::session::evidence::persist_result_evidence;
use krometrail_core::{BROWSER_OPERATION_REGISTRY, OperationMutability, RetryAdvice};

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct OperationExecutionContext {
    pub(crate) deadline: Option<tokio::time::Instant>,
    pub(crate) parent_batch: Option<krometrail_core::InteractionId>,
}

pub(crate) async fn execute_operation(
    page_control: &mut PageControl,
    state: &mut SupervisorState,
    transport: Arc<dyn CdpTransport>,
    shared: &Arc<SessionShared>,
    request: BrowserOperationRequest,
    cancellation: &OperationCancellation,
    context: OperationExecutionContext,
) -> Result<BrowserOperationResult> {
    if cancellation.request_is_cancelled() {
        return Err(request_operation_error(
            ErrorCode::Cancelled,
            direct_request_target(&request),
            "browser operation was cancelled before dispatch",
        ));
    }
    let kind = request.kind();
    let state_changing = BROWSER_OPERATION_REGISTRY
        .iter()
        .find(|definition| definition.kind == kind)
        .is_some_and(|definition| definition.mutability == OperationMutability::StateChanging);
    if state_changing && shared.interaction_evidence.is_none() {
        return Err(missing_evidence_sink(
            shared.session_id,
            direct_request_target(&request),
        ));
    }
    let outer_batch = matches!(request, BrowserOperationRequest::Batch(_));
    let result = execute_operation_unfenced(
        page_control,
        state,
        transport,
        shared,
        request,
        cancellation,
        context,
    )
    .await?;
    if state_changing && !outer_batch {
        let sink = shared
            .interaction_evidence
            .as_deref()
            .expect("state-changing dispatch requires an evidence sink");
        persist_result_evidence(
            &result,
            sink,
            page_control.clock.as_ref(),
            page_control.ids.as_ref(),
        )
        .await?;
    }
    Ok(result)
}

async fn execute_operation_unfenced(
    page_control: &mut PageControl,
    state: &mut SupervisorState,
    transport: Arc<dyn CdpTransport>,
    shared: &Arc<SessionShared>,
    request: BrowserOperationRequest,
    cancellation: &OperationCancellation,
    context: OperationExecutionContext,
) -> Result<BrowserOperationResult> {
    if cancellation.request_is_cancelled() {
        return Err(request_operation_error(
            ErrorCode::Cancelled,
            direct_request_target(&request),
            "browser operation was cancelled before dispatch",
        ));
    }
    if let BrowserOperationRequest::Batch(request) = request {
        return page_control
            .execute_batch(transport, state, shared, request, cancellation, context)
            .await
            .map(|result| BrowserOperationResult::Batch(Box::new(result)));
    }
    if request.kind().is_interaction() {
        return page_control
            .execute_interaction_request(
                transport.as_ref(),
                shared.browser_events.as_ref(),
                state,
                request,
                cancellation,
                context.parent_batch,
            )
            .await;
    }
    match request {
        BrowserOperationRequest::CreatePage(request) => {
            let started_at = page_control.session_time()?;
            let dispatched_at = page_control.session_time()?;
            let response = transport
                .send_raw(
                    &CommandScope::Browser,
                    "Target.createTarget",
                    serde_json::json!({"url": request.initial_url.as_ref().map_or("about:blank", |url| url.as_str())}),
                )
                .await
                .map_err(|error| transport_error_to_core(error, true))?;
            let target_key = response
                .get("targetId")
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| {
                    stable_error(
                        ErrorCode::TargetFailed,
                        "browser returned an invalid created target",
                    )
                })?
                .to_owned();
            let info = transport
                .send_raw(
                    &CommandScope::Browser,
                    "Target.getTargetInfo",
                    serde_json::json!({"targetId": target_key}),
                )
                .await
                .map_err(|error| transport_error_to_core(error, true))?
                .get("targetInfo")
                .and_then(parse_target_info)
                .ok_or_else(|| {
                    stable_error(
                        ErrorCode::TargetFailed,
                        "created browser target could not be reconciled",
                    )
                })?;
            // The browser has supplied a concrete target before the interaction is allocated.
            // Reducing first gives the anchor its real TargetId; every fallible attach/select step
            // after allocation can therefore return an honest anchored failure instead of a plain
            // browser-scoped error or a fabricated target identity.
            let reduction = reduce(state.clone(), SupervisorInput::TargetCreated(info))?;
            *state = reduction.state;
            let target_id = state
                .targets_by_key
                .get(&target_key)
                .map(|target| target.target.target.id())
                .ok_or_else(|| {
                    stable_error(
                        ErrorCode::TargetFailed,
                        "created browser target could not be reconciled",
                    )
                })?;
            let interaction_id = page_control.next_interaction_id();
            let browser_event_support = *shared
                .browser_event_support
                .lock()
                .expect("browser event support lock");
            let attach = apply_effects(
                state,
                reduction.effects,
                Arc::clone(&transport),
                Arc::clone(&shared.subscribers),
                shared.capture.clone(),
                Arc::clone(&shared.browser_events),
                browser_event_support,
                None,
            )
            .await;
            *shared.state.lock().expect("session state lock") = state.clone();
            if attach.is_err()
                || state
                    .resolve_selection(PageSelection::Target(target_id))
                    .is_err()
            {
                return page_failure_result(
                    page_control,
                    target_id,
                    krometrail_core::BrowserOperationKind::CreatePage,
                    interaction_id,
                    started_at,
                    dispatched_at,
                    operation_error(
                        ErrorCode::TargetFailed,
                        target_id,
                        "created browser target could not be attached",
                    ),
                );
            }
            let activation = transport
                .send_raw(
                    &CommandScope::Browser,
                    "Target.activateTarget",
                    serde_json::json!({"targetId": target_key}),
                )
                .await;
            if let Err(error) = activation {
                return page_failure_result(
                    page_control,
                    target_id,
                    krometrail_core::BrowserOperationKind::CreatePage,
                    interaction_id,
                    started_at,
                    dispatched_at,
                    transport_page_error(error, ErrorCode::TargetFailed, target_id),
                );
            }
            if commit_supervisor_input(
                state,
                SupervisorInput::SelectTarget { target_key },
                Arc::clone(&transport),
                shared,
            )
            .await
            .is_err()
            {
                return page_failure_result(
                    page_control,
                    target_id,
                    krometrail_core::BrowserOperationKind::CreatePage,
                    interaction_id,
                    started_at,
                    dispatched_at,
                    operation_error(
                        ErrorCode::TargetFailed,
                        target_id,
                        "created browser target could not be selected",
                    ),
                );
            }
            page_success_result(
                page_control,
                transport.as_ref(),
                state,
                target_id,
                krometrail_core::BrowserOperationKind::CreatePage,
                interaction_id,
                started_at,
                dispatched_at,
                PageChange::Created { target_id },
                PageSelection::Target(target_id),
                cancellation,
            )
            .await
            .map(|result| BrowserOperationResult::CreatePage(Box::new(result)))
        }
        BrowserOperationRequest::SelectPage(request) => {
            let target = state.resolve_selection(PageSelection::Target(request.target_id))?;
            let target_key = target.target.target.browser_target_key().to_owned();
            let target_id = target.target.target.id();
            let previous = state
                .selected_target()
                .map(|target| target.target.target.id());
            let started_at = page_control.session_time()?;
            let interaction_id = page_control.next_interaction_id();
            let dispatched_at = page_control.session_time()?;
            if let Err(error) = transport
                .send_raw(
                    &CommandScope::Browser,
                    "Target.activateTarget",
                    serde_json::json!({"targetId": target_key}),
                )
                .await
            {
                return page_failure_result(
                    page_control,
                    target_id,
                    krometrail_core::BrowserOperationKind::SelectPage,
                    interaction_id,
                    started_at,
                    dispatched_at,
                    transport_page_error(error, ErrorCode::TargetFailed, target_id),
                );
            }
            commit_supervisor_input(
                state,
                SupervisorInput::SelectTarget { target_key },
                Arc::clone(&transport),
                shared,
            )
            .await?;
            page_success_result(
                page_control,
                transport.as_ref(),
                state,
                target_id,
                krometrail_core::BrowserOperationKind::SelectPage,
                interaction_id,
                started_at,
                dispatched_at,
                PageChange::Selected {
                    previous,
                    selected: target_id,
                },
                PageSelection::Target(target_id),
                cancellation,
            )
            .await
            .map(|result| BrowserOperationResult::SelectPage(Box::new(result)))
        }
        BrowserOperationRequest::NavigatePage(request) => {
            page_control
                .navigate(transport.as_ref(), state, request, cancellation)
                .await
        }
        BrowserOperationRequest::ReloadPage(request) => {
            page_control
                .reload(transport.as_ref(), state, request, cancellation)
                .await
        }
        BrowserOperationRequest::GoBack(request) => {
            page_control
                .go_back(transport.as_ref(), state, request, cancellation)
                .await
        }
        BrowserOperationRequest::GoForward(request) => {
            page_control
                .go_forward(transport.as_ref(), state, request, cancellation)
                .await
        }
        BrowserOperationRequest::ClosePage(request) => {
            let target = state.resolve_selection(request.target)?;
            let target_key = target.target.target.browser_target_key().to_owned();
            let target_id = target.target.target.id();
            let started_at = page_control.session_time()?;
            let interaction_id = page_control.next_interaction_id();
            let dispatched_at = page_control.session_time()?;
            let response = transport
                .send_raw(
                    &CommandScope::Browser,
                    "Target.closeTarget",
                    serde_json::json!({"targetId": target_key}),
                )
                .await;
            let success = match response {
                Ok(response) => response.get("success").and_then(Value::as_bool) == Some(true),
                Err(error) => {
                    return page_failure_result(
                        page_control,
                        target_id,
                        krometrail_core::BrowserOperationKind::ClosePage,
                        interaction_id,
                        started_at,
                        dispatched_at,
                        transport_page_error(error, ErrorCode::TargetFailed, target_id),
                    );
                }
            };
            if !success {
                return page_failure_result(
                    page_control,
                    target_id,
                    krometrail_core::BrowserOperationKind::ClosePage,
                    interaction_id,
                    started_at,
                    dispatched_at,
                    operation_error(
                        ErrorCode::TargetFailed,
                        target_id,
                        "browser did not confirm page closure",
                    ),
                );
            }
            commit_supervisor_input(
                state,
                SupervisorInput::TargetDestroyed { target_key },
                Arc::clone(&transport),
                shared,
            )
            .await?;
            page_control.invalidate_target_snapshot(target_id);
            let selected = state
                .selected_target()
                .map(|target| target.target.target.id());
            let (observation, interruption) = match selected {
                Some(selected) => {
                    let observed = page_control
                        .observe_after_operation(
                            transport.as_ref(),
                            state,
                            PageSelection::Target(selected),
                            cancellation,
                        )
                        .await?;
                    (observed.observation, observed.interruption)
                }
                None => (
                    ObservationPart::Unavailable(KrometrailError::new(
                        ErrorCode::NotFound,
                        NonEmptyText::new("no browser page remains selected after closure")
                            .unwrap(),
                    )),
                    None,
                ),
            };
            let outcome = interruption.map_or_else(
                || {
                    PageOperationOutcome::Succeeded(PageChange::Closed {
                        closed: target_id,
                        selected,
                    })
                },
                PageOperationOutcome::Failed,
            );
            let result = build_page_result(
                page_control,
                target_id,
                krometrail_core::BrowserOperationKind::ClosePage,
                interaction_id,
                started_at,
                dispatched_at,
                outcome,
                observation,
            )?;
            Ok(BrowserOperationResult::ClosePage(Box::new(result)))
        }
        request => {
            page_control
                .execute(
                    transport.as_ref(),
                    shared.browser_events.as_ref(),
                    state,
                    request,
                    cancellation,
                    context.deadline,
                )
                .await
        }
    }
}

async fn commit_supervisor_input(
    state: &mut SupervisorState,
    input: SupervisorInput,
    transport: Arc<dyn CdpTransport>,
    shared: &Arc<SessionShared>,
) -> Result<()> {
    let previous = std::mem::replace(state, SupervisorState::new(shared.compatibility.clone()));
    let reduction = reduce(previous, input)?;
    *state = reduction.state;
    let browser_event_support = *shared
        .browser_event_support
        .lock()
        .expect("browser event support lock");
    apply_effects(
        state,
        reduction.effects,
        transport,
        Arc::clone(&shared.subscribers),
        shared.capture.clone(),
        Arc::clone(&shared.browser_events),
        browser_event_support,
        None,
    )
    .await?;
    *shared.state.lock().expect("session state lock") = state.clone();
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn page_success_result(
    page_control: &mut PageControl,
    transport: &dyn CdpTransport,
    state: &SupervisorState,
    target_id: krometrail_core::TargetId,
    operation: krometrail_core::BrowserOperationKind,
    interaction_id: krometrail_core::InteractionId,
    started_at: krometrail_core::SessionTime,
    dispatched_at: krometrail_core::SessionTime,
    change: PageChange,
    observation_target: PageSelection,
    cancel: &OperationCancellation,
) -> Result<PageOperationResult> {
    let observation = page_control
        .observe_after_operation(transport, state, observation_target, cancel)
        .await?;
    let outcome = observation.interruption.map_or_else(
        || PageOperationOutcome::Succeeded(change),
        PageOperationOutcome::Failed,
    );
    build_page_result(
        page_control,
        target_id,
        operation,
        interaction_id,
        started_at,
        dispatched_at,
        outcome,
        observation.observation,
    )
}

fn page_failure_result(
    page_control: &PageControl,
    target_id: krometrail_core::TargetId,
    operation: krometrail_core::BrowserOperationKind,
    interaction_id: krometrail_core::InteractionId,
    started_at: krometrail_core::SessionTime,
    dispatched_at: krometrail_core::SessionTime,
    error: KrometrailError,
) -> Result<BrowserOperationResult> {
    let result = build_page_result(
        page_control,
        target_id,
        operation,
        interaction_id,
        started_at,
        dispatched_at,
        PageOperationOutcome::Failed(error.clone()),
        ObservationPart::Unavailable(error),
    )?;
    Ok(match operation {
        krometrail_core::BrowserOperationKind::CreatePage => {
            BrowserOperationResult::CreatePage(Box::new(result))
        }
        krometrail_core::BrowserOperationKind::SelectPage => {
            BrowserOperationResult::SelectPage(Box::new(result))
        }
        krometrail_core::BrowserOperationKind::ClosePage => {
            BrowserOperationResult::ClosePage(Box::new(result))
        }
        krometrail_core::BrowserOperationKind::NavigatePage => {
            BrowserOperationResult::NavigatePage(Box::new(result))
        }
        krometrail_core::BrowserOperationKind::ReloadPage => {
            BrowserOperationResult::ReloadPage(Box::new(result))
        }
        krometrail_core::BrowserOperationKind::GoBack => {
            BrowserOperationResult::GoBack(Box::new(result))
        }
        krometrail_core::BrowserOperationKind::GoForward => {
            BrowserOperationResult::GoForward(Box::new(result))
        }
        _ => unreachable!("only state-changing page operations produce page failures"),
    })
}

#[allow(clippy::too_many_arguments)]
fn build_page_result(
    page_control: &PageControl,
    target_id: krometrail_core::TargetId,
    operation: krometrail_core::BrowserOperationKind,
    interaction_id: krometrail_core::InteractionId,
    started_at: krometrail_core::SessionTime,
    dispatched_at: krometrail_core::SessionTime,
    outcome: PageOperationOutcome,
    observation: ObservationPart<krometrail_core::LiveObservation>,
) -> Result<PageOperationResult> {
    let (completed_at, observed_at) = match &observation {
        ObservationPart::Available(observation) => (
            observation.context.started_at,
            Some(observation.context.completed_at),
        ),
        ObservationPart::Unavailable(_) => (page_control.session_time()?, None),
    };
    let timing = InteractionTiming::new(started_at, dispatched_at, completed_at, observed_at)?;
    let interaction = InteractionAnchor::new(
        interaction_id,
        page_control.session_id,
        target_id,
        operation,
        timing,
    )?;
    let outcome = match outcome {
        PageOperationOutcome::Failed(error) => PageOperationOutcome::failed(error, &interaction),
        outcome => outcome,
    };
    PageOperationResult::new(interaction, outcome, observation)
}

fn transport_page_error(
    error: TransportError,
    fallback: ErrorCode,
    target_id: krometrail_core::TargetId,
) -> KrometrailError {
    crate::control::transport_error(error, fallback, target_id)
}

fn missing_evidence_sink(
    session_id: SessionId,
    target_id: Option<krometrail_core::TargetId>,
) -> KrometrailError {
    KrometrailError::new(
        ErrorCode::PersistenceFailed,
        NonEmptyText::new("state-changing browser operations require durable temporal evidence")
            .expect("static evidence error is non-empty"),
    )
    .with_context(krometrail_core::ErrorContext {
        session_id: Some(session_id),
        target_id,
        ..krometrail_core::ErrorContext::default()
    })
    .with_retry(RetryAdvice::Never)
    .with_recovery(
        NonEmptyText::new("restore the recording store before dispatching browser changes")
            .expect("static evidence recovery is non-empty"),
    )
}
