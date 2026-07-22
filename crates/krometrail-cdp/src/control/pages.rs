use krometrail_core::{
    BrowserOperationResult, ErrorCode, ErrorContext, InteractionId, KrometrailError,
    LiveObservation, LiveObservationRequest, NonEmptyText, ObservationPart, PageSelection, Result,
    TargetId,
};

use super::{PageControl, bind_target, navigation::OperationCancellation};
use crate::transport::CommandScope;
use crate::{SupervisorState, transport::CdpTransport};
use serde_json::json;

const COMPOSITOR_READY_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(250);

pub(crate) struct PostOperationObservation {
    pub(crate) observation: ObservationPart<LiveObservation>,
}

impl PageControl {
    pub(crate) fn next_interaction_id(&self) -> InteractionId {
        InteractionId::from_uuid(*self.ids.next().as_uuid())
    }

    pub(crate) const fn session_id(&self) -> krometrail_core::SessionId {
        self.session_id
    }

    pub(crate) fn invalidate_target_snapshot(&mut self, target_id: TargetId) {
        self.snapshots.invalidate_target(target_id);
    }

    pub(crate) async fn observe_after_operation(
        &mut self,
        transport: &dyn CdpTransport,
        state: &SupervisorState,
        selection: PageSelection,
        cancel: &OperationCancellation,
    ) -> Result<PostOperationObservation> {
        self.observe_after_operation_with_geometry(transport, state, selection, cancel, false)
            .await
    }

    pub(crate) async fn observe_after_operation_with_geometry(
        &mut self,
        transport: &dyn CdpTransport,
        state: &SupervisorState,
        selection: PageSelection,
        cancel: &OperationCancellation,
        include_document_geometry: bool,
    ) -> Result<PostOperationObservation> {
        let bound = match bind_target(state, selection) {
            Ok(bound) => bound,
            Err(error) => {
                return Ok(PostOperationObservation {
                    observation: ObservationPart::Unavailable(error),
                });
            }
        };
        let compositor_marker = self
            .await_compositor_ready(transport, &bound, cancel, state.connection_generation)
            .await;
        let started_at = self.session_time()?;
        match self
            .observe_live(
                transport,
                &bound,
                LiveObservationRequest { target: selection },
                started_at,
                include_document_geometry,
                Some((cancel, state.connection_generation)),
            )
            .await
        {
            Ok((BrowserOperationResult::ObserveLive(mut observation), _)) => {
                if let Some(warning) = compositor_marker {
                    observation.attach_screenshot_warning(warning);
                }
                Ok(PostOperationObservation {
                    observation: ObservationPart::Available(*observation),
                })
            }
            Ok(_) => unreachable!("live observation returns its associated result"),
            Err(error) => Ok(PostOperationObservation {
                observation: ObservationPart::Unavailable(error),
            }),
        }
    }

    pub(crate) async fn await_compositor_ready(
        &self,
        transport: &dyn CdpTransport,
        bound: &super::BoundTarget,
        cancel: &OperationCancellation,
        connection_generation: u64,
    ) -> Option<KrometrailError> {
        let signal = transport.send_raw(
            &CommandScope::Session(bound.transport_session.clone()),
            "Runtime.evaluate",
            json!({
                "expression": "new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(()=>resolve(true))))",
                "awaitPromise": true,
                "returnByValue": true,
                "silent": true
            }),
        );
        let ready = tokio::time::timeout(
            COMPOSITOR_READY_TIMEOUT,
            cancel.race(connection_generation, bound.target_id, signal),
        )
        .await;
        if !matches!(ready, Ok(Ok(Ok(_)))) {
            tracing::warn!(
                event = "browser.compositor.signal_unavailable",
                failure_stage = "compositor_readiness",
                error_code = krometrail_core::ErrorCode::PageObservationFailed.as_str(),
                session_id = %self.session_id,
                target_id = %bound.target_id,
                attachment_generation = bound.attachment_generation,
                "browser.compositor.signal_unavailable"
            );
            return Some(self.compositor_rendezvous_unobserved(bound.target_id));
        }
        None
    }

    fn compositor_rendezvous_unobserved(&self, target_id: TargetId) -> KrometrailError {
        KrometrailError::from_browser_failure(
            ErrorCode::CompositorRendezvousUnobserved,
            NonEmptyText::new(
                "compositor readiness was not confirmed within the bounded wait; the immediate screenshot may not show the settled page state",
            )
            .expect("static compositor warning is non-empty"),
        )
        .with_context(ErrorContext {
            target_id: Some(target_id),
            ..ErrorContext::default()
        })
    }
}
