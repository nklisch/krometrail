use std::{
    future::Future,
    sync::{Arc, atomic::{AtomicBool, AtomicU64, Ordering}},
    time::Duration,
};

use krometrail_core::{
    BrowserOperationKind, BrowserOperationResult, ErrorCode, ErrorContext, GoBackRequest,
    GoForwardRequest, InteractionAnchor, InteractionTiming, KrometrailError, NavigatePageRequest,
    NonEmptyText, ObservationPart, PageChange, PageOperationOutcome, PageOperationResult,
    PageSelection, ReloadPageRequest, Result, RetryAdvice, SessionTime, TargetId,
};
use serde_json::{Value, json};
use tokio::sync::Notify;

use super::{PageControl, bind_target, operation_error, transport_error};
use crate::{SupervisorState, transport::{CdpTransport, CommandScope}};

#[derive(Clone, Debug)]
pub(crate) struct NavigationConfig {
    pub(crate) commit_timeout: Duration,
    pub(crate) poll_interval: Duration,
}

impl Default for NavigationConfig {
    fn default() -> Self {
        Self {
            commit_timeout: Duration::from_secs(5),
            poll_interval: Duration::from_millis(25),
        }
    }
}

#[derive(Clone, Default)]
pub(crate) struct OperationCancellation {
    stopped: Arc<AtomicBool>,
    disconnected_generation: Arc<AtomicU64>,
    notify: Arc<Notify>,
}

impl OperationCancellation {
    pub(crate) fn stop(&self) {
        self.stopped.store(true, Ordering::Release);
        self.notify.notify_waiters();
    }

    pub(crate) fn disconnect(&self, generation: u64) {
        self.disconnected_generation
            .fetch_max(generation.saturating_add(1), Ordering::AcqRel);
        self.notify.notify_waiters();
    }

    fn verdict(&self, generation: u64, target_id: TargetId) -> Option<KrometrailError> {
        if self.stopped.load(Ordering::Acquire) {
            return Some(operation_error(
                ErrorCode::Cancelled,
                target_id,
                "browser operation was cancelled by session shutdown",
            ));
        }
        if self.disconnected_generation.load(Ordering::Acquire) >= generation.saturating_add(1) {
            return Some(operation_error(
                ErrorCode::BrowserDisconnected,
                target_id,
                "browser disconnected during page navigation",
            ));
        }
        None
    }

    async fn wait(&self, generation: u64, target_id: TargetId) -> KrometrailError {
        loop {
            if let Some(error) = self.verdict(generation, target_id) {
                return error;
            }
            let notified = self.notify.notified();
            if let Some(error) = self.verdict(generation, target_id) {
                return error;
            }
            notified.await;
        }
    }

    pub(crate) async fn race<F, T>(
        &self,
        generation: u64,
        target_id: TargetId,
        future: F,
    ) -> std::result::Result<T, KrometrailError>
    where
        F: Future<Output = T>,
    {
        tokio::select! {
            biased;
            error = self.wait(generation, target_id) => Err(error),
            value = future => Ok(value),
        }
    }
}

#[derive(Clone, Debug)]
struct DocumentState {
    loader_id: String,
    url: String,
    history_index: u32,
    history_entries: Vec<(i64, String)>,
}

#[derive(Clone, Copy)]
enum Direction { Back, Forward }

impl PageControl {
    pub(crate) async fn navigate(
        &mut self,
        transport: &dyn CdpTransport,
        state: &SupervisorState,
        request: NavigatePageRequest,
        cancel: &OperationCancellation,
    ) -> Result<BrowserOperationResult> {
        let bound = bind_target(state, request.target)?;
        let before = read_document(transport, &bound.transport_session, bound.target_id).await?;
        let started = self.session_time()?;
        let interaction_id = self.next_interaction_id();
        let dispatched = self.session_time()?;
        let response = match cancel
            .race(
                state.connection_generation,
                bound.target_id,
                transport.send_raw(
                    &CommandScope::Session(bound.transport_session.clone()),
                    "Page.navigate",
                    json!({"url": request.url.as_str()}),
                ),
            )
            .await
        {
            Ok(Ok(response)) => response,
            Ok(Err(error)) => {
                return self.navigation_failure(
                    transport, state, request.target, bound.target_id,
                    BrowserOperationKind::NavigatePage, interaction_id, started, dispatched,
                    transport_error(error, ErrorCode::NavigationFailed, bound.target_id), false,
                ).await;
            }
            Err(error) => {
                return self.navigation_failure(
                    transport, state, request.target, bound.target_id,
                    BrowserOperationKind::NavigatePage, interaction_id, started, dispatched,
                    error, false,
                ).await;
            }
        };
        if response.get("errorText").and_then(Value::as_str).is_some_and(|text| !text.is_empty()) {
            return self.navigation_failure(
                transport, state, request.target, bound.target_id,
                BrowserOperationKind::NavigatePage, interaction_id, started, dispatched,
                navigation_error(bound.target_id, "browser rejected page navigation"), false,
            ).await;
        }
        self.invalidate_target_snapshot(bound.target_id);
        let expected_loader = response.get("loaderId").and_then(Value::as_str).filter(|v| !v.is_empty()).map(str::to_owned);
        let committed = self.await_commit(
            transport, &bound.transport_session, bound.target_id, state.connection_generation,
            cancel, |current| expected_loader.as_ref().map_or_else(
                || current.loader_id != before.loader_id || current.url != before.url || current.history_index != before.history_index,
                |loader| &current.loader_id == loader,
            )
        ).await;
        match committed {
            Ok(()) => self.navigation_success(
                transport, state, request.target, bound.target_id, BrowserOperationKind::NavigatePage,
                interaction_id, started, dispatched, PageChange::Navigated,
            ).await,
            Err(error) => self.navigation_failure(
                transport, state, request.target, bound.target_id, BrowserOperationKind::NavigatePage,
                interaction_id, started, dispatched, error, true,
            ).await,
        }
    }

    pub(crate) async fn reload(
        &mut self,
        transport: &dyn CdpTransport,
        state: &SupervisorState,
        request: ReloadPageRequest,
        cancel: &OperationCancellation,
    ) -> Result<BrowserOperationResult> {
        let bound = bind_target(state, request.target)?;
        let before = read_document(transport, &bound.transport_session, bound.target_id).await?;
        let started = self.session_time()?;
        let interaction_id = self.next_interaction_id();
        let dispatched = self.session_time()?;
        let command = cancel.race(
            state.connection_generation,
            bound.target_id,
            transport.send_raw(
                &CommandScope::Session(bound.transport_session.clone()),
                "Page.reload",
                json!({"ignoreCache": request.bypass_cache}),
            ),
        ).await;
        match command {
            Ok(Ok(_)) => {}
            Ok(Err(error)) => return self.navigation_failure(
                transport, state, request.target, bound.target_id, BrowserOperationKind::ReloadPage,
                interaction_id, started, dispatched,
                transport_error(error, ErrorCode::NavigationFailed, bound.target_id), false,
            ).await,
            Err(error) => return self.navigation_failure(
                transport, state, request.target, bound.target_id, BrowserOperationKind::ReloadPage,
                interaction_id, started, dispatched, error, false,
            ).await,
        }
        self.invalidate_target_snapshot(bound.target_id);
        match self.await_commit(
            transport, &bound.transport_session, bound.target_id, state.connection_generation,
            cancel, |current| current.loader_id != before.loader_id,
        ).await {
            Ok(()) => self.navigation_success(
                transport, state, request.target, bound.target_id, BrowserOperationKind::ReloadPage,
                interaction_id, started, dispatched, PageChange::Reloaded,
            ).await,
            Err(error) => self.navigation_failure(
                transport, state, request.target, bound.target_id, BrowserOperationKind::ReloadPage,
                interaction_id, started, dispatched, error, true,
            ).await,
        }
    }

    pub(crate) async fn go_back(
        &mut self,
        transport: &dyn CdpTransport,
        state: &SupervisorState,
        request: GoBackRequest,
        cancel: &OperationCancellation,
    ) -> Result<BrowserOperationResult> {
        self.history(transport, state, request.target, Direction::Back, cancel).await
    }

    pub(crate) async fn go_forward(
        &mut self,
        transport: &dyn CdpTransport,
        state: &SupervisorState,
        request: GoForwardRequest,
        cancel: &OperationCancellation,
    ) -> Result<BrowserOperationResult> {
        self.history(transport, state, request.target, Direction::Forward, cancel).await
    }

    async fn history(
        &mut self,
        transport: &dyn CdpTransport,
        state: &SupervisorState,
        selection: PageSelection,
        direction: Direction,
        cancel: &OperationCancellation,
    ) -> Result<BrowserOperationResult> {
        let bound = bind_target(state, selection)?;
        let before = read_document(transport, &bound.transport_session, bound.target_id).await?;
        let expected_index = match direction {
            Direction::Back => before.history_index.checked_sub(1),
            Direction::Forward => before.history_index.checked_add(1).filter(|index| usize::try_from(*index).ok().is_some_and(|index| index < before.history_entries.len())),
        }.ok_or_else(|| operation_error(ErrorCode::InvalidInput, bound.target_id, "page history has no entry in the requested direction"))?;
        let entry_id = before.history_entries
            .get(usize::try_from(expected_index).map_err(|_| operation_error(ErrorCode::InvalidInput, bound.target_id, "page history index is invalid"))?)
            .map(|entry| entry.0)
            .ok_or_else(|| operation_error(ErrorCode::InvalidInput, bound.target_id, "page history entry is unavailable"))?;
        let (kind, change) = match direction {
            Direction::Back => (BrowserOperationKind::GoBack, PageChange::WentBack),
            Direction::Forward => (BrowserOperationKind::GoForward, PageChange::WentForward),
        };
        let started = self.session_time()?;
        let interaction_id = self.next_interaction_id();
        let dispatched = self.session_time()?;
        let command = cancel.race(
            state.connection_generation,
            bound.target_id,
            transport.send_raw(
                &CommandScope::Session(bound.transport_session.clone()),
                "Page.navigateToHistoryEntry",
                json!({"entryId": entry_id}),
            ),
        ).await;
        match command {
            Ok(Ok(_)) => {}
            Ok(Err(error)) => return self.navigation_failure(
                transport, state, selection, bound.target_id, kind, interaction_id, started,
                dispatched, transport_error(error, ErrorCode::NavigationFailed, bound.target_id), false,
            ).await,
            Err(error) => return self.navigation_failure(
                transport, state, selection, bound.target_id, kind, interaction_id, started,
                dispatched, error, false,
            ).await,
        }
        self.invalidate_target_snapshot(bound.target_id);
        match self.await_commit(
            transport, &bound.transport_session, bound.target_id, state.connection_generation,
            cancel, |current| current.history_index == expected_index,
        ).await {
            Ok(()) => self.navigation_success(
                transport, state, selection, bound.target_id, kind, interaction_id, started,
                dispatched, change,
            ).await,
            Err(error) => self.navigation_failure(
                transport, state, selection, bound.target_id, kind, interaction_id, started,
                dispatched, error, true,
            ).await,
        }
    }

    async fn await_commit(
        &self,
        transport: &dyn CdpTransport,
        session: &crate::transport::TransportSessionId,
        target_id: TargetId,
        connection_generation: u64,
        cancel: &OperationCancellation,
        committed: impl Fn(&DocumentState) -> bool,
    ) -> Result<()> {
        let deadline = tokio::time::Instant::now() + self.navigation.commit_timeout;
        loop {
            if tokio::time::Instant::now() >= deadline {
                return Err(navigation_error(target_id, "page navigation did not commit before its deadline"));
            }
            let current = cancel.race(
                connection_generation,
                target_id,
                read_document(transport, session, target_id),
            ).await?;
            match current {
                Ok(current) if committed(&current) => return Ok(()),
                Ok(_) => {}
                Err(error) if error.code == ErrorCode::BrowserDisconnected => return Err(error),
                Err(_) => return Err(navigation_error(target_id, "page navigation state could not be observed")),
            }
            let wake = tokio::time::sleep_until((tokio::time::Instant::now() + self.navigation.poll_interval).min(deadline));
            cancel.race(connection_generation, target_id, wake).await?;
        }
    }

    #[allow(clippy::too_many_arguments)]
    async fn navigation_success(
        &mut self,
        transport: &dyn CdpTransport,
        state: &SupervisorState,
        selection: PageSelection,
        target_id: TargetId,
        kind: BrowserOperationKind,
        interaction_id: krometrail_core::InteractionId,
        started: SessionTime,
        dispatched: SessionTime,
        change: PageChange,
    ) -> Result<BrowserOperationResult> {
        let observation = self.observe_after_operation(transport, state, selection).await?;
        let result = self.navigation_result(target_id, kind, interaction_id, started, dispatched, PageOperationOutcome::Succeeded(change), observation)?;
        Ok(wrap_result(kind, result))
    }

    #[allow(clippy::too_many_arguments)]
    async fn navigation_failure(
        &mut self,
        transport: &dyn CdpTransport,
        state: &SupervisorState,
        selection: PageSelection,
        target_id: TargetId,
        kind: BrowserOperationKind,
        interaction_id: krometrail_core::InteractionId,
        started: SessionTime,
        dispatched: SessionTime,
        error: KrometrailError,
        mutation_accepted: bool,
    ) -> Result<BrowserOperationResult> {
        let observation = if mutation_accepted && !matches!(error.code, ErrorCode::Cancelled | ErrorCode::BrowserDisconnected) {
            self.observe_after_operation(transport, state, selection).await?
        } else {
            ObservationPart::Unavailable(error.clone())
        };
        let result = self.navigation_result(target_id, kind, interaction_id, started, dispatched, PageOperationOutcome::Failed(error), observation)?;
        Ok(wrap_result(kind, result))
    }

    #[allow(clippy::too_many_arguments)]
    fn navigation_result(
        &self,
        target_id: TargetId,
        kind: BrowserOperationKind,
        interaction_id: krometrail_core::InteractionId,
        started: SessionTime,
        dispatched: SessionTime,
        outcome: PageOperationOutcome,
        observation: ObservationPart<krometrail_core::LiveObservation>,
    ) -> Result<PageOperationResult> {
        let (completed, observed) = match &observation {
            ObservationPart::Available(observation) => (observation.context.started_at, Some(observation.context.completed_at)),
            ObservationPart::Unavailable(_) => (self.session_time()?, None),
        };
        let timing = InteractionTiming::new(started, dispatched, completed, observed)?;
        let anchor = InteractionAnchor::new(interaction_id, self.session_id, target_id, kind, timing)?;
        let outcome = match outcome {
            PageOperationOutcome::Failed(mut error) => {
                error.context = ErrorContext {
                    session_id: Some(self.session_id),
                    target_id: Some(target_id),
                    interaction_id: Some(interaction_id),
                    range: error.context.range,
                };
                PageOperationOutcome::Failed(error)
            }
            outcome => outcome,
        };
        PageOperationResult::new(anchor, outcome, observation)
    }
}

async fn read_document(
    transport: &dyn CdpTransport,
    session: &crate::transport::TransportSessionId,
    target_id: TargetId,
) -> Result<DocumentState> {
    let scope = CommandScope::Session(session.clone());
    let frame = transport.send_raw(&scope, "Page.getFrameTree", json!({})).await
        .map_err(|error| transport_error(error, ErrorCode::NavigationFailed, target_id))?;
    let frame = frame.pointer("/frameTree/frame").or_else(|| frame.pointer("/result/frameTree/frame"))
        .ok_or_else(|| navigation_error(target_id, "main frame response is malformed"))?;
    let loader_id = frame.get("loaderId").and_then(Value::as_str).unwrap_or_default().to_owned();
    let url = frame.get("url").and_then(Value::as_str).unwrap_or_default().to_owned();
    let history = transport.send_raw(&scope, "Page.getNavigationHistory", json!({})).await
        .map_err(|error| transport_error(error, ErrorCode::NavigationFailed, target_id))?;
    let history = history.get("result").filter(|value| value.get("entries").is_some()).unwrap_or(&history);
    let history_index = history.get("currentIndex").and_then(Value::as_u64)
        .and_then(|value| u32::try_from(value).ok())
        .ok_or_else(|| navigation_error(target_id, "navigation history index is malformed"))?;
    let history_entries = history.get("entries").and_then(Value::as_array)
        .ok_or_else(|| navigation_error(target_id, "navigation history entries are malformed"))?
        .iter().map(|entry| {
            let id = entry.get("id").and_then(Value::as_i64)
                .ok_or_else(|| navigation_error(target_id, "navigation history entry id is malformed"))?;
            let url = entry.get("url").and_then(Value::as_str).unwrap_or_default().to_owned();
            Ok((id, url))
        }).collect::<Result<Vec<_>>>()?;
    if usize::try_from(history_index).ok().is_none_or(|index| index >= history_entries.len()) {
        return Err(navigation_error(target_id, "navigation history index is out of range"));
    }
    Ok(DocumentState { loader_id, url, history_index, history_entries })
}

fn navigation_error(target_id: TargetId, message: &'static str) -> KrometrailError {
    operation_error(ErrorCode::NavigationFailed, target_id, message)
        .with_retry(RetryAdvice::Safe)
        .with_recovery(NonEmptyText::new("inspect current page status before deciding whether to retry").unwrap())
}

fn wrap_result(kind: BrowserOperationKind, result: PageOperationResult) -> BrowserOperationResult {
    match kind {
        BrowserOperationKind::NavigatePage => BrowserOperationResult::NavigatePage(Box::new(result)),
        BrowserOperationKind::ReloadPage => BrowserOperationResult::ReloadPage(Box::new(result)),
        BrowserOperationKind::GoBack => BrowserOperationResult::GoBack(Box::new(result)),
        BrowserOperationKind::GoForward => BrowserOperationResult::GoForward(Box::new(result)),
        _ => unreachable!("navigation result kind"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn target() -> TargetId {
        TargetId::from_uuid(Uuid::from_u128(1))
    }

    #[tokio::test]
    async fn stop_interrupts_an_in_flight_operation_without_waiting_for_transport() {
        let cancellation = OperationCancellation::default();
        let task = {
            let cancellation = cancellation.clone();
            tokio::spawn(async move {
                cancellation
                    .race(0, target(), std::future::pending::<()>())
                    .await
            })
        };
        tokio::task::yield_now().await;
        cancellation.stop();
        let error = task.await.unwrap().unwrap_err();
        assert_eq!(error.code, ErrorCode::Cancelled);
    }

    #[test]
    fn disconnect_is_generation_aware() {
        let cancellation = OperationCancellation::default();
        cancellation.disconnect(0);
        assert_eq!(
            cancellation.verdict(0, target()).unwrap().code,
            ErrorCode::BrowserDisconnected
        );
        assert!(cancellation.verdict(1, target()).is_none());
    }
}
