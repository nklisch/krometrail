use krometrail_core::{DialogAction, ErrorCode, HandleDialogRequest, OpenDialogState, Result};
use serde_json::{Map, Value};

use super::{
    BoundTarget, interaction::interaction_error, navigation::OperationCancellation, operation_error,
};
use crate::transport::{CdpTransport, CommandScope, TransportError};

/// Handle the dialog that is already open on this target.
///
/// The session restoration path enables `Page` before a target becomes visible, so Chrome routes
/// dialog state and events to this exact flat session. There is no reliable benefit in probing,
/// yielding, or retrying: a rejected command is either the stable "not open" boundary or a real
/// transport/protocol failure, and retries could repeat a user-visible dialog action.
///
/// `dialog_state` is the same reported open-dialog state that page status and the
/// blocked-observation boundary read. Only a positive `None` short-circuits; `Unknown` still
/// dispatches so a page without installed dialog sources keeps working.
pub(super) async fn handle_dialog(
    transport: &dyn CdpTransport,
    bound: &BoundTarget,
    request: &HandleDialogRequest,
    dialog_state: OpenDialogState,
    cancel: &OperationCancellation,
    generation: u64,
) -> Result<()> {
    if dialog_state.is_known_absent() {
        return Err(dialog_not_open(bound.target_id));
    }
    let response = cancel
        .race(
            generation,
            bound.target_id,
            transport.send_raw(
                &CommandScope::Session(bound.transport_session.clone()),
                "Page.handleJavaScriptDialog",
                dialog_params(&request.action),
            ),
        )
        .await?;
    match response {
        Ok(_) => Ok(()),
        // Chrome rejects `Page.handleJavaScriptDialog` with a protocol error ("No dialog is
        // showing") rather than a transport-level command failure. Both classify as the stable
        // "not open" boundary; neither is a compatibility or connectivity problem.
        Err(TransportError::CommandFailed | TransportError::Protocol) => {
            Err(dialog_not_open(bound.target_id))
        }
        Err(
            TransportError::Disconnected
            | TransportError::Closed
            | TransportError::SubscriptionClosed,
        ) => Err(operation_error(
            ErrorCode::BrowserDisconnected,
            bound.target_id,
            "browser disconnected while handling the dialog",
        )),
        Err(_) => Err(interaction_error(
            bound.target_id,
            "browser rejected the dialog operation",
        )),
    }
}

fn dialog_params(action: &DialogAction) -> Value {
    let mut params = Map::new();
    match action {
        DialogAction::Accept { prompt_text } => {
            params.insert("accept".into(), Value::Bool(true));
            if let Some(prompt_text) = prompt_text {
                params.insert(
                    "promptText".into(),
                    Value::String(prompt_text.as_str().to_owned()),
                );
            }
        }
        DialogAction::Dismiss => {
            params.insert("accept".into(), Value::Bool(false));
        }
    }
    Value::Object(params)
}

fn dialog_not_open(target_id: krometrail_core::TargetId) -> krometrail_core::KrometrailError {
    operation_error(
        ErrorCode::NotFound,
        target_id,
        "dialog_not_open: no JavaScript dialog is currently open",
    )
}
