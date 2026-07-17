use krometrail_core::{
    BrowserOperationResult, InteractionId, LiveObservation, LiveObservationRequest,
    ObservationPart, PageSelection, Result, TargetId,
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
        let bound = match bind_target(state, selection) {
            Ok(bound) => bound,
            Err(error) => {
                return Ok(PostOperationObservation {
                    observation: ObservationPart::Unavailable(error),
                });
            }
        };
        self.await_compositor_ready(transport, &bound, cancel, state.connection_generation)
            .await;
        let started_at = self.session_time()?;
        match self
            .observe_live(
                transport,
                &bound,
                LiveObservationRequest { target: selection },
                started_at,
                Some((cancel, state.connection_generation)),
            )
            .await
        {
            Ok((BrowserOperationResult::ObserveLive(observation), _)) => {
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
    ) {
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
        }
    }
}
