use krometrail_core::{
    BrowserOperationResult, InteractionId, LiveObservation, LiveObservationRequest,
    ObservationPart, PageSelection, Result, TargetId,
};

use super::{PageControl, bind_target, navigation::OperationCancellation};
use crate::{SupervisorState, transport::CdpTransport};

pub(crate) struct PostOperationObservation {
    pub(crate) observation: ObservationPart<LiveObservation>,
    pub(crate) interruption: Option<krometrail_core::KrometrailError>,
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
                let interruption = matches!(
                    error.code,
                    krometrail_core::ErrorCode::Cancelled
                        | krometrail_core::ErrorCode::BrowserDisconnected
                )
                .then(|| error.clone());
                return Ok(PostOperationObservation {
                    observation: ObservationPart::Unavailable(error),
                    interruption,
                });
            }
        };
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
            Ok((BrowserOperationResult::ObserveLive(observation), interruption)) => {
                Ok(PostOperationObservation {
                    observation: ObservationPart::Available(*observation),
                    interruption,
                })
            }
            Ok(_) => unreachable!("live observation returns its associated result"),
            Err(error) => Ok(PostOperationObservation {
                observation: ObservationPart::Unavailable(error),
                interruption: None,
            }),
        }
    }
}
