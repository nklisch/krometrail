use super::*;
use crate::session::evidence::persist_result_evidence;
use krometrail_core::{BROWSER_OPERATION_REGISTRY, OperationMutability, RetryAdvice};

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct OperationExecutionContext {
    pub(crate) deadline: Option<tokio::time::Instant>,
    pub(crate) parent_batch: Option<krometrail_core::InteractionId>,
}

/// Ceiling for post-interaction side-channel reconciliation. The page and
/// download waits share this one batch-deadline-capped window.
const SIDE_CHANNEL_RECONCILE_WINDOW: Duration = Duration::from_secs(2);
const SIDE_CHANNEL_RECONCILE_POLL_INTERVAL: Duration = Duration::from_millis(50);

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
    let direct_target = direct_request_target(&request);
    let result = execute_operation_unfenced(
        page_control,
        state,
        Arc::clone(&transport),
        shared,
        request,
        cancellation,
        context,
    )
    .await
    .map_err(|error| classify_open_dialog(error, kind, direct_target, shared))?;
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

/// The blocked-observation consumption site for reported open-dialog state.
///
/// An open modal JavaScript dialog stops the renderer from answering observation, evaluation, and
/// input commands. Every one of those surfaces reports a generic failure whose recovery ("retry
/// once; inspect browser compatibility") is wrong for this cause: retrying never succeeds while
/// the dialog is open, and compatibility is irrelevant. Re-code the failure once, here, from the
/// same state that page status and `handle_dialog` read.
fn classify_open_dialog(
    error: KrometrailError,
    kind: krometrail_core::BrowserOperationKind,
    direct_target: Option<krometrail_core::TargetId>,
    shared: &Arc<SessionShared>,
) -> KrometrailError {
    // handle_dialog is the recovery for this state, so its own failures keep their own codes.
    if kind == krometrail_core::BrowserOperationKind::HandleDialog {
        return error;
    }
    // Only codes whose recovery actively misdirects for this cause. A wait timeout is deliberately
    // excluded: its recovery already sends the caller to page status, which now names the dialog,
    // and its code is load-bearing for batch termination.
    if !matches!(
        error.code,
        ErrorCode::PageObservationFailed
            | ErrorCode::ScreenshotFailed
            | ErrorCode::EvaluationFailed
            | ErrorCode::InteractionFailed
    ) {
        return error;
    }
    let Some(target_id) = error.context.target_id.or(direct_target) else {
        return error;
    };
    let state = shared.browser_events.open_dialog_state(target_id);
    let Some(dialog_type) = state.dialog_type() else {
        return error;
    };
    KrometrailError::from_browser_failure(
        ErrorCode::DialogOpen,
        NonEmptyText::new(format!(
            "an open {} JavaScript dialog is blocking the renderer on this page",
            dialog_type.as_str()
        ))
        .expect("dialog-open message is non-empty"),
    )
    .with_context(error.context)
}

pub(super) async fn execute_operation_unfenced(
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
    if matches!(
        &request,
        BrowserOperationRequest::ReadClipboard(_)
            | BrowserOperationRequest::WriteClipboard(_)
            | BrowserOperationRequest::ListDownloads(_)
            | BrowserOperationRequest::WaitForDownload(_)
            | BrowserOperationRequest::CancelDownload(_)
    ) && shared.ownership != BrowserOwnership::Managed
    {
        return Err(stable_error(
            ErrorCode::Unsupported,
            "local clipboard and download operations require a Krometrail-managed browser session",
        )
        .with_recovery(
            NonEmptyText::new(
                "start a managed browser profile and retry the explicit local operation",
            )
            .unwrap(),
        ));
    }
    if let BrowserOperationRequest::Batch(request) = request {
        return page_control
            .execute_batch(transport, state, shared, request, cancellation, context)
            .await
            .map(|result| BrowserOperationResult::Batch(Box::new(result)));
    }
    if let BrowserOperationRequest::WriteClipboard(request) = request {
        let bound = crate::control::bind_target(state, request.target)?;
        cancellation.check(state.connection_generation, bound.target_id)?;
        let started_at = page_control.session_time()?;
        let dispatched_at = page_control.session_time()?;
        page_control
            .write_clipboard(transport.as_ref(), &bound, &request)
            .await?;
        let interaction_id = page_control.next_interaction_id();
        let bytes = request.text.len() as u64;
        let operation = page_success_result(
            page_control,
            transport.as_ref(),
            state,
            bound.target_id,
            krometrail_core::BrowserOperationKind::WriteClipboard,
            interaction_id,
            started_at,
            dispatched_at,
            PageChange::ClipboardWritten,
            request.target,
            cancellation,
        )
        .await?;
        return Ok(BrowserOperationResult::WriteClipboard(Box::new(
            krometrail_core::ClipboardWriteResult {
                utf8_bytes: bytes,
                operation,
            },
        )));
    }
    match request {
        BrowserOperationRequest::WaitForPage(request) => {
            wait_for_page(state, transport, shared, request, cancellation).await
        }
        BrowserOperationRequest::ListDownloads(_) => {
            let authority = shared
                .downloads
                .as_ref()
                .expect("managed ownership has download authority");
            Ok(BrowserOperationResult::ListDownloads(Box::new(
                authority.list()?,
            )))
        }
        BrowserOperationRequest::WaitForDownload(request) => {
            let authority = shared
                .downloads
                .as_ref()
                .expect("managed ownership has download authority");
            authority
                .wait_with_cancellation(request, None)
                .await
                .map(|value| BrowserOperationResult::WaitForDownload(Box::new(value)))
        }
        BrowserOperationRequest::CancelDownload(request) => {
            let authority = shared
                .downloads
                .as_ref()
                .expect("managed ownership has download authority");
            let target_id = state
                .selected_target()
                .map(|target| target.target.target.id())
                .ok_or_else(|| {
                    stable_error(
                        ErrorCode::TargetFailed,
                        "download cancellation requires one current supervised page",
                    )
                })?;
            let started_at = page_control.session_time()?;
            let dispatched_at = page_control.session_time()?;
            let download_id = request.download_id;
            let state = authority
                .cancel(transport.as_ref(), request.download_id)
                .await?;
            let completed_at = page_control.session_time()?;
            let operation = InteractionAnchor::new(
                page_control.next_interaction_id(),
                shared.session_id,
                target_id,
                krometrail_core::BrowserOperationKind::CancelDownload,
                InteractionTiming::new(started_at, dispatched_at, completed_at, None)?,
            )?;
            Ok(BrowserOperationResult::CancelDownload(Box::new(
                krometrail_core::CancelDownloadResult {
                    download_id,
                    state,
                    operation,
                },
            )))
        }
        request => {
            execute_non_local_operation(
                page_control,
                state,
                transport,
                shared,
                request,
                cancellation,
                context,
            )
            .await
        }
    }
}

async fn wait_for_page(
    state: &mut SupervisorState,
    transport: Arc<dyn CdpTransport>,
    shared: &Arc<SessionShared>,
    request: krometrail_core::WaitForPageRequest,
    cancellation: &OperationCancellation,
) -> Result<BrowserOperationResult> {
    let deadline = tokio::time::Instant::now() + Duration::from_millis(request.timeout_ms);
    loop {
        if matches!(
            state.session_state,
            BrowserSessionState::Reconnecting
                | BrowserSessionState::Stopping
                | BrowserSessionState::Ended
        ) || transport.is_closed()
        {
            return Err(stable_error(
                ErrorCode::BrowserDisconnected,
                "browser page wait ended because the browser session disconnected",
            )
            .with_retry(RetryAdvice::AfterRecovery)
            .with_recovery(
                NonEmptyText::new("restore the browser session, refresh page contexts, and retry")
                    .unwrap(),
            ));
        }
        if let Some(matched) = next_page_match(state, &request)? {
            let cursor = state.page_contexts()?.cursor;
            return Ok(BrowserOperationResult::WaitForPage(Box::new(
                krometrail_core::WaitForPageResult { matched, cursor },
            )));
        }
        if cancellation.is_cancelled() {
            return Err(stable_error(
                ErrorCode::Cancelled,
                "browser page wait was cancelled",
            ));
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(stable_error(
                ErrorCode::WaitTimedOut,
                "no matching browser page appeared before the wait timeout",
            )
            .with_retry(RetryAdvice::Safe)
            .with_recovery(
                NonEmptyText::new("refresh page contexts and retry from the returned cursor")
                    .unwrap(),
            ));
        }
        reconcile_targets_once(state, &transport, shared).await?;
        tokio::time::sleep(
            Duration::from_millis(50)
                .min(deadline.saturating_duration_since(tokio::time::Instant::now())),
        )
        .await;
    }
}

/// One pull-based target reconciliation: `Target.getTargets` → parse →
/// reduce(`InitialTargets`) → apply effects → publish shared state. Shared by
/// the `wait_for_page` poll loop and post-interaction side-channel
/// enrichment; it observes the browser's authoritative inventory directly,
/// independent of target events still queued behind the running operation.
pub(super) async fn reconcile_targets_once(
    state: &mut SupervisorState,
    transport: &Arc<dyn CdpTransport>,
    shared: &Arc<SessionShared>,
) -> Result<()> {
    let infos = fetch_target_infos(transport).await?;
    apply_target_reconciliation(state, transport, shared, infos, None).await
}

/// Read-only target inventory acquisition. This is the only phase that a
/// post-interaction side-channel timeout may cancel.
async fn fetch_target_infos(transport: &Arc<dyn CdpTransport>) -> Result<Vec<TransportTargetInfo>> {
    let response = transport
        .send_raw(
            &CommandScope::Browser,
            "Target.getTargets",
            serde_json::json!({}),
        )
        .await
        .map_err(|error| transport_error_to_core(error, true))?;
    response
        .get("targetInfos")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            stable_error(
                ErrorCode::TargetFailed,
                "browser returned an invalid target inventory",
            )
        })?
        .iter()
        .map(parse_target_info)
        .collect::<Option<Vec<_>>>()
        .ok_or_else(|| {
            stable_error(
                ErrorCode::TargetFailed,
                "browser returned an invalid target inventory",
            )
        })
}

/// Applies a fetched inventory and runs its external effects under the caller's shared ceiling.
/// A timed-out adoption is reduced to the existing failed lifecycle so the authoritative state
/// never retains a target whose attach/enable queue was cancelled halfway through.
async fn apply_target_reconciliation(
    state: &mut SupervisorState,
    transport: &Arc<dyn CdpTransport>,
    shared: &Arc<SessionShared>,
    infos: Vec<TransportTargetInfo>,
    effect_deadline: Option<tokio::time::Instant>,
) -> Result<()> {
    let reduction = reduce(state.clone(), SupervisorInput::InitialTargets(infos))?;
    *state = reduction.state;
    let browser_event_support = *shared
        .browser_event_support
        .lock()
        .expect("browser event support lock");
    match effect_deadline {
        Some(effect_deadline) => {
            apply_effects_until(
                state,
                reduction.effects,
                Arc::clone(transport),
                Arc::clone(&shared.subscribers),
                shared.capture.clone(),
                Arc::clone(&shared.browser_events),
                browser_event_support,
                effect_deadline,
            )
            .await?
        }
        None => {
            apply_effects(
                state,
                reduction.effects,
                Arc::clone(transport),
                Arc::clone(&shared.subscribers),
                shared.capture.clone(),
                Arc::clone(&shared.browser_events),
                browser_event_support,
                None,
            )
            .await?
        }
    }
    *shared.state.lock().expect("session state lock") = state.clone();
    Ok(())
}

/// The interaction results whose record carries the postcondition block.
fn interaction_record_mut(
    result: &mut BrowserOperationResult,
) -> Option<&mut krometrail_core::InteractionRecord> {
    match result {
        BrowserOperationResult::Click(value)
        | BrowserOperationResult::Fill(value)
        | BrowserOperationResult::PressKeys(value)
        | BrowserOperationResult::SelectOption(value)
        | BrowserOperationResult::Hover(value)
        | BrowserOperationResult::Drag(value)
        | BrowserOperationResult::Scroll(value)
        | BrowserOperationResult::UploadFiles(value)
        | BrowserOperationResult::HandleDialog(value) => Some(&mut value.record),
        _ => None,
    }
}

fn finalize_expectation_note(result: &mut BrowserOperationResult) {
    if let Some(record) = interaction_record_mut(result) {
        record.refresh_expectation_note();
    }
}

/// Post-dispatch side-channel delta assembly. Pull-based target
/// reconciliation observes the browser inventory directly; when the drained
/// signals announce a window-open or download attempt, the pull repeats on a
/// short interval until the corresponding delta materializes or the bounded
/// ceiling (batch-deadline capped) elapses. Every failure degrades to absent
/// facts — never a claim that nothing opened — and the proven interaction
/// result is never failed by this enrichment phase.
async fn attach_side_channel_facts(
    result: &mut BrowserOperationResult,
    state: &mut SupervisorState,
    transport: &Arc<dyn CdpTransport>,
    shared: &Arc<SessionShared>,
    baselines: crate::control::InteractionDispatchBaselines,
    deadline: Option<tokio::time::Instant>,
) {
    let Some((acting_target, attempts, requests)) = interaction_record_mut(result).map(|record| {
        (
            record.context.target_id,
            record.postcondition.signals.window_open_attempts,
            record.postcondition.signals.download_requests,
        )
    }) else {
        return;
    };

    let ceiling = crate::control::bounded_deadline(deadline, SIDE_CHANNEL_RECONCILE_WINDOW);
    let mut page_delta = None;
    let mut download_delta = baselines.download_cursor_before.and_then(|cursor| {
        shared
            .downloads
            .as_ref()
            .map(|control| control.begun_after(cursor))
    });
    let mut first_iteration = baselines.page_cursor_before.is_some();

    loop {
        let page_pending =
            attempts.unwrap_or(0) > 0 && page_delta.as_ref().is_some_and(Vec::is_empty);
        if let (Some(cursor_before), true) = (
            baselines.page_cursor_before,
            first_iteration || page_pending,
        ) {
            first_iteration = false;
            let infos = tokio::time::timeout_at(ceiling, fetch_target_infos(transport)).await;
            let Ok(Ok(infos)) = infos else {
                break;
            };
            if apply_target_reconciliation(state, transport, shared, infos, Some(ceiling))
                .await
                .is_err()
            {
                break;
            }
            let Ok(inventory) = state.page_contexts() else {
                break;
            };
            let mut pages = inventory
                .pages
                .into_iter()
                .filter(|page| page.sequence > cursor_before)
                .map(|page| krometrail_core::NewPageFact {
                    target_id: page.page.target.target.id(),
                    sequence: page.sequence,
                    opener_matched: page.opener_target_id == Some(acting_target),
                })
                .collect::<Vec<_>>();
            pages.sort_by_key(|fact| fact.sequence);
            page_delta = Some(pages);
        } else {
            first_iteration = false;
        }

        if let (Some(cursor), Some(control)) =
            (baselines.download_cursor_before, shared.downloads.as_ref())
        {
            download_delta = Some(control.begun_after(cursor));
        }

        let page_pending =
            attempts.unwrap_or(0) > 0 && page_delta.as_ref().is_some_and(Vec::is_empty);
        let download_pending =
            requests.unwrap_or(0) > 0 && download_delta.as_ref().is_some_and(Vec::is_empty);
        if !(page_pending || download_pending) || tokio::time::Instant::now() >= ceiling {
            break;
        }
        tokio::time::sleep(
            SIDE_CHANNEL_RECONCILE_POLL_INTERVAL
                .min(ceiling.saturating_duration_since(tokio::time::Instant::now())),
        )
        .await;
    }

    if let (Some(cursor_before), Some(pages)) = (baselines.page_cursor_before, page_delta)
        && let Some(record) = interaction_record_mut(result)
    {
        record.postcondition.attach_new_pages(
            krometrail_core::NewPagePostcondition::from_observed(cursor_before, pages),
        );
    }
    if let (Some(cursor_before), Some(facts)) = (baselines.download_cursor_before, download_delta)
        && let Some(record) = interaction_record_mut(result)
    {
        record.postcondition.attach_downloads(
            krometrail_core::DownloadPostcondition::from_observed(cursor_before, facts),
        );
    }
}

fn next_page_match(
    state: &SupervisorState,
    request: &krometrail_core::WaitForPageRequest,
) -> Result<Option<krometrail_core::PageContextStatus>> {
    Ok(state
        .page_contexts()?
        .pages
        .into_iter()
        .filter(|page| page.sequence > request.after)
        .filter(|page| {
            request
                .opener_target_id
                .is_none_or(|opener| page.opener_target_id == Some(opener))
        })
        .min_by_key(|page| page.sequence))
}

async fn execute_non_local_operation(
    page_control: &mut PageControl,
    state: &mut SupervisorState,
    transport: Arc<dyn CdpTransport>,
    shared: &Arc<SessionShared>,
    request: BrowserOperationRequest,
    cancellation: &OperationCancellation,
    context: OperationExecutionContext,
) -> Result<BrowserOperationResult> {
    if request.kind().is_interaction() {
        let interaction_id = page_control.next_interaction_id();
        let target_id = match request.scope() {
            krometrail_core::BrowserOperationScope::Page(selection) => {
                crate::control::bind_target(state, selection)
                    .map(|bound| bound.target_id)
                    .map_err(|mut error| {
                        error.context.session_id = Some(page_control.session_id());
                        error.context.interaction_id = Some(interaction_id);
                        error
                    })?
            }
            krometrail_core::BrowserOperationScope::Browser => {
                return Err(stable_error(
                    ErrorCode::Unsupported,
                    "interaction requires a browser page target",
                ));
            }
        };
        let dispatch_baselines = || crate::control::InteractionDispatchBaselines {
            page_cursor_before: state.page_contexts().ok().map(|inventory| inventory.cursor),
            // The download cursor is absent only when the session does not
            // manage downloads; that delta then stays unobserved.
            download_cursor_before: shared
                .downloads
                .as_ref()
                .and_then(|control| control.cursor()),
        };
        let (mut result, observed_visibility, baselines) = page_control
            .execute_interaction_request(
                transport.as_ref(),
                shared.browser_events.as_ref(),
                state,
                request,
                cancellation,
                context,
                interaction_id,
                &dispatch_baselines,
            )
            .await?;
        attach_side_channel_facts(
            &mut result,
            state,
            &transport,
            shared,
            baselines,
            context.deadline,
        )
        .await;
        finalize_expectation_note(&mut result);
        if let Some(visibility) = observed_visibility {
            commit_observed_visibility(
                state,
                target_id,
                visibility,
                Arc::clone(&transport),
                shared,
            )
            .await?;
        }
        return Ok(result);
    }
    match request {
        BrowserOperationRequest::CreatePage(request) => {
            let started_at = page_control.session_time()?;
            let dispatched_at = page_control.session_time()?;
            let response = transport
                .send_raw(
                    &CommandScope::Browser,
                    "Target.createTarget",
                    create_target_params(&request, page_control.focus()),
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
            if let Err(error) =
                activate_target_if_foreground(transport.as_ref(), &target_key, page_control.focus())
                    .await
            {
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
            if let Err(error) =
                activate_target_if_foreground(transport.as_ref(), &target_key, page_control.focus())
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
        BrowserOperationRequest::ActivatePage(request) => {
            let selection = request
                .target
                .map_or(PageSelection::Selected, PageSelection::Target);
            let bound = crate::control::bind_target(state, selection)?;
            cancellation.check(state.connection_generation, bound.target_id)?;
            let started_at = page_control.session_time()?;
            let interaction_id = page_control.next_interaction_id();
            let dispatched_at = page_control.session_time()?;
            let visibility = match page_control
                .activate_target(
                    transport.as_ref(),
                    &bound,
                    cancellation,
                    state.connection_generation,
                )
                .await
            {
                Ok(visibility) => visibility,
                Err(error) => {
                    return page_failure_result(
                        page_control,
                        bound.target_id,
                        krometrail_core::BrowserOperationKind::ActivatePage,
                        interaction_id,
                        started_at,
                        dispatched_at,
                        error,
                    );
                }
            };
            if let Err(error) = commit_observed_visibility(
                state,
                bound.target_id,
                visibility,
                Arc::clone(&transport),
                shared,
            )
            .await
            {
                return page_failure_result(
                    page_control,
                    bound.target_id,
                    krometrail_core::BrowserOperationKind::ActivatePage,
                    interaction_id,
                    started_at,
                    dispatched_at,
                    error,
                );
            }
            page_success_result(
                page_control,
                transport.as_ref(),
                state,
                bound.target_id,
                krometrail_core::BrowserOperationKind::ActivatePage,
                interaction_id,
                started_at,
                dispatched_at,
                PageChange::Activated {
                    target_id: bound.target_id,
                },
                selection,
                cancellation,
            )
            .await
            .map(|result| BrowserOperationResult::ActivatePage(Box::new(result)))
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
        BrowserOperationRequest::SetViewport(request) => {
            let target = state.resolve_selection(request.target)?;
            let target_key = target.target.target.browser_target_key().to_owned();
            let target_id = target.target.target.id();
            let previous = target.viewport_override;
            let materialization = request.viewport.materialize();
            let requested = materialization.metrics;
            let bound = crate::control::bind_target(state, request.target)?;
            let started_at = page_control.session_time()?;
            let interaction_id = page_control.next_interaction_id();
            let dispatched_at = page_control.session_time()?;
            let geometry_transition = shared.capture.as_ref().and_then(|capture| {
                capture
                    .coordinator
                    .begin_geometry_transition(target_id, bound.attachment_generation)
            });
            let applied = cancellation
                .race(
                    state.connection_generation,
                    target_id,
                    crate::control::viewport::apply_viewport(transport.as_ref(), &bound, requested),
                )
                .await
                .and_then(|result| result);
            if let Err(error) = applied {
                rollback_viewport_or_fail_target(
                    state,
                    shared,
                    Arc::clone(&transport),
                    &bound,
                    &target_key,
                    previous,
                    geometry_transition,
                )
                .await;
                return viewport_failure_result(
                    page_control,
                    target_id,
                    interaction_id,
                    started_at,
                    dispatched_at,
                    error,
                    materialization,
                );
            }
            let effective = match crate::control::viewport::observe_effective_viewport(
                transport.as_ref(),
                &bound,
                requested,
            )
            .await
            {
                Ok(effective) => effective,
                Err(error) => {
                    rollback_viewport_or_fail_target(
                        state,
                        shared,
                        Arc::clone(&transport),
                        &bound,
                        &target_key,
                        previous,
                        geometry_transition,
                    )
                    .await;
                    return viewport_failure_result(
                        page_control,
                        target_id,
                        interaction_id,
                        started_at,
                        dispatched_at,
                        error,
                        materialization,
                    );
                }
            };
            let capture_geometry =
                match crate::control::viewport::capture_geometry(effective.clone()) {
                    Ok(geometry) => geometry,
                    Err(error) => {
                        rollback_viewport_or_fail_target(
                            state,
                            shared,
                            Arc::clone(&transport),
                            &bound,
                            &target_key,
                            previous,
                            geometry_transition,
                        )
                        .await;
                        return viewport_failure_result(
                            page_control,
                            target_id,
                            interaction_id,
                            started_at,
                            dispatched_at,
                            error,
                            materialization,
                        );
                    }
                };
            if let Err(error) = commit_supervisor_input(
                state,
                SupervisorInput::ViewportOverrideApplied {
                    target_key: target_key.clone(),
                    viewport: requested,
                },
                Arc::clone(&transport),
                shared,
            )
            .await
            {
                rollback_viewport_or_fail_target(
                    state,
                    shared,
                    Arc::clone(&transport),
                    &bound,
                    &target_key,
                    previous,
                    geometry_transition,
                )
                .await;
                return viewport_failure_result(
                    page_control,
                    target_id,
                    interaction_id,
                    started_at,
                    dispatched_at,
                    error,
                    materialization,
                );
            }
            if let Some(capture) = shared.capture.as_ref()
                && let Some(transition) = geometry_transition
            {
                capture
                    .coordinator
                    .commit_geometry_transition(transition, capture_geometry);
            }
            page_control.invalidate_target_snapshot(target_id);
            let observation = page_control
                .observe_after_operation_with_geometry(
                    transport.as_ref(),
                    state,
                    request.target,
                    cancellation,
                    true,
                )
                .await?;
            let outcome = PageOperationOutcome::Succeeded(PageChange::ViewportConfigured {
                override_active: requested.is_some(),
            });
            let operation = build_page_result(
                page_control,
                target_id,
                krometrail_core::BrowserOperationKind::SetViewport,
                interaction_id,
                started_at,
                dispatched_at,
                outcome,
                observation.observation,
            )?;
            let guidance = krometrail_core::viewport_guidance(materialization, &effective);
            Ok(BrowserOperationResult::SetViewport(Box::new(
                ViewportOperationResult {
                    operation,
                    effective: ObservationPart::Available(effective),
                    materialization,
                    guidance,
                },
            )))
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
            let observation = match selected {
                Some(selected) => {
                    let observed = page_control
                        .observe_after_operation(
                            transport.as_ref(),
                            state,
                            PageSelection::Target(selected),
                            cancellation,
                        )
                        .await?;
                    observed.observation
                }
                None => ObservationPart::Unavailable(KrometrailError::new(
                    ErrorCode::NotFound,
                    NonEmptyText::new("no browser page remains selected after closure").unwrap(),
                )),
            };
            let outcome = PageOperationOutcome::Succeeded(PageChange::Closed {
                closed: target_id,
                selected,
            });
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

async fn activate_target_if_foreground(
    transport: &dyn CdpTransport,
    target_key: &str,
    focus: krometrail_core::BrowserFocusPolicy,
) -> std::result::Result<(), TransportError> {
    if focus == krometrail_core::BrowserFocusPolicy::Foreground {
        transport
            .send_raw(
                &CommandScope::Browser,
                "Target.activateTarget",
                serde_json::json!({"targetId": target_key}),
            )
            .await?;
    }
    Ok(())
}

async fn commit_observed_visibility(
    state: &mut SupervisorState,
    target_id: krometrail_core::TargetId,
    visibility: krometrail_core::TargetVisibility,
    transport: Arc<dyn CdpTransport>,
    shared: &Arc<SessionShared>,
) -> Result<()> {
    let target_key = state
        .targets_by_key
        .iter()
        .find(|(_, target)| target.target.target.id() == target_id)
        .map(|(key, _)| key.clone())
        .ok_or_else(|| {
            operation_error(
                ErrorCode::TargetFailed,
                target_id,
                "target is no longer supervised",
            )
        })?;
    commit_supervisor_input(
        state,
        SupervisorInput::VisibilityChanged {
            target_key,
            visibility,
            observed_at: shared
                .browser_events
                .session_time()
                .unwrap_or(krometrail_core::SessionTime::ZERO),
        },
        transport,
        shared,
    )
    .await
}

fn create_target_params(
    request: &krometrail_core::CreatePageRequest,
    focus: krometrail_core::BrowserFocusPolicy,
) -> Value {
    let mut params = serde_json::json!({
        "url": request
            .initial_url
            .as_ref()
            .map_or("about:blank", |url| url.as_str())
    });
    if focus == krometrail_core::BrowserFocusPolicy::Preserve {
        // CDP otherwise defaults `background` to false and may focus both the new tab and Chrome's
        // window. Preserve mode keeps the tab in the visible browser UI without foregrounding it.
        params["background"] = Value::Bool(true);
    }
    params
}

async fn rollback_viewport_or_fail_target(
    state: &mut SupervisorState,
    shared: &Arc<SessionShared>,
    transport: Arc<dyn CdpTransport>,
    bound: &crate::control::BoundTarget,
    target_key: &str,
    previous: Option<krometrail_core::ViewportMetrics>,
    geometry_transition: Option<crate::capture::CaptureGeometryTransition>,
) {
    let restored_geometry = async {
        crate::control::viewport::apply_viewport(transport.as_ref(), bound, previous).await?;
        let effective = crate::control::viewport::observe_effective_viewport(
            transport.as_ref(),
            bound,
            previous,
        )
        .await?;
        crate::control::viewport::capture_geometry(effective)
    }
    .await;
    if let Ok(geometry) = restored_geometry {
        if let (Some(capture), Some(transition)) = (shared.capture.as_ref(), geometry_transition) {
            capture
                .coordinator
                .commit_geometry_transition(transition, geometry);
        }
        return;
    }
    tracing::error!(
        event = "viewport_rollback_failed",
        target_id = %bound.target_id,
        "viewport rollback failed; terminating affected target"
    );
    let _ = commit_supervisor_input(
        state,
        SupervisorInput::TargetAttachFailed {
            target_key: target_key.to_owned(),
        },
        transport,
        shared,
    )
    .await;
}

fn viewport_failure_result(
    page_control: &PageControl,
    target_id: krometrail_core::TargetId,
    interaction_id: krometrail_core::InteractionId,
    started_at: krometrail_core::SessionTime,
    dispatched_at: krometrail_core::SessionTime,
    error: KrometrailError,
    materialization: krometrail_core::ViewportMaterialization,
) -> Result<BrowserOperationResult> {
    let operation = build_page_result(
        page_control,
        target_id,
        krometrail_core::BrowserOperationKind::SetViewport,
        interaction_id,
        started_at,
        dispatched_at,
        PageOperationOutcome::Failed(error.clone()),
        ObservationPart::Unavailable(error.clone()),
    )?;
    let effective_error = match &operation.outcome {
        PageOperationOutcome::Failed(error) => error.clone(),
        PageOperationOutcome::Succeeded(_) => unreachable!("viewport failure has failed outcome"),
    };
    Ok(BrowserOperationResult::SetViewport(Box::new(
        ViewportOperationResult {
            operation,
            effective: ObservationPart::Unavailable(effective_error),
            materialization,
            guidance: Vec::new(),
        },
    )))
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
    build_page_result(
        page_control,
        target_id,
        operation,
        interaction_id,
        started_at,
        dispatched_at,
        PageOperationOutcome::Succeeded(change),
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
        krometrail_core::BrowserOperationKind::ActivatePage => {
            BrowserOperationResult::ActivatePage(Box::new(result))
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

#[cfg(test)]
mod focus_policy_tests {
    use std::sync::Mutex;

    use super::*;
    use crate::transport::{TransportClose, TransportEvents, TransportFuture};

    #[derive(Default)]
    struct RecordingTransport(Mutex<Vec<String>>);

    impl CdpTransport for RecordingTransport {
        fn send_raw(
            &self,
            _scope: &CommandScope,
            method: &str,
            _params: Value,
        ) -> TransportFuture<'_, std::result::Result<Value, TransportError>> {
            self.0.lock().unwrap().push(method.to_owned());
            Box::pin(std::future::ready(Ok(serde_json::json!({}))))
        }

        fn subscribe_named(
            &self,
            _scope: &CommandScope,
            _method: &str,
        ) -> TransportFuture<'_, std::result::Result<Box<dyn TransportEvents>, TransportError>>
        {
            unreachable!("activation tests do not subscribe")
        }

        fn close_reason(&self) -> Option<TransportClose> {
            None
        }

        fn is_closed(&self) -> bool {
            false
        }
    }

    #[tokio::test]
    async fn managed_page_selection_activation_obeys_the_immutable_focus_policy() {
        let foreground = RecordingTransport::default();
        activate_target_if_foreground(
            &foreground,
            "target-a",
            krometrail_core::BrowserFocusPolicy::Foreground,
        )
        .await
        .unwrap();
        assert_eq!(*foreground.0.lock().unwrap(), vec!["Target.activateTarget"]);

        let preserve = RecordingTransport::default();
        activate_target_if_foreground(
            &preserve,
            "target-a",
            krometrail_core::BrowserFocusPolicy::Preserve,
        )
        .await
        .unwrap();
        assert!(preserve.0.lock().unwrap().is_empty());
    }

    #[test]
    fn managed_page_creation_obeys_the_immutable_focus_policy() {
        let request =
            krometrail_core::CreatePageRequest::new(Some("https://example.test/")).unwrap();

        assert_eq!(
            create_target_params(&request, krometrail_core::BrowserFocusPolicy::Foreground),
            serde_json::json!({"url":"https://example.test/"})
        );
        assert_eq!(
            create_target_params(&request, krometrail_core::BrowserFocusPolicy::Preserve),
            serde_json::json!({"url":"https://example.test/","background":true})
        );
    }
}
