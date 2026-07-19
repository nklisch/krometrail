//! Transport-neutral target supervision values.
//!
//! These values intentionally contain only the adapter's validated projection of CDP target
//! information. The reducer below is the sole owner of target identity and lifecycle state.

use krometrail_core::{
    BrowserCompatibility, BrowserSessionEvent, BrowserSessionState, ErrorCode, ErrorContext,
    KrometrailError, NonEmptyText, PageSelection, PageStatus, PageTarget, Result, RetryAdvice,
    SessionTime, SupervisedTarget, TargetId, TargetLifecycle, TargetVisibility, ViewportMetrics,
    browser::{PageContextInventory, PageContextStatus, PageSequence},
};

use crate::transport::{TransportClose, TransportSessionId};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TransportTargetInfo {
    pub target_key: String,
    pub target_type: String,
    pub url: String,
    pub title: String,
    pub attached: bool,
    pub browser_context_key: Option<String>,
    pub opener_target_key: Option<String>,
}

impl TransportTargetInfo {
    pub fn new(
        target_key: impl Into<String>,
        target_type: impl Into<String>,
        url: impl Into<String>,
        title: impl Into<String>,
        attached: bool,
        browser_context_key: Option<String>,
    ) -> Result<Self> {
        let info = Self {
            target_key: target_key.into(),
            target_type: target_type.into(),
            url: url.into(),
            title: title.into(),
            attached,
            browser_context_key,
            opener_target_key: None,
        };
        if info.target_key.trim().is_empty() {
            return Err(krometrail_core::KrometrailError::new(
                krometrail_core::ErrorCode::InvalidInput,
                krometrail_core::NonEmptyText::new("browser target key must not be empty").unwrap(),
            ));
        }
        if info.target_type.trim().is_empty() {
            return Err(krometrail_core::KrometrailError::new(
                krometrail_core::ErrorCode::InvalidInput,
                krometrail_core::NonEmptyText::new("browser target type must not be empty")
                    .unwrap(),
            ));
        }
        Ok(info)
    }

    pub fn with_opener_target_key(mut self, opener_target_key: Option<String>) -> Self {
        self.opener_target_key = opener_target_key;
        self
    }

    pub fn is_recordable(&self) -> bool {
        self.target_type == "page" && !self.url.trim().is_empty() && !is_internal_url(&self.url)
    }
}

/// CDP exposes a number of inspector and browser-internal pages through the same target list.
/// They are intentionally ignored rather than reported as target failures: they are outside the
/// recording boundary and can appear during ordinary Chrome operation.
fn is_internal_url(url: &str) -> bool {
    let lower = url.trim().to_ascii_lowercase();
    [
        "devtools://",
        "chrome://",
        "chrome-extension://",
        "edge://",
        "about:devtools",
        "about:inspect",
    ]
    .iter()
    .any(|prefix| lower.starts_with(prefix))
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReconnectedTarget {
    pub info: TransportTargetInfo,
    pub session: Option<TransportSessionId>,
    pub visibility: TargetVisibility,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReconnectedSnapshot {
    pub connection_generation: u64,
    pub compatibility: BrowserCompatibility,
    pub targets: Vec<ReconnectedTarget>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CaptureEffectContext {
    pub target_id: TargetId,
    pub connection_generation: u64,
    pub attachment_generation: u64,
    pub transport_session: TransportSessionId,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ViewportEffectContext {
    pub target_id: TargetId,
    pub target_key: String,
    pub connection_generation: u64,
    pub attachment_generation: u64,
    pub transport_session: TransportSessionId,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CaptureBinding {
    Inactive,
    Active(CaptureEffectContext),
    Suspended(CaptureEffectContext),
    Terminal,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SupervisorTargetState {
    pub target: SupervisedTarget,
    pub transport_session: Option<TransportSessionId>,
    pub prior_to_suspension: Option<TargetLifecycle>,
    pub capture_binding: CaptureBinding,
    pub viewport_override: Option<ViewportMetrics>,
    pub page_sequence: PageSequence,
    pub opener_target_key: Option<String>,
    pub opener_target_id: Option<TargetId>,
    pub last_visibility_observed_at: Option<SessionTime>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SupervisorState {
    pub session_state: BrowserSessionState,
    pub connection_generation: u64,
    pub revision: u64,
    pub compatibility: BrowserCompatibility,
    pub targets_by_key: std::collections::HashMap<String, SupervisorTargetState>,
    pub target_key_by_session: std::collections::HashMap<TransportSessionId, String>,
    /// Flat auto-attach can report a new, not-yet-recordable target before its URL commits. Keep
    /// that session alive until TargetInfoChanged makes the target eligible for supervision.
    pub pending_attached_sessions: std::collections::HashMap<String, TransportSessionId>,
    pub pending_attached_order: std::collections::VecDeque<String>,
    pub selected_target_key: Option<String>,
    pub next_page_sequence: u64,
}

impl SupervisorState {
    pub fn new(compatibility: BrowserCompatibility) -> Self {
        Self {
            session_state: BrowserSessionState::Connecting,
            connection_generation: 0,
            revision: 0,
            compatibility,
            targets_by_key: std::collections::HashMap::new(),
            target_key_by_session: std::collections::HashMap::new(),
            pending_attached_sessions: std::collections::HashMap::new(),
            pending_attached_order: std::collections::VecDeque::new(),
            selected_target_key: None,
            // Sequence 1 is the empty-inventory cursor, so waiting after an initial empty list
            // cannot miss the first discovered page.
            next_page_sequence: 2,
        }
    }

    pub fn page_contexts(&self) -> Result<PageContextInventory> {
        let cursor = PageSequence::new(self.next_page_sequence.saturating_sub(1))?;
        let mut pages = self
            .targets_by_key
            .iter()
            .filter(|(_, target)| {
                !matches!(
                    target.target.lifecycle,
                    TargetLifecycle::Closed | TargetLifecycle::Failed
                )
            })
            .map(|(_, target)| PageContextStatus {
                page: PageStatus {
                    target: target.target.clone(),
                    selected: self.selected_target_key.as_deref()
                        == Some(target.target.target.browser_target_key()),
                },
                sequence: target.page_sequence,
                opener_target_id: target.opener_target_id.filter(|id| {
                    self.targets_by_key.values().any(|candidate| {
                        candidate.target.target.id() == *id
                            && !matches!(
                                candidate.target.lifecycle,
                                TargetLifecycle::Closed | TargetLifecycle::Failed
                            )
                    })
                }),
            })
            .collect::<Vec<_>>();
        pages.sort_by_key(|page| page.sequence);
        Ok(PageContextInventory { cursor, pages })
    }

    pub fn targets(&self) -> Vec<SupervisedTarget> {
        let mut targets = self
            .targets_by_key
            .values()
            .filter(|state| {
                !matches!(
                    state.target.lifecycle,
                    TargetLifecycle::Closed | TargetLifecycle::Failed
                )
            })
            .map(|state| state.target.clone())
            .collect::<Vec<_>>();
        targets.sort_by_key(|target| target.target.browser_target_key().to_owned());
        targets
    }

    pub fn target(&self, key: &str) -> Option<&SupervisorTargetState> {
        self.targets_by_key.get(key)
    }

    pub fn selected_target(&self) -> Option<&SupervisorTargetState> {
        self.selected_target_key
            .as_deref()
            .and_then(|key| self.targets_by_key.get(key))
            .filter(|target| {
                target.transport_session.is_some()
                    && !matches!(
                        target.target.lifecycle,
                        TargetLifecycle::Closed
                            | TargetLifecycle::Failed
                            | TargetLifecycle::Suspended
                    )
            })
    }

    pub fn resolve_selection(&self, selection: PageSelection) -> Result<&SupervisorTargetState> {
        let context = match &selection {
            PageSelection::Selected => ErrorContext::default(),
            PageSelection::Target(target_id) => ErrorContext {
                target_id: Some(*target_id),
                ..ErrorContext::default()
            },
        };
        let target = match selection {
            PageSelection::Selected => self.selected_target(),
            PageSelection::Target(id) => self
                .targets_by_key
                .values()
                .find(|target| target.target.target.id() == id),
        }
        .ok_or_else(|| {
            KrometrailError::new(
                ErrorCode::NotFound,
                NonEmptyText::new("selected browser page was not found").unwrap(),
            )
            .with_context(context)
            .with_recovery(
                NonEmptyText::new(
                    "create a page with create_page, or select an existing page with select_page",
                )
                .unwrap(),
            )
            .with_retry(RetryAdvice::AfterRecovery)
        })?;
        if target.transport_session.is_none()
            || matches!(
                target.target.lifecycle,
                TargetLifecycle::Closed | TargetLifecycle::Failed | TargetLifecycle::Suspended
            )
        {
            return Err(KrometrailError::from_browser_failure(
                ErrorCode::TargetFailed,
                NonEmptyText::new("browser page is not currently attached").unwrap(),
            ));
        }
        Ok(target)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ShutdownCause {
    StopRequested,
    Cancelled,
    BrowserProcessTerminated,
    ReconnectExhausted,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SupervisorInput {
    InitialTargets(Vec<TransportTargetInfo>),
    InitialReconciliationCompleted,
    TargetCreated(TransportTargetInfo),
    TargetInfoChanged(TransportTargetInfo),
    Attached {
        target_key: String,
        session: TransportSessionId,
    },
    TargetAttachFailed {
        target_key: String,
    },
    CaptureStartFailed {
        target_key: String,
    },
    Detached {
        session: TransportSessionId,
        reason: Option<String>,
    },
    TargetDestroyed {
        target_key: String,
    },
    VisibilityChanged {
        target_key: String,
        visibility: TargetVisibility,
        observed_at: SessionTime,
    },
    InitialVisibilityProbeFailed {
        target_key: String,
    },
    CaptureVisibilityChanged {
        target_id: TargetId,
        visibility: TargetVisibility,
        observed_at: SessionTime,
    },
    SelectTarget {
        target_key: String,
    },
    ViewportOverrideApplied {
        target_key: String,
        viewport: Option<ViewportMetrics>,
    },
    ConnectionLost(TransportClose),
    BrowserProcessTerminated {
        exit: crate::launcher::SanitizedProcessExit,
    },
    Reconnected(ReconnectedSnapshot),
    ReconnectExhausted,
    StopRequested,
    Cancelled,
    /// Transport tasks attach their connection generation to asynchronous observations. The
    /// reducer drops observations from an older connection after a successful rebuild; keeping
    /// this guard at the boundary prevents a late detach/attach event from undoing restored state.
    #[doc(hidden)]
    ForConnectionGeneration {
        generation: u64,
        input: Box<SupervisorInput>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SupervisorEffect {
    Attach {
        target_key: String,
    },
    Detach {
        session: TransportSessionId,
    },
    RestoreSessionDomains {
        target_key: String,
        session: TransportSessionId,
    },
    RestoreViewport {
        context: ViewportEffectContext,
        viewport: ViewportMetrics,
    },
    ProbeInitialVisibility {
        target_key: String,
        session: TransportSessionId,
    },
    StartCapture {
        context: CaptureEffectContext,
    },
    SuspendCapture {
        context: CaptureEffectContext,
    },
    ResumeCapture {
        context: CaptureEffectContext,
    },
    StopCapture {
        context: CaptureEffectContext,
    },
    Publish(BrowserSessionEvent),
    BeginReconnect,
    Shutdown {
        cause: ShutdownCause,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Reduction {
    pub state: SupervisorState,
    pub effects: Vec<SupervisorEffect>,
}

pub(crate) fn make_target(
    id: TargetId,
    info: &TransportTargetInfo,
    lifecycle: TargetLifecycle,
    visibility: TargetVisibility,
    generation: u64,
) -> Result<SupervisedTarget> {
    Ok(SupervisedTarget {
        target: PageTarget::new(
            id,
            info.target_key.clone(),
            info.url.clone(),
            info.title.clone(),
        )?,
        lifecycle,
        visibility,
        attachment_generation: generation,
    })
}

pub(crate) fn close_event(target: &SupervisorTargetState) -> SupervisorEffect {
    tracing::info!(
        target_id = %target.target.target.id(),
        attachment_generation = target.target.attachment_generation,
        "browser.target.closed"
    );
    SupervisorEffect::Publish(BrowserSessionEvent::TargetClosed {
        target_id: target.target.target.id(),
    })
}

pub(crate) fn target_changed_event(target: &SupervisorTargetState) -> SupervisorEffect {
    trace_target_lifecycle(target);
    SupervisorEffect::Publish(BrowserSessionEvent::TargetChanged {
        target: target.target.clone(),
    })
}

pub(crate) fn target_discovered_event(target: &SupervisorTargetState) -> SupervisorEffect {
    tracing::info!(
        target_id = %target.target.target.id(),
        attachment_generation = target.target.attachment_generation,
        target_type = "page",
        "browser.target.discovered"
    );
    SupervisorEffect::Publish(BrowserSessionEvent::TargetDiscovered {
        target: target.target.clone(),
    })
}

fn trace_target_lifecycle(target: &SupervisorTargetState) {
    let target_id = target.target.target.id();
    let attachment_generation = target.target.attachment_generation;
    match target.target.lifecycle {
        TargetLifecycle::Attached => tracing::info!(
            %target_id,
            attachment_generation,
            target_type = "page",
            "browser.target.attached"
        ),
        TargetLifecycle::Suspended => tracing::info!(
            %target_id,
            attachment_generation,
            target_type = "page",
            "browser.target.suspended"
        ),
        _ => tracing::info!(
            %target_id,
            attachment_generation,
            target_type = "page",
            "browser.target.changed"
        ),
    }
}

pub(crate) fn target_error() -> krometrail_core::KrometrailError {
    krometrail_core::KrometrailError::from_browser_failure(
        krometrail_core::ErrorCode::TargetFailed,
        krometrail_core::NonEmptyText::new("browser target supervision failed").unwrap(),
    )
}

pub(crate) fn process_error() -> krometrail_core::KrometrailError {
    krometrail_core::KrometrailError::from_browser_failure(
        krometrail_core::ErrorCode::BrowserProcessTerminated,
        krometrail_core::NonEmptyText::new("the managed browser process terminated").unwrap(),
    )
    .with_recovery(
        krometrail_core::NonEmptyText::new(
            "call start_browser to create a new browser session before continuing",
        )
        .unwrap(),
    )
}

pub(crate) fn reconnect_error() -> krometrail_core::KrometrailError {
    krometrail_core::KrometrailError::from_browser_failure(
        krometrail_core::ErrorCode::ReconnectExhausted,
        krometrail_core::NonEmptyText::new("browser reconnection attempts were exhausted").unwrap(),
    )
    .with_recovery(
        krometrail_core::NonEmptyText::new(
            "call start_browser to create a new browser session before continuing",
        )
        .unwrap(),
    )
}

pub(crate) fn cancelled_error() -> krometrail_core::KrometrailError {
    krometrail_core::KrometrailError::from_browser_failure(
        krometrail_core::ErrorCode::Cancelled,
        krometrail_core::NonEmptyText::new("browser supervision was cancelled").unwrap(),
    )
}

pub(crate) fn close_reason(_close: &TransportClose) -> &'static str {
    // The reason is deliberately not propagated into public errors or info-level tracing. It is
    // only an input that distinguishes transport closure from managed-process death.
    "transport connection lost"
}

#[cfg(test)]
mod tests {
    use super::*;
    use krometrail_core::{
        BrowserProduct, BrowserProductVersion, CapabilitySupport, RendererCapability,
    };

    fn compatibility() -> BrowserCompatibility {
        BrowserCompatibility::new(
            krometrail_core::BrowserVersion::new(
                BrowserProduct::Chrome,
                BrowserProductVersion::new("128").unwrap(),
                "revision",
                "1.3",
                "user-agent",
                "js",
            )
            .unwrap(),
            RendererCapability::ALL
                .iter()
                .map(|capability| CapabilitySupport::new(*capability, true, true, None).unwrap())
                .collect(),
        )
        .unwrap()
    }

    #[test]
    fn missing_selected_page_explains_how_to_recover() {
        let state = SupervisorState::new(compatibility());
        let error = state
            .resolve_selection(PageSelection::Selected)
            .unwrap_err();
        assert_eq!(error.code, ErrorCode::NotFound);
        assert_eq!(error.retry, RetryAdvice::AfterRecovery);
        assert_eq!(
            error.recovery.as_ref().unwrap().as_str(),
            "create a page with create_page, or select an existing page with select_page"
        );
    }
}
