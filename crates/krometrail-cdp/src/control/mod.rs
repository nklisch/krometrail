use std::{sync::Arc, time::Duration};

use krometrail_core::{
    BrowserOperationRequest, BrowserOperationResult, BrowserSessionState, CssPoint, CssRect,
    CssSize, DeviceScaleFactor, DocumentReadiness, ErrorCode, ErrorContext, InspectPageRequest,
    KrometrailError, MonotonicClock, NavigationState, NonEmptyText, ObservationContext, PageState,
    Result, RetryAdvice, SessionId, SessionOrigin, SessionTime, TargetId, TargetLifecycle,
    ViewportState,
};
use serde_json::{Value, json};

use crate::{
    SupervisorState,
    transport::{CdpTransport, CommandScope, TransportError, TransportSessionId},
};

mod evaluation;
mod snapshot;

use snapshot::SnapshotRegistry;

#[derive(Clone, Debug)]
pub(crate) struct PageControlConfig {
    pub(crate) evaluation_timeout: Duration,
}

impl Default for PageControlConfig {
    fn default() -> Self {
        Self {
            evaluation_timeout: Duration::from_secs(2),
        }
    }
}

pub(crate) struct PageControl {
    pub(crate) clock: Arc<dyn MonotonicClock>,
    pub(crate) session_id: SessionId,
    pub(crate) session_origin: SessionOrigin,
    pub(crate) config: PageControlConfig,
    pub(crate) snapshots: SnapshotRegistry,
}

#[derive(Clone, Debug)]
pub(crate) struct BoundTarget {
    pub(crate) target_id: TargetId,
    pub(crate) attachment_generation: u64,
    pub(crate) transport_session: TransportSessionId,
}

impl PageControl {
    pub(crate) fn new(
        clock: Arc<dyn MonotonicClock>,
        session_id: SessionId,
        session_origin: SessionOrigin,
    ) -> Self {
        Self {
            clock,
            session_id,
            session_origin,
            config: PageControlConfig::default(),
            snapshots: SnapshotRegistry::default(),
        }
    }

    pub(crate) async fn execute(
        &mut self,
        transport: &dyn CdpTransport,
        state: &SupervisorState,
        request: BrowserOperationRequest,
    ) -> Result<BrowserOperationResult> {
        self.snapshots.retain_targets(
            state
                .targets_by_key
                .values()
                .filter(|target| target.target.lifecycle == TargetLifecycle::Attached)
                .map(|target| target.target.target.id()),
        );
        let target_id = request.target_id();
        let bound = bind_target(state, target_id)?;
        let started_at = self.session_time()?;
        match request {
            BrowserOperationRequest::InspectPage(request) => {
                self.inspect(transport, &bound, request, started_at).await
            }
            BrowserOperationRequest::EvaluatePage(request) => {
                self.evaluate(transport, &bound, request, started_at).await
            }
            BrowserOperationRequest::SnapshotPage(request) => {
                self.snapshot(transport, &bound, request, started_at).await
            }
            BrowserOperationRequest::TakeScreenshot(_)
            | BrowserOperationRequest::ObserveLive(_) => Err(operation_error(
                ErrorCode::Unsupported,
                target_id,
                "this page observation operation is not available yet",
            )),
        }
    }

    pub(crate) fn session_time(&self) -> Result<SessionTime> {
        self.session_origin.normalize(self.clock.now())
    }

    async fn inspect(
        &self,
        transport: &dyn CdpTransport,
        bound: &BoundTarget,
        _request: InspectPageRequest,
        started_at: SessionTime,
    ) -> Result<BrowserOperationResult> {
        let scope = CommandScope::Session(bound.transport_session.clone());
        let identity = transport.send_raw(
            &scope,
            "Runtime.evaluate",
            json!({
                "expression": "({url:location.href,title:document.title,readiness:document.readyState,deviceScaleFactor:window.devicePixelRatio})",
                "returnByValue": true,
                "throwOnSideEffect": true,
                "silent": true,
            }),
        ).await.map_err(|error| transport_error(error, ErrorCode::PageObservationFailed, bound.target_id))?;
        let layout = transport
            .send_raw(&scope, "Page.getLayoutMetrics", json!({}))
            .await
            .map_err(|error| {
                transport_error(error, ErrorCode::PageObservationFailed, bound.target_id)
            })?;
        let history = transport
            .send_raw(&scope, "Page.getNavigationHistory", json!({}))
            .await
            .map_err(|error| {
                transport_error(error, ErrorCode::PageObservationFailed, bound.target_id)
            })?;
        let completed_at = self.session_time()?;
        let context = ObservationContext::new(
            self.session_id,
            bound.target_id,
            bound.attachment_generation,
            started_at,
            completed_at,
        )?;
        let state = decode_page_state(context, &identity, &layout, &history, bound.target_id)?;
        Ok(BrowserOperationResult::InspectPage(state))
    }
}

pub(crate) fn bind_target(state: &SupervisorState, target_id: TargetId) -> Result<BoundTarget> {
    match state.session_state {
        BrowserSessionState::Ready => {}
        BrowserSessionState::Reconnecting | BrowserSessionState::Connecting => {
            return Err(operation_error(
                ErrorCode::BrowserDisconnected,
                target_id,
                "browser session is not ready for page observation",
            ));
        }
        BrowserSessionState::Stopping | BrowserSessionState::Ended => {
            return Err(operation_error(
                ErrorCode::Cancelled,
                target_id,
                "browser session is stopping or ended",
            ));
        }
    }
    let target = state
        .targets_by_key
        .values()
        .find(|candidate| candidate.target.target.id() == target_id)
        .ok_or_else(|| {
            operation_error(
                ErrorCode::NotFound,
                target_id,
                "browser target was not found",
            )
        })?;
    if target.target.lifecycle != TargetLifecycle::Attached {
        return Err(operation_error(
            ErrorCode::TargetFailed,
            target_id,
            "browser target is not currently attached",
        ));
    }
    let transport_session = target.transport_session.clone().ok_or_else(|| {
        operation_error(
            ErrorCode::TargetFailed,
            target_id,
            "browser target has no active flat session",
        )
    })?;
    Ok(BoundTarget {
        target_id,
        attachment_generation: target.target.attachment_generation,
        transport_session,
    })
}

fn decode_page_state(
    context: ObservationContext,
    identity: &Value,
    layout: &Value,
    history: &Value,
    target_id: TargetId,
) -> Result<PageState> {
    let identity = identity
        .pointer("/result/value")
        .or_else(|| identity.pointer("/result/result/value"))
        .ok_or_else(|| malformed(target_id, "page identity response is malformed"))?;
    let url = identity
        .get("url")
        .and_then(Value::as_str)
        .ok_or_else(|| malformed(target_id, "page URL is missing"))?;
    let title = identity
        .get("title")
        .and_then(Value::as_str)
        .ok_or_else(|| malformed(target_id, "page title is missing"))?;
    let readiness = match identity.get("readiness").and_then(Value::as_str) {
        Some("loading") => DocumentReadiness::Loading,
        Some("interactive") => DocumentReadiness::Interactive,
        Some("complete") => DocumentReadiness::Complete,
        _ => return Err(malformed(target_id, "document readiness is invalid")),
    };
    let device_scale = identity
        .get("deviceScaleFactor")
        .and_then(Value::as_f64)
        .ok_or_else(|| malformed(target_id, "device scale factor is missing"))?;
    let layout_root = layout
        .get("result")
        .filter(|value| value.get("cssLayoutViewport").is_some())
        .unwrap_or(layout);
    let layout_viewport = rect_from_viewport(
        layout_root.get("cssLayoutViewport"),
        "layout viewport",
        target_id,
    )?;
    let visual = layout_root
        .get("cssVisualViewport")
        .ok_or_else(|| malformed(target_id, "visual viewport is missing"))?;
    let visual_origin = CssPoint::new(
        number(visual, "pageX", target_id)?,
        number(visual, "pageY", target_id)?,
    )?;
    let visual_viewport = CssRect::new(
        visual_origin,
        CssSize::new(
            number(visual, "clientWidth", target_id)?,
            number(visual, "clientHeight", target_id)?,
        )?,
    )?;
    let content = layout_root
        .get("cssContentSize")
        .ok_or_else(|| malformed(target_id, "content size is missing"))?;
    let content_size = CssSize::new(
        number(content, "width", target_id)?,
        number(content, "height", target_id)?,
    )?;
    let page_scale_factor = number(visual, "scale", target_id)
        .or_else(|_| number(visual, "pageScaleFactor", target_id))?;
    let history_root = history
        .get("result")
        .filter(|value| value.get("entries").is_some())
        .unwrap_or(history);
    let current_index = history_root
        .get("currentIndex")
        .and_then(Value::as_u64)
        .and_then(|value| u32::try_from(value).ok())
        .ok_or_else(|| malformed(target_id, "navigation history index is invalid"))?;
    let entry_count = history_root
        .get("entries")
        .and_then(Value::as_array)
        .and_then(|entries| u32::try_from(entries.len()).ok())
        .ok_or_else(|| malformed(target_id, "navigation history entries are invalid"))?;
    PageState::new(
        context,
        url,
        title,
        ViewportState::new(
            layout_viewport,
            visual_viewport,
            content_size,
            DeviceScaleFactor::new(device_scale)?,
            page_scale_factor,
        )?,
        NavigationState::new(current_index, entry_count, readiness)?,
    )
}

fn rect_from_viewport(value: Option<&Value>, label: &str, target_id: TargetId) -> Result<CssRect> {
    let value = value.ok_or_else(|| malformed(target_id, format!("{label} is missing")))?;
    CssRect::new(
        CssPoint::new(
            number(value, "pageX", target_id)?,
            number(value, "pageY", target_id)?,
        )?,
        CssSize::new(
            number(value, "clientWidth", target_id)?,
            number(value, "clientHeight", target_id)?,
        )?,
    )
}

pub(crate) fn number(value: &Value, field: &str, target_id: TargetId) -> Result<f64> {
    value
        .get(field)
        .and_then(Value::as_f64)
        .filter(|number| number.is_finite())
        .ok_or_else(|| malformed(target_id, format!("{field} is missing or non-finite")))
}

pub(crate) fn malformed(target_id: TargetId, message: impl Into<String>) -> KrometrailError {
    operation_error(ErrorCode::PageObservationFailed, target_id, message)
}

pub(crate) fn operation_error(
    code: ErrorCode,
    target_id: TargetId,
    message: impl Into<String>,
) -> KrometrailError {
    let mut error = KrometrailError::new(
        code,
        NonEmptyText::new(message.into()).expect("adapter operation errors are non-empty"),
    )
    .with_context(ErrorContext {
        target_id: Some(target_id),
        ..ErrorContext::default()
    })
    .with_retry(code.default_retry());
    if let Some(recovery) = code.default_recovery() {
        error = error
            .with_recovery(NonEmptyText::new(recovery).expect("default recovery is non-empty"));
    } else if code == ErrorCode::BrowserDisconnected {
        error = error.with_retry(RetryAdvice::AfterRecovery).with_recovery(
            NonEmptyText::new("wait for ready status, then repeat the read-only operation")
                .unwrap(),
        );
    }
    error
}

pub(crate) fn transport_error(
    error: TransportError,
    fallback: ErrorCode,
    target_id: TargetId,
) -> KrometrailError {
    let code = if matches!(
        error,
        TransportError::Disconnected | TransportError::Closed | TransportError::SubscriptionClosed
    ) {
        ErrorCode::BrowserDisconnected
    } else {
        fallback
    };
    operation_error(
        code,
        target_id,
        if code == ErrorCode::BrowserDisconnected {
            "browser transport disconnected during page observation"
        } else {
            "browser rejected or could not complete the page observation command"
        },
    )
}

#[cfg(test)]
mod tests;
