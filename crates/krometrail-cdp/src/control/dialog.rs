use krometrail_core::{DialogAction, ErrorCode, HandleDialogRequest, Result};
use serde_json::{Map, Value};

use super::{
    BoundTarget, interaction::interaction_error, navigation::OperationCancellation, operation_error,
};
use crate::transport::{CdpTransport, CommandScope, TransportError};

pub(super) async fn handle_dialog(
    transport: &dyn CdpTransport,
    bound: &BoundTarget,
    request: &HandleDialogRequest,
    cancel: &OperationCancellation,
    generation: u64,
) -> Result<()> {
    let mut params = Map::new();
    match &request.action {
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
    let response = cancel
        .race(
            generation,
            bound.target_id,
            transport.send_raw(
                &CommandScope::Session(bound.transport_session.clone()),
                "Page.handleJavaScriptDialog",
                Value::Object(params),
            ),
        )
        .await?;
    match response {
        Ok(_) => Ok(()),
        // The transport deliberately redacts Chrome's source message. For this command, Chrome's
        // ordinary command rejection means there is no current dialog; malformed protocol and
        // connection failures retain their distinct categories.
        Err(TransportError::CommandFailed) => Err(operation_error(
            ErrorCode::NotFound,
            bound.target_id,
            "dialog_not_open: no JavaScript dialog is currently open",
        )),
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
